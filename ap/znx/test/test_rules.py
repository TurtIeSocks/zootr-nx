"""Rule trees as the world reads them with progressive items on."""

import unittest

from ..exported import LEVELS
from ..rules import lined

FAKE = {"Progressive Fake": [["Fake One", "Fake Gift"], ["Fake Two"]]}


def rule(items: list, count: int) -> dict:
    return {"items": items, "count": count}


class TestLined(unittest.TestCase):
    def test_some_levels_read_as_the_lowest(self) -> None:
        both = ["Fake Gift", "Progressive Fake"]
        self.assertEqual(lined(rule(["Fake Two"], 1), FAKE), rule(both, 2))
        self.assertEqual(lined(rule(["Fake One", "Fake Two"], 1), FAKE), rule(both, 1))
        self.assertEqual(lined(rule(["Fake Gift"], 1), FAKE), rule(both, 1))

    def test_others_stay(self) -> None:
        for tree in (rule(["Fake Two", "Fake Other"], 1), rule(["Fake Two"], 2), True):
            self.assertEqual(lined(tree, FAKE), tree)

    def test_under_all_and_any(self) -> None:
        tree = {"all": [{"any": [rule(["Fake Two"], 1), False]}]}
        want = {"all": [{"any": [rule(["Fake Gift", "Progressive Fake"], 2), False]}]}
        self.assertEqual(lined(tree, FAKE), want)

    def test_longshot_is_the_second_hookshot(self) -> None:
        self.assertEqual(lined(rule(["Longshot"], 1), LEVELS), rule(["Progressive Hookshot"], 2))
