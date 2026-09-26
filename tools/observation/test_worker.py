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
                config = dict(bindings=bindings, file='common/traditions/example.txt',
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


if __name__ == '__main__':
    unittest.main()
