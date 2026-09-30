"""Attribution and capture bounds, independent of LLDB and a game."""
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation'))
from script_checks import DiagnosticCapture, attribute_message, stored_durations


def log_frame(level=2):
    frame = Mock()
    frame.FindRegister.return_value.GetValueAsUnsigned.return_value = level
    return frame


class AttributionTests(unittest.TestCase):
    def test_reader_and_validation_sources_join_the_same_check(self):
        for text in ['Error: "Unexpected token: bogus" in file: "first.txt" near line: 7',
                     'Wrong scope at  file: first.txt line: 7\nSupported Scopes: country']:
            check, line, normalized = attribute_message(text, {'first.txt': 1})
            self.assertEqual((check, line), (1, 7))
            self.assertIn('<script>', normalized)
            self.assertNotIn('first.txt', normalized)

    def test_tokens_or_multiple_sources_cannot_attribute_a_message(self):
        for text in ['Invalid technology being referenced: "first.txt"',
                     'file: first.txt line: 2 file: second.txt line: 4',
                     'file: unknown.txt line: 1']:
            self.assertEqual(attribute_message(text, {'first.txt': 1, 'second.txt': 2}), (None, None, text))

    def test_source_without_a_line_keeps_its_identity(self):
        self.assertEqual(attribute_message('error at file: first.txt', {'first.txt': 1}),
                         (1, None, 'error at file: <script>'))


class CaptureTests(unittest.TestCase):
    def capture(self, check=2, thread_id=None):
        target = Mock()
        hook = target.BreakpointCreateByAddress.return_value
        hook.GetNumLocations.return_value = 1
        hook.GetNumResolvedLocations.return_value = 1
        hook.IsEnabled.return_value = True
        hook.GetID.return_value = 8
        capture = DiagnosticCapture(target, Mock(), dict(logger_entry=1,
            logger_text_register='x4', logger_level_register='w1', string_tag_offset=23), {'first.txt': 1, 'second.txt': 2}, check, 32, 4096, thread_id)
        location = Mock()
        location.GetBreakpoint.return_value = hook
        return capture, hook, location

    def test_duplicates_stages_and_foreign_messages_stay_separate(self):
        capture, _, location = self.capture()
        for stage, text in [('read', 'Unexpected token: wrong in file: "second.txt" near line: 1'),
                            ('read', 'Unexpected token: wrong in file: "second.txt" near line: 1'),
                            ('validation', 'bad value at file: first.txt line: 2'),
                            ('validation', 'Invalid technology being referenced: "missing"')]:
            capture.stage = stage
            with patch('script_checks.read_string', return_value=(text, False)):
                capture.capture(log_frame(), location)
        result = capture.finish(1)
        self.assertEqual(len(result['diagnostics']), 2)
        self.assertEqual(result['diagnostics'][0], result['diagnostics'][1])
        self.assertEqual(result['foreign'][0]['check'], 1)
        self.assertEqual(result['foreign'][0]['diagnostic']['stage'], 'validation')
        self.assertEqual(len(result['unjoined']), 1)

    def test_raw_log_levels_are_preserved_without_filtering(self):
        capture, _, location = self.capture()
        for level in [0, 2, 0xffffffff]:
            with patch('script_checks.read_string', return_value=('message', False)):
                capture.capture(log_frame(level), location)
        self.assertEqual([message['level'] for message in capture.finish(0)['unjoined']], [0, 2, -1])

    def test_missing_or_lost_hook_never_claims_coverage(self):
        for before in [False, True]:
            capture, hook, _ = self.capture()
            hook.IsEnabled.return_value = False
            if before:
                capture.verify_hook()
                hook.IsEnabled.return_value = True
            self.assertFalse(capture.finish(0)['hooks_active'])

    def test_capture_stops_at_the_bound_but_the_call_can_continue(self):
        capture, _, location = self.capture()
        with patch('script_checks.read_string', return_value=('bad value', False)):
            for _ in range(100):
                capture.capture(log_frame(), location)
        result = capture.finish(1)
        self.assertEqual(len(result['unjoined']), 32)
        self.assertTrue(result['bound_reached'])

    def test_unreadable_message_loses_coverage(self):
        capture, _, location = self.capture()
        with patch('script_checks.read_string', side_effect=RuntimeError('unreadable')):
            capture.capture(log_frame(), location)
        self.assertFalse(capture.finish(1)['hooks_active'])

    def test_world_thread_capture_excludes_concurrent_logs_before_the_bound(self):
        capture, hook, location = self.capture(thread_id=7)
        hook.SetThreadID.assert_called_once_with(7)
        frame = log_frame()
        frame.GetThread.return_value.GetThreadID.return_value = 8
        with patch('script_checks.read_string') as read:
            for _ in range(100):
                capture.capture(frame, location)
            read.assert_not_called()
        self.assertFalse(capture.bound_reached)
        frame.GetThread.return_value.GetThreadID.return_value = 7
        with patch('script_checks.read_string', return_value=('unattributed engine error', False)):
            capture.capture(frame, location)
        self.assertEqual(capture.finish(1)['unjoined'][0]['text'], 'unattributed engine error')


