//! The slot picker's rows: each slot holding a save, newest first, with a
//! short summary read from it.

use std::path::Path;

use anyhow::Result;
use ounce_formats::{Save, murmur3};
use serde::Serialize;

use super::stamp;
use super::status::{MAX_LIFE, RUPEES};
use crate::inventory::{Cell, Glyph, Inventory, Section, in_save};
use crate::known::{Name, actor, flag, item};
use crate::savefile;

/// One slot holding a save.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SlotRow {
    pub slot: u8,
    /// When the save was last written, in milliseconds.
    pub stamp: u64,
    /// The slot Continue loads: the one written last.
    pub continues: bool,
    pub child: Option<bool>,
    /// The three stones and six medallions, each held or not.
    pub stones: Vec<Cell>,
    pub medallions: Vec<Cell>,
    /// Hearts, songs, Gold Skulltulas and rupees, each the save holds.
    pub stats: Vec<Stat>,
    /// Why the save couldn't be read, leaving the summary out.
    pub error: Option<String>,
}

/// A number a row shows beside its icon.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Stat {
    pub label: String,
    pub icon: String,
    /// What it draws with game art off.
    pub glyph: Glyph,
    /// In `per`ths of what the row shows, as a status field counts.
    pub value: i32,
    pub per: i32,
}

/// Each stat's label, icon, glyph and `per`, in the order a row shows
/// them. Max life counts quarters of a heart.
pub(crate) const STATS: [(&str, &Name, Glyph, i32); 4] = [
    ("Hearts", actor::OBJ_HEART_CONTAINER, Glyph::Heart, 4),
    ("Songs", item::OCARINA_TIME, Glyph::Song, 1),
    ("Gold Skulltulas", item::TOKEN_GOLD, Glyph::Skulltula, 1),
    ("Rupees", actor::OBJ_RUPEE_GREEN, Glyph::Rupee, 1),
];

/// Every slot under `dir`, newest first, summarised with the grid `inv`.
pub fn slots(inv: &Inventory, dir: &Path) -> Result<Vec<SlotRow>> {
    let mut rows: Vec<SlotRow> = (savefile::slots(dir)?.into_iter())
        .map(|slot| row(inv, dir, slot))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.stamp));
    if let Some(first) = rows.first_mut() {
        first.continues = true;
    }
    Ok(rows)
}

fn row(inv: &Inventory, dir: &Path, slot: u8) -> SlotRow {
    let path = savefile::slot_file(dir, slot);
    let mut row = SlotRow {
        slot,
        stamp: stamp(&path).unwrap_or(0),
        continues: false,
        child: None,
        stones: Vec::new(),
        medallions: Vec::new(),
        stats: Vec::new(),
        error: None,
    };
    match savefile::load(&path) {
        Ok(save) => summarise(&mut row, inv, &save),
        Err(e) => row.error = Some(crate::plain::shown(&e)),
    }
    row
}

fn summarise(row: &mut SlotRow, inv: &Inventory, save: &Save) {
    let scalar = |h| save.get(h).and_then(|e| save.scalar(e));
    row.child = scalar(murmur3(flag::SEQUENCE_IS_CHILDHOOD.as_bytes())).map(|v| v != 0);
    let mut inv = inv.clone();
    inv.load(in_save(save));
    let cells = inv.cells();
    let of = |section| {
        cells
            .iter()
            .filter(|c| c.section == section)
            .cloned()
            .collect()
    };
    row.stones = of(Section::Stones);
    row.medallions = of(Section::Medallions);
    let songs = (cells.iter())
        .filter(|c| c.section == Section::Songs && c.tier > 0)
        .count();
    let tokens = (cells.iter())
        .find(|c| c.icon == item::TOKEN_GOLD.get())
        .and_then(|c| c.count);
    // In STATS' order.
    let values = [
        scalar(MAX_LIFE).map(|v| v as i32),
        Some(songs as i32),
        tokens,
        scalar(RUPEES).map(|v| v as i32),
    ];
    row.stats = (STATS.iter().zip(values))
        .filter_map(|(&(label, icon, glyph, per), value)| {
            Some(Stat {
                label: label.into(),
                icon: icon.get().into(),
                glyph,
                value: value?,
                per,
            })
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_put_the_newest_first_and_mark_it_as_continue() {
        let dir = std::env::temp_dir().join(format!("zootr-slot-rows-{}", std::process::id()));
        // Each slot written a minute after the last, so the order doesn't
        // hang on the filesystem's clock.
        let start = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        let saves = [
            (0u8, Save::fixture(&[(0, 2), (MAX_LIFE, 12), (RUPEES, 99)])),
            (2, Save::fixture(&[(0, 2), (MAX_LIFE, 28), (RUPEES, 99)])),
            (5, b"nope".to_vec()),
        ];
        for (minute, (slot, bytes)) in saves.into_iter().enumerate() {
            let path = savefile::slot_file(&dir, slot);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            let at = start + std::time::Duration::from_secs(60 * minute as u64);
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(at).unwrap();
        }

        let rows = slots(&Inventory::default(), &dir).unwrap();
        let order: Vec<u8> = rows.iter().map(|r| r.slot).collect();
        assert_eq!(order, [5, 2, 0]);
        assert!(rows[0].continues && !rows[1].continues);
        assert!(rows[0].error.is_some());
        let stat = |label| {
            rows[1]
                .stats
                .iter()
                .find(|s| s.label == label)
                .map(|s| s.value)
        };
        assert_eq!(stat("Hearts"), Some(28));
        assert_eq!(stat("Rupees"), Some(99));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
