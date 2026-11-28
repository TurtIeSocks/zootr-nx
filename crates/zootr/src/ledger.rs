//! The check ledger: one row per check, dealt or not, with what the code
//! knows of it. `probe ledger` prints it for the maintainer's ledger app,
//! and the `accounting` test holds it to the EIE baseline.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Romfs};
use serde::Serialize;

use crate::checks::{self, GYAML, ItemCheck, actors, banc_files, hash_of, load_bancs, str_field};
use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::known::{Name, actor};
use crate::logic::World;
use crate::narrowing::{self, LeftOut, Let};
use crate::rules::{Book, Need};
use crate::settings::{self, Kind, NewGame, Settings, group};
use crate::shuffle::{self, Fixed, Placement};
use crate::tracker::{Flag, Tracker};
use crate::{cows, gifts, pickups, scrubs, shops};

/// Bumped whenever a row changes shape, so the app refuses a ledger it
/// can't read.
pub const SCHEMA: u32 = 1;

/// The EIE debt today. A PR that pays some off lowers it in the same PR,
/// and the `accounting` test fails one that adds debt.
pub const BASELINE: Debt = Debt {
    refused: 6,
    left_out: 323,
};

/// Placements `left_out` calls no holder that aren't breakables, by
/// holder, and why.
const NOT_HOLDERS: [(&Name, Why); 23] = [
    // Furniture in the Iron Knuckle rooms, a tree and a cow bone.
    (actor::OBJ_IRON_KNUCKLE_PILLAR_A_01, Why::Scenery),
    (actor::OBJ_IRON_KNUCKLE_CHAIR_A_02, Why::Scenery),
    (actor::OBJ_TREE_A_02, Why::Scenery),
    (actor::OBJ_GERUDO_COW_BONE_DECORATION_B_01, Why::Scenery),
    // Enemies that drop an item when beaten.
    (actor::ENEMY_WOLFOS, Why::Scenery),
    (actor::ENEMY_SKULLTULA, Why::Scenery),
    // The Treasure Chest Game's room chests: only its prize is a check.
    (actor::OBJ_HRL_CITY_SHOP_T_BOX_A_01, Why::Maintainer),
    (actor::OBJ_HRL_CITY_SHOP_T_BOX_A_02, Why::Maintainer),
    // The crates Link lifts: nothing breaks them (EIE row 17).
    (actor::OBJ_BROKEN_BOX_A_03, Why::Maintainer),
    // The boss rush's copies of the pots before Ganondorf, in Ganon's
    // Castle and the Dream World's stage: only the rush places them.
    (actor::OBJ_POT_A_06_DREAM, Why::Maintainer),
    // Silver rupees, obstacles a room counts until its door opens: their
    // own spec is next (EIE row 19).
    (actor::OBJ_RUPEE_TOKEN_SILVER, Why::Eie { eie: 19 }),
    // What Link catches in a bottle, fairies and Blue Fire: the loose
    // items spec's second phase (EIE row 20).
    (actor::OBJ_FAIRY, Why::Eie { eie: 20 }),
    (actor::OBJ_FAIRY_SPIRIT, Why::Eie { eie: 20 }),
    (
        actor::OBJ_FAIRY_HIDING_HOLE_FAIRY_FOUNTAIN,
        Why::Eie { eie: 20 },
    ),
    (actor::OBJ_BLUE_FIRE_A_01, Why::Eie { eie: 20 }),
    (actor::OBJ_BLUE_FIRE_A_06, Why::Eie { eie: 20 }),
    // The composer brothers' ghosts in the graveyard, an event's actors.
    (actor::OBJ_SPIRIT_POE_FLAT_EVENT, Why::Scenery),
    (actor::OBJ_SPIRIT_POE_SHARP_EVENT, Why::Scenery),
    // The fishing pond's sinking lure, gear for the fishing game.
    (actor::OBJ_SINKING_LURE, Why::Scenery),
    // Each Fortress key's placement its thief carries. The key is one
    // check, dealt through its `SmallKeyForEvent_` placement
    // (`research/keys.md`, Gerudo Fortress keys).
    (
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_01,
        Why::Scenery,
    ),
    (
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_02,
        Why::Scenery,
    ),
    (
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_03,
        Why::Scenery,
    ),
    (
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_04,
        Why::Scenery,
    ),
];