class DurationTests(unittest.TestCase):
    """An effect owner with two children: a timed flag at 0x5000 and an unknown command at 0x6000."""
    FLAG_VTABLE = 0x9010
    LAYOUT = dict(literal=8, location=16, trigger=48, script_value=56, modifier=64,
                  modifier_unset=0xffffffff, variable=72)
    COMMAND = dict(children_array_offset=0x10)

    def memory(self):
        return {(0x1010, 8): 0x2000, (0x2000, 8): 0x5000, (0x2008, 8): 0x6000,
                (0x5000, 8): self.FLAG_VTABLE, (0x6000, 8): 0x9990,
                (0x50a8 + 8, 4): 2, (0x50a8 + 48, 8): 0, (0x50a8 + 56, 8): 0,
                (0x50a8 + 64, 4): 0xffffffff, (0x52b0, 4): 30}

    def strings(self):
        return {0x50a8 + 16: 'native_1.txt:1', 0x50a8 + 72: ''}

    def receiver(self, groups_complete=True):
        group = dict(units=['days', 'months', 'years'], factor_offset=0x2b0,
                     count=dict(offset=0xa8, decoder={'ScopedNumeric': dict(literal='Integer', layout=self.LAYOUT)}))
        return dict(vtable=0x1000, groups=[group], groups_complete=groups_complete)

    def observe(self, memory, receivers, children=2):
        def read_unsigned(address, size):
            if (address, size) not in memory:
                raise RuntimeError('unreadable')
            return memory[(address, size)]
        strings = self.strings()

        def read_string(address):
            if address not in strings:
                raise RuntimeError('unreadable')
            return strings[address]
        return stored_durations(read_unsigned, read_string, 0x1000, self.COMMAND, children, receivers)

    def flag_entry(self):
        return dict(child=0, units=['days', 'months', 'years'], factor=30,
                    count={'ScopedNumeric': dict(literal={'Integer': 2}, has_source_location=True,
                           has_trigger=False, has_script_value=False, has_modifier=False, variable='')})

    def test_matched_children_report_count_and_factor(self):
        receivers = {self.FLAG_VTABLE: self.receiver(), 0x9990: dict(vtable=2, groups=[], groups_complete=True)}
        self.assertEqual(self.observe(self.memory(), receivers),
                         dict(complete=True, stored=[self.flag_entry()]))

    def test_an_unmatched_child_leaves_the_answer_partial(self):
        self.assertEqual(self.observe(self.memory(), {self.FLAG_VTABLE: self.receiver()}),
                         dict(complete=False, stored=[self.flag_entry()]))

    def test_unreadable_storage_adds_no_entry_and_leaves_the_answer_partial(self):
        memory = self.memory()
        del memory[(0x52b0, 4)]
        receivers = {self.FLAG_VTABLE: self.receiver(), 0x9990: dict(vtable=2, groups=[], groups_complete=True)}
        self.assertEqual(self.observe(memory, receivers), dict(complete=False, stored=[]))

    def test_unreadable_children_or_incomplete_groups_are_partial(self):
        memory = self.memory()
        del memory[(0x1010, 8)]
        self.assertEqual(self.observe(memory, {}), dict(complete=False, stored=[]))
        receivers = {self.FLAG_VTABLE: self.receiver(groups_complete=False)}
        self.assertEqual(self.observe(self.memory(), receivers, children=1),
                         dict(complete=False, stored=[self.flag_entry()]))

    def test_no_children_need_no_child_array(self):
        self.assertEqual(self.observe({}, {}, children=0), dict(complete=True, stored=[]))


