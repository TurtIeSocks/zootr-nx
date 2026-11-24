"""AP's default tests (every location reachable with everything, the
goal set, a fill that completes) for each option's other values."""

import unittest
from collections import Counter
from itertools import product

from BaseClasses import LocationProgressType
from .. import dealt, traded, trapped
from ..exported import JUNK, OPTIONS, POOL, SHARES, TRAP
from ..options import attr
from . import ZnxTestBase


def other_values(entry: dict) -> list:
    if entry["kind"] == "toggle":
        return [not entry["default"]]
    return [option["id"].replace("-", "_") for option in entry["options"] if option["id"] != entry["default"]]


for entry in OPTIONS:
    for value in other_values(entry):
        name = f"Test_{attr(entry['id'])}_{value}"
        globals()[name] = type(name, (ZnxTestBase,), {"options": {attr(entry["id"]): value}})

# The options that change the checks together (`ap::JOINT`), every
# combination with NPC Gifts on: Met Zelda keeps Zelda's Letter ✅ out,
# Freed the membership card's, Cows lose their milk to the bottle cap.
JOINT = ["met-zelda", "gerudo-fortress", "cows"]
JOINT_ENTRIES = [next(entry for entry in OPTIONS if entry["id"] == id_) for id_ in JOINT]
for combo in product(*([attr(str(e["default"])) if e["kind"] != "toggle" else e["default"], *other_values(e)] for e in JOINT_ENTRIES)):
    options = {"npc_gifts": True, **{attr(id_): value for id_, value in zip(JOINT, combo)}}
    name = "Test_npc_gifts_" + "_".join(str(value) for value in combo)
    globals()[name] = type(name, (ZnxTestBase,), {"options": options})


class TestDeadTreeOnlyWithAdultDekuTree(ZnxTestBase):
    """A place only an option opens is a region only with that option."""

    def test_no_way_in_no_region(self) -> None:
        names = {region.name for region in self.multiworld.get_regions(self.player)}
        self.assertNotIn("deku-tree-adult", names)
        self.assertIn("deku-tree-child", names)


class TestDeadTreeWithAdultDekuTree(ZnxTestBase):
    options = {"adult_deku_tree": True}

    def test_a_way_in_a_region(self) -> None:
        names = {region.name for region in self.multiworld.get_regions(self.player)}
        self.assertIn("deku-tree-adult", names)


RACE = "Gerudo Valley Running Man's Race"


class TestRaceGiftFollowsTheCarpenters(ZnxTestBase):
    """Freeing the carpenters in play opens the race, so it's a check without Freed too."""

    options = {"running_race": True}

    def test_a_check_without_freed(self) -> None:
        location = self.multiworld.get_location(RACE, self.player)
        self.assertNotEqual(location.progress_type, LocationProgressType.EXCLUDED)


class TestRaceGiftWithFreed(ZnxTestBase):
    options = {"running_race": True, "gerudo_fortress": "freed"}

    def test_a_check_with_freed(self) -> None:
        location = self.multiworld.get_location(RACE, self.player)
        self.assertNotEqual(location.progress_type, LocationProgressType.EXCLUDED)



class TestAddedItemsTakeTheCheapestRupees(ZnxTestBase):
    """What the mask and Bombchu Bag options add takes the place of the
    cheapest rupees, as in the app's deal."""

    options = {"shuffle_masks": True, "bombchus": "bag"}

    def test_each_takes_a_rupee(self) -> None:
        world = self.multiworld.worlds[self.player]
        held = [row for row in POOL if world.holds(row)]
        before = traded(row for row in held if not row.get("added"))
        added = Counter()
        for row in held:
            if row.get("added"):
                added[row["item"]] += row["count"]
        # The masks and bags, past what every world adds (the Magic Meter).
        always = sum(row["count"] for row in POOL if row.get("added") and "when" not in row)
        self.assertEqual(added.total(), always + 8 + 3)
        pool = world.pool()
        self.assertEqual(pool.total(), before.total())
        for item, count in added.items():
            self.assertEqual(pool[item], before[item] + count, item)
        takeable = Counter()
        for row in held:
            if row.get("junk"):
                takeable[row["item"]] += row["count"]
        lost = [before[rupee] - pool[rupee] for rupee in JUNK]
        self.assertEqual(sum(lost), added.total())
        for i, rupee in enumerate(JUNK):
            if lost[i]:
                self.assertTrue(all(lost[k] == takeable[c] for k, c in enumerate(JUNK[:i])), rupee)


class TestTradeOrder(unittest.TestCase):
    """The trade on its own: the cheapest rupee left first, and an item
    with none left is left out."""

    def test_cheapest_first(self) -> None:
        rows = [
            {"item": JUNK[2], "count": 1, "junk": True},
            {"item": JUNK[0], "count": 1, "junk": True},
            {"item": "Fake Bag", "count": 2, "added": True},
        ]
        self.assertEqual(traded(rows), Counter({"Fake Bag": 2}))

    def test_only_junk_rows(self) -> None:
        """A pot's Green Rupee shares the chest's name, but the trade
        takes only the chest's."""
        rows = [
            {"item": JUNK[0], "count": 1},
            {"item": JUNK[1], "count": 1, "junk": True},
            {"item": "Fake Bag", "count": 1, "added": True},
        ]
        self.assertEqual(traded(rows), Counter({JUNK[0]: 1, "Fake Bag": 1}))

    def test_none_left_leaves_it_out(self) -> None:
        rows = [
            {"item": JUNK[1], "count": 1, "junk": True},
            {"item": "Fake Bag", "count": 1, "added": True},
            {"item": "Fake Meter", "count": 1, "added": True},
        ]
        self.assertEqual(traded(rows), Counter({"Fake Bag": 1}))


class TestIceTrapsTakeTheirShare(ZnxTestBase):
    """Mayhem: every item a trap may take, after the trade, is a trap,
    and no other item goes."""

    options = {"ice_traps": "mayhem", "shuffle_masks": True}

    def test_every_trappable_item(self) -> None:
        world = self.multiworld.worlds[self.player]
        counts, trappable = dealt(row for row in POOL if world.holds(row))
        pool = world.pool()
        self.assertGreater(trappable.total(), 0)
        self.assertEqual(pool[TRAP], trappable.total())
        self.assertEqual(pool.total(), counts.total())
        for item, count in counts.items():
            self.assertEqual(pool[item], count - trappable[item], item)


class TestTrapShare(unittest.TestCase):
    """The share on its own: rounded up, picked only from what a trap may
    take."""

    def test_rounded_up(self) -> None:
        import random

        counts = Counter({"Fake Rupee": 11, "Fake Sword": 1})
        trappable = Counter({"Fake Rupee": 11})
        out = trapped(counts, trappable, SHARES["some"], random.Random(1))
        self.assertEqual(out[TRAP], 2)
        self.assertEqual(out["Fake Rupee"], 9)
        self.assertEqual(out["Fake Sword"], 1)

    def test_off_traps_nothing(self) -> None:
        import random

        counts = Counter({"Fake Rupee": 3})
        out = trapped(counts, Counter(counts), SHARES["off"], random.Random(1))
        self.assertEqual(out, counts)

    def test_the_trade_leaves_fewer_to_trap(self) -> None:
        rows = [
            {"item": JUNK[0], "count": 2, "junk": True, "trappable": True},
            {"item": "Fake Bag", "count": 1, "added": True},
        ]
        counts, trappable = dealt(rows)
        self.assertEqual(trappable, Counter({JUNK[0]: 1}))
        self.assertEqual(counts, Counter({JUNK[0]: 1, "Fake Bag": 1}))
