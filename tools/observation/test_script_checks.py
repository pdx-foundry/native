"""Attribution and capture bounds, independent of LLDB and a game."""
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation'))
from script_checks import DiagnosticCapture, attribute_message


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
            logger_text_register='x4', string_tag_offset=23), {'first.txt': 1, 'second.txt': 2}, check, 32, 4096)
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
                capture.capture(Mock(), location)
        result = capture.finish(1)
        self.assertEqual(len(result['diagnostics']), 2)
        self.assertEqual(result['diagnostics'][0], result['diagnostics'][1])
        self.assertEqual(result['foreign'][0]['check'], 1)
        self.assertEqual(result['foreign'][0]['diagnostic']['stage'], 'validation')
        self.assertEqual(len(result['unjoined']), 1)

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
                capture.capture(Mock(), location)
        result = capture.finish(1)
        self.assertEqual(len(result['unjoined']), 32)
        self.assertTrue(result['bound_reached'])

    def test_unreadable_message_loses_coverage(self):
        capture, _, location = self.capture()
        with patch('script_checks.read_string', side_effect=RuntimeError('unreadable')):
            capture.capture(Mock(), location)
        self.assertFalse(capture.finish(1)['hooks_active'])


class RegisterTests(unittest.TestCase):
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
