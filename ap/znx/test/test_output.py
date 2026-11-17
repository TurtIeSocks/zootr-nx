"""A generated seed's `.apznx`: a placement for every check, and the
settings as our settings JSON."""

import json
import os
import tempfile
import zipfile

from Fill import distribute_items_restrictive
from worlds.AutoWorld import call_all

from ..exported import OPTIONS, WORLD
from . import ZnxTestBase


class TestOutput(ZnxTestBase):
    options = {"open_forest": True, "chest_size": "small"}

    def test_apznx(self) -> None:
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
