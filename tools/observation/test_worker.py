"""Pause ownership and callback failures without LLDB or a game."""
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

SOURCE = Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation'
sys.path.insert(0, str(SOURCE))
import protocol

spec = importlib.util.spec_from_file_location('worker', SOURCE / 'worker.py')
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class PauseTests(unittest.TestCase):
    def setUp(self):
        worker.progress = worker.SessionProgress(['one', 'two'])
        worker.request = dict(fault=None, registries={'one': dict(name='one')}, attempt='test')
        worker.registry_owners.clear()
        worker.breakpoints.clear()
        worker.entry_thread = 7
        self.frame = Mock()
        self.frame.GetThread().GetThreadID.return_value = 7
        self.emit = patch.object(worker, 'emit').start()
        self.addCleanup(patch.stopall)

    def callback(self, name):
        hook = Mock()
        hook.GetID.return_value = 1
        worker.breakpoints[name] = hook
        location = Mock()
        location.GetBreakpoint.return_value = hook
        return worker.callback(self.frame, location, None)

    def test_registries_pause_only_after_every_active_loader_returns(self):
        self.assertEqual(worker.decide_pause(worker.progress), (False, None))
        worker.progress.returned_registries.append('one')
        self.assertEqual(worker.decide_pause(worker.progress), (False, None))
        worker.progress.returned_registries.append('two')
        self.assertEqual(worker.decide_pause(worker.progress), (True, 'loaders-returned'))

    def test_fixture_callback_never_owns_the_pause(self):
        worker.fixture = Mock()
        worker.fixture.callback.return_value = False
        self.assertFalse(self.callback(protocol.HOOK['fixture_return']))
        worker.progress.returned_registries.extend(['one', 'two'])
        self.assertEqual(worker.decide_pause(worker.progress), (True, 'loaders-returned'))

    def test_modifier_boundary_owns_pause_with_or_without_registry_hooks(self):
        for registries in [[], ['one'], ['one', 'two']]:
            with self.subTest(registries=registries):
                worker.progress = worker.SessionProgress(registries, modifier_active=True)
                self.assertEqual(worker.progress.pause_owner, 'modifiers')
                self.assertEqual(worker.decide_pause(worker.progress), (False, None))
                worker.progress.returned_registries.extend(registries)
                self.assertEqual(worker.decide_pause(worker.progress), (False, None))
                worker.progress.modifier_returned = True
                self.assertEqual(worker.decide_pause(worker.progress), (True, 'content-loaded'))

    def test_no_active_pause_owner_stops_without_a_pause(self):
        self.assertEqual(worker.decide_pause(worker.SessionProgress()), (True, None))

    def test_validation_delays_the_existing_pause_owner(self):
        for modifiers in [False, True]:
            state = worker.SessionProgress(['one'], modifiers, fixture_validation=True)
            state.returned_registries.append('one')
            state.modifier_returned = modifiers
            self.assertEqual(worker.decide_pause(state), (False, None))
            state.fixture_validation_pending = False
            boundary = 'content-loaded' if modifiers else 'loaders-returned'
            self.assertEqual(worker.decide_pause(state), (True, boundary))

    def test_validation_cannot_hide_failure_or_deadline(self):
        state = worker.SessionProgress(['one'], fixture_validation=True)
        state.deadline_stopped = True
        self.assertEqual(worker.decide_pause(state), (True, 'deadline'))
        state.callback_failed = True
        self.assertEqual(worker.decide_pause(state), (True, None))

    def test_registry_failure_before_return_stops_without_a_pause(self):
        with patch.object(worker, 'registry_begin', side_effect=RuntimeError('missing owner')):
            self.assertTrue(self.callback(protocol.HOOK['registry'] + 'one'))
        self.assertEqual(worker.decide_pause(worker.progress), (True, None))

    def test_registry_snapshot_failure_keeps_the_witnessed_boundary(self):
        def failed_read(*args):
            worker.progress.returned_registries.append('one')
            raise RuntimeError('memory access failed')

        for modifier_active in [False, True]:
            with self.subTest(modifier_active=modifier_active):
                worker.progress = worker.SessionProgress(['one'], modifier_active)
                with patch.object(worker, 'registry_snapshot', side_effect=failed_read):
                    self.assertEqual(self.callback(protocol.HOOK['registry_return'] + 'one'), not modifier_active)
                expected = (False, None) if modifier_active else (True, 'loaders-returned')
                self.assertEqual(worker.decide_pause(worker.progress), expected)

    def test_registry_return_waits_for_modifier_boundary(self):
        worker.progress = worker.SessionProgress(['one'], modifier_active=True)
        worker.request['registries']['one'] = dict(name='one', directory='common/example',
            directory_offset=8, string_tag_offset=23, key_offset=8, count_offset=16,
            data_offset=24, pointer_size=8)
        worker.registry_owners['one'] = 0x1000
        with patch.object(worker, 'uint', return_value=0), \
                patch.object(worker, 'cstring', return_value='common/example'):
            self.assertFalse(self.callback(protocol.HOOK['registry_return'] + 'one'))
        self.assertEqual(worker.progress.returned_registries, ['one'])
        self.assertEqual(worker.decide_pause(worker.progress), (False, None))
        self.assertIn('registry-end', [call.args[0] for call in self.emit.call_args_list])

    def test_modifier_read_success_or_failure_pauses_at_its_return(self):
        for failure in [None, RuntimeError('bad table')]:
            with self.subTest(failure=failure):
                worker.breakpoints.clear()
                worker.progress = worker.SessionProgress(modifier_active=True)
                worker.modifiers = worker.ModifierObserver(dict(registries={}))
                with patch.object(worker.modifiers, 'entries', return_value=[], side_effect=failure), \
                        patch.object(worker, 'atomic', return_value=b'table'):
                    self.assertTrue(self.callback(protocol.HOOK['modifiers_return']))
                self.assertEqual(worker.decide_pause(worker.progress), (True, 'content-loaded'))

    def test_failed_modifier_callback_stops_without_a_safe_pause(self):
        worker.progress = worker.SessionProgress(modifier_active=True)
        worker.modifiers = Mock()
        worker.modifiers.callback.side_effect = RuntimeError('wrong thread')
        self.assertTrue(self.callback(protocol.HOOK['modifiers_documentation']))
        self.assertEqual(worker.decide_pause(worker.progress), (True, None))
        self.assertFalse(worker.progress.callback_active)

    def test_fixture_failure_does_not_abort_other_observations(self):
        worker.fixture = Mock()
        worker.fixture.callback.side_effect = RuntimeError('fixture unavailable')
        self.assertFalse(self.callback(protocol.HOOK['fixture_load']))
        worker.fixture.emit.assert_called_once()

    def test_worker_loss_at_each_observer_stops_without_a_pause(self):
        for target, hook in [('fixture', 'fixture_field'), ('modifiers', 'modifiers_return')]:
            with self.subTest(target=target):
                worker.breakpoints.clear()
                worker.progress = worker.SessionProgress(['one'], modifier_active=True)
                observer = Mock()
                observer.callback.return_value = True
                setattr(worker, target, observer)
                self.assertTrue(self.callback(protocol.HOOK[hook]))
                self.assertEqual(worker.decide_pause(worker.progress), (True, None))
        worker.breakpoints.clear()
        worker.progress = worker.SessionProgress(['one'])
        worker.progress.returned_registries.append('one')
        with patch.object(worker, 'registry_snapshot', return_value=True):
            self.assertTrue(self.callback(protocol.HOOK['registry_return'] + 'one'))
        self.assertEqual(worker.decide_pause(worker.progress), (True, None))

    def test_in_progress_snapshot_cannot_publish_a_pause(self):
        worker.progress.returned_registries.extend(['one', 'two'])
        worker.progress.callback_active = True
        self.assertEqual(worker.decide_pause(worker.progress), (False, None))
        worker.progress.callback_active = False
        self.assertEqual(worker.decide_pause(worker.progress), (True, 'loaders-returned'))

    def test_deadline_pause_cannot_override_completed_boundary_or_failure(self):
        worker.progress.deadline_stopped = True
        self.assertEqual(worker.decide_pause(worker.progress), (True, 'deadline'))
        worker.progress.returned_registries.extend(['one', 'two'])
        self.assertEqual(worker.decide_pause(worker.progress), (True, 'loaders-returned'))
        worker.progress.callback_failed = True
        self.assertEqual(worker.decide_pause(worker.progress), (True, None))

    def test_fixture_hooks_use_the_shared_names_for_each_selection(self):
        outcome = dict(registry='common/traditions', load_entry=1, constructor_entry=2,
                       reader_entry=3, member_entry=4, malformed_entry=5, unexpected_entry=6)
        bindings = dict(fields=[], outcome_registries=[outcome], load_entry=7,
                        registration_entry=8, field_entry=9)
        for registrations, fields, questions, diagnostics in [
                (True, True, False, False), (True, False, False, False),
                (False, True, False, False), (False, False, True, False),
                (False, False, True, True)]:
            with self.subTest(selection=(registrations, fields, questions, diagnostics)):
                config = dict(validation=False, bindings=bindings, file='common/traditions/example.txt',
                              registration_entries=registrations, field_reads=fields,
                              questions=[dict(index=0, definition='one', token=1, diagnostics=diagnostics)] if questions else [])
                observer = worker.FixtureObserver(config)
                expected = ['fixture_load']
                if registrations:
                    expected.append('fixture_registration')
                if fields:
                    expected.append('fixture_field')
                if questions:
                    expected.extend(['fixture_constructor', 'fixture_reader', 'fixture_member'])
                if diagnostics:
                    expected.extend(['fixture_malformed', 'fixture_unexpected'])
                self.assertEqual([name for name, _ in observer.hooks()], [protocol.HOOK[name] for name in expected])

    def test_dropped_record_fault_is_restricted_to_its_target(self):
        request = dict(fault=dict(target={'registry': 'one'}, control=protocol.CONTROL['dropped_record']))
        self.assertTrue(worker.dropped_by_fault('registry-entry', dict(name='one', index=0), request))
        self.assertFalse(worker.dropped_by_fault('registry-entry', dict(name='two', index=0), request))
        self.assertFalse(worker.dropped_by_fault('registry-entry', dict(name='one', index=1), request))
        self.assertFalse(worker.dropped_by_fault('fixture', dict(event=dict(kind='field-read', ordinal=1)), request))
        request = dict(fault=dict(target='fixture', control=protocol.CONTROL['dropped_record']),
                       fixture=dict(field_reads=True))
        self.assertTrue(worker.dropped_by_fault('fixture', dict(event=dict(kind='field-read', ordinal=1)), request))
        self.assertFalse(worker.dropped_by_fault('registry-entry', dict(name='one', index=0), request))


