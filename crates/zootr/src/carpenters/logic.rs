//! What each cell's checks need, from [`CELLS`]:
//!
//! | Check | Needs |
//! |---|---|
//! | a jailed cell's thief | the cell's reach and `melee` |
//! | a jailed cell's pot | that cell open: its thief beaten (the door opens only then) and its key by key mode |
//! | a freed cell's pot | the cell's reach |
//! | the card, `event:carpenters-freed` | every jailed cell open |
//!
//! A cell's reach is the fortress floor for the 1 Torch Jail and the
//! Hookshot for the three up the walls. Its key, by key mode:
//!
//! | Fortress Keys | A cell opens with |
//! |---|---|
//! | Vanilla | its own thief's key, beaten for anyway |
//! | Start | the counter, filled from the start |
//! | Key rings | the ring |
//! | Keysanity | as many keys as cells start jailed, in the worst order |
//!
//! Mixed is whichever of the two the seed rolled.
//!
//! The mode may be left open (the Archipelago export), so a need is the
//! Normal one, or the Fast or Freed one behind its word. Each mode jails a
//! subset of the one before it, so its need is never more than Normal's:
//! the weakest met need wins, as the mode says.

use anyhow::Result;

use super::{CARD, CELLS, Cell, jailed, jailed_count};
use crate::key_tints;
use crate::keys::{FORTRESS_N, KeyMode};
use crate::rules::{Book, Ctx, Need};
use crate::settings::{FORTRESS_FAST, FORTRESS_FREED, FortressKeys, GerudoFortress};

/// The word every jailed cell open gives: the four carpenters freed.
const FREED: &str = "event:carpenters-freed";
/// The word beating a thief takes.
const MELEE: &str = "melee";
/// The word reaching a cell up the walls takes.
const HOOKSHOT: &str = "hookshot";

/// What the check at placement `hash`, holding `item` in the game, needs
/// from the quest beyond its place: a thief's, a cell pot's, or the
/// card's. `None` for every other check.
pub fn check_need(book: &Book, ctx: &Ctx, hash: u64, item: &str) -> Result<Option<Need>> {
    if let Some(cell) = CELLS.iter().find(|c| c.pot == hash) {
        return by_mode(ctx, |mode| inside(book, ctx, cell, mode)).map(Some);
    }
    // A thief's check exists only while her cell is jailed: a freed one's
    // leaves the pool.
    let thief = CELLS.iter().find(|c| c.thief == hash);
    if let Some(cell) = thief.or_else(|| super::cell_of_key(item)) {
        return beaten(book, ctx, cell).map(Some);
    }
    source_need(book, ctx, item)
}

/// What the source of `item`, a game id, needs from the quest: every
/// jailed cell open, for the card and the carpenters freed. `None` for
/// every other source.
pub fn source_need(book: &Book, ctx: &Ctx, item: &str) -> Result<Option<Need>> {
    if item != CARD.get() && item != crate::rules::vocab::id(FREED)? {
        return Ok(None);
    }
    by_mode(ctx, |mode| all_open(book, ctx, mode)).map(Some)
}

/// `need` for Normal, or for Fast or Freed behind its word.
fn by_mode(ctx: &Ctx, need: impl Fn(GerudoFortress) -> Result<Need>) -> Result<Need> {
    Ok(Need::Any(vec![
        need(GerudoFortress::Normal)?,
        Need::All(vec![
            ctx.setting(FORTRESS_FAST),
            need(GerudoFortress::Fast)?,
        ]),
        Need::All(vec![
            ctx.setting(FORTRESS_FREED),
            need(GerudoFortress::Freed)?,
        ]),
    ]))
}

/// Inside `cell` under `mode`: open from the start when freed, else
/// opened.
fn inside(book: &Book, ctx: &Ctx, cell: &Cell, mode: GerudoFortress) -> Result<Need> {
    match jailed(mode, cell) {
        true => Ok(Need::All(vec![beaten(book, ctx, cell)?, key(ctx, mode)])),
        false => reach(book, ctx, cell),
    }
}

/// Every cell jailed under `mode` opened.
fn all_open(book: &Book, ctx: &Ctx, mode: GerudoFortress) -> Result<Need> {
    let mut needs = vec![key(ctx, mode)];
    for cell in CELLS.iter().filter(|c| jailed(mode, c)) {
        needs.push(beaten(book, ctx, cell)?);
    }
    Ok(Need::All(needs))
}

/// `cell`'s thief beaten: the cell reached, and a weapon.
fn beaten(book: &Book, ctx: &Ctx, cell: &Cell) -> Result<Need> {
    Ok(Need::All(vec![
        reach(book, ctx, cell)?,
        book.resolve(MELEE, ctx)?,
    ]))
}

fn reach(book: &Book, ctx: &Ctx, cell: &Cell) -> Result<Need> {
    match cell.hookshot {
        true => book.resolve(HOOKSHOT, ctx),
        false => Ok(Need::Always),
    }
}

