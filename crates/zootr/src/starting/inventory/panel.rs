//! The Inventory panel's grid: the save editor's own, run on a fresh
//! save with the start granted onto it.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use ounce_formats::Save;

use super::{ALWAYS, BARRED, CONTAINERS, HEARTS, RUPEE_WORD, Start, WORDS, word_of};
use crate::bombchu_bags;
use crate::gamedata::GameData;
use crate::inventory::{self, Cell, Glyph, Intent, Inventory, Pouch, Section};
use crate::known::{Name, actor, field, item};
use crate::magic::meter;
use crate::save_editor::{self, ItemsView, LIFE, MAX_LIFE, Opened, RUPEES};
use crate::savefile::{self, Edit};
use crate::value;

/// A new game's save, as far as the grid and the kept Status fields go:
/// every value they read, at its GameData default, holding what every new
/// game holds ([`ALWAYS`]). The Magic Meter's, which only a seed's GameData
/// has, at the default of what it copies, and the Bombchu Bags' too where
/// `pouch` has them.
fn fresh(gd: &GameData, inv: &Inventory, pouch: &Pouch) -> Result<Save> {
    let chu_bags = pouch.bag_cap(bombchu_bags::BAGS[0]).is_some();
    let copied = |hash| {
        let bag = || chu_bags.then(|| bombchu_bags::copied_from(hash)).flatten();
        meter::copied_from(hash).or_else(bag)
    };
    let mut seen = HashSet::new();
    let hashes = (inv.cell_hashes(pouch).into_iter().flatten()).chain([MAX_LIFE, LIFE, RUPEES]);
    let mut entries = Vec::new();
    for hash in hashes.filter(|h| seen.insert(*h)) {
        let from = match gd.kind(hash) {
            Some(_) => hash,
            None => copied(hash).unwrap_or(hash),
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
    let mut save = Save::build(&entries)?;
    let always: Vec<(&Name, i32)> = ALWAYS.iter().map(|&a| (a, 1)).collect();
    savefile::apply(gd, &mut save, &inv.holding(pouch, &always)?)?;
    Ok(save)
}

/// `start` as a seed with `pouch` (`Pouch::seeded`) deals it, holding
/// what its weapons and bags bring.
fn shown(start: &Start, pouch: &Pouch) -> Start {
    let mut start = start.clone();
    start.settle(pouch);
    start.floored(pouch)
}

/// The editor's view of a new game holding `start`: a fresh save as
/// `original`, and it holding `start` as `edited`, settled to `pouch`.
pub fn open(gd: &GameData, inv: &Inventory, pouch: &Pouch, start: &Start) -> Result<Opened> {
    let original = fresh(gd, inv, pouch)?;
    let mut edited = Save::parse(original.bytes().to_vec())?;
    let start = &shown(start, pouch);
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
    // Not the defaults' rupees: the Status edits set them.
    for d in start.bagged() {
        edits.push(Edit::parse(&d)?);
    }
    edits.extend(start.status());
    let (kept, lacked): (Vec<Edit>, Vec<Edit>) =
        edits.into_iter().partition(|e| save.get(e.hash).is_some());
    let mut log = savefile::apply(gd, save, &kept)?;
    log.extend((lacked.iter()).map(|e| format!("{}: not in this save, skipped", gd.name(e.hash))));
    Ok(log)
}

/// A cell the panel adds to the grid's Quest section, for a count a new
/// game starts with that no pouch item holds.
struct Extra {
    label: &'static str,
    icon: &'static Name,
    glyph: Glyph,
    word: &'static str,
    /// What a click adds or takes, none for the whole cap at once.
    step: Option<i32>,
}

const EXTRAS: [Extra; 2] = [
    Extra {
        label: "Heart Containers",
        icon: actor::OBJ_HEART_CONTAINER,
        glyph: Glyph::Heart,
        word: HEARTS,
        step: Some(1),
    },
    Extra {
        label: "Rupees",
        icon: actor::OBJ_RUPEE_GREEN,
        glyph: Glyph::Rupee,
        word: RUPEE_WORD,
        step: None,
    },
];

impl Extra {
    /// The most it can hold with `save`'s wallet.
    fn cap(&self, pouch: &Pouch, save: &Save) -> i32 {
        match self.word {
            HEARTS => CONTAINERS,
            _ => (pouch.cap_of_bag(item::WALLET_SMALL.get(), save)).unwrap_or(0),
        }
    }

    /// Its count in `start`.
    fn of(&self, start: &Start) -> i32 {
        start.0.get(self.word).copied().unwrap_or(0)
    }

    /// `start` holding `n` of it, the word left out at none.
    fn set(&self, start: &mut Start, n: i32) {
        match n {
            0 => start.0.remove(self.word),
            n => start.0.insert(self.word.into(), n),
        };
    }

    fn cell(&self, n: i32) -> Cell {
        Cell {
            section: Section::Quest,
            label: self.label.into(),
            icon: self.icon.to_string(),
            glyph: self.glyph,
            tier: u8::from(n > 0),
            tiers: 1,
            count: Some(n),
            row: None,
            hidden: false,
        }
    }

    /// Its count after `intent` from `n`: a click adds a step, wrapping
    /// to none past the cap, and a right click takes one.
    fn after(&self, n: i32, cap: i32, intent: Intent) -> i32 {
        let step = self.step.unwrap_or(cap);
        match intent {
            Intent::Up if n >= cap => 0,
            Intent::Up => (n + step).min(cap),
            Intent::Down if n <= 0 => cap,
            Intent::Down => (n - step).max(0),
            Intent::Count(c) => c.clamp(0, cap),
        }
    }
}

/// The Inventory panel: the grid without its Dungeons, whose keys belong to
/// the Dungeon keys setting, or the stones and medallions no new game
/// starts with, then the [`EXTRAS`]. No Status fields: the extras hold
/// what they set. `given` holds the words each setting gives outside
/// `start`: they show held, each cell locked to the first that gives it.
/// `barred` holds the words the settings keep out, each with why
/// ([`crate::settings::barred`]): their cells lock past the givers,
/// `view.bars` saying why.
pub fn items(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    start: &Start,
    given: &[Vec<String>],
    barred: &[(&str, String)],
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
    let mut bars = Vec::new();
    for words in given {
        let mut alone = Start::default();
        words.iter().try_for_each(|w| alone.add(w, 1))?;
        giving.push(grid(&alone)?);
    }
    // A barred word's own cell: the one it raises past what it brings
    // along (a bottle brings the slots before it).
    let mut keeping = Vec::new();
    for (word, why) in barred {
        let mut with = Start::default();
        with.add(word, 1)?;
        let mut without = with.clone();
        without.0.remove(*word);
        keeping.push((grid(&with)?, grid(&without)?));
        bars.push(why.clone());
    }
    let opened = open(gd, inv, pouch, &shown)?;
    let mut view = save_editor::items(gd, inv, pouch, &opened);
    view.locked = (0..view.cells.len())
        .map(|at| {
            let gives = |tiers: &Vec<u8>| tiers[at] > bare[at];
            let keeps = |(with, without): &(Vec<u8>, Vec<u8>)| with[at] > without[at];
            let barred = || keeping.iter().position(keeps).map(|i| i + given.len());
            (giving.iter().position(gives).or_else(barred)).map(|i| i as u32)
        })
        .collect();
    view.bars = bars;
    for e in &EXTRAS {
        let n = e.of(&shown);
        view.cells.push(e.cell(n));
        view.caps.push(Some(e.cap(pouch, &opened.edited)));
        view.staged.push(n > 0);
        view.locked.push(None);
    }
    (view.sections.iter_mut())
        .filter(|p| p.section == inventory::Section::Dungeons || BARRED.contains(&p.section))
        .for_each(|p| p.shown = false);
    view.status.clear();
    Ok(view)
}

/// `start` after a click on cell `at`, a grid cell or one of the
/// [`EXTRAS`] after them.
pub fn click(
    gd: &GameData,
    inv: &Inventory,
    pouch: &Pouch,
    start: &Start,
    at: usize,
    intent: Intent,
) -> Result<Start> {
    let opened = open(gd, inv, pouch, start)?;
    let extra = at.checked_sub(inv.cells().len());
    if let Some(e) = extra.and_then(|i| EXTRAS.get(i)) {
        let mut out = start.clone();
        let cap = e.cap(pouch, &opened.edited);
        e.set(&mut out, e.after(e.of(start), cap, intent));
        return Ok(out);
    }
    let edits = save_editor::click(inv, pouch, &opened, at, intent)?;
    let mut out = reread(gd, inv, pouch, opened, &edits)?;
    // A bag every new game holds has no none: past either end, the click
    // goes on to the other.
    let tiers = inventory::tiers_at(at);
    let floor = tiers
        .first()
        .filter(|t| pouch.brought_bags().any(|b| b == t.get()));
    if floor.is_some() && tiers.iter().all(|t| out.count(t) == 0) {
        let to = if intent == Intent::Down {
            tiers.last()
        } else {
            floor
        };
        out.add(word_of(to.expect("a tier"))?, 1)?;
    }
    keep_chosen(&mut out, start, pouch);
    // A smaller wallet holds fewer rupees.
    let now = open(gd, inv, pouch, &out)?;
    for e in &EXTRAS {
        e.set(&mut out, e.of(start).min(e.cap(pouch, &now.edited)));
    }
    Ok(out)
}

/// Keep `out` to what the player chose, not what settling `start` made of
/// it: what settling took and the click left alone comes back, so a change
/// of settings gives it back, and a bag settling added stays out while it's
/// the only tier of its line held.
fn keep_chosen(out: &mut Start, start: &Start, pouch: &Pouch) {
    let settled = shown(start, pouch);
    for (w, a) in WORDS {
        let get = |s: &Start| s.0.get(*w).copied();
        let (chose, made, now) = (get(start), get(&settled), get(out));
        let took = made.unwrap_or(0) < chose.unwrap_or(0) && now == made;
        let line = inventory::tiers_of(a);
        let alone = !line.iter().any(|t| t != a && out.count(t) > 0);
        let added = chose.is_none() && made.is_some() && now == made && alone;
        if took || added {
            match chose {
                Some(n) => out.0.insert((*w).to_owned(), n),
                None => out.0.remove(*w),
            };
        }
    }
}

/// What `opened` holds after `edits`, as words: only what differs from a
/// fresh save. The [`EXTRAS`] aren't read back: the caller keeps them.
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
    Ok(Start(out).floored(pouch))
}

#[cfg(test)]
mod tests {
    use super::super::tests::start;
    use super::*;
    use crate::settings::{Bombchus, NewGame};
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
        // A save without Bombchu Bags reads back without them.
        let mut want = shown(&kit, &pouch);
        EXTRAS.iter().for_each(|e| e.set(&mut want, 0));
        assert_eq!(read, want);
    }

    #[test]
    fn onto_sets_the_rupees_once() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let mut save = fresh(&gd, &inv, &pouch).unwrap();
        let log = onto(&gd, &inv, &pouch, &mut save, &start(&[(RUPEE_WORD, 99)])).unwrap();
        let rupees = format!("{}: ", gd.name(RUPEES));
        let set: Vec<_> = log.iter().filter(|l| l.starts_with(&rupees)).collect();
        assert_eq!(set.len(), 1, "{log:?}");
    }

    #[test]
    fn the_panel_holds_the_arsenal_and_its_magic_meter() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let mut kit = Start::default();
        kit.add(ARSENAL, 1).unwrap();
        let view = items(&gd, &inv, &pouch, &kit, &[], &[]).unwrap();
        let magic = view.cells.iter().position(|c| c.label == "Magic").unwrap();
        assert_eq!(view.cells[magic].tier, 2);
        let once = click(&gd, &inv, &pouch, &Start::default(), magic, Intent::Up).unwrap();
        assert_eq!(once, start(&[("magic-meter", 1)]));
    }

    /// The panel holding the arsenal, and the cell labelled `label` in it.
    fn arsenal_cell(label: &str) -> Option<(ItemsView, crate::inventory::Cell)> {
        let (gd, inv, pouch) = layout()?;
        let view = items(&gd, &inv, &pouch, &Start::of(&[ARSENAL]), &[], &[]).unwrap();
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
        let Some((view, rupees)) = arsenal_cell("Rupees") else {
            return;
        };
        let at = view.cells.iter().position(|c| c == &rupees).unwrap();
        assert_eq!(rupees.count, Some(1000));
        assert_eq!(view.caps[at], Some(1000));
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
        let view = items(&gd, &inv, &pouch, &twice, &[], &[]).unwrap();
        assert!(view.staged[at("Hookshot")]);
        assert!(view.locked.iter().all(Option::is_none));
        assert!(view.status.is_empty());
        let hidden: Vec<_> = (view.sections.iter())
            .filter(|p| !p.shown)
            .map(|p| p.section)
            .collect();
        use inventory::Section::{Dungeons, Medallions, Stones};
        assert_eq!(hidden, [Stones, Medallions, Dungeons]);
    }

    #[test]
    fn the_wallet_starts_at_the_child_s_and_clicks_up_to_the_adult_s() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let view = items(&gd, &inv, &pouch, &Start::default(), &[], &[]).unwrap();
        let at = view.cells.iter().position(|c| c.label == "Wallet").unwrap();
        assert_eq!(view.cells[at].tier, 1);
        let adult = click(&gd, &inv, &pouch, &Start::default(), at, Intent::Up).unwrap();
        assert_eq!(adult, start(&[("adults-wallet", 1)]));
    }

    /// The pouch table as a seed with `new_game` writes it.
    fn seeded(pouch: &Pouch, new_game: NewGame) -> Pouch {
        pouch.clone().seeded(&new_game)
    }

    #[test]
    fn the_bombchu_bag_cell_shows_only_in_a_seed_with_bombchu_bags() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let cell = |view: &ItemsView, label: &str| {
            view.cells.iter().position(|c| c.label == label).unwrap()
        };
        let free = seeded(
            &pouch,
            NewGame {
                bombchus: Bombchus::Free,
                ..Default::default()
            },
        );
        let view = items(&gd, &inv, &free, &Start::default(), &[], &[]).unwrap();
        let (bag, chus) = (cell(&view, "Bombchu Bag"), cell(&view, "Bombchus"));
        assert!(view.cells[bag].hidden);
        assert_eq!(view.caps[chus], Some(40), "free bombchus need no bag");
        let bags = seeded(
            &pouch,
            NewGame {
                bombchus: Bombchus::Bag,
                ..Default::default()
            },
        );
        let view = items(&gd, &inv, &bags, &Start::default(), &[], &[]).unwrap();
        assert!(!view.cells[bag].hidden);
        assert_eq!(view.caps[chus], Some(0), "none before the first bag");
        let up = |s: &Start| click(&gd, &inv, &bags, s, bag, Intent::Up).unwrap();
        let medium = up(&up(&Start::default()));
        assert!(medium.holds("big-bombchu-bag"), "{medium:?}");
        let view = items(&gd, &inv, &bags, &medium, &[], &[]).unwrap();
        assert_eq!(view.caps[chus], Some(30));
    }

    #[test]
    fn an_unshuffled_bag_has_no_none() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let game = seeded(&pouch, NewGame::default());
        let view = items(&gd, &inv, &game, &Start::default(), &[], &[]).unwrap();
        let at = view
            .cells
            .iter()
            .position(|c| c.label == "Nut Capacity")
            .unwrap();
        assert_eq!(view.cells[at].tier, 1, "every new game holds the Small bag");
        let go = |s: &Start, intent| click(&gd, &inv, &game, s, at, intent).unwrap();
        let large = go(&Start::default(), Intent::Down);
        assert!(large.holds("biggest-nut-bag"), "Small wraps down to Large");
        let small = go(&large, Intent::Up);
        assert_eq!(small, start(&[("nut-bag", 1)]), "Large wraps up to Small");
        let shuffled = seeded(
            &pouch,
            NewGame {
                shuffle_nut_bag: true,
                ..Default::default()
            },
        );
        let view = items(&gd, &inv, &shuffled, &Start::default(), &[], &[]).unwrap();
        assert_eq!(view.cells[at].tier, 0, "a shuffled bag can be none");
    }

    #[test]
    fn what_the_settings_leave_no_room_for_stays_chosen_through_a_click() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let bags = seeded(
            &pouch,
            NewGame {
                bombchus: Bombchus::Bag,
                ..Default::default()
            },
        );
        let chosen = start(&[("bombchus", 20)]);
        let view = items(&gd, &inv, &bags, &chosen, &[], &[]).unwrap();
        let at = |label: &str| view.cells.iter().position(|c| c.label == label).unwrap();
        assert_eq!(view.cells[at("Bombchus")].count, Some(0));
        let after = click(&gd, &inv, &bags, &chosen, at("Hookshot"), Intent::Up).unwrap();
        assert_eq!(after, start(&[("bombchus", 20), ("hookshot", 1)]));
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
        let view = items(&gd, &inv, &pouch, &Start::default(), &given, &[]).unwrap();
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
        let view = items(&gd, &inv, &pouch, &Start::default(), &given, &[]).unwrap();
        let at = (view.cells.iter())
            .position(|c| c.label == "Kokiri Tunic")
            .unwrap();
        assert_eq!(view.cells[at].tier, 1);
        assert_eq!(view.locked[at], Some(0));
    }

    #[test]
    fn a_barred_bottle_locks_its_own_cell_past_the_givers() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let given = [vec!["kokiri-tunic".to_owned()]];
        let barred = [("bottle-5", "Needs FakeSetting".to_owned())];
        let four = Start::of(&["bottle-1", "bottle-2", "bottle-3", "bottle-4"]);
        let view = items(&gd, &inv, &pouch, &four, &given, &barred).unwrap();
        let bars: Vec<_> = (view.locked.iter().enumerate())
            .filter(|(_, l)| **l == Some(1))
            .map(|(at, _)| at)
            .collect();
        assert_eq!(bars.len(), 1, "one bottle cell");
        assert_eq!(view.cells[bars[0]].tier, 0, "shown empty");
        assert_eq!(view.bars, ["Needs FakeSetting"]);
        let free = items(&gd, &inv, &pouch, &four, &given, &[]).unwrap();
        let mut locked = free.locked.clone();
        locked[bars[0]] = Some(1);
        assert_eq!(view.locked, locked, "only bottle 5 locks");
    }

    #[test]
    fn rupees_are_a_quest_cell_the_wallet_caps() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let click = |s: &Start, at, intent| click(&gd, &inv, &pouch, s, at, intent).unwrap();
        let view = items(&gd, &inv, &pouch, &Start::default(), &[], &[]).unwrap();
        let cell = |label: &str| view.cells.iter().position(|c| c.label == label).unwrap();
        let (at, wallet) = (cell("Rupees"), cell("Wallet"));
        assert_eq!(view.cells[at].section, Section::Quest);
        assert_eq!((view.cells[at].count, view.caps[at]), (Some(0), Some(200)));
        // A click fills the wallet, a second empties it.
        let full = click(&Start::default(), at, Intent::Up);
        assert_eq!(full, start(&[(RUPEE_WORD, 200)]));
        assert_eq!(click(&full, at, Intent::Up), Start::default());
        let some = click(&full, at, Intent::Count(50));
        assert_eq!(some, start(&[(RUPEE_WORD, 50)]));
        // A bigger wallet raises the cap, and a smaller one brings the
        // rupees down to it.
        let adult = click(&some, wallet, Intent::Up);
        let rich = click(&adult, at, Intent::Count(350));
        assert_eq!(rich.0[RUPEE_WORD], 350);
        let back = click(&rich, wallet, Intent::Down);
        assert!(!back.holds("adults-wallet"));
        assert_eq!(back, start(&[(RUPEE_WORD, 200)]));
    }

    #[test]
    fn heart_containers_are_a_quest_cell_counted_to_eight() {
        let Some((gd, inv, pouch)) = layout() else {
            return;
        };
        let click = |s: &Start, at, intent| click(&gd, &inv, &pouch, s, at, intent).unwrap();
        let view = items(&gd, &inv, &pouch, &Start::default(), &[], &[]).unwrap();
        let at = (view.cells.iter())
            .position(|c| c.label == "Heart Containers" && !c.hidden)
            .unwrap();
        assert_eq!(view.cells[at].section, Section::Quest);
        assert_eq!((view.cells[at].count, view.caps[at]), (Some(0), Some(8)));
        let one = click(&Start::default(), at, Intent::Up);
        assert_eq!(one, start(&[(HEARTS, 1)]));
        assert_eq!(click(&one, at, Intent::Count(20)), start(&[(HEARTS, 8)]));
        assert_eq!(click(&one, at, Intent::Down), Start::default());
        assert_eq!(
            click(&Start::default(), at, Intent::Down),
            start(&[(HEARTS, 8)])
        );
        // A grid click keeps the containers.
        let hookshot = view
            .cells
            .iter()
            .position(|c| c.label == "Hookshot")
            .unwrap();
        let both = click(&one, hookshot, Intent::Up);
        assert_eq!(both, start(&[(HEARTS, 1), ("hookshot", 1)]));
    }

    #[test]
    fn heart_pieces_count_to_thirty_six() {
        let Some((view, pieces)) = arsenal_cell("Heart Pieces") else {
            return;
        };
        let at = view.cells.iter().position(|c| c == &pieces).unwrap();
        assert_eq!((pieces.count, view.caps[at]), (Some(36), Some(36)));
    }
}
