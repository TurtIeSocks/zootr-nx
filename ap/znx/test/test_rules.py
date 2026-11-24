"""Rule trees as the world reads them with progressive items on."""

import unittest

from .. import kept
from ..rules import lined
from . import ZnxTestBase

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
        every = kept(lambda level: True)
        self.assertEqual(lined(rule(["Longshot"], 1), every), rule(["Progressive Hookshot"], 2))


class TestNutBagLine(ZnxTestBase):
    """With progressive items on, the Nut Bag (30) is the line's first
    level. The pool deals one upgrade as the line, from the scrub that
    sells it."""

    run_default_tests = False
    options = {"progressive_items": True, "scrubs": True}
    level = 1
    dealt = 1

    def test_the_30_bag_is_its_level_of_the_line(self) -> None:
        world = self.multiworld.worlds[self.player]
        tree = lined(rule(["Deku Nut Bag (30)"], 1), world.levels)
        self.assertEqual(tree["count"], self.level)
        self.assertEqual(world.pool()["Progressive Deku Nut Bag"], self.dealt)
        self.assertNotIn("Deku Nut Bag (20)", world.pool())


class TestNutBagLineShuffled(TestNutBagLine):
    """The shuffle puts the (20) first: the (30) is the second, and the
    pool deals the (20) as the line too."""

    options = {"progressive_items": True, "scrubs": True, "shuffle_nut_bag": True}
    level = 2
    dealt = 2

