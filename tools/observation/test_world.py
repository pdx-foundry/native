"""World readiness excludes startup and loader stacks; absence preserves zero counts and values."""
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


class WorldVariableTests(unittest.TestCase):
    STORE, SCALE = 0x5000, 100000

    def observed(self, engine):
        """`engine` maps a variable name to its store and, when set, its unsigned raw value."""
        observer = world.WorldObserver(MagicMock(), 1, dict(
            binding=dict(variable_scale=self.SCALE), input=dict(variables=list(engine))),
            dict(string_size=24), 'attempt', 0, {})
        allocated, names = [], {}
        calls = MagicMock()

        def allocate(size, data=b''):
            allocated.append(data)
            return len(allocated)

        def call(binding, operation, *arguments):
            if operation == 'string_constructor':
                string, text = arguments
                names[string] = allocated[text - 1][:-1].decode()
                return 0
            store, raw = engine[names[arguments[1]]]
            if operation == 'variable_store':
                return store
            if operation == 'variable_is_set':
                return int(raw is not None)
            return raw

        calls.allocate.side_effect = allocate
        calls.call.side_effect = call
        return observer.variables(calls, 0x9000), calls

    def test_unset_zero_and_negative_variables_stay_distinct(self):
        observed, _ = self.observed(dict(unset=(self.STORE, None), zero=(self.STORE, 0),
                                         negative=(self.STORE, (1 << 64) - 275000)))
        self.assertEqual(observed, [
            dict(name='unset', value=None),
            dict(name='zero', value=dict(raw=0, scale=self.SCALE)),
            dict(name='negative', value=dict(raw=-275000, scale=self.SCALE))])

    def test_a_name_without_a_store_is_unset_and_is_not_read(self):
        observed, calls = self.observed(dict(local_absent=(0, None)))
        self.assertEqual(observed, [dict(name='local_absent', value=None)])
        self.assertEqual([entry.args[1] for entry in calls.call.call_args_list],
                         ['string_constructor', 'variable_store'])


if __name__ == '__main__':
    unittest.main()
