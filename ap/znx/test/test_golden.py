"""Our logic and the AP world reach the same checks and the same goal:
golden.json holds what Rust reached for each option and item set."""

import unittest

from BaseClasses import CollectionState, LocationProgressType

from .. import VICTORY
from ..exported import OPTIONS, load
from ..options import attr
from . import ZnxTestBase, ap_value


class TestGolden(ZnxTestBase):
    def test_golden(self) -> None:
        whole = 0
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
                self.assertEqual(state.has(VICTORY, self.player), case["goal"])
                # The whole pool in hand: what a rule too loose to see
                # shows only here.
                whole += case["goal"] and len(case["items"]) == self.world.dealt.total()
        self.assertGreater(whole, 0)


class TestGoldenCoversWhatItChecks(unittest.TestCase):
    """The cases have to reach what a loose rule would let through: the
    goal both met and not, and every option beside Progressive Items on."""

    def test_the_goal_is_met_and_not(self) -> None:
        goals = {case["goal"] for case in load("golden")}
        self.assertEqual(goals, {True, False})

    def test_progressive_items_on_beside_each_option(self) -> None:
        cases = load("golden")
        base = cases[0]["settings"]
        paired = set()
        for case in cases:
            if case["settings"]["progressive-items"] and not base["progressive-items"]:
                paired |= {k for k, v in case["settings"].items() if v != base[k]}
        want = {entry["id"] for entry in OPTIONS if entry["kind"] != "set"} - {"progressive-items"}
        self.assertEqual(sorted(want - paired), [])
