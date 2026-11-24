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
