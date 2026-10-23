import unittest

import export as export_module
import normalize
import rows

TOKENS = {'10': 'name', '11': 'icon', '12': 'potential', '13': 'stages'}


def call(name):
    return f'CALL {name} (CReader * param_1, int param_2)'


def walk(events):
    return {'leaves': 1, 'unknown': 0, 'truncated': False, 'value_test': False, 'events': events}


def branch_walk(texts):
    """The walk of one value branch with one path through `texts`."""
    return walk([(True, text) for text in texts])


def raw_row(token, kind='scalar', nested=None, **fields):
    row = {'token': token, 'kind': kind, 'value': 'Scalar', 'reader_kind': 'String', 'repeat': 'Replace',
           'conditional': False, 'nested': nested, 'family': None, 'items': None, 'call': None, 'branches': None,
           'paths': {'leaves': 1, 'unknown': 0, 'truncated': False}, 'store': None, 'scope_mask': None}
    row.update(fields)
    return row


def raw_reader(name, entry, table, form='token-switch', default=('persistent', None), delegates=(), **fields):
    reader = {'name': name, 'entry': entry, 'source': 'key', 'form': form,
              'switches': [{'at': '0x1', 'on_token': True, 'labels_known': True, 'cases': len(table)}],
              'compared': [row['token'] for row in table if row['token'] is not None],
              'default': {'kind': default[0], 'reader': default[1]}, 'delegates': list(delegates),
              'token_delegates': [],
              'key_text_read': False, 'loops': 0, 'truncated': False, 'rows': table}
    reader.update(fields)
    return reader


def export_of(*readers, registries=(), walked_names=None):
    walked = {reader['entry']: reader['entry'] for reader in readers}
    walked.update({reader['name']: reader['entry'] for reader in readers})
    walked.update(walked_names or {})
    return {'readers': {reader['entry']: reader for reader in readers}, 'walked': walked, 'resolved': {},
            'failures': {}, 'registries': list(registries), 'enums': {}}


def tables():
    return {'tokens': TOKENS, 'build': {'executable': 'e' * 64, 'version': 'v'}, 'schema': 1, 'tool': 'native-tables'}


def registry(name, entry):
    return {'registry': name, 'roots': [{'key': '', 'class': 'COwner', 'reader': 'COwner::ReadMember(CReader&, int)',
                                         'address': int(entry, 16)}]}


