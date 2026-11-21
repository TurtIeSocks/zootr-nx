//! Magic apart from the items that spend it. In the game every magic item
//! (Din's Fire, Farore's Wind, Nayru's Love, the Lens of Truth, the magic
//! arrows, even the plain bow's pouch row) turns the magic meter on as it's
//! got, through its pouch row. With magic apart only the meter's own rows
//! raise it. Research: `research/magic-meter.md`.

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::BymlMap;
use ounce_formats::EventKind;
use ounce_formats::Romfs;

use crate::flows::{self, GAME_DATA, Rewrites, param};
use crate::known::{field, flag, flow};
use crate::standins::Tables;

/// A pouch row's ability, and the value that raises the meter.
const ABILITY: &str = "ExpansionAvility";
const RAISES_METER: &str = "MPMaxUP";
/// The meter's size a row sets.
const SIZE: &str = "MPMax";

/// A pouch row's special deal, which names the meter's own rows (the
/// gifts and their pouch twins) whatever they're called.
const DEAL: &str = "PouchSpecialDeal";
const METER_DEALS: [&str; 2] = ["FairyBlessMP", "FairyBlessMPDouble"];

fn text<'a>(row: &'a BymlMap, key: &str) -> Option<&'a str> {
    row.get(key)?.as_string().ok().map(|s| s.as_str())
}

fn is_meter(row: &BymlMap) -> bool {
    text(row, DEAL).is_some_and(|d| METER_DEALS.contains(&d))
}

fn raises_meter(row: &BymlMap) -> bool {
    text(row, ABILITY) == Some(RAISES_METER)
}

/// Take the meter off every pouch row but the meter's own. Run before the
/// stand-ins, so a copy of a magic item copies it apart. Returns the rows
/// changed.
pub fn write(tables: &mut Tables) -> Result<Vec<String>> {
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

/// The spells' get window ends on a page saying magic was unleashed while
/// Link has no meter, since in the game the spell is about to give him one.
/// With magic apart it never does, so the page goes: the switch asking
/// whether the meter is empty loses its "less" and "equal" cases and ends
/// as it does on "greater".
pub fn quiet(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, flow::GET_POUCH_CONTENT, |f| {
        let window = flows::reachable(f, flow::OPEN_GET_WINDOW_RELEASE_MAGIC)?;
        let asks = flows::events(
            f,
            (field::GAME_DATA_ACTOR, flow::EVENT_QUERY_IS_GAME_DATA_INT),
        );
        let empty: Vec<u16> = (asks.into_iter())
            .filter(|(i, p)| {
                window.contains(i) && param(p, GAME_DATA) == Some(flag::PLAYER_STATUS_MP_MAX)
            })
            .map(|(i, _)| i)
            .collect();
        let [at] = empty[..] else {
            bail!(
                "the window asks whether the meter is empty {} times",
                empty.len()
            );
        };
        let event = f.events.get_mut(at as usize).context("no such event")?;
        let EventKind::Switch { cases, .. } = &mut event.kind else {
            bail!("event {at} isn't a switch");
        };
        cases.clear();
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::item;

    /// The items that are the meter: single, then double.
    const METER: [&crate::known::Name; 2] = [item::FAIRY_BLESS_MP, item::FAIRY_BLESS_MP_DOUBLE];

    #[test]
    fn only_the_meter_raises_the_meter() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut tables = Tables::load(&romfs).unwrap();
        let stripped = write(&mut tables).unwrap();
        for magic in [
            item::DIN_FIRE,
            item::FARORE_WIND,
            item::NAYRU_LOVE,
            item::GLASS_TRUTH,
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
                    raising.push((name, is_meter(row)));
                }
                false
            })
            .unwrap();
        assert!(raising.iter().all(|(_, meter)| *meter), "{raising:?}");
        for m in METER {
            assert!(
                raising.iter().any(|(r, _)| r == m.get()),
                "{} lost the meter",
                m.get()
            );
        }
        assert_eq!(
            raising.len(),
            4,
            "the meter's gifts and pouch twins: {raising:?}"
        );
    }

    #[test]
    fn the_spells_window_never_says_magic_was_unleashed() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let asks = |rewrites: &Rewrites| {
            let f = flows::current(&romfs, rewrites, flow::GET_POUCH_CONTENT).unwrap();
            let needle = format!("\"{}\"", flag::PLAYER_STATUS_MP_MAX.get());
            let window = flows::reachable(&f, flow::OPEN_GET_WINDOW_RELEASE_MAGIC).unwrap();
            (flows::render(&f).lines())
                .filter(|l| l.contains(&needle))
                .find(|l| window.iter().any(|i| l.starts_with(&format!("#{i} "))))
                .unwrap()
                .to_owned()
        };
        let mut rewrites = Rewrites::new();
        let was = asks(&rewrites);
        quiet(&romfs, &mut rewrites).unwrap();
        let now = asks(&rewrites);
        assert!(was.contains("? 0:#") && was.contains(" 1:#"), "{was}");
        assert!(!now.contains(":#"), "{now}");
    }
}
