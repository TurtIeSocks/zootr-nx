"""The data `zootr ap export` writes, loaded once."""

import json
import pkgutil


def load(name: str):
    return json.loads(pkgutil.get_data(__name__, f"data/{name}.json"))


def shared(world: dict) -> dict:
    """`world` with each `{"ref": n}` as the rule it names: the export
    writes a rule many places hold once, in `rules`, each after the rules
    it holds."""
    rules = []

    def expand(tree):
        if isinstance(tree, dict):
            return rules[tree["ref"]] if "ref" in tree else {k: expand(v) for k, v in tree.items()}
        return [expand(v) for v in tree] if isinstance(tree, list) else tree

    for rule in world.pop("rules"):
        rules.append(expand(rule))
    return expand(world)


WORLD = shared(load("world"))
ITEMS = load("items")
_POOL = load("pool")
# Rows of the pool before the trade; an `added` row's item takes the place
# of the cheapest of `JUNK` left, as the app's `starter_bags::swap_in` does.
POOL = _POOL["rows"]
JUNK = _POOL["junk"]
# The trap the world swaps in for a share of its `trappable` items, by
# Ice Traps' value, as the app's `ice_traps::swap` does.
TRAP = _POOL["trap"]
SHARES = _POOL["shares"]
# Each upgrade line's levels by its name, lowest first: each one's names
# (its own, then the events that give it) and the `when` it holds under.
# With progressive items on, the world deals each level as its line and
# reads rules that way, over the levels its options keep (`kept`,
# `rules.lined`).
LEVELS = _POOL["levels"]
LINES = {level["names"][0]: line for line, levels in LEVELS.items() for level in levels}
OPTIONS = load("options")

ITEM_NAME_TO_ID = {item["name"]: item["id"] for item in ITEMS}
LOCATION_NAME_TO_ID = {check["name"]: check["id"] for check in WORLD["checks"]}
CLASSES = {item["name"]: item["class"] for item in ITEMS}
HASHES = {check["name"]: check["hash"] for check in WORLD["checks"]}


def golden() -> list:
    """golden.json's rows, each with its settings and items whole: the
    export writes one case a line, its settings as they differ from each
    option's default, and each row's items as ids, a repeated one as
    `[id, count]`."""
    defaults = {entry["id"]: entry["default"] for entry in OPTIONS}
    names = {item["id"]: item["name"] for item in ITEMS}
    rows = []
    for case in load("golden"):
        settings = {**defaults, **case["settings"]}
        for row in case["rows"]:
            counts = (entry if isinstance(entry, list) else [entry, 1] for entry in row["items"])
            items = [names[i] for i, n in counts for _ in range(n)]
            rows.append({**row, "settings": settings, "items": items})
    return rows
