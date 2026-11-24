"""One option class per exposed setting, built from options.json."""

import types
from dataclasses import make_dataclass

from Options import (
    Choice,
    DeathLink,
    DefaultOnToggle,
    OptionSet,
    PerGameCommonOptions,
    StartInventoryPool,
    Toggle,
)

from .exported import OPTIONS


def attr(setting: str) -> str:
    return setting.replace("-", "_")


def option_class(entry: dict) -> type:
    body = {"__doc__": entry["description"], "display_name": entry["label"], "__module__": __name__}
    if entry["kind"] == "toggle":
        base = DefaultOnToggle if entry["default"] else Toggle
    elif entry["kind"] == "set":
        base = OptionSet
        labels = {option["id"]: option["label"] for option in entry["options"]}
        body["valid_keys"] = frozenset(labels.values())
        body["default"] = frozenset(labels[id_] for id_ in entry["default"])
    else:
        base = Choice
        ids = [option["id"] for option in entry["options"]]
        body.update({f"option_{attr(id_)}": n for n, id_ in enumerate(ids)})
        body["default"] = ids.index(entry["default"])
    name = "".join(part.title() for part in entry["id"].split("-"))
    cls = types.new_class(name, (base,), exec_body=lambda ns: ns.update(body))
    globals()[name] = cls
    return cls


def setting_values(entry: dict, option) -> dict[str, bool | str]:
    """The option as our settings JSON holds it: a set as one toggle for
    each setting it lists."""
    if entry["kind"] == "toggle":
        return {entry["id"]: bool(option.value)}
    if entry["kind"] == "set":
        return {o["id"]: o["label"] in option.value for o in entry["options"]}
    return {entry["id"]: entry["options"][option.value]["id"]}


ZnxOptions = make_dataclass(
    "ZnxOptions",
    [(attr(entry["id"]), option_class(entry)) for entry in OPTIONS]
    + [("start_inventory_from_pool", StartInventoryPool), ("death_link", DeathLink)],
    bases=(PerGameCommonOptions,),
)
ZnxOptions.__module__ = __name__
