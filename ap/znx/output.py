"""The `.apznx` the app turns into a mod."""

import json
import zipfile

from BaseClasses import Item, ItemClassification
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