#[derive(Serialize)]
pub struct Ledger {
    pub schema: u32,
    pub rows: Vec<Row>,
}

#[derive(Serialize)]
pub struct Row {
    #[serde(with = "crate::generate::u64_text")]
    pub hash: u64,
    pub area: String,
    pub area_name: String,
    pub file: String,
    pub holder: String,
    pub kind: &'static str,
    /// What it holds in the vanilla game: empty for a random drop.
    pub vanilla: String,
    /// What the app shows it as.
    pub name: String,
    /// Each place logic puts it, by era, and what it needs there.
    pub logic: Vec<LogicNeed>,
    /// Whether a rule file names it.
    pub rule: bool,
    /// Logic has it, but nothing reaches it at either age.
    pub never: bool,
    /// Why the shuffle leaves it out. `None` for a dealt check.
    pub left_out: Option<Why>,
    /// Each item it refuses, and the narrowing that refuses it.
    pub refused: Vec<Refused>,
    /// What's set once it's checked, as `probe data` shows it.
    pub tracker: Option<String>,
    /// How the sweep triggers it, or why it skips it.
    pub sweep: Option<String>,
    /// Its room's rule, for a check in a dungeon with room files.
    pub room: Option<RoomRule>,
    /// The hand rules for getting into its area: exits into it, and what
    /// being inside needs.
    pub ways_in: Vec<WayIn>,
}

/// A hand exit into an area, or (no `from`) what being inside it needs.
#[derive(Clone, Serialize)]
pub struct WayIn {
    pub from: Option<String>,
    pub to: String,
    pub needs: String,
}

/// A check's rule in its room file: the baseline with confirmed laid over.
#[derive(Serialize)]
pub struct RoomRule {
    /// The dungeon's id (`deku-tree`).
    pub dungeon: &'static str,
    pub room: u32,
    pub room_name: String,
    pub needs: String,
    pub why: String,
    pub guess: bool,
    pub confirmed: bool,
}

#[derive(Serialize)]
pub struct LogicNeed {
    pub era: String,
    pub node: String,
    pub need: String,
}

#[derive(Serialize)]
pub struct Refused {
    pub item: String,
    pub eie: u8,
}

/// Why a placement stays out of the shuffle.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", tag = "why")]
pub enum Why {
    /// A narrowing: debt, by the EIE spec.
    Eie { eie: u8 },
    /// Not a check: no holder kind EIE names.
    Scenery,
    /// The maintainer's call, written in a spec.
    Maintainer,
    /// A setting the player picks: a check kind off.
    Setting,
    /// Nothing explains it yet: the `accounting` test fails on one.
    Unknown,
}

impl Why {
    fn of(left: LeftOut) -> Self {
        match left {
            LeftOut::Narrowed(n) => Self::Eie { eie: n.number() },
            LeftOut::Setting => Self::Setting,
            LeftOut::Maintainer => Self::Maintainer,
            LeftOut::NotAHolder => Self::Unknown,
        }
    }
}

/// EIE #12: a breakable the shuffle doesn't deal, its drop random or not.
const BREAKABLES: u8 = 12;
/// EIE #2: a key logic keeps out of its own dungeon, by asking for all
/// the dungeon's keys at each of its checks.
const OWN_DUNGEON_KEYS: u8 = 2;

/// The EIE debt the code may carry: neither count may grow.
#[derive(Debug, PartialEq, Eq)]
pub struct Debt {
    /// (check, item) pairs the shuffle refuses.
    pub refused: usize,
    /// Placements left out for a narrowing.
    pub left_out: usize,
}

impl Debt {
    pub fn of(ledger: &Ledger) -> Self {
        let rows = ledger.rows.iter();
        Self {
            refused: rows.clone().map(|r| r.refused.len()).sum(),
            left_out: rows
                .filter(|r| matches!(r.left_out, Some(Why::Eie { .. })))
                .count(),
        }
    }
}

