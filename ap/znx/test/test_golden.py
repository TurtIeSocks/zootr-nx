"""Our logic and the AP world reach the same checks: golden.json holds
what Rust reached for each option and item set."""

from BaseClasses import CollectionState, LocationProgressType

from ..exported import OPTIONS, load
from ..options import attr
from . import ZnxTestBase

def ap_value(setting: str, value):
    entry = next(e for e in OPTIONS if e["id"] == setting)
    return value if entry["kind"] == "toggle" else attr(value)


class TestGolden(ZnxTestBase):
    def test_golden(self) -> None:
        for n, case in enumerate(load("golden")):
            with self.subTest(case=n, settings=case["settings"], items=len(case["items"])):
                self.options = {attr(k): ap_value(k, v) for k, v in case["settings"].items()}
                self.world_setup()
                state = CollectionState(self.multiworld)
                for name in case["items"]:
                    state.collect(self.world.create_item(name), True)
                state.sweep_for_advancements()
                reached = {
                    location.address
                    for location in self.multiworld.get_reachable_locations(state, self.player)
                    # Our fill gives an excluded check filler: no way in.
                    if location.address is not None and location.progress_type != LocationProgressType.EXCLUDED
                }
                self.assertEqual(sorted(reached), case["reachable"])
