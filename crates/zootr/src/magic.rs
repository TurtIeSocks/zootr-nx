//! Magic apart from the items that spend it. In the game every magic item
//! (Din's Fire, Farore's Wind, Nayru's Love, the Lens of Truth, the magic
//! arrows, even the plain bow's pouch row) turns the magic meter on as it's
//! got, through its pouch row. With magic apart (the setting "magic comes
//! with the first magic item" off) only a Magic Meter ([`meter`]) or
//! Doubled Magic raises it. Improved Spin's wave never spends magic, so it
//! loses the meter too. Research: `research/magic-meter.md`.

mod look;
pub mod meter;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::BymlMap;
use ounce_formats::EventKind;
use ounce_formats::{ModBuilder, Romfs};

use crate::flows::{self, GAME_DATA, Rewrites, param};
use crate::known::{field, flag, flow, scene};
use crate::standins::Tables;

/// A pouch row's ability, and the value that raises the meter.
const ABILITY: &str = "ExpansionAvility";
const RAISES_METER: &str = "MPMaxUP";
/// The meter's size a row sets.
const SIZE: &str = "MPMax";

/// A pouch row's special deal, which names Doubled Magic's rows (the gift
/// and its pouch twin) whatever they're called. Improved Spin's
/// `FairyBlessMP` rows aren't the meter's.
const DEAL: &str = "PouchSpecialDeal";
const METER_DEAL: &str = "FairyBlessMPDouble";

fn text<'a>(row: &'a BymlMap, key: &str) -> Option<&'a str> {
    row.get(key)?.as_string().ok().map(|s| s.as_str())
}

fn is_meter(row: &BymlMap) -> bool {
    text(row, DEAL) == Some(METER_DEAL)
}

fn raises_meter(row: &BymlMap) -> bool {
    text(row, ABILITY) == Some(RAISES_METER)
}

/// With magic `apart`, take the meter off every pouch row but Doubled
/// Magic's. Then add the Magic Meter, which every seed has, so starting
/// with it works either way. Run before the stand-ins, so a copy of a
/// magic item copies it apart.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    apart: bool,
) -> Result<()> {
    if apart {
        strip(tables)?;
    }
    meter::write(romfs, builder, tables)
}

/// Take the meter off every pouch row but Doubled Magic's. Returns the
/// rows changed.
fn strip(tables: &mut Tables) -> Result<Vec<String>> {
    let mut stripped = Vec::new();
    tables.edit_pouch(|row| {
        if !raises_meter(row) || is_meter(row) {
            return false;
        }
        row.remove(ABILITY);
        row.remove(SIZE);
        stripped.push(
            text(row, crate::rsdb::ROW_ID)
                .unwrap_or_default()
                .to_owned(),
        );
        true
    })?;
    ensure!(!stripped.is_empty(), "no pouch row raises the magic meter");
    Ok(stripped)
}

/// The flows that ask whether the meter is empty, since in the game a
/// magic item is about to give Link one: the spells' get window ("Your
/// magic power has been unleashed!"), a chest's and a gift's meter-born
/// animation, and Din's fountain, which plays it around Din's Fire.
const ASK_EMPTY: [&crate::known::Name; 3] = [
    flow::GET_POUCH_CONTENT,
    flow::SY_GET,
    scene::FAIRY_FOUNTAIN_DIN,
];