/// Every shuffle on: each toggle the Shuffles panel shows,
/// and the dungeon keys moved.
pub fn every_shuffle() -> Settings {
    let mut out: Settings = (settings::schema().into_iter())
        .filter(|d| d.group == group::SHUFFLES && matches!(d.kind, Kind::Toggle { .. }))
        .map(|d| (d.id, true.into()))
        .collect();
    out.insert(settings::KEYS.into(), "singles".into());
    out.insert(settings::FORTRESS_KEYS.into(), "singles".into());
    out
}

/// The ledger, with every shuffle on.
pub fn build(romfs: &Romfs) -> Result<Ledger> {
    planned(romfs).map(|(ledger, _)| ledger)
}

/// [`build`], with the sweep's plan of each dealt check: where its giver
/// stands, for a photo.
pub(crate) fn planned(romfs: &Romfs) -> Result<(Ledger, Vec<crate::sweep::plan::Planned>)> {
    let new_game = NewGame::from_settings(&every_shuffle(), &[])?;
    let dealt = checks::dealt(romfs, &new_game)?;
    let fixed = Fixed::new();
    let deal = shuffle::deal(&dealt, &fixed, &new_game)?;
    let free: BTreeSet<u64> = deal.free.iter().map(|c| c.hash).collect();
    let items: BTreeSet<&str> = deal.items.iter().copied().collect();

    let mut seen = BTreeSet::new();
    let mut rows: Vec<(&ItemCheck, Option<Why>)> = deal
        .free
        .iter()
        .filter(|c| seen.insert(c.hash))
        .map(|&c| (c, None))
        .collect();
    let breakable = breakables(romfs)?;
    let others = census(romfs, &breakable)?;
    let open = Let::of(&new_game);
    for c in others
        .iter()
        .filter(|c| crate::pickups::variants::canonical(c.hash) == c.hash && seen.insert(c.hash))
    {
        let why = narrowing::left_out(c, open).map_or(Why::Unknown, Why::of);
        let why = match why {
            Why::Unknown => explain(c, &new_game, &breakable),
            why => why,
        };
        rows.push((c, Some(why)));
    }

    let book = Book::load()?;
    let ctx = crate::rules::Ctx::new(&new_game);
    let world = World::build(romfs, &book, &ctx, &dealt)?;
    let held = rows
        .iter()
        .map(|(c, _)| c.item.as_str())
        .filter(|i| !i.is_empty());
    let names = crate::names::Names::load(romfs, held)?;
    let mut spots: Vec<Spot> = (rows.iter())
        .map(|(check, _)| {
            Spot::new(
                &Placement {
                    check,
                    item: &check.item,
                },
                &names,
            )
        })
        .collect();
    crate::places::fill(romfs, &mut spots)?;
    let gd = GameData::load(romfs)?;
    let tracker = Tracker::new(&gd, &spots)?;
    let dealt_spots: Vec<Spot> = (spots.iter())
        .filter(|s| free.contains(&s.hash))
        .cloned()
        .collect();
    let planned = crate::sweep::plan::plan(romfs, &dealt_spots)?;
    let sweep: BTreeMap<u64, String> = (planned.iter())
        .map(|p| (p.spot.hash, crate::sweep::night::how(&p.trigger)))
        .collect();

    let mut in_rooms = room_rules(&book);
    let ways = ways_by_node(&book);
    let rows = rows
        .into_iter()
        .zip(spots)
        .map(|((c, left_out), spot)| Row {
            hash: c.hash,
            area: c.area.clone(),
            area_name: spot.area_name.clone(),
            file: c.file.clone(),
            holder: c.holder.clone(),
            kind: kind(c),
            vanilla: c.item.clone(),
            name: spot.name().to_owned(),
            logic: logic_needs(&world, c.hash),
            rule: book.check(c.hash).is_some(),
            never: world.never(c.hash),
            refused: match left_out {
                Some(_) => Vec::new(),
                None => refused(c, &items, &new_game),
            },
            left_out,
            tracker: tracker.flag(c.hash).map(|f| f.to_string()),
            sweep: sweep.get(&c.hash).cloned(),
            room: in_rooms.remove(&c.hash),
            ways_in: ways_in(&world, &ways, c.hash),
        })
        .collect();
    let ledger = Ledger {
        schema: SCHEMA,
        rows,
    };
    Ok((ledger, planned))
}

