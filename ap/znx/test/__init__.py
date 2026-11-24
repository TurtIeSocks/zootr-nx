from test.bases import WorldTestBase

from ..exported import OPTIONS
from ..options import attr
from ..output import GAME


def ap_value(setting: str, value):
    """A setting's value as its AP option takes it: a toggle's as is, a
    choice's option id as an attribute name, a set's ids as labels."""
    entry = next(e for e in OPTIONS if e["id"] == setting)
    if entry["kind"] == "set":
        return [o["label"] for o in entry["options"] if o["id"] in value]
    return value if entry["kind"] == "toggle" else attr(value)


class ZnxTestBase(WorldTestBase):
    game = GAME

    def test_pool_fills_the_checks(self) -> None:
        """Every option's checks and pool grow together, one item a check."""
        if not self.run_default_tests:
            return
        checks = [loc for loc in self.multiworld.get_locations(self.player) if loc.address is not None]
        items = [item for item in self.multiworld.itempool if item.player == self.player]
        self.assertEqual(len(items), len(checks))
