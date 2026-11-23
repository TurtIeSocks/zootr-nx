//! The Inventory panel's grid: the save editor's own, run on a fresh
//! save with the start granted onto it.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use anyhow::{Result, bail};
use ounce_formats::Save;

use super::{HEARTS, RUPEE_WORD, Start, WORDS};
use crate::gamedata::GameData;
use crate::inventory::{self, Intent, Inventory, Pouch};
use crate::known::{Name, field};
use crate::magic::meter;
use crate::save_editor::{self, ItemsView, LIFE, MAX_LIFE, Opened, RUPEES};
use crate::savefile::{self, Edit};
use crate::value;

/// A new game's save, as far as the grid and the kept Status fields go:
/// every value they read, at its GameData default. The Magic Meter's,
/// which only a seed's GameData has, at the default of what it copies.
fn fresh(gd: &GameData, inv: &Inventory, pouch: &Pouch) -> Result<Save> {
    let mut seen = HashSet::new();
    let hashes = (inv.cell_hashes(pouch).into_iter().flatten()).chain([MAX_LIFE, LIFE, RUPEES]);
    let mut entries = Vec::new();
    for hash in hashes.filter(|h| seen.insert(*h)) {
        let from = match gd.kind(hash) {
            Some(_) => hash,
            None => meter::copied_from(hash).unwrap_or(hash),
        };
        let (Some(kind), Some(entry)) = (gd.kind(from), gd.entry(from)) else {
            continue;
        };
        let Some(default) = entry.as_map()?.get(&**field::DEFAULT_VALUE) else {
            continue;
        };
        let Some(width) = ounce_formats::save_width(kind) else {
            continue;
        };
        let bytes = match (savefile::raw(default), default.as_string()) {
            (Some(raw), _) => raw.to_le_bytes().to_vec(),
            (None, Ok(text)) => value::encode(kind, text, width)?,
            _ => continue,
        };
        entries.push((hash, kind, bytes));
    }
    Save::build(&entries)
}

/// The editor's view of a new game holding `start`: a fresh save as
/// `original`, and it holding `start` as `edited`.
pub fn open(gd: &GameData, inv: &Inventory, pouch: &Pouch, start: &Start) -> Result<Opened> {
    let original = fresh(gd, inv, pouch)?;
    let mut edited = Save::parse(original.bytes().to_vec())?;
    let start = &start.clone().floored(pouch);
    let held: Vec<(&Name, i32)> = (start.held().into_iter())
        .map(|(h, n)| (h.actor, n))
        .collect();
    savefile::apply(gd, &mut edited, &inv.holding(pouch, &held)?)?;
    savefile::apply(gd, &mut edited, &start.status())?;
    Ok(Opened {
        path: PathBuf::new(),
        stamp: 0,
        original,
        edited,
    })
}

/// Make `save` hold `start` on top of what it has: its grid at `start`'s
/// counts, the ammo its bags mark got, then the heart containers with
/// health full, and the rupees. Each change as `name: old -> new`. An item
/// of our own (the Magic Meter) is in a save only once a seed has added it:
/// without it, it's named as skipped.
pub fn onto(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    save: &mut Save,
    start: &Start,
) -> Result<Vec<String>> {
    let start = start.clone().floored(pouch);
    let held: Vec<(&Name, i32)> = (start.held().into_iter())
        .map(|(h, n)| (h.actor, n))
        .collect();
    let mut edits = inv.holding(pouch, &held)?;
    for d in start.defaults() {
        edits.push(Edit::parse(&d)?);
    }
    edits.extend(start.status());
    let (kept, lacked): (Vec<Edit>, Vec<Edit>) =
        edits.into_iter().partition(|e| save.get(e.hash).is_some());
    let mut log = savefile::apply(gd, save, &kept)?;
    log.extend((lacked.iter()).map(|e| format!("{}: not in this save, skipped", gd.name(e.hash))));
    Ok(log)
}

/// The Status fields the panel keeps: the rest follow from the grid.
const STATUS: [u32; 2] = [MAX_LIFE, RUPEES];