/// Each era's need for the check `hash` as it reads there: one that era
/// can never meet (a child row needing `adult`) isn't shown.
fn logic_needs(world: &World, hash: u64) -> Vec<LogicNeed> {
    (world.locations.iter())
        .filter(|l| l.hash == hash)
        .filter_map(|l| {
            let (node, era) = &world.places[l.place];
            let need = l.need.in_era(*era);
            (need != Need::Never).then(|| LogicNeed {
                era: era.to_string(),
                node: node.clone(),
                need: need.to_string(),
            })
        })
        .collect()
}

/// Each dungeon check's rule in its room file, by hash.
fn room_rules(book: &Book) -> BTreeMap<u64, RoomRule> {
    let mut in_rooms = BTreeMap::new();
    for room in &book.rooms {
        let Some(d) = (crate::entrances::DUNGEONS.iter()).find(|d| d.node.get() == room.node)
        else {
            continue;
        };
        for c in &room.file.check {
            let rule = RoomRule {
                dungeon: d.id,
                room: room.file.room,
                room_name: room.file.name.clone(),
                needs: c.needs.clone(),
                why: c.why.clone(),
                guess: c.guess,
                confirmed: c.confirmed,
            };
            in_rooms.insert(c.hash, rule);
        }
    }
    in_rooms
}

/// Exits and inside rules by the node they lead into, as the rule files
/// spell their places.
fn ways_by_node(book: &Book) -> BTreeMap<&str, Vec<WayIn>> {
    let word =
        |node: &str| crate::rules::vocab::place_word(node).map_or(node.to_owned(), str::to_owned);
    let mut ways: BTreeMap<&str, Vec<WayIn>> = BTreeMap::new();
    for e in &book.exits {
        ways.entry(&e.to).or_default().push(WayIn {
            from: Some(word(&e.from)),
            to: word(&e.to),
            needs: e.needs.clone(),
        });
    }
    for i in &book.insides {
        ways.entry(&i.place).or_default().push(WayIn {
            from: None,
            to: word(&i.place),
            needs: i.needs.clone(),
        });
    }
    ways
}

/// The ways into each node the check `hash` stands at, from [`ways_by_node`].
fn ways_in(world: &World, ways: &BTreeMap<&str, Vec<WayIn>>, hash: u64) -> Vec<WayIn> {
    let nodes: BTreeSet<&str> = (world.locations.iter())
        .filter(|l| l.hash == hash)
        .map(|l| world.places[l.place].0.as_str())
        .collect();
    (nodes.iter())
        .flat_map(|n| ways.get(n).into_iter().flatten().cloned())
        .collect()
}

/// Prints the ledger as JSON, or with `unknown` each left-out placement
/// nothing explains, a line each.
pub fn print(romfs: &Romfs, unknown: bool) -> Result<()> {
    let ledger = build(romfs)?;
    if !unknown {
        println!("{}", serde_json::to_string(&ledger)?);
        return Ok(());
    }
    for r in ledger
        .rows
        .iter()
        .filter(|r| r.left_out == Some(Why::Unknown))
    {
        println!(
            "{:<20} {:<24} {:<48} {:<32} {}",
            r.hash, r.area, r.holder, r.vanilla, r.name
        );
    }
    eprintln!(
        "{} rows, EIE debt {:?}, baseline {BASELINE:?}",
        ledger.rows.len(),
        Debt::of(&ledger)
    );
    Ok(())
}

