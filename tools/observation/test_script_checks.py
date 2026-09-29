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
    def capture(self, check=2):
        target = Mock()
        hook = target.BreakpointCreateByAddress.return_value
        hook.GetNumLocations.return_value = 1
        hook.GetNumResolvedLocations.return_value = 1
        hook.IsEnabled.return_value = True
        hook.GetID.return_value = 8
        capture = DiagnosticCapture(target, Mock(), dict(logger_entry=1,
            logger_text_register='x4', logger_level_register='w1', string_tag_offset=23), {'first.txt': 1, 'second.txt': 2}, check, 32, 4096)
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
