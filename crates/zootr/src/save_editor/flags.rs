//! The Flags tab: every value in a save, grouped by its name's first part,
//! searched and filtered. Only the groups that are open carry rows, a page
//! at a time, so the app never holds the whole table.

use std::collections::{BTreeMap, HashSet};

use ounce_formats::{Save, SaveEntry, save_width};
use serde::{Deserialize, Serialize};

use crate::gamedata::GameData;
use crate::savefile;
use crate::value::{self, Input};

/// Where values the game doesn't name are grouped.
pub const UNNAMED: &str = "Unnamed";
/// Rows an open group shows before Show more.
pub const PAGE: u32 = 200;

/// What the tab asks for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FlagQuery {
    /// Matched against the name, the hash in hex and any label.
    pub search: String,
    /// Only values of these types, every type when empty.
    pub kinds: Vec<String>,
    /// Only values that differ from what a new game starts with.
    pub changed: bool,
    /// Only values the pending edits change.
    pub staged: bool,
    /// Each open group, to how many rows it shows. A search opens the
    /// groups it hits, at a page each.
    pub open: BTreeMap<String, u32>,
}

/// One value.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FlagRow {
    pub hash: u32,
    /// The name, else `#hash`.
    pub name: String,
    pub label: Option<String>,
    pub kind: String,
    /// The field each element is edited in.
    pub input: Input,
    pub choices: Vec<String>,
    /// One per element: one for a scalar. Empty when the type can't be
    /// read, with `size` saying how big it is.
    pub values: Vec<String>,
    pub size: u32,
    pub default: Option<String>,
    pub changed: bool,
    pub staged: bool,
}

/// One group's header, with its rows when it's open.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FlagGroup {
    pub name: String,
    /// Values that pass the filters.
    pub count: u32,
    /// Of those, how many differ from their default.
    pub changed: u32,
    pub open: bool,
    pub rows: Vec<FlagRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FlagsView {
    pub groups: Vec<FlagGroup>,
    /// Every type the save holds, for the type filter.
    pub kinds: Vec<String>,
}

/// The group a name sits in: its first part, split on `_` or `.`.
fn group_of(name: &str) -> &str {
    match name.starts_with('#') {
        true => UNNAMED,
        false => name.split(['_', '.']).next().unwrap_or(name),
    }
}

/// The tab for `save`, with `staged` the values pending edits change.
pub fn flags(gd: &GameData, save: &Save, staged: &HashSet<u32>, q: &FlagQuery) -> FlagsView {
    let search = q.search.trim().to_lowercase();
    let hits = |e: &SaveEntry, name: &str, label: Option<&str>| {
        search.is_empty()
            || name.to_lowercase().contains(&search)
            || format!("{:08x}", e.hash).contains(search.trim_start_matches(['#', '0', 'x']))
            || label.is_some_and(|l| l.to_lowercase().contains(&search))
    };
    let mut kinds: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, (FlagGroup, Vec<(String, SaveEntry)>)> = BTreeMap::new();
    for &e in save.entries() {
        if e.hash == 0 {
            continue;
        }
        if !kinds.iter().any(|k| k == e.kind) {
            kinds.push(e.kind.into());
        }
        let name = gd.bare(e.hash);
        let label = gd.label(e.hash);
        let changed = savefile::changed(gd, save, e);
        let wanted = (q.kinds.is_empty() || q.kinds.iter().any(|k| k == e.kind))
            && (!q.changed || changed)
            && (!q.staged || staged.contains(&e.hash))
            && hits(&e, &name, label.as_deref());
        if !wanted {
            continue;
        }
        // The game spells some parts two ways (`NPC`, `Npc`): one group,
        // named by the spelling that sorts first.
        let group = group_of(&name);
        let (header, members) = groups.entry(group.to_lowercase()).or_insert_with(|| {
            let header = FlagGroup {
                open: false,
                name: group.to_owned(),
                count: 0,
                changed: 0,
                rows: Vec::new(),
            };
            (header, Vec::new())
        });
        if group < header.name.as_str() {
            header.name = group.to_owned();
        }
        header.count += 1;
        header.changed += u32::from(changed);
        members.push((name, e));
    }
    kinds.sort();
    let mut out: Vec<FlagGroup> = groups
        .into_values()
        .map(|(mut header, mut members)| {
            header.open = q.open.contains_key(&header.name) || !search.is_empty();
            if !header.open {
                return header;
            }
            members.sort_by(|a, b| a.0.cmp(&b.0));
            let limit = q.open.get(&header.name).copied().unwrap_or(PAGE);
            header.rows = (members.into_iter().take(limit as usize))
                .map(|(name, e)| row(gd, save, staged, name, e))
                .collect();
            header
        })
        .collect();
    // The unnamed sit last.
    out.sort_by_key(|g| g.name == UNNAMED);
    FlagsView { groups: out, kinds }
}

