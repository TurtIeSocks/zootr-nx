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
from .exported import (
    CLASSES,
    HASHES,
    ITEM_NAME_TO_ID,
    JUNK,
    LEVELS,
    LINES,
    LOCATION_NAME_TO_ID,
    OPTIONS,
    POOL,
    SHARES,
    TRAP,
    WORLD,
)
from .options import ZnxOptions, attr, setting_value
from .output import GAME, ZnxContainer, placement

VICTORY = WORLD["goal"]
ICE_TRAPS = "ice-traps"
PROGRESSIVE = "progressive-items"



def dealt(rows) -> tuple[Counter, Counter]:
    """The items `rows` hold, each `added` one in place of the cheapest
    rupee left among the `junk` rows (a chest's, not a pot's), in row
    order, one with none left left out; and those of them a trap may
    take."""
    counts, takeable, trappable = Counter(), Counter(), Counter()
    trappable_junk = set()
    added = []
    for row in rows:
        item, count = row["item"], row["count"]
        if row.get("added"):
            added += [item] * count
        else:
            counts[item] += count
        if row.get("junk"):
            takeable[item] += count
        if row.get("trappable"):
            trappable[item] += count
            if row.get("junk"):
                trappable_junk.add(item)
    for item in added:
        rupee = next((j for j in JUNK if takeable[j] > 0), None)
        if rupee is not None:
            takeable[rupee] -= 1
            counts[rupee] -= 1
            counts[item] += 1
            if rupee in trappable_junk:
                trappable[rupee] -= 1
    return +counts, +trappable


def traded(rows) -> Counter:
    """The items `rows` hold after the trade."""
    return dealt(rows)[0]


def trapped(counts: Counter, trappable: Counter, share: list[int], random) -> Counter:
    """`counts` with `share` of `trappable` (rounded up) turned to traps,
    picked by `random`."""
    n, d = share
    pieces = sorted(trappable.elements())
    traps = -(-len(pieces) * n // d)
    out = counts.copy()
    for item in random.sample(pieces, traps):
        out[item] -= 1
    out[TRAP] += traps
    return +out


def lined(counts: Counter) -> Counter:
    """`counts` with each line's levels named by the line."""
    out = Counter()
    for item, count in counts.items():
        out[LINES.get(item, item)] += count
    return out


def forced(values: dict) -> dict:
    """`values` with what each picked choice forces written in, as the
    app and our seed do (`settings::with_picks`): Adult opens the Door of
    Time and the forest."""
    out = dict(values)
    for entry in OPTIONS:
        picked = next((o for o in entry.get("options", []) if o["id"] == values[entry["id"]]), None)
        for word in picked["forces"] if picked else []:
            setting, _, value = word.partition("=")
            out[setting] = value or True
    return out


def start(values: dict) -> str:
    """Where these options start Link: the first of the world's starts
    that holds, else its own."""
    held = (s["place"] for s in WORLD["starts"] if rules.fold(s["when"], values) is True)
    return next(held, WORLD["start"])


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
    Kokiri Forest with the world's items shuffled across chests, scrubs,
    and the songs and gifts the options add, and every other game's items
    mixed in.
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
    dealt: Counter

    def generate_early(self) -> None:
        self.setting_values = forced({
            entry["id"]: setting_value(entry, getattr(self.options, attr(entry["id"])))
            for entry in OPTIONS
        })
        self.origin_region_name = start(self.setting_values)
        share = SHARES[self.setting_values.get(ICE_TRAPS, "off")]
        self.dealt = trapped(*dealt(filter(self.holds, POOL)), share, self.random)
        if self.setting_values[PROGRESSIVE]:
            self.dealt = lined(self.dealt)

    def holds(self, row: dict) -> bool:
        """Whether these options hold `row`: a check, event or pool row an
        option adds or takes away carries `when`."""
        return "when" not in row or rules.fold(row["when"], self.setting_values) is True

    def rule(self, tree) -> rules.Rule | None:
        tree = rules.fold(tree, self.setting_values)
        if self.setting_values[PROGRESSIVE]:
            tree = rules.lined(tree, LEVELS)
        return None if tree is True else rules.closure(tree, self.player)

    def live_places(self, exits: list) -> set[str]:
        """The places the options' exits lead to from the start. The world
        holds every place some option opens, like the dead Deku Tree under
        Adult Deku Tree, and the rest have no way in."""
        live, todo = {self.origin_region_name}, [self.origin_region_name]
        while todo:
            here = todo.pop()
            for exit_ in exits:
                if exit_["from"] == here and exit_["to"] not in live:
                    live.add(exit_["to"])
                    todo.append(exit_["to"])
        return live

    def create_regions(self) -> None:
        exits = [e for e in WORLD["exits"] if rules.fold(e["rule"], self.setting_values) is not False]
        live = self.live_places(exits)
        regions = {name: Region(name, self.player, self.multiworld) for name in WORLD["places"] if name in live}
        self.multiworld.regions.extend(regions.values())
        for exit_ in exits:
            if exit_["from"] in regions:
                regions[exit_["from"]].connect(regions[exit_["to"]], rule=self.rule(exit_["rule"]))

        origin = regions[self.origin_region_name]
        for check in filter(self.holds, WORLD["checks"]):
            at = self.spots(check["at"], regions)
            if check["unprovable"] or (check["at"] and not at):
                # Our logic shows no way in, or none with these options, so
                # our fill gives it filler.
                location = self.place(check["name"], check["id"], [], regions, origin)
                location.progress_type = LocationProgressType.EXCLUDED
            else:
                location = self.place(check["name"], check["id"], at, regions, origin)
            if check["forbid"]:
                forbid = frozenset(check["forbid"])
                location.item_rule = lambda item, forbid=forbid: (
                    item.player != self.player or item.name not in forbid
                )
        for event in filter(self.holds, WORLD["events"]):
            at = self.spots(event["at"], regions)
            if event["at"] and not at:
                # No way in with these options: never reached, not free.
                continue
            location = self.place(event["name"], None, at, regions, origin)
            location.place_locked_item(ZnxItem(event["name"], ItemClassification.progression, None, self.player))

    def spots(self, at: list, regions: dict) -> list:
        """The spots of `at` these options leave a way into."""
        return [
            spot
            for spot in at
            if spot["place"] in regions and rules.fold(spot["rule"], self.setting_values) is not False
        ]

    def place(self, name: str, code: int | None, at: list, regions: dict, origin: Region) -> ZnxLocation:
        """A location where `at` says, or at the start when it says nowhere.
        At several places, it gets a region of its own entered from each."""
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

    def pool(self) -> Counter:
        """The items these options deal, as the app's deal counts them."""
        return self.dealt

    def create_items(self) -> None:
        for item, count in self.pool().items():
            self.multiworld.itempool += [self.create_item(item) for _ in range(count)]

    def get_filler_item_name(self) -> str:
        # One our deal has too. An item link's group world has no options
        # set: every row.
        counts = traded(POOL) if not hasattr(self, "dealt") else self.pool()
        filler = Counter({item: n for item, n in counts.items() if CLASSES[item] == "filler"})
        return filler.most_common(1)[0][0]

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
            # Every slot by number, so the mod can name whoever sends Link
            # an item.
            "players": {player: names[player] for player in self.multiworld.player_ids},
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
