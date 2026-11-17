"""The data `zootr ap export` writes, loaded once."""

import json
import pkgutil


def load(name: str):
    return json.loads(pkgutil.get_data(__name__, f"data/{name}.json"))


WORLD = load("world")
ITEMS = load("items")
POOL = load("pool")
OPTIONS = load("options")

ITEM_NAME_TO_ID = {item["name"]: item["id"] for item in ITEMS}
LOCATION_NAME_TO_ID = {check["name"]: check["id"] for check in WORLD["checks"]}
CLASSES = {item["name"]: item["class"] for item in ITEMS}
HASHES = {check["name"]: check["hash"] for check in WORLD["checks"]}