class CompletenessTests(unittest.TestCase):
    def test_an_enumerated_switch_with_a_rejecting_default_is_complete(self):
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10), raw_row(11)])
        document = normalize.normalize(export_of(owner), tables())

        reader = document['readers']['0x10']
        self.assertTrue(reader['complete'])
        self.assertEqual([row['key'] for row in reader['rows']], ['name', 'icon'])

    def test_only_a_switch_on_the_token_must_have_known_labels(self):
        switches = [{'at': '0x1', 'on_token': False, 'labels_known': False, 'cases': 3}]
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10)], switches=switches)
        self.assertTrue(normalize.normalize(export_of(owner), tables())['readers']['0x10']['complete'])

        switches[0]['on_token'] = True
        reader = normalize.normalize(export_of(owner), tables())['readers']['0x10']
        self.assertEqual(reader['incomplete_because'], ['domain-not-enumerated'])

    def test_default_routed_tokens_leave_the_rows(self):
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10), raw_row(11, kind='default-routed')])
        reader = normalize.normalize(export_of(owner), tables())['readers']['0x10']

        self.assertEqual([row['key'] for row in reader['rows']], ['name'])
        self.assertEqual(reader['default_routed'], [11])

    def test_an_incomplete_base_reader_makes_its_derived_reader_incomplete(self):
        base = raw_reader('CBase::ReadMember', '0x20', [raw_row(12)], default=('helper', None))
        derived = raw_reader('CDerived::ReadMember', '0x10', [raw_row(10)],
                             default=('base-reader', 'CBase::ReadMember'))
        readers = normalize.normalize(export_of(derived, base), tables())['readers']

        self.assertEqual(readers['0x20']['incomplete_because'], ['default-unresolved'])
        self.assertEqual(readers['0x10']['incomplete_because'], ['default-base-incomplete'])

    def test_a_default_that_calls_an_unwalked_reader_fails_the_delegate_condition(self):
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10)], default=('helper', None),
                           delegates=['CNotWalked::ReadMember'])
        reader = normalize.normalize(export_of(owner), tables())['readers']['0x10']

        self.assertEqual(reader['incomplete_because'], ['default-unresolved', 'delegate-unfollowed'])
        self.assertEqual(reader['dispatch']['delegates'],
                         [{'reader': None, 'name': 'CNotWalked::ReadMember', 'walked': False}])

    def test_a_token_path_that_calls_an_unwalked_member_reader_fails_the_delegate_condition(self):
        events = [(True, 'CALL CBase::ReadMember (CReader * param_1, int param_2) a1=this+0x8')]
        delegate = rows.classify(events, ['CALL CPersistent::ReadMember (CReader * param_1, int param_2)'], {}, 1)
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10, **delegate)], delegates=['CBase::ReadMember'])
        reader = normalize.normalize(export_of(owner), tables())['readers']['0x10']

        self.assertEqual((delegate['kind'], delegate['nested']), ('delegates', 'CBase::ReadMember'))
        self.assertEqual(reader['incomplete_because'], ['delegate-unfollowed', 'nested-unresolved'])

    def test_every_member_reader_that_a_token_path_calls_is_a_delegate(self):
        export = {'name': 'COwner::ReadMember', 'entry': '0x10', 'source': 'key', 'switches': [], 'compared': [10, 11],
                  'walks': {
                      'default': walk([(True, call('CPersistent::ReadMember'))]),
                      '10': walk([(True, call('CFirst::ReadMember')), (True, call('CSecond::ReadMember'))]),
                      '11': walk([(True, 'CALL CReader::Read (CReader * this, CString * param_1, bool param_2) '
                                         'a2=this+0x10'), (True, call('CThird::ReadMember'))]),
                  }}
        reader = export_module.walk_reader(export, {}, 1)
        first = raw_reader('CFirst::ReadMember', '0x20', [raw_row(12)])
        document = normalize.normalize(export_of(reader, first), tables())

        self.assertEqual(reader['delegates'], ['CFirst::ReadMember', 'CSecond::ReadMember', 'CThird::ReadMember'])
        self.assertEqual(export_module.nested_specs(reader) >= set(reader['delegates']), True)
        self.assertIn('delegate-unfollowed', document['readers']['0x10']['incomplete_because'])

    def test_a_delegate_with_an_incomplete_nested_reader_makes_the_tree_incomplete(self):
        stage = raw_reader('CStage::ReadMember', '0x30', [raw_row(10)], default=('helper', None))
        delegate = raw_reader('CDelegate::ReadMember', '0x20', [raw_row(13, kind='object-list', nested='0x30')])
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10)], delegates=['CDelegate::ReadMember'])
        document = normalize.normalize(export_of(owner, delegate, stage), tables())

        self.assertTrue(document['readers']['0x10']['complete'])
        self.assertFalse(normalize.complete_tree(document['readers'], '0x10'))

    def test_a_value_branch_that_reads_an_object_without_a_reader_fails_the_nested_condition(self):
        branch = {'value_token': 'block', 'kind': 'persistent', 'value': 'Block', 'reader_kind': 'Block',
                  'repeat': 'Unknown', 'nested': None, 'paths': {'leaves': 1, 'unknown': 0, 'truncated': False}}
        scalar = dict(branch, value_token='other', kind='scalar', value='Scalar', reader_kind='String')
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10, value='Any', branches=[branch, scalar])])
        document = normalize.normalize(export_of(owner, registries=[registry('common/x', '0x10')]), tables())

        self.assertEqual(document['readers']['0x10']['incomplete_because'], ['nested-unresolved'])
        self.assertEqual(normalize.summary(document)['complete_registry_trees'], 0)

    def test_the_key_text_outside_an_entry_reader_and_an_unwalked_nested_reader_fail(self):
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(13, kind='persistent', nested='class:CStage')],
                           key_text_read=True)
        reader = normalize.normalize(export_of(owner), tables())['readers']['0x10']

        self.assertEqual(reader['incomplete_because'], ['key-text-read', 'nested-unresolved'])

    def test_a_cut_loop_and_a_truncated_path_fail(self):
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(10, paths={'leaves': 1, 'unknown': 0,
                                                                             'truncated': True})], loops=1)
        reader = normalize.normalize(export_of(owner), tables())['readers']['0x10']

        self.assertEqual(reader['incomplete_because'], ['budget', 'loop-cut'])

    def test_an_entry_reader_is_complete_but_its_tree_proves_no_key_domain(self):
        entry = raw_reader('CEntry::ReadMember', '0x10', [raw_row(None, kind='object-list', nested='0x20')],
                           form='entry', default=('entry', None), key_text_read=True)
        culture = raw_reader('CCulture::ReadMember', '0x20', [raw_row(10)])
        document = normalize.normalize(export_of(entry, culture, registries=[registry('common/x', '0x10')]),
                                       tables())

        self.assertTrue(document['readers']['0x10']['complete'])
        self.assertFalse(normalize.complete_tree(document['readers'], '0x10'))
        self.assertEqual(normalize.summary(document)['complete_registry_trees'], 0)

    def test_a_registry_tree_is_complete_only_when_each_nested_reader_is(self):
        stage = raw_reader('CStage::ReadMember', '0x20', [raw_row(10)], default=('helper', None))
        owner = raw_reader('COwner::ReadMember', '0x10', [raw_row(13, kind='object-list', nested='0x20')])
        document = normalize.normalize(export_of(owner, stage, registries=[registry('common/x', '0x10')]),
                                       tables())

        self.assertTrue(document['readers']['0x10']['complete'])
        self.assertFalse(normalize.complete_tree(document['readers'], '0x10'))
        self.assertEqual(document['registries']['common/x']['roots'][0]['reader'], '0x10')


