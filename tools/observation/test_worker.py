"""Pause ownership and callback failures without LLDB or a game."""
import importlib.util
from pathlib import Path
import sys
import unittest
import subprocess
import tempfile
import time
from unittest.mock import Mock, patch

SOURCE = Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation'
sys.path.insert(0, str(SOURCE))
import protocol

spec = importlib.util.spec_from_file_location('worker', SOURCE / 'worker.py')
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class ProbePauseTests(unittest.TestCase):
    def test_paused_session_disables_all_remaining_observation_hooks(self):
        hooks = {'fixture:failed': Mock(), 'registry:return': Mock(), 'modifiers': Mock()}
        with patch.object(worker, 'breakpoints', hooks):
            worker.disable_observation_hooks()
        for hook in hooks.values():
            hook.SetEnabled.assert_called_once_with(False)

    def test_pause_reads_current_registers_without_cached_frame_pc(self):
        process = Mock()
        frame = process.GetThreadByID.return_value.GetFrameAtIndex.return_value
        frame.GetPC.side_effect = AssertionError('cached frame PC must not be used')
        values = {'pc': 100, 'sp': 200, 'fp': 300, 'lr': 400}
        with patch.object(worker, 'register', side_effect=lambda current, name: values[name]):
            self.assertEqual(worker.pause_registers(process, 7), values)
            values['sp'] = 500
            self.assertEqual(worker.pause_registers(process, 7)['sp'], 500)
        self.assertEqual(process.GetThreadByID.call_count, 2)


class AttachTests(unittest.TestCase):
    def test_completed_attach_returns_process_and_preserves_error(self):
        target, info, error = Mock(), Mock(), Mock()
        self.assertIs(worker.attach(target, info, error), target.Attach.return_value)
        target.Attach.assert_called_once_with(info, error)

    def test_attach_exception_reaches_worker(self):
        target = Mock()
        target.Attach.side_effect = RuntimeError('connection failed')
        with self.assertRaisesRegex(RuntimeError, 'connection failed'):
            worker.attach(target, None, None)

    def test_blocked_attach_records_reason_and_exits_without_shutdown(self):
        with tempfile.TemporaryDirectory() as root:
            script = '''
import sys, time
from pathlib import Path
from unittest.mock import Mock
sys.path.insert(0, sys.argv[1])
import worker
worker.ROOT = Path(sys.argv[2])
worker.request = dict(attempt='test', fault=None)
(worker.ROOT / 'raw-trace.jsonl').touch()
target = Mock()
target.Attach.side_effect = lambda *args: time.sleep(30)
worker.attach(target, None, None, timeout=.05)
raise AssertionError('timed out attach returned')
'''
            started = time.monotonic()
            result = subprocess.run([sys.executable, '-c', script, str(SOURCE), root], timeout=5)
            self.assertEqual(result.returncode, 1)
            self.assertLess(time.monotonic() - started, 5)
            record = protocol.decode('record', (Path(root) / 'raw-trace.jsonl').read_bytes())
            self.assertEqual(record['kind'], 'capability-unavailable')
            self.assertIn('debugger attach timed out', record['reason'])
            self.assertIn('approve one debugger attach', record['reason'])


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
            state = worker.SessionProgress(['one'], modifiers, fixture_pending=True)
            state.returned_registries.append('one')
            state.modifier_returned = modifiers
            self.assertEqual(worker.decide_pause(state), (False, None))
            state.fixture_pending = False
            boundary = 'content-loaded' if modifiers else 'loaders-returned'
            self.assertEqual(worker.decide_pause(state), (True, boundary))

    def test_validation_cannot_hide_failure_or_deadline(self):
        state = worker.SessionProgress(['one'], fixture_pending=True)
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

    def test_requested_hooks_include_the_hook_that_a_fault_leaves_out(self):
        request = dict(registries={'one': dict(load_entry=16), 'two': dict(load_entry=32)},
                       fault=dict(target={'registry': 'one'}, control=protocol.CONTROL['missing_hook']))
        self.assertEqual(worker.requested_hooks(request, None, None),
                         [(protocol.HOOK['registry'] + 'one', 16), (protocol.HOOK['registry'] + 'two', 32)])
        self.assertEqual(worker.controlled_hook(request), protocol.HOOK['registry'] + 'one')

        fixture = Mock()
        fixture.hooks.return_value = [(protocol.HOOK['fixture_field'], 48)]
        request = dict(registries={}, fixture=dict(field_reads=True, registration_entries=True),
                       fault=dict(target='fixture', control=protocol.CONTROL['late_hook']))
        self.assertEqual(worker.requested_hooks(request, None, fixture), [(protocol.HOOK['fixture_field'], 48)])
        self.assertEqual(worker.controlled_hook(request), protocol.HOOK['fixture_field'])
        request['fixture']['field_reads'] = False
        self.assertEqual(worker.controlled_hook(request), protocol.HOOK['fixture_registration'])
        request['fault'] = None
        self.assertIsNone(worker.controlled_hook(request))

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
                        reader_kind='Block', storage=None, storage_unavailable='No storage decoder')
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

    def test_validation_return_keeps_all_bound_logs_and_disables_parser_entries(self):
        self.observer.validation = True
        self.observer.bindings['validation'] = dict(log_entry=1, unformatted_log_entry=2,
            stream_log_entry=3, sourced_log_entry=4, complete_entry=5)
        self.observer.finish_questions = Mock()
        retained = [name for name, _ in self.observer.validation_hooks()]
        retained += [protocol.HOOK['fixture_malformed'], protocol.HOOK['fixture_unexpected']]
        parser_entry = protocol.HOOK['fixture_member']
        for name in retained + [parser_entry]:
            worker.breakpoints[name] = Mock()
        self.observer.on_return(Mock(), 7)
        for name in retained:
            worker.breakpoints[name].SetEnabled.assert_not_called()
        worker.breakpoints[parser_entry].SetEnabled.assert_called_once_with(False)

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

    def test_engine_log_callback_keeps_its_background_thread(self):
        self.observer.bindings['validation'] = dict(log_text_register='x4',
            source_file_prefix='file: ', source_line_prefix=' line: ')
        self.observer.returned = True
        self.frame.GetThread.return_value.GetThreadID.return_value = 8
        self.observer.stored_string = Mock(return_value=f'file: {self.file} line: 3')
        with patch.object(worker, 'entry_thread', 7), \
                patch.object(worker, 'request', {'machine': {'registers': {}}}), \
                patch.object(worker, 'register', return_value=1):
            self.observer.callback(self.frame, protocol.HOOK['fixture_log'])
        self.assertEqual(self.observer.emit.call_args.args, ('diagnostic', 8))
        self.assertEqual(self.observer.emit.call_args.kwargs['stage'], 'engine-validation-log')

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
        worker.progress = worker.SessionProgress(['one'], fixture_pending=True)
        with self.assertRaises(RuntimeError):
            self.observer.on_validated(7)
        self.assertTrue(worker.progress.fixture_pending)
        self.observer.returned = True
        self.observer.on_validated(7)
        self.assertFalse(worker.progress.fixture_pending)
        kinds = [call.args[0] for call in self.observer.emit.call_args_list]
        self.assertEqual(kinds, ['validation-complete', 'diagnostics-unavailable', 'end'])
        with self.assertRaises(RuntimeError):
            self.observer.on_validated(7)



