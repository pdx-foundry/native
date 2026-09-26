"""Game-free checks for the generated worker codec."""
import importlib.util
from pathlib import Path
import unittest

PATH = Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation/protocol.py'
spec = importlib.util.spec_from_file_location('native_wire', PATH)
wire = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wire)


class ProtocolTests(unittest.TestCase):
    def test_worker_accepts_a_fixture_request_with_typed_engine_bindings(self):
        outcome = dict(registry='common/traditions', load_entry=16384, reader_entry=16400,
                       reader_return=16416, constructor_entry=16432, member_entry=16448,
                       malformed_entry=16464, unexpected_entry=16480,
                       fields=[dict(token=10001, name='custom_tooltip',
                       storage_offset=448)])
        fixture = dict(file='common/tradition_categories/atlas.txt', registration_entries=True,
                       field_reads=True, questions=[], bindings=dict(registration_entry=4096, load_entry=8192,
                       field_entry=12288, reader_lexer_offset=48, lexer_file_offset=8,
                       file_name_offset=32, string_tag_offset=23, file_line_offset=8,
                       fields=[dict(token=16793, name='tree_template'), dict(token=14263, name='traditions')],
                       outcome_registries=[outcome]))
        request = dict(version=wire.VERSION, attempt='a', game=1, executable='/game', target='build',
                       source_hashes={}, machine=dict(architecture='arm64', spawn_preference=0, registers={}),
                       registries={}, fault=None, deadline_seconds=180,
                       fixture=fixture, modifiers=None)
        self.assertEqual(wire.decode('request', wire.encode('request', request)), request)
        fixture['bindings']['fields'][0]['token'] = 'not an integer'
        with self.assertRaises(ValueError):
            wire.encode('request', request)

    def test_every_reader_kind_round_trips_in_fixture_events(self):
        schema_values = {variant['const'] for variant in wire.SCHEMAS['reader_kind']['oneOf']}
        self.assertEqual(set(wire.READER_KIND.values()), schema_values)
        for kind in wire.READER_KIND.values():
            with self.subTest(kind=kind):
                self.assertEqual(wire.decode('reader_kind', wire.encode('reader_kind', kind)), kind)
                event = dict(kind='field-authority', question=0, reader_id=None,
                             reader_kind=kind, storage_supported=False, unavailable='unsupported')
                row = dict(run='a', seq=1, thread=7, kind='fixture', event=event)
                self.assertEqual(wire.decode('record', wire.encode('record', row)), row)
        for kind in ['string', 'Unsupported', 1, None]:
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                wire.encode('reader_kind', kind)

    def test_fault_wire_has_one_target_and_kind(self):
        request = dict(version=wire.VERSION, attempt='a', game=1, executable='/game', target='build',
                       source_hashes={}, machine=dict(architecture='arm64', spawn_preference=0, registers={}),
                       registries={}, fault=None, deadline_seconds=180, fixture=None, modifiers=None)
        for target in [{'registry': 'common/traditions'}, 'fixture', 'modifiers']:
            for control in wire.CONTROL.values():
                with self.subTest(target=target, control=control):
                    value = dict(request, fault=dict(target=target, control=control))
                    self.assertEqual(wire.decode('request', wire.encode('request', value)), value)
        for fault in [dict(target='missing', control='worker-loss'),
                      dict(target='fixture', control='missing'),
                      dict(target={'registry': 'one', 'fixture': None}, control='worker-loss'),
                      dict(control='worker-loss')]:
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                wire.encode('request', dict(request, fault=fault))
        with self.assertRaises(ValueError):
            wire.encode('request', dict(request, fixture_fault=True))

    def test_modifier_table_is_typed_and_bounded_by_its_own_limit(self):
        table = dict(attempt='a', entries=[dict(name='pop_happiness', mask=1)],
                     registries={'common/buildings': dict(keys=['building_capital']),
                                 'common/bypass': dict(unavailable='item key storage was not established')})
        self.assertEqual(wire.decode('modifier_table', wire.encode('modifier_table', table).rstrip(b'\n')), table)
        for entry in [dict(name='pop_happiness'), dict(name='pop_happiness', mask=-1), dict(name=1, mask=1)]:
            with self.subTest(entry=entry), self.assertRaises(ValueError):
                wire.encode('modifier_table', dict(table, entries=[entry]))
        large = dict(table, entries=[dict(name='modifier_%06d' % index, mask=1) for index in range(3000)])
        with self.assertRaises(ValueError):
            wire.encode('modifier_table', large)
        self.assertLess(len(wire.encode('modifier_table', large, wire.MAX_MODIFIER_TABLE)), wire.MAX_MODIFIER_TABLE)

    def test_fixture_records_keep_typed_source_and_terminal_counts(self):
        event = dict(kind='field-read', file='common/tradition_categories/atlas.txt',
                     line=2, field='tree_template', owner='0x2000', ordinal=1)
        row = dict(run='attempt', seq=9, thread=7, kind='fixture', event=event)
        self.assertEqual(wire.decode('record', wire.encode('record', row)), row)
        for changes in [dict(line=-1), dict(ordinal=True), dict(owner=None), dict(extra='unknown')]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                wire.encode('record', dict(row, event=dict(event, **changes)))
        terminal = dict(kind='end', registrations=3, field_reads=2, field_outcomes=0,
                        diagnostics=0, producer_last_sequence=12)
        self.assertEqual(wire.decode('record', wire.encode('record', dict(row, event=terminal)))['event'], terminal)
        diagnostic = dict(kind='diagnostic', text='Unexpected token',
                          stage='reader-unexpected-report', file=None, line=None,
                          definition=None, field=None, occurrence=None)
        self.assertEqual(wire.decode('record', wire.encode('record', dict(row, event=diagnostic)))['event'], diagnostic)

    def test_trace_requires_valid_event_and_envelope(self):
        row = dict(run='attempt', seq=1, kind='registry-entry', name='traditions',
                   owner='0x1000', index=0, object='0x2000', key='tr_example', thread=7)
        self.assertEqual(wire.decode('record', wire.encode('record', row)), row)
        for changes in [dict(seq=-1), dict(seq=True), dict(seq=2**64), dict(kind='unknown'),
                        dict(index='0'), dict(owner=None)]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                wire.encode('record', dict(row, **changes))
        del row['owner']
        with self.assertRaises(ValueError):
            wire.encode('record', row)

    def test_pause_record_names_its_cause(self):
        row = dict(run='attempt', seq=9, kind='session-paused', returned=['common/traditions'], thread=7)
        for cause in ['loaders-returned', 'content-loaded', 'deadline']:
            paused = dict(row, cause=cause)
            self.assertEqual(wire.decode('record', wire.encode('record', paused)), paused)
        for changes in [dict(), dict(cause='stopped'), dict(cause=None)]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                wire.encode('record', dict(row, **changes))

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
