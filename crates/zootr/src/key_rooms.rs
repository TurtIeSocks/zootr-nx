//! How many small keys a dungeon check needs, room by room (EIE row 2).
//! `logic/keys.toml` counts them for the worst order Link could spend
//! keys in. A dungeon it leaves out asks for all its keys at every check,
//! which keeps them out of their own dungeon: debt the ledger counts.

use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::checks::ItemCheck;
use crate::keys;
use crate::names::pretty;
use crate::sweep::spot::slug;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    dungeon: Vec<Dungeon>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dungeon {
    name: String,
    /// Every room it doesn't list.
    keys: u32,
    #[serde(default)]
    rooms: Vec<Room>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Room {
    room: u32,
    keys: u32,
}

/// The table's file in `logic/`, built in here rather than with the rules.
#[cfg(test)]
pub(crate) const FILE: &str = "keys.toml";

/// Each tabled dungeon, by its number in [`keys::DUNGEONS`].
static TABLED: LazyLock<Vec<(usize, Dungeon)>> =
    LazyLock::new(|| load(include_str!("../logic/keys.toml")).expect("logic/keys.toml"));

fn load(text: &str) -> Result<Vec<(usize, Dungeon)>> {
    let table: Table = toml::from_str(text)?;
    let mut out: Vec<(usize, Dungeon)> = Vec::new();
    for d in table.dungeon {
        let n = (keys::DUNGEONS.iter())
            .position(|(name, ..)| slug(&pretty(name.get())) == d.name)
            .with_context(|| format!("no dungeon with keys is called {}", d.name))?
            + 1;
        ensure!(out.iter().all(|(m, _)| *m != n), "{} twice", d.name);
        let all = keys::copies_of(n);
        for (i, r) in d.rooms.iter().enumerate() {
            ensure!(
                r.keys <= all,
                "{} room {}: {} of {all} keys",
                d.name,
                r.room,
                r.keys
            );
            ensure!(
                d.rooms[..i].iter().all(|s| s.room != r.room),
                "{} room {} twice",
                d.name,
                r.room
            );
        }
        ensure!(d.keys <= all, "{}: {} of {all} keys", d.name, d.keys);
        out.push((n, d));
    }
    Ok(out)
}

/// `check`'s dungeon, by its number in [`keys::DUNGEONS`], and how many of
/// its small keys the check needs. `None` outside a dungeon with keys.
pub fn count(check: &ItemCheck) -> Option<(usize, u32)> {
    let n = keys::dungeon_in(&check.area)?;
    let room = |d: &Dungeon| {
        let room = check.room?;
        let listed = d.rooms.iter().find(|r| r.room == room);
        Some(listed.map_or(d.keys, |r| r.keys))
    };
    let tabled = TABLED.iter().find(|(m, _)| *m == n);
    Some((
        n,
        tabled
            .and_then(|(_, d)| room(d))
            .unwrap_or(keys::copies_of(n)),
    ))
}

/// Whether logic counts `check`'s keys by its room, rather than asking for
/// all its dungeon's keys.
pub fn by_room(check: &ItemCheck) -> bool {
    check.room.is_some()
        && keys::dungeon_in(&check.area).is_some_and(|n| TABLED.iter().any(|(m, _)| *m == n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::flow;

    fn at(dungeon: &str, room: Option<u32>) -> ItemCheck {
        ItemCheck {
            file: format!("Banc/{dungeon}/{dungeon}_Dynamic.bcett.byml.zs"),
            area: dungeon.into(),
            room,
            ..crate::narrowing::tests::chest(1, "Fake_Item")
        }
    }

    #[test]
    fn the_table_names_dungeons_with_keys_and_counts_no_more_than_they_have() {
        needs_names!();
        assert!(!TABLED.is_empty());
        let bad = "[[dungeon]]\nname = \"bottom-of-the-well\"\nkeys = 3\n";
        assert!(load(bad).is_err());
        let unknown = "[[dungeon]]\nname = \"fake-temple\"\nkeys = 0\n";
        assert!(load(unknown).is_err());
    }

    #[test]
    fn the_well_s_inner_rooms_need_both_keys_and_the_rest_none() {
        needs_names!();
        let well = flow::BOTTOM_OF_THE_WELL.get();
        assert_eq!(count(&at(well, Some(4))), Some((1, 2)));
        assert_eq!(count(&at(well, Some(5))), Some((1, 2)));
        assert_eq!(count(&at(well, Some(1))), Some((1, 0)));
        // A placement naming no room asks for them all.
        assert_eq!(count(&at(well, None)), Some((1, 2)));
        assert!(by_room(&at(well, Some(1))) && !by_room(&at(well, None)));
        assert_eq!(count(&at("FakeField", Some(1))), None);
    }

    #[test]
    fn the_well_s_checks_name_their_rooms() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let settings =
            crate::settings::Settings::from([(crate::settings::POTS.into(), true.into())]);
        let new_game = crate::settings::NewGame::from_settings(&settings, &[]).unwrap();
        let checks = crate::checks::dealt(&romfs, &new_game).unwrap();
        let well = |c: &&ItemCheck| c.area == flow::BOTTOM_OF_THE_WELL.get();
        let by_hash = |hash| checks.iter().find(|c| c.hash == hash).unwrap();
        // Its key chest and a pot in the hall, and the purple rupee chest
        // past it: no key.
        for hash in [
            8397646530660039355,
            13474828754365101442,
            11718708984125260282,
        ] {
            assert_eq!(count(by_hash(hash)), Some((1, 0)), "{hash}");
        }
        // The west inner room's three pots, behind a lock.
        let inner: Vec<_> = (checks.iter().filter(well))
            .filter(|c| matches!(c.room, Some(4 | 5)))
            .collect();
        assert!(inner.len() >= 3, "{}", inner.len());
        assert!(inner.iter().all(|c| count(c) == Some((1, 2))));
        // Every check in the well names its room.
        let roomless: Vec<_> = (checks.iter().filter(well))
            .filter(|c| !by_room(c))
            .map(|c| (&c.holder, c.hash, &c.item))
            .collect();
        assert!(roomless.is_empty(), "{roomless:?}");
    }
}
