"""A generated seed's `.apznx`: a placement for every check, the settings
as our settings JSON, and the start inventory."""

import json
import os
import random
import tempfile
import zipfile
from types import SimpleNamespace

from Fill import distribute_items_restrictive
from worlds.AutoWorld import call_all

from ..exported import HASHES, ITEM_NAME_TO_ID, OPTIONS, WORLD
from ..output import STONES, hint, hints
from . import ZnxTestBase


def apznx(test: ZnxTestBase) -> tuple[dict, dict, dict]:
    """The test's world filled and written: its manifest, seed and
    placements."""
    distribute_items_restrictive(test.multiworld)
    call_all(test.multiworld, "post_fill")
    with tempfile.TemporaryDirectory() as out:
        test.world.generate_output(out)
        [name] = os.listdir(out)
        test.assertTrue(name.endswith(".apznx"))
        with zipfile.ZipFile(os.path.join(out, name)) as zf:
            files = ("archipelago.json", "seed.json", "placements.json")
            manifest, seed, placements = (json.loads(zf.read(f)) for f in files)
    return manifest, seed, placements


class TestOutput(ZnxTestBase):
    options = {"open_forest": True, "chest_size": "small", "death_link": True}

    def test_apznx(self) -> None:
        # Generate.py's Main hands out the start inventory, which the test
        # base skips.
        self.multiworld.push_precollected(self.world.create_item("Boomerang"))
        manifest, seed, placements = apznx(self)
        self.assertEqual(manifest["game"], self.game)
        self.assertEqual(set(placements), {check["hash"] for check in WORLD["checks"] if self.world.holds(check)})
        self.assertTrue(all("own" in p for p in placements.values()))
        ids = set()
        for entry in OPTIONS:
            # A set's settings are the ones it lists.
            ids |= {o["id"] for o in entry["options"]} if entry["kind"] == "set" else {entry["id"]}
        self.assertEqual(set(seed["settings"]), ids)
        self.assertIs(seed["settings"]["open-forest"], True)
        self.assertEqual(seed["settings"]["chest-size"], "small")
        self.assertEqual(seed["start"], [ITEM_NAME_TO_ID["Boomerang"]])
        self.assertIs(seed["death_link"], True)
        self.assertEqual(seed["players"], {str(self.player): self.multiworld.player_name[self.player]})
        self.assertNotIn("hints", seed)


class TestHints(ZnxTestBase):
    options = {"hints": True}
    run_default_tests = False

    def test_stones_hint_this_slot_s_progression(self) -> None:
        _, seed, placements = apznx(self)
        wanted = {
            int(HASHES[location.name])
            for location in self.multiworld.get_locations(self.player)
            if location.address is not None and location.item.player == self.player and location.item.advancement
        }
        self.assertEqual(len(seed["hints"]), min(STONES, len(wanted)))
        self.assertEqual(len({h["own"] for h in seed["hints"]}), len(seed["hints"]))
        for h in seed["hints"]:
            self.assertIn(h["own"], wanted)
            self.assertEqual(placements[str(h["own"])], {"own": h["item"]})

    def test_the_same_seed_gives_the_same_hints(self) -> None:
        distribute_items_restrictive(self.multiworld)
        locations = list(self.multiworld.get_locations())
        names = self.multiworld.player_name
        pick = lambda seed: hints(locations, self.player, names, HASHES, random.Random(seed))
        self.assertEqual(pick(7), pick(7))
        self.assertNotEqual(pick(7), pick(8))

    def test_another_slot_s_check_goes_by_its_names(self) -> None:
        location = SimpleNamespace(player=2, name="Fake Pot 3", item=SimpleNamespace(code=5))
        want = {"item": 5, "player": "Zelda", "location": "Fake Pot 3"}
        self.assertEqual(hint(location, 1, {2: "Zelda"}, {}), want)


class TestFillerIsInTheDeal(ZnxTestBase):
    """Start inventory from the pool puts filler in its place: one these
    options' pool holds, or our client can't give it."""

    def test_filler_is_held(self) -> None:
        world = self.multiworld.worlds[self.player]
        held = set(world.pool())
        self.assertIn(world.get_filler_item_name(), held)