/// Each item of the pool `c` refuses, and why: the shuffle's refusals,
/// then logic's where it asks for all a dungeon's keys (keys kept out of
/// their own dungeon). Counted room by room, a refusal is logic's own.
fn refused(c: &ItemCheck, items: &BTreeSet<&str>, new_game: &NewGame) -> Vec<Refused> {
    let need = (!crate::key_rooms::by_room(c))
        .then(|| crate::keys::dungeon_need(c, &new_game.key_modes()))
        .flatten();
    let own_key = |item: &str| matches!(&need, Some(Need::Items { actors, .. }) if actors.iter().any(|a| a == item));
    (items.iter())
        .filter_map(|&item| {
            let eie = match narrowing::refusal(c, item, new_game.progressive) {
                Some(n) => n.number(),
                None if own_key(item) => OWN_DUNGEON_KEYS,
                None => return None,
            };
            Some(Refused {
                item: item.to_owned(),
                eie,
            })
        })
        .collect()
}

/// Why the shuffle leaves out `c`, a placement `left_out` finds no reason
/// for: a breakable, a loose rupee or heart, a thief's key, a gift a
/// setting keeps out, or one of [`NOT_HOLDERS`].
fn explain(c: &ItemCheck, new_game: &NewGame, breakable: &BTreeSet<String>) -> Why {
    let listed = NOT_HOLDERS.iter().find(|(holder, _)| **holder == *c.holder);
    if let Some((_, why)) = listed {
        return why.clone();
    }
    if crate::pickups::variants::excluded(c.hash) {
        return Why::Maintainer;
    }
    if breakable.contains(&c.holder) {
        return Why::Eie { eie: BREAKABLES };
    }
    // A thief's carried drop is the key its check deals once the
    // thieves' keys move, and the seed clears it: no holder of its own.
    if crate::fortress::is_carried(c) {
        return match new_game.fortress_keys.moved() {
            true => Why::Scenery,
            false => Why::Setting,
        };
    }
    if gifts::of(c).is_some_and(|g| !g.dealt(&new_game.on)) {
        return Why::Setting;
    }
    Why::Unknown
}

/// Whether the maintainer keeps `c` out of the pool for good (the Treasure
/// Chest Game's room chests, Dampe's repeat prizes): never a check.
pub(crate) fn kept_out(c: &ItemCheck) -> bool {
    let open = Let {
        maps: true,
        keys: true,
    };
    let holder = || NOT_HOLDERS.iter().find(|(h, _)| **h == *c.holder);
    crate::pickups::variants::excluded(c.hash)
        || matches!(narrowing::left_out(c, open), Some(LeftOut::Maintainer))
        || !gifts::is_gift(c) && holder().is_some_and(|(_, why)| matches!(why, Why::Maintainer))
}

/// What the tracker reads once check `hash` is done in a vanilla game,
/// as the Checks tab does. `None` when it has no known flag.
pub fn flag(romfs: &Romfs, gd: &GameData, hash: u64) -> Result<Option<Flag>> {
    let new_game = NewGame::from_settings(&every_shuffle(), &[])?;
    let dealt = checks::dealt(romfs, &new_game)?;
    let check = match dealt.into_iter().find(|c| c.hash == hash) {
        Some(c) => c,
        None => (census(romfs, &breakables(romfs)?)?.into_iter())
            .find(|c| c.hash == hash)
            .with_context(|| format!("no check {hash}"))?,
    };
    let held = [check.item.as_str()].into_iter().filter(|i| !i.is_empty());
    let names = crate::names::Names::load(romfs, held)?;
    let placement = Placement {
        check: &check,
        item: &check.item,
    };
    Ok(Tracker::new(gd, &[Spot::new(&placement, &names)])?.flag(hash))
}