/// The key a jailed cell's door takes under `mode`, as the Fortress keys
/// are dealt.
fn key(ctx: &Ctx, mode: GerudoFortress) -> Need {
    let copies = |count| Need::Items {
        actors: vec![key_tints::fortress()],
        count,
    };
    match ctx.keys.fortress() {
        FortressKeys::Vanilla | FortressKeys::Start => Need::Always,
        _ if jailed_count(mode) == 0 => Need::Always,
        _ => match ctx.keys.mode(FORTRESS_N) {
            KeyMode::Rings => copies(1),
            KeyMode::Singles => copies(jailed_count(mode) as u32),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Have;
    use crate::rules::vocab::id;
    use crate::settings::{FORTRESS_KEYS, GERUDO_FORTRESS, NewGame, Settings};
    use crate::world::Era;

    const SWORD: &str = "item:master-sword";
    const HOOK: &str = "item:hookshot";

    /// A seed's logic under Gerudo Fortress `mode` and Fortress Keys `keys`,
    /// as the settings make it.
    fn ctx(mode: &str, keys: &str) -> Ctx {
        let settings = Settings::from([
            (GERUDO_FORTRESS.into(), mode.into()),
            (FORTRESS_KEYS.into(), keys.into()),
        ]);
        Ctx::new(&NewGame::from_settings(&settings, &[]).unwrap())
    }

    /// Link with `items` and `copies` Fortress key copies.
    fn have(items: &[&str], copies: usize) -> Have {
        let mut have = Have::default();
        items.iter().for_each(|i| have.add(&id(i).unwrap()));
        (0..copies).for_each(|_| have.add(&key_tints::fortress()));
        have
    }

    /// Whether the check at `hash` holding `item` is in logic for `have`.
    fn met(ctx: &Ctx, hash: u64, item: &str, have: &Have) -> bool {
        let book = Book::load().unwrap();
        let need = check_need(&book, ctx, hash, item).unwrap().unwrap();
        need.met(have, Era::Adult)
    }

    fn card(ctx: &Ctx, have: &Have) -> bool {
        met(ctx, 0, CARD.get(), have)
    }

    /// The 1 Torch Jail's pot, off the floor, and a pot up the walls.
    fn pots(ctx: &Ctx, have: &Have) -> [bool; 2] {
        [CELLS[0].pot, CELLS[1].pot].map(|p| met(ctx, p, "", have))
    }

    #[test]
    fn normal_opens_each_cell_by_beating_its_thief() {
        let ctx = ctx("normal", "vanilla");
        assert!(!card(&ctx, &have(&[SWORD], 0)));
        assert!(card(&ctx, &have(&[SWORD, HOOK], 0)));
        assert_eq!(pots(&ctx, &have(&[], 0)), [false, false]);
        assert_eq!(pots(&ctx, &have(&[SWORD], 0)), [true, false]);
        assert_eq!(pots(&ctx, &have(&[SWORD, HOOK], 0)), [true, true]);
        let thief = |cell: &Cell, items| met(&ctx, cell.thief, "", &have(items, 0));
        assert!(!thief(&CELLS[0], &[]) && thief(&CELLS[0], &[SWORD]));
        assert!(!thief(&CELLS[1], &[SWORD]) && thief(&CELLS[1], &[SWORD, HOOK]));
    }

    #[test]
    fn fast_leaves_the_one_torch_jail_to_open() {
        let ctx = ctx("fast", "vanilla");
        assert!(!card(&ctx, &have(&[], 0)));
        assert!(card(&ctx, &have(&[SWORD], 0)));
        // A freed cell up the walls takes only its reach.
        assert_eq!(pots(&ctx, &have(&[HOOK], 0)), [false, true]);
    }

    #[test]
    fn freed_opens_every_cell_from_the_start() {
        let ctx = ctx("freed", "singles");
        assert!(card(&ctx, &have(&[], 0)));
        assert_eq!(pots(&ctx, &have(&[], 0)), [true, false]);
        assert_eq!(pots(&ctx, &have(&[HOOK], 0)), [true, true]);
    }

    #[test]
    fn keysanity_asks_a_key_for_every_jailed_cell() {
        let all = [SWORD, HOOK];
        let normal = ctx("normal", "singles");
        assert!(!card(&normal, &have(&all, 3)) && card(&normal, &have(&all, 4)));
        assert!(!pots(&normal, &have(&all, 3))[0]);
        let fast = ctx("fast", "singles");
        assert!(!card(&fast, &have(&[SWORD], 0)) && card(&fast, &have(&[SWORD], 1)));
    }

    #[test]
    fn a_ring_opens_every_cell_and_start_needs_no_key() {
        let all = [SWORD, HOOK];
        let rings = ctx("normal", "rings");
        assert!(!card(&rings, &have(&all, 0)) && card(&rings, &have(&all, 1)));
        assert!(card(&ctx("normal", "start"), &have(&all, 0)));
    }

    #[test]
    fn other_checks_ask_nothing_of_the_quest() {
        let book = Book::load().unwrap();
        let need = check_need(&book, &ctx("normal", "singles"), 1, "").unwrap();
        assert!(need.is_none());
    }
}