fn row(gd: &GameData, save: &Save, staged: &HashSet<u32>, name: String, e: SaveEntry) -> FlagRow {
    let info = gd.info(&name).ok();
    let input = match save_width(e.kind) {
        Some(_) => value::input(value::element(e.kind)),
        None => Input::Read,
    };
    FlagRow {
        hash: e.hash,
        label: gd.label(e.hash),
        kind: e.kind.into(),
        input,
        choices: info.as_ref().map(|i| i.choices.clone()).unwrap_or_default(),
        values: savefile::values(gd, save, e).unwrap_or_default(),
        size: save.data(e).len() as u32,
        default: info.and_then(|i| i.default),
        changed: savefile::changed(gd, save, e),
        staged: staged.contains(&e.hash),
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bools 11 (on) and 12, Ints 21 (5) and 22.
    fn save() -> Save {
        Save::parse(Save::fixture(&[
            (0, 0),
            (11, 1),
            (12, 0),
            (0, 2),
            (21, 5),
            (22, 0),
        ]))
        .unwrap()
    }

    fn query() -> FlagQuery {
        FlagQuery::default()
    }

    #[test]
    fn unnamed_values_group_together_folded() {
        let gd = GameData::default();
        let view = flags(&gd, &save(), &HashSet::new(), &query());
        assert_eq!(view.kinds, ["Bool", "Int"]);
        let [g] = &view.groups[..] else {
            panic!("{view:?}")
        };
        assert_eq!(
            (g.name.as_str(), g.count, g.open, g.rows.len()),
            (UNNAMED, 4, false, 0)
        );
    }

    #[test]
    fn filters_and_search_narrow_the_rows() {
        let gd = GameData::default();
        let staged = HashSet::from([21]);
        let ask = |q: FlagQuery| {
            let view = flags(&gd, &save(), &staged, &q);
            let rows = view.groups.into_iter().flat_map(|g| g.rows);
            rows.map(|r| r.hash).collect::<Vec<_>>()
        };
        let open = BTreeMap::from([(UNNAMED.to_owned(), PAGE)]);
        let all = FlagQuery {
            open: open.clone(),
            ..query()
        };
        assert_eq!(ask(all.clone()), [11, 12, 21, 22]);
        assert_eq!(
            ask(FlagQuery {
                kinds: vec!["Int".into()],
                ..all.clone()
            }),
            [21, 22]
        );
        assert_eq!(
            ask(FlagQuery {
                kinds: vec!["Bool".into(), "Int".into()],
                ..all.clone()
            }),
            [11, 12, 21, 22]
        );
        assert_eq!(
            ask(FlagQuery {
                staged: true,
                ..all.clone()
            }),
            [21]
        );
        assert_eq!(
            ask(FlagQuery {
                search: "#00000016".into(),
                ..query()
            }),
            [22]
        );
        let paged = FlagQuery {
            open: BTreeMap::from([(UNNAMED.to_owned(), 3)]),
            ..query()
        };
        assert_eq!(ask(paged), [11, 12, 21]);
    }

    #[test]
    fn a_row_carries_its_values_and_field() {
        let gd = GameData::default();
        let q = FlagQuery {
            search: "15".into(),
            ..query()
        };
        let view = flags(&gd, &save(), &HashSet::new(), &q);
        let r = &view.groups[0].rows[0];
        assert_eq!(
            (r.hash, r.kind.as_str(), r.input, r.values.clone()),
            (21, "Int", Input::Number, vec!["5".into()])
        );
    }

    #[test]
    fn names_group_by_their_first_part() {
        assert_eq!(group_of("FakeHill_Child_Door"), "FakeHill");
        assert_eq!(group_of("PlayerStatus.MaxLife"), "PlayerStatus");
        assert_eq!(group_of("#0000000b"), UNNAMED);
    }
}
