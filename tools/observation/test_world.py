"""World readiness excludes startup and loader stacks; absence preserves zero counts."""
from pathlib import Path
import sys
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation'))
import world


class WorldBoundaryTests(unittest.TestCase):
    def boundary(self, ready=1, paused=1, state=100, idler=200, receiver=200, caller='Idle', parent='Frame'):
        frame = MagicMock()
        target = frame.GetThread.return_value.GetProcess.return_value.GetTarget.return_value
        target.ResolveFileAddress.side_effect = lambda address: address
        frame.FindRegister.return_value.GetValueAsUnsigned.return_value = receiver
        stack = [MagicMock(), MagicMock(), MagicMock()]
        for entry, name in zip(stack, ['Update', caller, parent]):
            entry.GetFunctionName.return_value = name
        frame.GetThread.return_value.__iter__.return_value = iter(stack)
        binding = dict(game_state=1, idler=2, ready_offset=8, paused_offset=16,
                       normal_stack=['Idle', 'Frame'])
        target.ResolveFileAddress.side_effect = None
        target.ResolveFileAddress.return_value.GetLoadAddress.side_effect = [1, 2]
        memory = {1: state, 2: idler, state + 8: ready, idler + 16: paused}
        with patch.object(world, 'read_unsigned', side_effect=lambda process, address, width=8: memory[address]):
            return world.ready_boundary(frame, binding)

    def test_only_paused_actual_idler_on_normal_stack_is_ready(self):
        self.assertTrue(self.boundary())
        for change in [dict(ready=0), dict(paused=0), dict(state=0), dict(idler=0),
                       dict(receiver=201), dict(caller='LoadSave'), dict(parent='RenderLoadingFrame')]:
            with self.subTest(change=change):
                self.assertFalse(self.boundary(**change))

    def test_absent_zero_and_negative_flags_stay_distinct(self):
        self.assertEqual(world.selected_flags(['absent', 'zero', 'negative'], {'zero': 0, 'negative': -1}),
                         [dict(name='absent', remaining=None), dict(name='zero', remaining=0),
                          dict(name='negative', remaining=-1)])


if __name__ == '__main__':
    unittest.main()