/// With magic apart the item never gives a meter, so every "is the meter
/// empty?" switch goes where "greater" goes (an end where it has none).
pub fn quiet(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    for name in ASK_EMPTY {
        flows::edit(romfs, rewrites, name, |f| {
            let asks = (flows::events(
                f,
                (field::GAME_DATA_ACTOR, flow::EVENT_QUERY_IS_GAME_DATA_INT),
            ))
            .into_iter()
            .filter(|(_, p)| param(p, GAME_DATA) == Some(flag::PLAYER_STATUS_MP_MAX))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
            ensure!(!asks.is_empty(), "{} never asks about the meter", f.name);
            for at in asks {
                let event = f.events.get_mut(at as usize).context("no such event")?;
                let EventKind::Switch { cases, .. } = &mut event.kind else {
                    bail!("{} event {at} isn't a switch", f.name);
                };
                // An int switch with no case for its answer ends.
                let greater = cases.iter().find(|(k, _)| *k == GREATER).map(|c| c.1);
                *cases = greater.map_or_else(Vec::new, |to| vec![(0, to), (1, to), (GREATER, to)]);
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// An int query's answer when the value is above the one asked.
const GREATER: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::item;

    #[test]
    fn only_the_meter_items_raise_the_meter() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-magic-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let stripped = strip(&mut tables).unwrap();
        meter::write(&romfs, &mut builder, &mut tables).unwrap();
        for magic in [
            item::DIN_FIRE,
            item::FARORE_WIND,
            item::NAYRU_LOVE,
            item::GLASS_TRUTH,
            item::ARROW_FIRE,
            item::FAIRY_BLESS_MP,
        ] {
            assert!(
                stripped.iter().any(|s| s == magic.get()),
                "{} kept the meter",
                magic.get()
            );
        }
        let mut raising = Vec::new();
        tables
            .edit_pouch(|row| {
                if raises_meter(row) {
                    let name = text(row, crate::rsdb::ROW_ID).unwrap().to_owned();
                    let size = row[SIZE].as_i32().unwrap();
                    raising.push((name, text(row, DEAL).unwrap().to_owned(), size));
                }
                false
            })
            .unwrap();
        // Doubled Magic's gift and pouch twin, and the Magic Meter.
        assert_eq!(raising.len(), 3, "{raising:?}");
        let double = item::FAIRY_BLESS_MP_DOUBLE.get();
        assert!(
            raising
                .iter()
                .any(|(r, d, n)| r == double && d == METER_DEAL && *n == 48)
        );
        let single = (
            meter::NAME.to_owned(),
            crate::standins::PLAIN_DEAL.to_owned(),
            24,
        );
        assert!(raising.contains(&single), "{raising:?}");
        std::fs::remove_dir_all(dir).ok();
    }

    /// Each "is the meter empty?" switch, as rendered, in `name`.
    fn asks(romfs: &Romfs, rewrites: &Rewrites, name: &crate::known::Name) -> Vec<String> {
        let f = flows::current(romfs, rewrites, name).unwrap();
        let needle = format!("\"{}\"", flag::PLAYER_STATUS_MP_MAX.get());
        (flows::render(&f).lines())
            .filter(|l| l.contains(&needle))
            .map(str::to_owned)
            .collect()
    }

    /// The events a rendered switch goes to, by answer.
    fn ways(line: &str) -> Vec<&str> {
        line.split(' ')
            .filter_map(|w| w.split_once(":#"))
            .map(|(_, to)| to)
            .collect()
    }

    #[test]
    fn nothing_says_magic_was_unleashed() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut rewrites = Rewrites::new();
        let was: Vec<_> = ASK_EMPTY
            .iter()
            .map(|n| asks(&romfs, &rewrites, n))
            .collect();
        quiet(&romfs, &mut rewrites).unwrap();
        for (name, was) in ASK_EMPTY.iter().zip(was) {
            let now = asks(&romfs, &rewrites, name);
            assert_eq!(now.len(), was.len(), "{}", name.get());
            for (was, now) in was.iter().zip(&now) {
                // Empty once led somewhere else: the get window's page, the
                // meter-born animation, Din's no-meter path.
                assert!(was.contains(" 0:#"), "{was}");
                let ways = ways(now);
                assert!(ways.windows(2).all(|w| w[0] == w[1]), "{now}");
                assert!(ways.is_empty() || ways.len() == 3, "{now}");
            }
        }
        // Din's fountain still goes on to its gift.
        let din = asks(&romfs, &rewrites, scene::FAIRY_FOUNTAIN_DIN);
        assert!(din.iter().all(|l| ways(l).len() == 3), "{din:?}");
    }
}