class RegisterTests(unittest.TestCase):
    def test_calls_use_the_thread_stack_below_its_red_zone_without_allocating(self):
        from script_checks import EngineCalls
        process = MagicMock()
        process.__iter__.return_value = iter([])
        frame = process.GetThreadByID.return_value.GetFrameAtIndex.return_value
        frame.FindRegister.return_value.GetValueAsUnsigned.return_value = 0x100000
        frame.FindRegister.return_value.GetData.return_value.GetByteSize.return_value = 8
        with patch.dict(sys.modules, lldb=Mock()):
            calls = EngineCalls(process, 7, 10)
        self.assertEqual(calls.stack, 0x100000 - 256)
        process.AllocateMemory.assert_not_called()

    def test_invalid_allocation_address_cannot_be_written_despite_success_status(self):
        from script_checks import EngineCalls
        calls = EngineCalls.__new__(EngineCalls)
        calls.process = Mock()
        calls.process.AllocateMemory.return_value = (1 << 64) - 1
        lldb = Mock(LLDB_INVALID_ADDRESS=(1 << 64) - 1,
                    ePermissionsReadable=1, ePermissionsWritable=2)
        lldb.SBError.return_value.Fail.return_value = False
        with patch.dict(sys.modules, lldb=lldb), self.assertRaisesRegex(RuntimeError, 'allocation failed'):
            calls.allocate(4)
        calls.process.WriteMemory.assert_not_called()

    def test_changed_pause_registers_cannot_return_to_held(self):
        from script_checks import EngineCalls
        for name in ['pc', 'sp', 'fp', 'lr', 'v0']:
            with self.subTest(name=name):
                calls = EngineCalls.__new__(EngineCalls)
                calls.process = Mock()
                calls.process.GetState.return_value = 1
                calls.frame = Mock()
                saved = Mock()
                saved.GetByteSize.return_value = 8
                saved.ReadRawData.return_value = b'original'
                calls.registers = {name: saved}
                calls.frame.return_value.FindRegister.return_value.GetData.return_value.ReadRawData.return_value = b'changed!'
                lldb = Mock(eStateStopped=1)
                lldb.SBError.return_value.Fail.return_value = False
                with patch.dict(sys.modules, lldb=lldb), self.assertRaisesRegex(RuntimeError, name):
                    calls.finish()


