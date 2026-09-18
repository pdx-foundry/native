"""Game-free checks for the generated worker codec."""
import importlib.util
from pathlib import Path
import unittest

PATH = Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation/protocol.py'
spec = importlib.util.spec_from_file_location('native_wire', PATH)
wire = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wire)


class ProtocolTests(unittest.TestCase):
    def test_trace_requires_valid_event_and_envelope(self):
        row = dict(run='attempt', seq=1, kind='field-observed', file='fixture', line=2,
                   field='tree_template', owner='0x42', ordinal=1, thread=7)
        self.assertEqual(wire.decode('record', wire.encode('record', row)), row)
        for changes in [dict(seq=-1), dict(seq=True), dict(seq=2**64), dict(kind='unknown'),
                        dict(line='2'), dict(owner=None)]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                wire.encode('record', dict(row, **changes))
        del row['owner']
        with self.assertRaises(ValueError):
            wire.encode('record', row)

    def test_handshake_and_grant_reject_unknown_fields(self):
        grant = dict(version=wire.VERSION, attempt='a', game=10, worker=11)
        self.assertEqual(wire.decode('grant', wire.encode('grant', grant)), grant)
        for value in [dict(grant, bypass=True), dict(grant, game=-1), dict(grant, worker='11')]:
            with self.assertRaises(ValueError):
                wire.encode('grant', value)

    def test_wire_is_bounded_and_rejects_partial_json(self):
        for body in [b'{' , b' ' * (wire.MAX_RECORD + 1)]:
            with self.assertRaises(ValueError):
                wire.decode('grant', body)
        with self.assertRaises(ValueError):
            wire.encode('record', dict(seq=1, run='a', kind='callback-error', error='x' * wire.MAX_RECORD))


if __name__ == '__main__':
    unittest.main()