class NumericStorageTests(unittest.TestCase):
    def setUp(self):
        patch.object(worker, 'request', dict(fault=None)).start()
        self.addCleanup(patch.stopall)

    def test_signed_storage_uses_the_bound_width_and_owner_offset(self):
        observer = worker.FixtureObserver(dict(validation=False, file='common/synthetic/test.txt',
            bindings=dict(fields=[], outcome_registries=[]), questions=[]))
        for decoder, width, raw, expected in [
            ('Integer', 4, 0x80000000, {'Integer': -2147483648}),
            ('Integer', 4, 0x7fffffff, {'Integer': 2147483647}),
            ({'FixedPoint': {'scale': 100000}}, 8, 2**64 - 125000,
             {'FixedPoint': {'raw': -125000, 'scale': 100000}}),
            ({'FixedPoint': {'scale': 32768}}, 8, 2**63,
             {'FixedPoint': {'raw': -(2**63), 'scale': 32768}}),
        ]:
            with self.subTest(decoder=decoder, raw=raw), patch.object(worker, 'uint', return_value=raw) as read:
                process = Mock()
                self.assertEqual(observer.stored_value(process, 0x2000, dict(offset=48, decoder=decoder)), expected)
                read.assert_called_once_with(process, 0x2030, width)
        with patch.object(worker, 'uint', side_effect=RuntimeError('unreadable')):
            with self.assertRaisesRegex(RuntimeError, 'unreadable'):
                observer.stored_value(Mock(), 0x2000, dict(offset=48, decoder='Integer'))


if __name__ == '__main__':
    unittest.main()


