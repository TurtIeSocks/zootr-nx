"""AP's default tests (every location reachable with everything, the
goal set, a fill that completes) for each option's other values."""

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


RACE = "Gerudo Valley Running man's race's Gift"


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
