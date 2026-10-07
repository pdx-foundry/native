import unittest

from callbacks import count


KNOWN = "this=country root=SelfLink from=[leader,SelfLink] prev=[colony,SelfLink]"
OTHER = "this=planet root=SelfLink from=[SelfLink] prev=[SelfLink]"
UNRESOLVED = "this=Unresolved root=Unresolved from=[Unresolved] prev=[Unresolved]"
PATH_LIMIT = "a call site has more paths than the method follows (path-limit)"


class CallbackCountTests(unittest.TestCase):
    def test_names_are_counted_by_their_known_contexts_and_gaps(self):
        answer = {"completeness": "Partial", "names": {
            "several": [KNOWN, OTHER],
            "unresolved": [UNRESOLVED],
            "none": [],
        }, "gaps": [
            ["UnresolvedPath", {"kind": "answer_item", "name": "unresolved"}, PATH_LIMIT],
            ["UnresolvedPath", {"kind": "answer_item", "name": "none"}, PATH_LIMIT],
            ["OutsideMethod", None, "outside"],
        ]}

        report = count(answer, with_kinds=False)

        self.assertEqual(report["names"], 3)
        self.assertEqual(report["with_context"], 2)
        self.assertEqual(report["with_known_context"], 1)
        self.assertEqual(report["several_known_contexts"], 1)
        self.assertEqual(report["typed_prev"], 1)
        self.assertEqual(report["only_unresolved"], 1)
        self.assertEqual(report["without_context"], 1)
        self.assertEqual(report["without_context_gaps"], {PATH_LIMIT: 1})
        self.assertEqual(report["only_unresolved_gaps"], {PATH_LIMIT: 1})
        self.assertEqual(report["answer_gaps"], {"outside": 1})

    def test_rules_carry_their_kind(self):
        answer = {"names": {"rule": ["Weighted", [OTHER]]}, "gaps": []}

        report = count(answer, with_kinds=True)

        self.assertEqual(report["kinds"], {"Weighted": 1})
        self.assertEqual(report["typed_prev"], 0)


if __name__ == "__main__":
    unittest.main()