/// Every check that holds one of `items` in a vanilla game, as a spot
/// holding it, so the tracker reads what each sets once done.
pub(crate) fn holding(romfs: &Romfs, items: &[&Name]) -> Result<Vec<Spot>> {
    let wanted = |c: &ItemCheck| items.iter().any(|i| c.item == i.get());
    let new_game = NewGame::from_settings(&every_shuffle(), &[])?;
    // What no setting deals comes after, as a pickup placed as itself. A
    // check is listed once, though each age's map has a copy.
    let census = census(romfs, &breakables(romfs)?)?;
    let mut found: Vec<ItemCheck> = Vec::new();
    for c in checks::dealt(romfs, &new_game)?.into_iter().chain(census) {
        if wanted(&c) && !found.iter().any(|f| f.hash == c.hash) {
            found.push(c);
        }
    }
    let names = crate::names::Names::load(romfs, items.iter().map(|i| i.get()))?;
    let spot = |check: &ItemCheck| {
        let placement = Placement {
            check,
            item: &check.item,
        };
        Spot::new(&placement, &names)
    };
    Ok(found.iter().map(spot).collect())
}

/// What the tracker reads once check `hash` is done in a seed whose spots
/// are `spots`, as the app's client does: a check dealt another item can
/// tick another flag (a Gold Skulltula's own pickup Bool, not its token's
/// key). [`flag`]'s vanilla answer for a check the seed doesn't hold.
pub fn seed_flag(romfs: &Romfs, gd: &GameData, spots: &[Spot], hash: u64) -> Result<Option<Flag>> {
    match spots.iter().find(|s| s.hash == hash) {
        Some(spot) => Ok(Tracker::new(gd, std::slice::from_ref(spot))?.flag(hash)),
        None => flag(romfs, gd, hash),
    }
}

/// Every placement that holds something, dealt or not: what drops a
/// fixed item, every gift and scrub, every breakable, whose drop is
/// random unless placed, and every pickup placed as itself.
fn census(romfs: &Romfs, breakable: &BTreeSet<String>) -> Result<Vec<ItemCheck>> {
    let mut out = checks::scan(romfs)?;
    out.extend(gifts::checks());
    out.extend(scrubs::checks());
    out.extend(pickups::LOOSE.checks(romfs)?);
    // The rocks over grottos, which no seed deals.
    out.extend(pickups::BOULDERS.checks(romfs)?);
    out.extend(pickups::RED_BOULDERS.checks(romfs)?);
    out.extend(pickups::SILVER_ROCKS.checks(romfs)?);
    let placed: BTreeSet<u64> = out.iter().map(|c| c.hash).collect();
    let pickups = placed_pickups(romfs)?;
    for (file, doc) in load_bancs(romfs, banc_files(romfs)?) {
        let doc = doc?;
        for actor in actors(&doc.root)? {
            let holder = str_field(actor, GYAML).unwrap_or_default();
            let hash = hash_of(actor).context("actor without Hash")?;
            let holds = breakable.contains(holder) || pickups.contains(holder);
            if holds && !placed.contains(&hash) {
                out.push(ItemCheck {
                    file: file.clone(),
                    area: crate::checks::area_of(&file).to_owned(),
                    hash,
                    holder: holder.to_owned(),
                    item: String::new(),
                    get_event: None,
                    substitute: None,
                    presence: crate::world::presence_of(actor),
                    watched: crate::checks::Watched::No,
                    room: None,
                });
            }
        }
    }
    Ok(out)
}

/// A pack's drop tables. Every pack has `Default`, chests included.
const DROPS: &str = "Component/DropParam/";
/// What a pickup's pack gives Link when he takes it.
const POUCH: &str = "Component/PouchContentParam/";

/// Every placed object that is a pickup itself: its pack says what Link
/// gets for taking it. So a pickup no kind knows still gets a row, and a
/// reason when it's left out. Silver rupees count toward their room, not
/// the pouch, so they're named.
fn placed_pickups(romfs: &Romfs) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::from([actor::OBJ_RUPEE_TOKEN_SILVER.get().to_owned()]);
    for name in placed_objects(romfs)? {
        let Ok(pack) = ActorPack::load(romfs, &name) else {
            continue;
        };
        if pack.files().iter().any(|f| f.starts_with(POUCH)) {
            out.insert(name);
        }
    }
    Ok(out)
}

