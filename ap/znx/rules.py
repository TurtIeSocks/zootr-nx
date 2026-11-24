"""Rule trees from world.json, as AP access rules.

A tree is `true`, `false`, `{items, count}`, `{setting}`, `{all}` or
`{any}`. Settings fold to constants for the player first.
"""

from collections.abc import Callable

from BaseClasses import CollectionState

Rule = Callable[[CollectionState], bool]


def held(word: str, settings: dict) -> bool:
    """A setting word: a toggle's id, or `id=value` for a choice's option
    or a toggle's `on` or `off`."""
    setting, _, value = word.partition("=")
    current = settings[setting]
    if not value:
        return bool(current)
    if isinstance(current, bool):
        return current == (value == "on")
    return current == value


def fold(tree, settings: dict):
    """`tree` with its settings read from `settings`: `True`, `False`, or a
    smaller tree."""
    if isinstance(tree, bool):
        return tree
    if "setting" in tree:
        return held(tree["setting"], settings)
    for key, absorbs in (("all", False), ("any", True)):
        if key in tree:
            parts = [fold(part, settings) for part in tree[key]]
            if absorbs in parts:
                return absorbs
            identity = not absorbs
            rest = [part for part in parts if part is not identity]
            if not rest:
                return identity
            return rest[0] if len(rest) == 1 else {key: rest}
    return tree


def lined(tree, levels: dict):
    """`tree` read with progressive items on: one of some levels of a line,
    the lowest the nth, is n of the line or its levels' events, as the
    export checks (`ap::rule::lined`)."""
    if isinstance(tree, bool):
        return tree
    for key in ("all", "any"):
        if key in tree:
            return {key: [lined(part, levels) for part in tree[key]]}
    if tree["count"] == 1:
        for line, steps in levels.items():
            at = [next((i for i, s in enumerate(steps) if name in s), None) for name in tree["items"]]
            if None not in at:
                events = {e for step in steps for e in step[1:]}
                return {"items": sorted(events | {line}), "count": min(at) + 1}
    return tree


def closure(tree, player: int) -> Rule:
    if isinstance(tree, bool):
        return lambda state: tree
    if "all" in tree:
        parts = [closure(part, player) for part in tree["all"]]
        return lambda state: all(part(state) for part in parts)
    if "any" in tree:
        parts = [closure(part, player) for part in tree["any"]]
        return lambda state: any(part(state) for part in parts)
    names, count = tree["items"], tree["count"]
    if len(names) == 1:
        name = names[0]
        return lambda state: state.has(name, player, count)
    return lambda state: state.has_from_list(names, player, count)