class EngineFailureTests(unittest.TestCase):
    def setUp(self):
        from script_checks import EngineCalls, WorkerDiagnostics
        import copy
        self.snapshots = []
        self.clock = 0
        self.clock_patch = patch('script_checks.time.monotonic', side_effect=lambda: self.clock)
        self.clock_patch.start()
        self.addCleanup(self.clock_patch.stop)
        self.report = WorkerDiagnostics('attempt', 9, lambda state: self.snapshots.append(copy.deepcopy(state)))
        self.lldb = Mock(eStateStopped=1, eStateRunning=2, eStopReasonBreakpoint=3,
                         eStopReasonException=4, LLDB_INVALID_ADDRESS=(1 << 64) - 1,
                         ePermissionsReadable=1, ePermissionsWritable=2)
        self.lldb.SBError.return_value.Fail.return_value = False
        self.lldb.SBError.return_value.Success.return_value = True
        self.module_patch = patch.dict(sys.modules, lldb=self.lldb)
        self.module_patch.start()
        self.addCleanup(self.module_patch.stop)
        self.calls = EngineCalls.__new__(EngineCalls)
        self.calls.diagnostics = self.report
        self.calls.thread_id = 7
        self.calls.deadline = 1
        self.calls.return_address = 0x1000
        self.calls.stack = 0x2000
        self.calls.registers = {}
        self.calls.process = MagicMock()
        self.calls.target = Mock()
        self.calls.address = Mock(return_value=0x3000)
        self.thread = self.calls.process.GetThreadByID.return_value
        self.calls.process.__iter__.side_effect = lambda: iter([self.thread])
        self.thread.GetThreadID.return_value = 7
        self.thread.GetStopReason.return_value = 3
        self.thread.GetStopReasonDataAtIndex.return_value = 8
        self.frame = self.thread.GetFrameAtIndex.return_value
        self.values = {}
        self.native_registers = {}

        def find(name):
            if name not in self.native_registers:
                register = Mock()
                register.SetValueFromCString.side_effect = lambda value: self.values.update({name: int(value, 16)}) or True
                register.GetValueAsUnsigned.side_effect = lambda: self.values.get(name, 0)
                self.native_registers[name] = register
            return self.native_registers[name]

        self.frame.FindRegister.side_effect = find
        self.calls.process.GetState.return_value = 1
        self.stop_id = 1
        self.calls.process.GetStopID.side_effect = lambda: self.stop_id
        self.hook = self.calls.target.BreakpointCreateByAddress.return_value
        self.hook.GetID.return_value = 8
        self.hook.GetNumResolvedLocations.return_value = 1
        self.calls.process.Continue.side_effect = self.return_normally
        self.bindings = {name: dict(address=0x3000, widths=[])
                         for name in ('read', 'fast_forward', 'effect_execute')}

    def return_normally(self):
        self.stop_id += 1
        self.values.update(pc=0x1000, sp=0x2000)
        error = Mock()
        error.Fail.return_value = False
        return error

    def fail_return(self, **change):
        self.return_normally()
        self.clock = 2
        if 'pc' in change or 'sp' in change:
            self.values.update(change)
        return self.lldb.SBError()

    def failure(self):
        return self.snapshots[-1]['failure']

    def test_completed_call_requires_return_and_register_verification(self):
        self.calls.call(self.bindings, 'fast_forward')
        self.assertEqual(self.snapshots[-1]['last_attempted_call'], self.snapshots[-1]['last_completed_call'])
        self.calls.verify_registers = Mock(side_effect=RuntimeError('verification failed'))
        with self.assertRaisesRegex(RuntimeError, 'verification failed'):
            self.calls.call(self.bindings, 'fast_forward')
        self.assertEqual(self.snapshots[-1]['last_attempted_call']['ordinal'], 2)
        self.assertEqual(self.snapshots[-1]['last_completed_call']['ordinal'], 1)

    def test_missing_return_hook_has_expected_address_and_stack(self):
        self.hook.GetNumResolvedLocations.return_value = 0
        with self.assertRaisesRegex(RuntimeError, 'return hook unresolved'):
            self.calls.call(self.bindings, 'read')
        self.assertEqual(self.failure()['kind'], 'missing-return-hook')
        self.assertEqual(self.failure()['details']['expected_pc'], '0x1000')
        self.calls.process.Continue.assert_not_called()

    def test_wrong_return_evidence_never_completes_a_call(self):
        for mismatch in ('pc', 'sp', 'breakpoint', 'thread', 'absent'):
            with self.subTest(mismatch=mismatch):
                self.clock = 0
                self.stop_id = 1
                self.values.clear()
                self.snapshots.clear()
                self.report.state['failure'] = None
                self.calls.process.GetState.return_value = 1
                self.thread.GetThreadID.return_value = 7
                self.thread.GetStopReason.return_value = 3
                self.thread.GetStopReasonDataAtIndex.return_value = 8
                if mismatch in ('pc', 'sp'):
                    self.calls.process.Continue.side_effect = lambda: self.fail_return(**{mismatch: 0xdead})
                else:
                    self.calls.process.Continue.side_effect = self.fail_return
                    if mismatch == 'breakpoint':
                        self.thread.GetStopReasonDataAtIndex.return_value = 99
                    elif mismatch == 'thread':
                        self.thread.GetThreadID.return_value = 99
                        self.thread.GetStopReason.return_value = 0
                    else:
                        self.calls.process.GetState.return_value = 2
                with self.assertRaisesRegex(RuntimeError, 'did not stop'):
                    self.calls.call(self.bindings, 'fast_forward')
                failure = self.failure()
                self.assertEqual(failure['kind'], 'return-stop')
                self.assertEqual(failure['details']['expected_sp'], '0x2000')
                self.assertEqual(failure['details']['expected_breakpoint'], '8')
                self.assertIsNone(self.snapshots[-1]['last_completed_call'])

    def test_native_exception_is_distinct_from_wrong_return(self):
        self.calls.process.Continue.side_effect = self.fail_return
        self.thread.GetStopReason.return_value = 4
        with self.assertRaises(RuntimeError):
            self.calls.call(self.bindings, 'effect_execute')
        self.assertEqual(self.failure()['kind'], 'native-exception')

    def test_world_job_exception_reports_the_faulting_thread_and_stack(self):
        job = MagicMock()
        job.GetThreadID.return_value = 17
        job.GetStopReason.return_value = 4
        frame = Mock()
        frame.GetPC.return_value = 0xdead
        frame.GetFunctionName.return_value = 'world_job_fault'
        job.__iter__.side_effect = lambda: iter([frame])
        unrelated = [Mock() for _ in range(8)]
        for thread in unrelated:
            thread.GetStopReason.return_value = 0
        self.calls.process.__iter__.side_effect = lambda: iter(unrelated + [self.thread, job])
        self.thread.GetStopReason.return_value = 0
        self.calls.process.Continue.side_effect = self.fail_return
        with self.assertRaisesRegex(RuntimeError, 'did not stop'):
            self.calls.call(self.bindings, 'fast_forward')
        failure = self.failure()
        self.assertEqual(failure['kind'], 'native-exception')
        self.assertEqual(failure['details']['thread'], '7')
        self.assertEqual(failure['details']['exception_thread'], '17')
        self.assertEqual(failure['details']['exception_stack'], '0xdead world_job_fault')
        self.assertIsNone(self.snapshots[-1]['last_completed_call'])

    def test_elapsed_deadline_reports_attempt_without_resume(self):
        self.clock = 2
        with self.assertRaisesRegex(RuntimeError, 'deadline elapsed'):
            self.calls.call(self.bindings, 'read')
        self.assertEqual(self.failure()['kind'], 'call-timeout')
        self.calls.process.Continue.assert_not_called()

    def test_allocation_success_status_with_invalid_address_retains_both(self):
        self.calls.process.AllocateMemory.return_value = self.lldb.LLDB_INVALID_ADDRESS
        with self.assertRaisesRegex(RuntimeError, 'allocation failed'):
            self.calls.allocate(64)
        self.assertEqual(self.failure()['kind'], 'allocation')
        self.assertEqual(self.failure()['details']['size'], '64')
        self.assertEqual(self.failure()['details']['address'], '0xffffffffffffffff')
        self.assertIn('debugger_error', self.failure()['details'])
        self.calls.process.WriteMemory.assert_not_called()

    def test_debugger_allocation_error_retains_status_and_address(self):
        self.lldb.SBError.return_value.Fail.return_value = True
        self.calls.process.AllocateMemory.return_value = 0
        with self.assertRaisesRegex(RuntimeError, 'allocation failed'):
            self.calls.allocate(16)
        self.assertEqual(self.failure()['details']['address'], '0x0')
        self.assertIn('debugger_error', self.failure()['details'])

    def test_vector_mismatch_reports_bytes_and_cannot_complete(self):
        saved = Mock()
        saved.GetByteSize.return_value = 16
        saved.ReadRawData.return_value = bytes(16)
        self.calls.registers = dict(v0=saved)
        register = self.frame.FindRegister('v0')
        register.SetValueFromCString.side_effect = None
        register.SetValueFromCString.return_value = True
        register.GetData.return_value.ReadRawData.return_value = bytes([1]) * 16
        with self.assertRaisesRegex(RuntimeError, 'changed a saved register'):
            self.calls.call(self.bindings, 'read')
        self.assertEqual(self.failure()['kind'], 'register-mismatch')
        self.assertEqual(self.failure()['details']['register'], 'v0')
        self.assertEqual(self.failure()['details']['actual'], (bytes([1]) * 16).hex())
        self.assertIsNone(self.snapshots[-1]['last_completed_call'])

    def test_register_restoration_failure_names_register_and_values(self):
        saved = Mock()
        saved.GetByteSize.return_value = 8
        saved.ReadRawData.return_value = b'expected'
        self.calls.registers = dict(cpsr=saved)
        register = self.frame.FindRegister('cpsr')
        register.SetData.return_value = False
        register.GetData.return_value.ReadRawData.return_value = b'observed'
        with self.assertRaisesRegex(RuntimeError, 'restoration failed'):
            self.calls.call(self.bindings, 'read')
        self.assertEqual(self.failure()['details']['register'], 'cpsr')
        self.assertEqual(self.failure()['details']['expected'], b'expected'.hex())
        self.assertEqual(self.failure()['details']['actual'], b'observed'.hex())
        self.assertIsNone(self.snapshots[-1]['last_completed_call'])

    def test_checkpoint_is_bounded_and_first_failure_survives_later_reports(self):
        self.report.failure('allocation', 'x' * 1000, debugger_error='y' * 1000)
        self.report.failure('worker-exit', 'later failure')
        self.report.update('cleanup')
        self.report.complete_call()
        self.assertEqual(self.failure()['kind'], 'allocation')
        self.assertEqual(len(self.failure()['reason']), 240)
        self.assertEqual(len(self.failure()['details']['debugger_error']), 240)

    def test_checkpoint_write_failure_cannot_replace_call_failure(self):
        self.report.publish = Mock(side_effect=OSError('disk full'))
        self.clock = 2
        with self.assertRaisesRegex(RuntimeError, 'deadline elapsed'):
            self.calls.call(self.bindings, 'read')
        self.assertEqual(self.report.state['failure']['kind'], 'call-timeout')


if __name__ == '__main__':
    unittest.main()