class ParserObservationTests(unittest.TestCase):
    def setUp(self):
        worker.request = dict(fault=None)
        worker.breakpoints.clear()
        self.file = 'common/traditions/example.txt'
        question = dict(index=0, definition='one', field='potential', token=1,
                        parsing=True, diagnostics=True, runtime=False, reader_id='block', reader_family='Trigger',
                        reader_kind='Block', storage_offset=None, unavailable='No storage decoder')
        self.observer = worker.FixtureObserver(dict(validation=False, file=self.file,
            bindings=dict(fields=[], outcome_registries=[]), questions=[question]))
        self.observer.loading = True
        self.observer.definitions['one'] = dict(owner=0x2000, line=1)
        self.observer.emit = Mock()
        self.observer.stored_string = Mock(side_effect=AssertionError('block storage was read'))
        self.observer.return_hook = Mock()
        self.observer.location = Mock(side_effect=[(self.file, 2), (self.file, 4)])
        self.frame = Mock()
        self.frame.GetThread().GetThreadID.return_value = 7

    def test_block_reader_returns_without_a_storage_decoder(self):
        registers = dict(owner='x0', reader='x1', **{'field-token': 'x2'})
        values = dict(x0=0x2000, x1=0x3000, x2=1)
        with patch.object(worker, 'register', side_effect=lambda frame, name: values[name]):
            self.observer.on_member(self.frame, Mock(), registers)
        name = self.observer.return_hook.call_args.args[1]
        worker.breakpoints[name] = Mock()
        self.observer.on_member_return(Mock(), 7, name)
        self.observer.stored_string.assert_not_called()
        calls = self.observer.emit.call_args_list
        self.assertEqual([call.args[0] for call in calls], ['field-parse', 'field-parse'])
        self.assertEqual([call.kwargs['returned'] for call in calls], [False, True])
        self.assertEqual([call.kwargs['line'] for call in calls], [2, 4])
        self.assertEqual([call.kwargs['occurrence'] for call in calls], [1, 1])
        self.assertFalse(self.observer.pending_fields)

    def test_unbound_parser_does_not_report_a_complete_empty_observation(self):
        self.observer.questions[0]['token'] = None
        self.observer.finish_questions(Mock(), 7)
        terminal = next(call for call in self.observer.emit.call_args_list
                        if call.args[0] == 'parsing-terminal')
        self.assertEqual(terminal.kwargs['count'], 0)
        self.assertIsNotNone(terminal.kwargs['unavailable'])
        self.observer.stored_string.assert_not_called()

    def test_engine_log_keeps_source_and_validation_stage(self):
        self.observer.validation = True
        self.observer.bindings['validation'] = dict(log_text_register='x4',
            source_file_prefix='file: ', source_line_prefix=' line: ')
        for returned in [False, True]:
            self.observer.returned = returned
            self.observer.stored_string = Mock(return_value=f'Wrong scope file: {self.file} line: 3')
            with patch.object(worker, 'register', return_value=1):
                self.observer.on_log(self.frame, Mock(), 7)
            call = self.observer.emit.call_args
            self.assertEqual(call.kwargs['file'], self.file)
            self.assertEqual(call.kwargs['line'], 3)
            self.assertEqual(call.kwargs['stage'],
                'engine-validation-log' if returned else 'engine-parser-log')

    def test_unresolved_or_ambiguous_log_source_is_retained(self):
        for text in [f'Unknown command @ {self.file}',
                     f'file: {self.file} line: 2 file: {self.file} line: 4']:
            diagnostic = worker.interpret_fixture_log(text, self.file, 'file: ', ' line: ', True)
            self.assertEqual(diagnostic['text'], text)
            self.assertIsNone(diagnostic['line'])
        self.assertIsNone(worker.interpret_fixture_log(
            'file: other.txt line: 3', self.file, 'file: ', ' line: ', True))

    def test_stream_log_reads_the_formatted_c_string(self):
        self.observer.bindings['validation'] = dict(stream_log_text_register='x1',
            source_file_prefix='file: ', source_line_prefix=' line: ')
        self.observer.returned = True
        with patch.object(worker, 'register', return_value=1), \
                patch.object(worker, 'string', return_value=f'[file: {self.file} line: 3]: Error'):
            self.observer.on_log(self.frame, Mock(), 7, stream=True)
        self.assertEqual(self.observer.emit.call_args.kwargs['line'], 3)
        self.assertEqual(self.observer.emit.call_args.kwargs['stage'], 'engine-validation-log')
        self.observer.stored_string.assert_not_called()

    def test_sourced_log_joins_the_bound_owner_source(self):
        self.observer.bindings['validation'] = dict(sourced_log_text_register='x1',
            sourced_log_owner_register='x0', sourced_log_source_offset=0x28,
            source_file_prefix='file: ', source_line_prefix=' line: ')
        self.observer.returned = True
        self.observer.stored_string = Mock(side_effect=[
            f'file: {self.file} line: 3', 'failed effect validation'])
        with patch.object(worker, 'register', side_effect=[0x2000, 0x3000]):
            self.observer.on_sourced_log(self.frame, Mock(), 7)
        self.assertEqual([call.args[1] for call in self.observer.stored_string.call_args_list],
                         [0x2028, 0x3000])
        self.assertEqual(self.observer.emit.call_args.kwargs['line'], 3)
        self.assertEqual(self.observer.emit.call_args.kwargs['stage'], 'engine-validation-log')

    def test_sourced_log_ignores_other_files_and_closed_windows(self):
        self.observer.bindings['validation'] = dict(sourced_log_owner_register='x0',
            sourced_log_source_offset=0x28)
        self.observer.stored_string = Mock(return_value='file: unrelated.txt line: 3')
        with patch.object(worker, 'register', return_value=0x2000):
            self.observer.on_sourced_log(self.frame, Mock(), 7)
        self.observer.emit.assert_not_called()
        self.assertEqual(self.observer.stored_string.call_count, 1)
        self.observer.validation_finished = True
        self.observer.on_sourced_log(self.frame, Mock(), 7)
        self.assertEqual(self.observer.stored_string.call_count, 1)

    def test_validation_requires_a_returned_load_and_finishes_before_pause(self):
        self.observer.validation = True
        worker.progress = worker.SessionProgress(['one'], fixture_validation=True)
        with self.assertRaises(RuntimeError):
            self.observer.on_validated(7)
        self.assertTrue(worker.progress.fixture_validation_pending)
        self.observer.returned = True
        self.observer.on_validated(7)
        self.assertFalse(worker.progress.fixture_validation_pending)
        kinds = [call.args[0] for call in self.observer.emit.call_args_list]
        self.assertEqual(kinds, ['validation-complete', 'diagnostics-unavailable', 'end'])
        with self.assertRaises(RuntimeError):
            self.observer.on_validated(7)


if __name__ == '__main__':
    unittest.main()
