import unittest

from .check import mismatches


def reader(name, complete, form='token-switch', failed=(), rows=()):
    return {'name': name, 'complete': complete, 'incomplete_because': list(failed), 'rows': list(rows),
            'dispatch': {'form': form, 'default': {'kind': 'persistent'}, 'loops': 0}}


def expectation(name, **expect):
    return {'reader': name, 'why': 'test', 'expect': expect}


class CheckTests(unittest.TestCase):
    def test_met_expectations_give_no_mismatch(self):
        document = {'readers': {
            '0x1': reader('CEntry::ReadMember', True, form='entry', rows=[{'key': None, 'kind': 'object-list',
                                                                          'nested': {'name': 'CCulture::ReadMember'}}]),
            '0x2': reader('CText::ReadMember', False, form='key-text', failed=['dispatch-form', 'key-text-read']),
        }}
        expectations = {'readers': [
            expectation('CEntry::ReadMember', form='entry', complete=True,
                        rows={'*': {'kind': 'object-list', 'nested': 'CCulture::ReadMember'}}),
            expectation('CText::ReadMember', complete=False, incomplete_because=['key-text-read']),
        ]}

        self.assertEqual(mismatches(expectations, document), [])

    def test_each_unmet_flag_and_a_missing_reader_is_a_mismatch(self):
        document = {'readers': {'0x1': reader('CText::ReadMember', True)}}
        expectations = {'readers': [
            expectation('CText::ReadMember', form='entry', complete=False, incomplete_because=['key-text-read'],
                        rows={'stages': {'kind': 'object-list'}}),
            expectation('CMissing::ReadMember', complete=True),
        ]}

        found = mismatches(expectations, document)

        self.assertEqual(len(found), 5)
        self.assertTrue(found[0].startswith("CText::ReadMember (0x1): complete is True"))
        self.assertIn('CMissing::ReadMember: 0 walked readers', found[-1])

    def test_a_value_branch_without_the_expected_proof_is_a_mismatch(self):
        branches = [{'value_token': 'block', 'proved': True, 'kind': 'list'},
                    {'value_token': 'other', 'proved': False, 'kind': 'scalar'}]
        document = {'readers': {'0x1': reader('CDesign::ReadMember', True, rows=[
            {'key': 'upgrades_to', 'kind': 'list', 'nested': None, 'branches': branches}])}}
        expectations = {'readers': [expectation('CDesign::ReadMember', rows={
            'upgrades_to': {'branches': {'block': {'proved': True, 'kind': 'list'}, 'other': {'proved': True}}}})]}

        self.assertEqual(mismatches(expectations, document),
                         ["CDesign::ReadMember (0x1): row 'upgrades_to' other branch proved is False, expected True"])


if __name__ == '__main__':
    unittest.main()