class RowTests(unittest.TestCase):
    def test_default_kinds(self):
        call = lambda name: (True, f'CALL {name} (CReader * param_1, int param_2)')
        self.assertEqual(rows.default_kind([call('CPersistent::ReadMember')]), ('persistent', None))
        self.assertEqual(rows.default_kind([call('CBase::ReadMember'), call('CPersistent::ReadMember')]),
                         ('base-reader', 'CBase::ReadMember'))
        self.assertEqual(rows.default_kind([call('CReader::ReportUnexpected')]), ('log-and-skip', None))
        self.assertEqual(rows.default_kind([call('CPreset::SetPresetData'), call('CPersistent::ReadMember')]),
                         ('helper', None))
        self.assertEqual(rows.default_kind([]), ('return', None))

    def test_a_default_rejects_only_when_its_deciding_call_is_on_every_path(self):
        persistent = 'CALL CPersistent::ReadMember (CReader * param_1, int param_2)'
        self.assertEqual(rows.default_kind([(False, persistent)]), ('helper', None))
        self.assertEqual(rows.default_kind([(False, 'CALL CBase::ReadMember (CReader * param_1, int param_2)')]),
                         ('helper', None))
        self.assertEqual(rows.default_kind([(True, 'CALL CString::CString (CString * this, char * param_1)')]),
                         ('helper', None))

    def test_a_statement_loop_is_recognized_and_a_byte_loop_is_not(self):
        self.assertTrue(rows.recognized_loop(['CALLIND slot+0x10 on=reader', 'CString::CString']))
        self.assertTrue(rows.recognized_loop(['CReader::ReadSimpleStatement']))
        self.assertFalse(rows.recognized_loop(['CPdxCommonStringAllocator::deallocate']))
        self.assertFalse(rows.recognized_loop([]))

    def test_a_scalar_read_keeps_its_owner_offset_and_scope_mask(self):
        events = [(True, 'CALL CReader::Read (CReader * this, CString * param_1, bool param_2) a2=this+0xa10 a3=#0x0')]
        result = rows.classify(events, [], {}, 0x3ff8)
        self.assertEqual((result['kind'], result['reader_kind'], result['repeat']), ('scalar', 'String', 'Replace'))
        self.assertEqual(rows.store_of(events, result), {'offset': 0xa10, 'size': None})

        trigger = [(True, 'CALL NParserUtil::ReadTrigger<CRootTrigger> (CReader * param_1, CRootTrigger * param_2, '
                          'EScopeType param_3) a2=this+0x2f0 scope=#0x0')]
        result = rows.classify(trigger, [], {}, 0x3ff8)
        self.assertEqual((result['kind'], result['family']), ('trigger', 'Trigger'))
        self.assertEqual(rows.scope_mask_of(result), 0)

    def test_a_templated_trigger_reader_replaces_and_a_direct_one_stays_unknown(self):
        templated = [(True, 'CALL NParserUtil::ReadTrigger<CRootTrigger> (CReader * param_1, CRootTrigger * param_2, '
                            'EScopeType param_3) a2=this+0x2f0')]
        direct = [(True, 'CALL CTrigger::Read (CTrigger * this, CReader * param_1, EScopeType param_2) a1=this+0x40')]
        self.assertEqual(rows.classify(templated, [], {}, 1)['repeat'], 'Replace')
        self.assertEqual(rows.classify(direct, [], {}, 1)['repeat'], 'Unknown')

    def test_a_constructed_object_on_one_path_is_conditional(self):
        events = [(False, 'CALL CStage::CStage (CStage * this)'), (False, 'CALLIND slot+0x20')]
        result = rows.classify(events, [], {}, 1)
        self.assertEqual((result['kind'], result['nested'], result['conditional']), ('object', 'class:CStage', True))

    def test_a_class_without_a_named_reader_is_found_only_through_its_own_vtable(self):
        constructors = [{'entry': '0x1', 'members': [],
                         'vtables': [{'offset': 0, 'address': '0x100'}, {'offset': 56, 'address': '0x200'}]}]
        self.assertEqual(export_module.primary_vtables(constructors), ['0x100'])

    def test_a_path_that_reads_no_value_and_stores_a_constant_is_a_presence_row(self):
        marker = [(True, 'STORE this+0x5fa size=1 val=#0x1')]
        result = rows.classify(marker, [], {}, 0x3ff8)
        self.assertEqual((result['kind'], result['value'], result['reader_kind'], result['conditional']),
                         ('presence', 'Scalar', 'Presence', False))

        skipped = [(True, 'CALL CReader::SkipToken (CReader * this)')] + marker
        self.assertEqual(rows.classify(skipped, [], {}, 0x3ff8)['value'], 'Any')

    def test_a_store_that_reads_the_value_or_a_path_that_reports_is_no_presence_row(self):
        tested = [(True, 'STORE this+0x5fa size=1 val=eq(reader+0x278,#0x3ff8)')]
        self.assertEqual(rows.classify(tested, [], {}, 0x3ff8)['kind'], 'store')

        logged = [(True, 'CALL CReader::ReportUnexpected (CReader * this)'), (True, 'STORE this+0x8 size=1 val=#0x0')]
        self.assertNotEqual(rows.classify(logged, [], {}, 0x3ff8)['kind'], 'presence')

    def test_a_value_branch_is_proved_when_the_test_comes_first_and_the_branch_reads_its_form(self):
        persistent = branch_walk(['VALUEBRANCH', 'CALL CReader::Read (CReader * this, CPersistent * param_1) a2=this+0x48'])
        block = rows.value_branch('block', persistent, rows.classify(persistent['events'], [], {0x48: '0x30'}, 1))
        boolean = branch_walk(['VALUEBRANCH', 'CALL CReader::Read (CReader * this, bool * param_1) a2=this+0x40'])
        other = rows.value_branch('other', boolean, rows.classify(boolean['events'], [], {}, 1))

        self.assertEqual((block['proved'], block['kind'], block['value'], block['nested']),
                         (True, 'persistent', 'Block', '0x30'))
        self.assertEqual((other['proved'], other['reader_kind'], other['value']), (True, 'Boolean', 'Scalar'))

    def test_a_value_branch_after_a_read_or_on_several_paths_is_not_proved(self):
        read_first = branch_walk(['CALL CReader::Read (CReader * this, bool * param_1)', 'STORE this+0x114 size=4 val=op61',
                                  'VALUEBRANCH', 'CALL CReader::ReadUniform<CString> (CReader * this, CPdxArray * '
                                  'param_1) a2=this+0x140'])
        outcome = rows.value_branch('block', read_first, rows.classify(read_first['events'], [], {}, 1))
        self.assertFalse(outcome['proved'])

        string = branch_walk(['VALUEBRANCH', 'CALL CReader::Read (CReader * this, CString * param_1, bool param_2)'])
        string.update(leaves=2, unknown=2)
        self.assertFalse(rows.value_branch('other', string, rows.classify(string['events'], [], {}, 1))['proved'])

    def test_a_value_branch_with_another_reader_or_a_cut_loop_after_its_read_is_not_proved(self):
        boolean = 'CALL CReader::Read (CReader * this, bool * param_1) a2=this+0x40'
        for continuation in ('CALL CUnqualified::Read (CReader * param_1)', 'LOOP CReader::Read'):
            branch = branch_walk(['VALUEBRANCH', boolean, continuation])
            outcome = rows.value_branch('other', branch, rows.classify(branch['events'], [], {}, 1))
            self.assertFalse(outcome['proved'], continuation)

    def test_a_path_that_constructs_a_string_in_the_owner_is_no_presence_row(self):
        events = [(True, 'CALL CString::CString (CString * this, char * param_1) a1=this+0x48'),
                  (True, 'STORE this+0x50 size=1 val=#0x1')]
        self.assertNotEqual(rows.classify(events, [], {}, 0x3ff8)['kind'], 'presence')

    def test_a_skipped_block_and_a_scalar_branch_that_does_nothing_keep_nothing(self):
        skip = branch_walk(['VALUEBRANCH', 'CALL CReader::SkipBody (CReader * this)'])
        block = rows.value_branch('block', skip, rows.classify(skip['events'], [], {}, 1))
        nothing = branch_walk(['VALUEBRANCH'])
        other = rows.value_branch('other', nothing, rows.classify(nothing['events'], [], {}, 1))

        self.assertEqual((block['proved'], block['reader_kind'], block['value']), (True, 'Skipped', 'Block'))
        self.assertEqual((other['proved'], other['reader_kind'], other['value']), (True, 'Skipped', 'Scalar'))

    def test_an_emplaced_string_that_the_path_reads_one_value_into_is_an_appended_scalar(self):
        events = [(True, 'CALL CPdxArray<CString,int>::InsertAtEmplace<> (CPdxArray<CString,int> * this, int param_1) '
                         'a1=this+0x140'),
                  (True, 'CALL CReader::Read (CReader * this, CString * param_1, bool param_2) a3=#0x0')]
        result = rows.classify(events, [], {}, 0x3ff8)
        self.assertEqual((result['kind'], result['reader_kind'], result['repeat']), ('scalar', 'String', 'Accumulate'))

    def test_a_script_value_is_a_weight_family_row(self):
        events = [(True, 'CALL CReader::Read (CReader * this, CPersistent * param_1) a2=this+0x40')]
        result = rows.classify(events, [], {0x40: 'class:CScriptableValue'}, 0x3ff8)
        self.assertEqual((result['kind'], result['family'], result['nested']), ('script-value', 'Weight', None))


if __name__ == '__main__':
    unittest.main()
