"""AP's default tests (every location reachable with everything, the
goal set, a fill that completes) for each option's other values."""

from itertools import product

from BaseClasses import LocationProgressType

from ..exported import OPTIONS
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

    def test_a_check_without_freed(self) -> None:
        location = self.multiworld.get_location(RACE, self.player)
        self.assertNotEqual(location.progress_type, LocationProgressType.EXCLUDED)


class TestRaceGiftWithFreed(ZnxTestBase):
    options = {"gerudo_fortress": "freed"}

    def test_a_check_with_freed(self) -> None:
        location = self.multiworld.get_location(RACE, self.player)
        self.assertNotEqual(location.progress_type, LocationProgressType.EXCLUDED)
