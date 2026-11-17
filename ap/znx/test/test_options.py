"""AP's default tests (every location reachable with everything, the
goal set, a fill that completes) for each option's other values."""

from ..exported import OPTIONS
from ..options import attr
from . import ZnxTestBase


def other_values(entry: dict) -> list:
    if entry["kind"] == "toggle":
        return [not entry["default"]]
    return [option["id"].replace("-", "_") for option in entry["options"] if option["id"] != entry["default"]]


for entry in OPTIONS:
    for value in other_values(entry):
        name = f"Test_{attr(entry['id'])}_{value}"
        globals()[name] = type(name, (ZnxTestBase,), {"options": {attr(entry["id"]): value}})