/// The Inventory panel: the grid without its Dungeons, whose keys belong to
/// the Dungeon keys setting, and the kept Status fields. `given` holds the
/// words each setting gives outside `start`: they show held, each cell
/// locked to the first that gives it.
pub fn items(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    start: &Start,
    given: &[Vec<String>],
) -> Result<ItemsView> {
    let mut shown = start.clone();
    for word in given.iter().flatten() {
        if !shown.holds(word) {
            shown.add(word, 1)?;
        }
    }
    let grid = |start: &Start| -> Result<Vec<u8>> {
        let view = save_editor::items(gd, inv, pouch, &open(gd, inv, pouch, start)?);
        Ok(view.cells.iter().map(|c| c.tier).collect())
    };
    let bare = grid(&Start::default())?;
    let mut giving = Vec::new();
    for words in given {
        let mut alone = Start::default();
        words.iter().try_for_each(|w| alone.add(w, 1))?;
        giving.push(grid(&alone)?);
    }
    let mut view = save_editor::items(gd, inv, pouch, &open(gd, inv, pouch, &shown)?);
    view.locked = (0..view.cells.len())
        .map(|at| {
            let gives = |tiers: &Vec<u8>| tiers[at] > bare[at];
            giving.iter().position(gives).map(|i| i as u32)
        })
        .collect();
    (view.sections.iter_mut())
        .filter(|p| p.section == inventory::Section::Dungeons)
        .for_each(|p| p.shown = false);
    view.status.retain(|f| STATUS.contains(&f.hash));
    Ok(view)
}

/// `start` after a click on cell `at`.
pub fn click(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    start: &Start,
    at: usize,
    intent: Intent,
) -> Result<Start> {
    let opened = open(gd, inv, pouch, start)?;
    let edits = save_editor::click(inv, pouch, &opened, at, intent)?;
    reread(gd, inv, pouch, opened, &edits)
}

/// `start` after a Status field takes `edit`. Health follows the heart
/// containers.
pub fn set(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    start: &Start,
    edit: Edit,
) -> Result<Start> {
    if !STATUS.contains(&edit.hash) {
        bail!("a new game only starts with heart containers and rupees set");
    }
    let opened = open(gd, inv, pouch, start)?;
    reread(gd, inv, pouch, opened, &[edit])
}

/// What `opened` holds after `edits`, as words: only what differs from a
/// fresh save.
fn reread(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    mut opened: Opened,
    edits: &[Edit],
) -> Result<Start> {
    savefile::apply(gd, &mut opened.edited, edits)?;
    let read = |save: &Save| {
        let mut inv = inv.clone();
        inv.load(inventory::in_save(save));
        inv.values()
    };
    let (was, now) = (read(&opened.original), read(&opened.edited));
    let mut out = BTreeMap::new();
    for (w, a) in WORDS {
        let n = now.get(a).copied().unwrap_or(0);
        if n > was.get(a).copied().unwrap_or(0) {
            out.insert((*w).to_owned(), n);
        }
    }
    let int = |save: &Save, hash| {
        save.get(hash)
            .and_then(|e| save.scalar(e))
            .map(|r| r as i32)
    };
    for (word, hash, per) in [(HEARTS, MAX_LIFE, 4), (RUPEE_WORD, RUPEES, 1)] {
        let n = int(&opened.edited, hash);
        if n.is_some() && n != int(&opened.original, hash) {
            out.insert(word.into(), n.unwrap_or(0) / per);
        }
    }
    Ok(Start(out).floored(pouch))
}

#[cfg(test)]
mod tests {
    use super::super::tests::start;
    use super::*;
    use crate::starting::ARSENAL;

    /// The grid and pouch table from the dump, with GameData.
    fn layout() -> Option<(GameData, Inventory, Pouch)> {
        let romfs = crate::game::dev_dump()?;
        let gd = GameData::load(&romfs).unwrap();
        let inv = Inventory::new(&gd, &romfs).unwrap();
        Some((gd, inv, Pouch::load(&romfs).unwrap()))
    }

    #[test]
    fn the_arsenal_onto_a_save_reads_back_as_the_arsenal() {
        needs_names!();
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let mut kit = Start::default();
        kit.add(ARSENAL, 1).unwrap();
        let mut save = fresh(&gd, &inv, &pouch).unwrap();
        let original = Save::parse(save.bytes().to_vec()).unwrap();
        onto(&gd, &inv, &pouch, &mut save, &kit).unwrap();
        let opened = Opened {
            path: PathBuf::new(),
            stamp: 0,
            original,
            edited: save,
        };
        let read = reread(&gd, &inv, &pouch, opened, &[]).unwrap();
        assert_eq!(read, kit.floored(&pouch));
    }

