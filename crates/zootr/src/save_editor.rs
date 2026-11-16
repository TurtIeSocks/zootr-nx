//! The save editor: a slot's save as the pending edits would leave it, the
//! views the app shows of it, and writing it. Nothing is kept between
//! calls. Each one reads the slot again and applies the edits it's given,
//! so the views always show the file as it is now.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, ensure};
use ounce_formats::Save;
use serde::{Deserialize, Serialize};

use crate::gamedata::GameData;
use crate::inventory::{Cell, Intent, Inventory, Pouch, in_save};
use crate::presets;
use crate::savefile::{self, Change, Edit};

mod flags;
mod slots;
mod status;

pub use flags::{FlagGroup, FlagQuery, FlagRow, FlagsView, flags};
pub use slots::{SlotRow, slots};
pub use status::{Choice, StatusField, StatusGroup, status};

/// What's waiting to be saved: the presets turned on, then single edits
/// on top of them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Pending {
    pub presets: Vec<String>,
    pub edits: Vec<Edit>,
}

/// A slot's save as read, and as the pending edits leave it.
pub struct Opened {
    pub path: PathBuf,
    /// When the file was last written. A save written since a view was
    /// read is stale.
    pub stamp: u64,
    pub original: Save,
    pub edited: Save,
}

impl Opened {
    /// Every element the pending edits change.
    pub fn changes(&self, gd: &GameData) -> Vec<Change> {
        savefile::diff(gd, &self.original, &self.edited, |_| true)
    }

    /// The hash of every value the pending edits change.
    pub fn staged(&self, gd: &GameData) -> HashSet<u32> {
        self.changes(gd).into_iter().map(|c| c.hash).collect()
    }
}

/// The Items tab: the grid and the Status panel, as the pending edits
/// leave the save.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ItemsView {
    pub cells: Vec<Cell>,
    /// How many each counted cell can hold, in `cells`' order.
    pub caps: Vec<Option<i32>>,
    /// Whether a pending edit changes each cell.
    pub staged: Vec<bool>,
    pub status: Vec<StatusField>,
    /// The file's stamp, which Save sends back.
    pub stamp: u64,
}

/// The Items tab for `opened`, read with the grid `inv`.
pub fn items(gd: &GameData, inv: &Inventory, pouch: &Pouch, opened: &Opened) -> ItemsView {
    let mut inv = inv.clone();
    inv.load(in_save(&opened.edited));
    let staged = opened.staged(gd);
    ItemsView {
        cells: inv.cells(),
        caps: inv.caps(pouch, &opened.edited),
        staged: (inv.cell_hashes().iter())
            .map(|hs| hs.iter().any(|h| staged.contains(h)))
            .collect(),
        status: status(gd, pouch, &opened.edited),
        stamp: opened.stamp,
    }
}

/// The edits a click on cell `at` makes, on top of what's pending.
pub fn click(
    inv: &Inventory,
    pouch: &Pouch,
    opened: &Opened,
    at: usize,
    intent: Intent,
) -> Result<Vec<Edit>> {
    let mut inv = inv.clone();
    inv.load(in_save(&opened.edited));
    inv.edits(pouch, &opened.edited, at, intent)
}

/// Slot `slot` under `dir`, with `pending` applied in memory.
pub fn open(gd: &GameData, dir: &Path, slot: u8, pending: &Pending) -> Result<Opened> {
    let path = savefile::slot_file(dir, slot);
    let stamp = stamp(&path)?;
    let original = savefile::load(&path)?;
    let mut edited = Save::parse(original.bytes().to_vec())?;
    let from_presets: Vec<Edit> = presets::for_save(&pending.presets, &[])?
        .iter()
        .map(|e| Edit::parse(e))
        .collect::<Result<_>>()?;
    savefile::apply(gd, &mut edited, &from_presets)?;
    savefile::apply(gd, &mut edited, &pending.edits)?;
    Ok(Opened {
        path,
        stamp,
        original,
        edited,
    })
}

/// When the file at `path` was last written, in milliseconds.
pub fn stamp(path: &Path) -> Result<u64> {
    let time = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(time.duration_since(UNIX_EPOCH)?.as_millis() as u64)
}

/// Write `pending` to slot `slot`, keeping a backup of the old file.
/// Refuses when the file changed since `stamp`, the stamp of the view the
/// edits were made on. Returns each change as `name: old -> new` and the
/// backup.
pub fn write(
    gd: &GameData,
    dir: &Path,
    slot: u8,
    pending: &Pending,
    stamp: u64,
) -> Result<(Vec<String>, PathBuf)> {
    let opened = open(gd, dir, slot, pending)?;
    ensure!(
        opened.stamp == stamp,
        "The game wrote this slot since it was read. Reload it to see what changed"
    );
    let log = (opened.changes(gd).iter())
        .map(|c| format!("{}: {}", c.name, c.change))
        .collect();
    let backup = savefile::write(&opened.path, &opened.edited)?;
    Ok((log, backup))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ounce_formats::murmur3;

    /// A save folder holding slot 0 with Bool 11 false and Int 22 at 5.
    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("zootr-save-editor-{name}-{}", std::process::id()));
        let path = savefile::slot_file(&dir, 0);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, Save::fixture(&[(0, 0), (11, 0), (0, 2), (22, 5)])).unwrap();
        dir
    }

    fn pending(edits: &[(u32, &str)]) -> Pending {
        Pending {
            presets: vec![],
            edits: edits.iter().map(|&(h, v)| Edit::new(h, v)).collect(),
        }
    }

    #[test]
    fn open_applies_the_pending_edits_and_says_what_they_change() {
        let dir = folder("open");
        let gd = GameData::default();
        let opened = open(&gd, &dir, 0, &pending(&[(11, "true"), (22, "5")])).unwrap();
        let changed: Vec<u32> = opened.changes(&gd).iter().map(|c| c.hash).collect();
        assert_eq!(changed, [11]);
        assert_eq!(
            opened.original.scalar(opened.original.get(11).unwrap()),
            Some(0)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_keeps_a_backup_and_refuses_a_stale_view() {
        let dir = folder("write");
        let gd = GameData::default();
        let path = savefile::slot_file(&dir, 0);
        let edits = pending(&[(22, "9")]);
        assert!(write(&gd, &dir, 0, &edits, stamp(&path).unwrap() + 1).is_err());
        let (log, backup) = write(&gd, &dir, 0, &edits, stamp(&path).unwrap()).unwrap();
        assert_eq!(log, ["#00000016: 5 -> 9"]);
        assert!(backup.exists());
        let saved = savefile::load(&path).unwrap();
        assert_eq!(saved.scalar(saved.get(22).unwrap()), Some(9));
        assert!(open(&gd, &dir, 0, &pending(&[(murmur3(b"Nope"), "1")])).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
