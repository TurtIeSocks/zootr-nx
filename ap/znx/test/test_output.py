"""A generated seed's `.apznx`: a placement for every check, the settings
as our settings JSON, and the start inventory."""

import json
import os
import tempfile
import zipfile

from Fill import distribute_items_restrictive
from worlds.AutoWorld import call_all

from ..exported import ITEM_NAME_TO_ID, OPTIONS, WORLD
from . import ZnxTestBase


class TestOutput(ZnxTestBase):
    options = {"open_forest": True, "chest_size": "small", "death_link": True}

    def test_apznx(self) -> None:
        # Generate.py's Main hands out the start inventory, which the test
        # base skips.
        self.multiworld.push_precollected(self.world.create_item("Boomerang"))
        distribute_items_restrictive(self.multiworld)
        call_all(self.multiworld, "post_fill")
        with tempfile.TemporaryDirectory() as out:
            self.world.generate_output(out)
            [name] = os.listdir(out)
            self.assertTrue(name.endswith(".apznx"))
            with zipfile.ZipFile(os.path.join(out, name)) as zf:
                manifest = json.loads(zf.read("archipelago.json"))
                seed = json.loads(zf.read("seed.json"))
                placements = json.loads(zf.read("placements.json"))
        self.assertEqual(manifest["game"], self.game)
        self.assertEqual(set(placements), {check["hash"] for check in WORLD["checks"]})
        self.assertTrue(all("own" in p for p in placements.values()))
        self.assertEqual(set(seed["settings"]), {entry["id"] for entry in OPTIONS})
        self.assertIs(seed["settings"]["open-forest"], True)
        self.assertEqual(seed["settings"]["chest-size"], "small")
        self.assertEqual(seed["start"], [ITEM_NAME_TO_ID["Boomerang"]])
        self.assertIs(seed["death_link"], True)
        self.assertEqual(seed["players"], {str(self.player): self.multiworld.player_name[self.player]})
