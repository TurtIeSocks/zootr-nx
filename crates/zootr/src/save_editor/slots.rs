//! The slot picker's rows: each slot holding a save, newest first, with a
//! short summary read from it.

use std::path::Path;

use anyhow::Result;
use ounce_formats::{Save, murmur3};
use serde::Serialize;

use super::stamp;
use super::status::MAX_LIFE;
use crate::inventory::{Cell, Inventory, Section, in_save};
use crate::known::flag;
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
    /// Heart containers, in quarters.
    pub max_life: Option<i32>,
    pub child: Option<bool>,
    /// The three stones and six medallions, each held or not.
    pub stones: Vec<Cell>,
    pub medallions: Vec<Cell>,
    /// Why the save couldn't be read, leaving the summary out.
    pub error: Option<String>,
}

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
        max_life: None,
        child: None,
        stones: Vec::new(),
        medallions: Vec::new(),
        error: None,
    };
    match savefile::load(&path) {
        Ok(save) => summarise(&mut row, inv, &save),
        Err(e) => row.error = Some(format!("{e:#}")),
    }
    row
}

fn summarise(row: &mut SlotRow, inv: &Inventory, save: &Save) {
    let scalar = |h| save.get(h).and_then(|e| save.scalar(e));
    row.max_life = scalar(MAX_LIFE).map(|v| v as i32);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_put_the_newest_first_and_mark_it_as_continue() {
        let dir = std::env::temp_dir().join(format!("zootr-slot-rows-{}", std::process::id()));
        for (slot, life) in [(0u8, 12u32), (2, 28)] {
            let path = savefile::slot_file(&dir, slot);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, Save::fixture(&[(0, 2), (MAX_LIFE, life)])).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let broken = savefile::slot_file(&dir, 5);
        std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
        std::fs::write(&broken, b"nope").unwrap();

        let rows = slots(&Inventory::default(), &dir).unwrap();
        let order: Vec<u8> = rows.iter().map(|r| r.slot).collect();
        assert_eq!(order, [5, 2, 0]);
        assert!(rows[0].continues && !rows[1].continues);
        assert!(rows[0].error.is_some());
        assert_eq!(rows[1].max_life, Some(28));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
