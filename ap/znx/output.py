"""The `.apznx` the app turns into a mod."""

import json
import zipfile

from BaseClasses import Item, ItemClassification, Location
from worlds.Files import APPlayerContainer

GAME = "Ocarina of Time (znx)"


class ZnxContainer(APPlayerContainer):
    game = GAME
    patch_file_ending = ".apznx"

    def __init__(self, seed: dict, placements: dict, world_version: str, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.seed = seed
        self.placements = placements
        self.world_version = world_version

    def get_manifest(self) -> dict:
        manifest = super().get_manifest()
        manifest["world_version"] = self.world_version
        return manifest

    def write_contents(self, opened_zipfile: zipfile.ZipFile) -> None:
        super().write_contents(opened_zipfile)
        opened_zipfile.writestr("seed.json", json.dumps(self.seed))
        opened_zipfile.writestr("placements.json", json.dumps(self.placements))


def item_class(item: Item) -> str:
    flags = item.classification
    if flags & ItemClassification.progression:
        return "progression"
    if flags & ItemClassification.trap:
        return "trap"
    if flags & ItemClassification.useful:
        return "useful"
    return "filler"


def placement(item: Item, player: int, player_names: dict) -> dict:
    """What a check holds: our own item by id, or another player's."""
    if item.player == player:
        return {"own": item.code}
    foreign = {
        "player": item.player,
        "player_name": player_names[item.player],
        "game": item.game,
        "item": item.name,
        "class": item_class(item),
    }
    if item.game == GAME:
        foreign["id"] = item.code
    return {"foreign": foreign}


# The game's gossip stones, one hint each.
STONES = 40


def hints(locations: list[Location], player: int, names: dict, hashes: dict, random) -> list[dict]:
    """Up to one hint a stone, in stone order: where this slot's progression
    items are, in any slot's world. The mod repeats them over the stones
    when there are fewer."""
    found = [
        location
        for location in locations
        if location.address is not None
        and location.item is not None
        and location.item.player == player
        and location.item.advancement
    ]
    found.sort(key=lambda location: (location.player, location.name))
    random.shuffle(found)
    return [hint(location, player, names, hashes) for location in found[:STONES]]


def hint(location: Location, player: int, names: dict, hashes: dict) -> dict:
    """One stone's hint: our own check by hash, another slot's by name."""
    if location.player == player:
        return {"item": location.item.code, "own": int(hashes[location.name])}
    return {"item": location.item.code, "player": names[location.player], "location": location.name}