class NestedFixtureTests(unittest.TestCase):
    def setUp(self):
        worker.request = dict(fault=None, machine=dict(registers={'owner': 'x0', 'reader': 'x1'}))
        worker.breakpoints.clear()
        self.question = dict(index=0, definition='late_key', field='number', parent_field='block',
            nested=dict(parent_token=7, owner_offset=64, member_entry=900), token=8,
            storage=dict(offset=80, decoder={'FixedPoint': {'scale': 32768}}),
            storage_unavailable=None, reader_id='reader', reader_kind='FixedPoint', reader_family='NotApplicable',
            parsing=True, diagnostics=True, runtime=False)
        binding = dict(registry='common/example', load_entry=10, reader_entry=20, constructor_entry=24,
            member_entry=30, reader_return=40, malformed_entry=50, unexpected_entry=60, fields=[],
            inline=dict(root_return=24, key_storage=dict(offset=8, decoder='String')))
        config = dict(file='common/example/nested.txt', questions=[self.question], validation=False,
            registration_entries=False, field_reads=False, bindings=dict(fields=[], outcome_registries=[binding]))
        self.observer = worker.InlineFixtureObserver(config)
        self.observer.loading = True
        self.observer.thread = 7
        self.observer.active_reader = 2000
        self.frame = Mock()
        self.frame.GetThread().GetThreadID.return_value = 7
        self.registers = {'x0': 1000, 'x1': 2000, 'w2': 7}
        patch.object(worker, 'register', side_effect=lambda frame, name: self.registers[name]).start()
        self.observer.location = Mock(return_value=(config['file'], 2))
        self.observer.emit = Mock()
        self.observer.return_hook = Mock(side_effect=lambda frame, name: worker.breakpoints.setdefault(name, Mock()))
        self.observer.stored_value = Mock(side_effect=lambda process, owner, storage:
            {'String': 'late_key'} if storage['decoder'] == 'String' else {'FixedPoint': {'raw': 40960, 'scale': 32768}})
        self.addCleanup(patch.stopall)
        self.observer.begin_root(self.frame, None)

    def begin_leaf(self):
        self.observer.begin_parent(self.frame, None)
        self.registers.update(x0=1064, w2=8)
        self.observer.begin_leaf(self.frame, None, 900)

    def finish_parent(self):
        self.observer.callback(self.frame, self.observer.parent['name'])

    def test_late_key_preserves_source_occurrences_and_root_identity(self):
        self.begin_leaf()
        self.assertFalse(self.observer.emit.called)
        self.observer.finish_leaf(None, self.observer.leaf['name'])
        self.finish_parent()
        self.observer.finish_root(None, 7)
        calls = self.observer.emit.call_args_list
        self.assertEqual([call.args[0] for call in calls], ['definition', 'field-parse', 'field-parse', 'field-storage'])
        self.assertTrue(all(call.kwargs['owner'] == hex(1000) for call in calls))
        self.assertEqual(calls[-1].kwargs['line'], 2)
        self.assertEqual(calls[-1].kwargs['value']['FixedPoint']['raw'], 40960)
        self.assertEqual(self.observer.occurrences[0], 1)
        self.observer.finish_questions(None, 7)
        terminal = next(call for call in self.observer.emit.call_args_list if call.args[0] == 'field-terminal')
        self.assertEqual(terminal.kwargs['final_value']['FixedPoint']['scale'], 32768)

    def test_shared_return_address_is_restricted_to_each_call_stack(self):
        worker.request['machine']['registers']['return'] = 'x30'
        self.registers.update(x30=3000, sp=4096)
        target = self.frame.GetThread().GetProcess().GetTarget()
        parent_hook, leaf_hook = Mock(), Mock()
        for hook in (parent_hook, leaf_hook):
            hook.GetNumResolvedLocations.return_value = 1
        target.BreakpointCreateByAddress.side_effect = [parent_hook, leaf_hook]
        worker.FixtureObserver.return_hook(self.observer, self.frame, 'parent')
        self.registers['sp'] = 3840
        worker.FixtureObserver.return_hook(self.observer, self.frame, 'leaf')
        self.assertEqual([call.args[0] for call in target.BreakpointCreateByAddress.call_args_list], [3000, 3000])
        parent_hook.SetCondition.assert_called_once_with('$sp == 0x1000')
        leaf_hook.SetCondition.assert_called_once_with('$sp == 0xf00')

    def test_wrong_nested_owner_reader_or_source_remains_unavailable(self):
        for changes, location in [({'x0': 1065}, 'common/example/nested.txt'),
                                  ({'x1': 2001}, 'common/example/nested.txt'),
                                  ({}, 'common/other/inline.txt')]:
            with self.subTest(changes=changes, location=location):
                self.observer.parent = None
                self.registers.update(x0=1000, x1=2000, w2=7)
                self.observer.location.return_value = ('common/example/nested.txt', 2)
                self.observer.begin_parent(self.frame, None)
                self.registers.update(x0=1064, w2=8)
                self.registers.update(changes)
                self.observer.location.return_value = (location, 2)
                self.observer.begin_leaf(self.frame, None, 900)
                self.assertIsNone(self.observer.leaf)
                self.assertTrue(self.observer.root_unavailable)
        self.finish_parent()
        self.observer.finish_root(None, 7)
        self.assertIsNotNone(self.question['storage_unavailable'])
        self.assertFalse(any(call.args[0] == 'field-storage' for call in self.observer.emit.call_args_list))
        self.observer.finish_questions(None, 7)
        terminal = next(call for call in self.observer.emit.call_args_list if call.args[0] == 'parsing-terminal')
        self.assertIsNotNone(terminal.kwargs['unavailable'])

    def test_root_and_file_terminal_refuse_unfinished_member(self):
        self.begin_leaf()
        with self.assertRaisesRegex(RuntimeError, 'unfinished owner join'):
            self.observer.finish_root(None, 7)
        self.registers['x0'] = 2000
        with self.assertRaisesRegex(RuntimeError, 'unfinished owner join'):
            self.observer.callback(self.frame, protocol.HOOK['fixture_return'])

    def test_nested_callbacks_cannot_hide_a_thread_change_in_the_buffer(self):
        self.frame.GetThread().GetThreadID.return_value = 8
        with self.assertRaisesRegex(RuntimeError, 'another thread'):
            self.observer.callback(self.frame, protocol.HOOK['fixture_member'])

    def test_unrequested_root_keeps_diagnostics_unscoped(self):
        self.observer.buffer = [('diagnostic', dict(selector=None, occurrence=None, text='bad token',
            stage=protocol.DIAGNOSTIC_STAGE['reader_malformed'], file='common/example/nested.txt', line=3))]
        self.observer.stored_value.return_value = {'String': 'other'}
        self.observer.stored_value.side_effect = None
        self.observer.finish_root(None, 7)
        self.observer.emit.assert_called_once()
        self.assertEqual(self.observer.emit.call_args.args[0], 'diagnostic')
        self.assertIsNone(self.observer.emit.call_args.kwargs['definition'])

    def test_file_final_value_is_read_again_after_root_completion(self):
        self.begin_leaf()
        self.observer.finish_leaf(None, self.observer.leaf['name'])
        self.finish_parent()
        self.observer.finish_root(None, 7)
        self.observer.stored_value.side_effect = None
        self.observer.stored_value.return_value = {'FixedPoint': {'raw': 99, 'scale': 32768}}
        self.observer.finish_questions(None, 7)
        terminal = next(call for call in self.observer.emit.call_args_list if call.args[0] == 'field-terminal')
        self.assertEqual(terminal.kwargs['final_value']['FixedPoint']['raw'], 99)

    def test_pause_waits_for_the_inline_file_boundary(self):
        state = worker.SessionProgress(['registry'], fixture_pending=True)
        state.returned_registries = ['registry']
        self.assertFalse(worker.decide_pause(state).stop)
        state.fixture_pending = False
        self.assertTrue(worker.decide_pause(state).stop)

    def test_worker_loss_waits_for_a_joined_parent_in_the_selected_file(self):
        self.observer.control = protocol.CONTROL['worker_loss']
        with tempfile.TemporaryDirectory() as root, patch.object(worker, 'ROOT', Path(root)), patch.object(worker, 'emit') as emit:
            marker = Path(root) / 'worker-loss-ready'
            self.registers['x0'] = 999
            self.assertFalse(self.observer.callback(self.frame, protocol.HOOK['fixture_member']))
            self.assertFalse(marker.exists())
            self.registers['x0'] = 1000
            self.observer.location.return_value = ('common/other/source.txt', 2)
            self.assertFalse(self.observer.callback(self.frame, protocol.HOOK['fixture_member']))
            self.assertFalse(marker.exists())
            self.observer.location.return_value = ('common/example/nested.txt', 2)
            self.assertTrue(self.observer.callback(self.frame, protocol.HOOK['fixture_member']))
            self.assertTrue(marker.exists())
            emit.assert_called_once_with('worker-loss-ready')
            self.observer.return_hook.assert_not_called()