    #[test]
    fn the_panel_holds_the_arsenal_and_its_magic_meter() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let mut kit = Start::default();
        kit.add(ARSENAL, 1).unwrap();
        let view = items(&gd, &inv, &pouch, &kit, &[]).unwrap();
        let magic = view.cells.iter().position(|c| c.label == "Magic").unwrap();
        assert_eq!(view.cells[magic].tier, 2);
        let once = click(&gd, &inv, &pouch, &Start::default(), magic, Intent::Up).unwrap();
        assert_eq!(once, start(&[("magic-meter", 1)]));
    }

    /// The panel holding the arsenal, and the cell labelled `label` in it.
    fn arsenal_cell(label: &str) -> Option<(ItemsView, crate::inventory::Cell)> {
        let (gd, inv, pouch) = layout()?;
        let view = items(&gd, &inv, &pouch, &Start::of(&[ARSENAL]), &[]).unwrap();
        let cell = view
            .cells
            .iter()
            .find(|c| c.label == label)
            .unwrap()
            .clone();
        Some((view, cell))
    }

    #[test]
    fn the_arsenal_fills_its_wallet() {
        let Some((view, _)) = arsenal_cell("Wallet") else {
            return;
        };
        let rupees = view.status.iter().find(|f| f.hash == RUPEES).unwrap();
        assert_eq!(
            rupees.max.map(|m| m.to_string()),
            Some(rupees.value.clone())
        );
    }

    #[test]
    fn the_arsenal_holds_improved_spin() {
        let Some((_, spin)) = arsenal_cell("Improved Spin") else {
            return;
        };
        assert_eq!(spin.tier, 1);
    }

    #[test]
    fn the_arsenal_holds_a_hundred_gold_skulltulas() {
        let Some((_, tokens)) = arsenal_cell("Gold Skulltulas") else {
            return;
        };
        assert_eq!(tokens.count, Some(100));
    }

    #[test]
    fn the_arsenal_holds_every_trade_item() {
        let Some((view, _)) = arsenal_cell("Weird Egg") else {
            return;
        };
        let trade: Vec<_> = (view.cells.iter())
            .filter(|c| c.section == inventory::Section::Trade)
            .collect();
        assert_eq!(trade.len(), 14);
        assert!(trade.iter().all(|c| c.tier == 1), "{trade:?}");
    }

    #[test]
    fn clicks_on_a_fresh_save_come_back_as_words() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let cells = inv.cells();
        let at = |label: &str| cells.iter().position(|c| c.label == label).unwrap();
        let once = click(
            &gd,
            &inv,
            &pouch,
            &Start::default(),
            at("Hookshot"),
            Intent::Up,
        )
        .unwrap();
        assert_eq!(once, start(&[("hookshot", 1)]));
        let twice = click(&gd, &inv, &pouch, &once, at("Hookshot"), Intent::Up).unwrap();
        assert_eq!(twice, start(&[("hookshot", 1), ("longshot", 1)]));
        let back = click(&gd, &inv, &pouch, &twice, at("Hookshot"), Intent::Up).unwrap();
        assert_eq!(back, Start::default());
        let view = items(&gd, &inv, &pouch, &twice, &[]).unwrap();
        assert!(view.staged[at("Hookshot")]);
        assert!(view.locked.iter().all(Option::is_none));
        let fields: Vec<_> = view.status.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(fields, ["Heart containers", "Rupees"]);
        let dungeons = view
            .sections
            .iter()
            .find(|p| p.section == inventory::Section::Dungeons);
        assert!(!dungeons.unwrap().shown);
    }

    #[test]
    fn a_setting_s_items_show_held_and_locked_to_it() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let given = [
            vec!["zeldas-letter".to_owned(), "zeldas-lullaby".to_owned()],
            vec!["gerudo-card".to_owned()],
        ];
        let view = items(&gd, &inv, &pouch, &Start::default(), &given).unwrap();
        let locked_to = |giver: u32| {
            (view.locked.iter().enumerate())
                .filter(|(_, l)| **l == Some(giver))
                .map(|(at, _)| at)
                .collect::<Vec<_>>()
        };
        assert_eq!(locked_to(0).len(), 2);
        assert_eq!(locked_to(1).len(), 1);
        assert!((locked_to(0).into_iter().chain(locked_to(1))).all(|at| view.cells[at].tier > 0));
        let hookshot = view
            .cells
            .iter()
            .position(|c| c.label == "Hookshot")
            .unwrap();
        assert_eq!(view.locked[hookshot], None);
    }

    #[test]
    fn the_kokiri_tunic_given_shows_held_and_locked() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let given = [vec!["kokiri-tunic".to_owned()]];
        let view = items(&gd, &inv, &pouch, &Start::default(), &given).unwrap();
        let at = (view.cells.iter())
            .position(|c| c.label == "Kokiri Tunic")
            .unwrap();
        assert_eq!(view.cells[at].tier, 1);
        assert_eq!(view.locked[at], Some(0));
    }

    #[test]
    fn status_edits_come_back_as_hearts_and_rupees() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let s = set(
            &gd,
            &inv,
            &pouch,
            &Start::default(),
            Edit::new(MAX_LIFE, "20"),
        )
        .unwrap();
        assert_eq!(s, start(&[(HEARTS, 5)]));
        let s = set(&gd, &inv, &pouch, &s, Edit::new(RUPEES, "50")).unwrap();
        assert_eq!(s, start(&[(HEARTS, 5), (RUPEE_WORD, 50)]));
        assert!(set(&gd, &inv, &pouch, &s, Edit::new(LIFE, "4")).is_err());
    }
}
