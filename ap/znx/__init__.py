"""Ocarina of Time (znx): the Switch 2 remake through zootr-nx.

The world only evaluates what `zootr ap export` wrote to `data/`: the
places, exits and checks of our logic, the pool and the options.
"""

import os
from collections import Counter
from typing import Any

from BaseClasses import Item, ItemClassification, Location, LocationProgressType, Region, Tutorial
from worlds.AutoWorld import WebWorld, World

from . import rules
from .exported import CLASSES, HASHES, ITEM_NAME_TO_ID, LOCATION_NAME_TO_ID, OPTIONS, POOL, WORLD
from .options import ZnxOptions, attr, setting_value
from .output import GAME, ZnxContainer, placement

VICTORY = WORLD["goal"]


class ZnxItem(Item):
    game = GAME


class ZnxLocation(Location):
    game = GAME


class ZnxWeb(WebWorld):
    tutorials = [
        Tutorial(
            "Multiworld Setup Guide",
            "Setting up Ocarina of Time (znx) for Archipelago.",
            "English",
            "setup_en.md",
            "setup/en",
            ["TurtIeSocks"],
        )
    ]


class ZnxWorld(World):
    """
    The Switch 2 Ocarina of Time, randomized by zootr-nx. Link wakes in
    Kokiri Forest with the world's items shuffled across chests, gifts and
    scrubs, and every other game's items mixed in.
    """

    game = GAME
    web = ZnxWeb()
    options_dataclass = ZnxOptions
    options: ZnxOptions
    item_name_to_id = ITEM_NAME_TO_ID
    location_name_to_id = LOCATION_NAME_TO_ID
    origin_region_name = WORLD["start"]
    topology_present = True

    setting_values: dict[str, Any]

    def generate_early(self) -> None:
        self.setting_values = {
            entry["id"]: setting_value(entry, getattr(self.options, attr(entry["id"])))
            for entry in OPTIONS
        }

    def rule(self, tree) -> rules.Rule | None:
        tree = rules.fold(tree, self.setting_values)
        return None if tree is True else rules.closure(tree, self.player)

    def create_regions(self) -> None:
        regions = {name: Region(name, self.player, self.multiworld) for name in WORLD["places"]}
        self.multiworld.regions.extend(regions.values())
        for exit_ in WORLD["exits"]:
            if rules.fold(exit_["rule"], self.setting_values) is not False:
                regions[exit_["from"]].connect(regions[exit_["to"]], rule=self.rule(exit_["rule"]))

        origin = regions[self.origin_region_name]
        for check in WORLD["checks"]:
            if check["unprovable"]:
                # Our logic shows no way in, so our fill gives it filler.
                location = self.place(check["name"], check["id"], [], regions, origin)
                location.progress_type = LocationProgressType.EXCLUDED
            else:
                location = self.place(check["name"], check["id"], check["at"], regions, origin)
            if check["forbid"]:
                forbid = frozenset(check["forbid"])
                location.item_rule = lambda item, forbid=forbid: (
                    item.player != self.player or item.name not in forbid
                )
        for event in WORLD["events"]:
            location = self.place(event["name"], None, event["at"], regions, origin)
            location.place_locked_item(ZnxItem(event["name"], ItemClassification.progression, None, self.player))

    def place(self, name: str, code: int | None, at: list, regions: dict, origin: Region) -> ZnxLocation:
        """A location where `at` says, or at the start when it says nowhere.
        At several places, it gets a region of its own entered from each."""
        at = [spot for spot in at if rules.fold(spot["rule"], self.setting_values) is not False]
        if len(at) == 1:
            region, rule = regions[at[0]["place"]], self.rule(at[0]["rule"])
        elif not at:
            region, rule = origin, None
        else:
            region, rule = Region(name, self.player, self.multiworld), None
            self.multiworld.regions.append(region)
            for spot in at:
                regions[spot["place"]].connect(region, rule=self.rule(spot["rule"]))
        location = ZnxLocation(self.player, name, code, region)
        if rule:
            location.access_rule = rule
        region.locations.append(location)
        return location

    def create_item(self, name: str) -> ZnxItem:
        return ZnxItem(name, ItemClassification[CLASSES[name]], ITEM_NAME_TO_ID[name], self.player)

    def create_items(self) -> None:
        for row in POOL:
            self.multiworld.itempool += [self.create_item(row["item"]) for _ in range(row["count"])]

    def get_filler_item_name(self) -> str:
        counts = Counter({row["item"]: row["count"] for row in POOL if CLASSES[row["item"]] == "filler"})
        return counts.most_common(1)[0][0]

    def set_rules(self) -> None:
        self.multiworld.completion_condition[self.player] = lambda state: state.has(VICTORY, self.player)

    def fill_slot_data(self) -> dict[str, Any]:
        return {"world_version": self.world_version.as_simple_string(), "settings": self.setting_values}

    def generate_output(self, output_directory: str) -> None:
        names = self.multiworld.player_name
        placements = {
            HASHES[location.name]: placement(location.item, self.player, names)
            for location in self.multiworld.get_locations(self.player)
            if location.address is not None
        }
        seed = {
            "settings": self.setting_values,
            "seed": self.random.getrandbits(64),
            "slot": names[self.player],
            "player": self.player,
            "death_link": bool(self.options.death_link.value),
            # The server never sends the start inventory with our
            # items_handling, so the mod hands it over.
            "start": [
                item.code
                for item in self.multiworld.precollected_items[self.player]
                if item.code is not None
            ],
        }
        name = self.multiworld.get_out_file_name_base(self.player)
        ZnxContainer(
            seed,
            placements,
            self.world_version.as_simple_string(),
            os.path.join(output_directory, name + ZnxContainer.patch_file_ending),
            player=self.player,
            player_name=names[self.player],
        ).write()