/// The name of every `Obj_` actor any Banc file places.
fn placed_objects(romfs: &Romfs) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for (_, doc) in load_bancs(romfs, banc_files(romfs)?) {
        for actor in actors(&doc?.root)? {
            let name = str_field(actor, GYAML).unwrap_or_default();
            if name.starts_with("Obj_") {
                names.insert(name.to_owned());
            }
        }
    }
    Ok(names)
}

/// The placed objects Link breaks or lifts for a drop: their pack has a
/// drop table of its own, and either lifts or is a kind pickups already
/// deals. Grass Link can only cut does neither.
fn breakables(romfs: &Romfs) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for name in placed_objects(romfs)? {
        let Ok(pack) = ActorPack::load(romfs, &name) else {
            continue;
        };
        let files = pack.files();
        let has = |part: &str| files.iter().any(|f| f.starts_with(part));
        let drops = (files.iter())
            .any(|f| f.starts_with(DROPS) && !f.starts_with(&format!("{DROPS}Default.")));
        if drops && (has("Component/LiftableParam/") || pickups::broken(&name)) {
            out.insert(name);
        }
    }
    Ok(out)
}

/// The holder kind, as the app groups them.
fn kind(c: &ItemCheck) -> &'static str {
    if gifts::is_prize(c) {
        "prize"
    } else if scrubs::of(c).is_some() {
        "scrub"
    } else if shops::is_shop(c) {
        "shop"
    } else if cows::is_cow(c) {
        "cow"
    } else if pickups::is_pickup(c) || c.item.is_empty() {
        "pickup"
    } else if gifts::of(c).is_some() {
        "gift"
    } else if checks::is_chest(&c.holder) {
        "chest"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use ounce_formats::murmur3;

    use super::*;

    /// A Gold Skulltula dealt another item ticks its own pickup Bool, not
    /// the token's key a vanilla one sets.
    #[test]
    fn a_seed_s_skulltula_holding_another_item_ticks_its_own_flag() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let skulltula = &pickups::SKULLTULAS.checks(&romfs).unwrap()[0];
        let rupee = actor::T_BOX_RUPEE_RED.get();
        let names = crate::names::Names::load(&romfs, [rupee]).unwrap();
        let placement = Placement {
            check: skulltula,
            item: rupee,
        };
        let spots = [Spot::new(&placement, &names)];
        let vanilla = flag(&romfs, &gd, skulltula.hash).unwrap();
        let seeded = seed_flag(&romfs, &gd, &spots, skulltula.hash).unwrap();
        let picked = murmur3(crate::picked::flag(skulltula.hash).as_bytes());
        assert_eq!(seeded, Some(Flag::Bool(picked)));
        assert_ne!(seeded, vanilla);
        assert_eq!(
            seed_flag(&romfs, &gd, &[], skulltula.hash).unwrap(),
            vanilla
        );
    }

    /// Every check ticks in a shuffled seed too, not just with its vanilla
    /// item: some flags hang on what a check holds (a scrub's sale, a
    /// lying item, a Gold Skulltula's token).
    #[test]
    fn every_check_in_a_seed_ticks() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let new_game = NewGame::from_settings(&every_shuffle(), &[]).unwrap();
        let dealt = checks::dealt(&romfs, &new_game).unwrap();
        let fixed = Fixed::new();
        for seed in 0..3 {
            let deal = shuffle::deal(&dealt, &fixed, &new_game).unwrap();
            let placements = shuffle::scatter(deal, seed, &new_game).unwrap();
            let held = placements
                .iter()
                .flat_map(|p| [p.check.item.as_str(), p.item]);
            let names = crate::names::Names::load(&romfs, held).unwrap();
            let spots: Vec<Spot> = placements.iter().map(|p| Spot::new(p, &names)).collect();
            let tracker = Tracker::new(&gd, &spots).unwrap();
            let untracked: Vec<_> = (spots.iter())
                .filter(|s| tracker.flag(s.hash).is_none())
                .map(|s| (s.hash, &s.holder, &s.item))
                .collect();
            assert!(untracked.is_empty(), "seed {seed}: {untracked:?}");
        }
    }

    #[test]
    fn every_shuffle_moves_the_keys_and_deals_the_trade_chain() {
        let on = every_shuffle();
        assert_eq!(on.get(settings::KEYS), Some(&"singles".into()));
        assert_eq!(on.get(settings::FORTRESS_KEYS), Some(&"singles".into()));
        assert_eq!(on.get(settings::TRADE_CHAIN), Some(&true.into()));
    }

    /// With no logic, every dungeon's small key copies land in their own
    /// dungeon, and key copies land in every kind of holder: logic is all
    /// that keeps them out (EIE row 2).
    #[test]
    fn with_no_logic_small_keys_land_anywhere() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let new_game = NewGame {
            logic: settings::Logic::NoLogic,
            ..NewGame::from_settings(&every_shuffle(), &[]).unwrap()
        };
        let dealt = checks::dealt(&romfs, &new_game).unwrap();
        let fixed = Fixed::new();
        let rings: Vec<_> = (1..=crate::keys::DUNGEONS.len())
            .map(crate::key_tints::ring)
            .collect();
        const KINDS: [&str; 7] = ["chest", "pickup", "gift", "scrub", "shop", "cow", "prize"];
        let (mut home, mut kinds) = (BTreeSet::new(), BTreeSet::new());
        // Until every dungeon's key has come home and every kind held one:
        // a dungeon of one key (Ganon's Castle) can miss home 30 seeds running.
        for seed in 0..100 {
            if home.len() == rings.len() && KINDS.iter().all(|k| kinds.contains(k)) {
                break;
            }
            let deal = shuffle::deal(&dealt, &fixed, &new_game).unwrap();
            for p in shuffle::scatter(deal, seed, &new_game).unwrap() {
                let Some(n) = rings.iter().position(|r| *r == p.item) else {
                    continue;
                };
                if crate::keys::dungeon_in(&p.check.area) == Some(n + 1) {
                    home.insert(n + 1);
                }
                kinds.insert(kind(p.check));
            }
        }
        assert_eq!(
            home.len(),
            rings.len(),
            "dungeons whose keys came home: {home:?}"
        );
        for k in KINDS {
            assert!(kinds.contains(k), "no small key in a {k}: {kinds:?}");
        }
    }

    /// The ledger's three rules: every dealt check ticks on the tracker,
    /// every left-out placement has a reason, and the EIE debt is the
    /// baseline, so paying some off lowers the baseline in the same PR.
    #[test]
    fn accounting() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let ledger = build(&romfs).unwrap();
        let dealt = ledger.rows.iter().filter(|r| r.left_out.is_none());
        let untracked: Vec<_> = dealt
            .filter(|r| r.tracker.is_none())
            .map(|r| r.hash)
            .collect();
        assert_eq!(untracked, [0u64; 0], "dealt checks with no tracker flag");
        // `dev checked` finds each kind's flag as the tracker column does.
        let gd = GameData::load(&romfs).unwrap();
        let mut kinds = BTreeSet::new();
        for r in ledger.rows.iter().filter(|r| kinds.insert(r.kind)) {
            let one = flag(&romfs, &gd, r.hash).unwrap().map(|f| f.to_string());
            assert_eq!(one, r.tracker, "{} ({:?})", r.hash, r.kind);
        }
        let unknown: Vec<_> = (ledger.rows.iter())
            .filter(|r| r.left_out == Some(Why::Unknown))
            .map(|r| (r.hash, &r.holder))
            .collect();
        assert!(
            unknown.is_empty(),
            "left out for no reason (NOT_HOLDERS): {unknown:?}"
        );
        // Every loose pickup is dealt, with logic.
        let loose: Vec<_> = (ledger.rows.iter())
            .filter(|r| pickups::LOOSE.places(&r.holder))
            .collect();
        assert_eq!(loose.len(), 140);
        assert!(
            loose
                .iter()
                .all(|r| r.left_out.is_none() && !r.logic.is_empty())
        );
        assert_eq!(
            Debt::of(&ledger),
            BASELINE,
            "the EIE debt moved: update BASELINE"
        );
    }
}
