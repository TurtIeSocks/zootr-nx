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
use crate::narrowing::{self, LeftOut, Let};
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
    refused: 6057,
    left_out: 48,
};

/// EIE #11: the Gerudo thieves' keys, which `fortress-keys` never shuffles.
const THIEF_KEYS: u8 = 11;

/// Placements `left_out` calls no holder that aren't breakables, by
/// holder, and why.
const NOT_HOLDERS: [(&Name, Why); 8] = [
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
    pub logic: Vec<Need>,
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
}

#[derive(Serialize)]
pub struct Need {
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
    /// A setting the player picks: a check kind off, or the Door of Time
    /// open, which skips Rauru's scene.
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

/// Every shuffle on: each toggle the Shuffles and Dungeons panels show,
/// and the dungeon keys moved.
pub fn every_shuffle() -> Settings {
    let shuffles = [group::SHUFFLES, group::DUNGEONS];
    let mut out: Settings = (settings::schema().into_iter())
        .filter(|d| shuffles.contains(&d.group) && matches!(d.kind, Kind::Toggle { .. }))
        .map(|d| (d.id, true.into()))
        .collect();
    out.insert(settings::KEYS.into(), "singles".into());
    out
}

/// The ledger, with every shuffle on.
pub fn build(romfs: &Romfs) -> Result<Ledger> {
    let new_game = NewGame::from_settings(&every_shuffle(), &[])?;
    let dealt = checks::dealt(romfs, &new_game)?;
    let fixed = Fixed::new();
    let deal = shuffle::deal(&dealt, &fixed, &new_game)?;
    let free: BTreeSet<u64> = deal.free.iter().map(|c| c.hash).collect();
    let items: BTreeSet<&str> = deal.items.iter().copied().collect();
    let map_areas: BTreeSet<&str> = (dealt.iter())
        .filter(|c| narrowing::OWN_DUNGEON.iter().any(|m| **m == *c.item))
        .map(|c| c.area.as_str())
        .collect();

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
    for c in others.iter().filter(|c| seen.insert(c.hash)) {
        let why = narrowing::left_out(c, open).map_or(Why::Unknown, Why::of);
        let why = match why {
            Why::Unknown => explain(c, &new_game, &breakable),
            why => why,
        };
        rows.push((c, Some(why)));
    }

    let book = crate::rules::Book::load()?;
    let ctx = crate::rules::Ctx::new(&new_game);
    let world = crate::logic::World::build(romfs, &book, &ctx, &dealt)?;
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
    let sweep: BTreeMap<u64, String> = crate::sweep::plan::plan(romfs, &dealt_spots)?
        .into_iter()
        .map(|p| (p.spot.hash, crate::sweep::night::how(&p.trigger)))
        .collect();

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
            logic: (world.locations.iter())
                .filter(|l| l.hash == c.hash)
                .map(|l| {
                    let (node, era) = &world.places[l.place];
                    Need {
                        era: era.to_string(),
                        node: node.clone(),
                        need: l.need.to_string(),
                    }
                })
                .collect(),
            rule: book.check(c.hash).is_some(),
            never: world.never(c.hash),
            refused: match left_out {
                Some(_) => Vec::new(),
                None => refused(c, &items, &map_areas, &new_game),
            },
            left_out,
            tracker: tracker.flag(c.hash).map(|f| f.to_string()),
            sweep: sweep.get(&c.hash).cloned(),
        })
        .collect();
    Ok(Ledger {
        schema: SCHEMA,
        rows,
    })
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
/// then logic's (keys kept out of their own dungeon), then maps and
/// compasses, which only a chest in an area that had one holds.
fn refused(
    c: &ItemCheck,
    items: &BTreeSet<&str>,
    map_areas: &BTreeSet<&str>,
    new_game: &NewGame,
) -> Vec<Refused> {
    let need = crate::keys::dungeon_need(&c.file, new_game.keys);
    let own_key = |item: &str| matches!(&need, Some(crate::rules::Need::Items { actors, .. }) if actors.iter().any(|a| a == item));
    let map = |item: &str| {
        narrowing::OWN_DUNGEON.iter().any(|m| **m == *item)
            && !map_areas.iter().any(|a| narrowing::holds_map_from(c, a))
    };
    (items.iter())
        .filter_map(|&item| {
            let eie = match narrowing::refusal(c, item, new_game.progressive) {
                Some(n) => n.number(),
                None if own_key(item) => 2,
                None if map(item) => 3,
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
/// for: a breakable, a thief's key, a gift a setting keeps out, or one of
/// [`NOT_HOLDERS`].
fn explain(c: &ItemCheck, new_game: &NewGame, breakable: &BTreeSet<String>) -> Why {
    if breakable.contains(&c.holder) {
        return Why::Eie { eie: BREAKABLES };
    }
    if crate::fortress::is_carried(c) {
        return Why::Eie { eie: THIEF_KEYS };
    }
    if gifts::of(c).is_some_and(|g| !g.dealt(&new_game.on)) {
        return Why::Setting;
    }
    (NOT_HOLDERS.iter())
        .find(|(holder, _)| **holder == *c.holder)
        .map_or(Why::Unknown, |(_, why)| why.clone())
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

/// Every placement that holds something, dealt or not: what drops a
/// fixed item, every gift and scrub, and every breakable, whose drop
/// is random unless placed.
fn census(romfs: &Romfs, breakable: &BTreeSet<String>) -> Result<Vec<ItemCheck>> {
    let mut out = checks::scan(romfs)?;
    out.extend(gifts::checks());
    out.extend(scrubs::checks());
    let placed: BTreeSet<u64> = out.iter().map(|c| c.hash).collect();
    for (file, doc) in load_bancs(romfs, banc_files(romfs)?) {
        let doc = doc?;
        for actor in actors(&doc.root)? {
            let holder = str_field(actor, GYAML).unwrap_or_default();
            let hash = hash_of(actor).context("actor without Hash")?;
            if breakable.contains(holder) && !placed.contains(&hash) {
                out.push(ItemCheck {
                    file: file.clone(),
                    area: file.split('/').nth(1).unwrap_or("?").to_owned(),
                    hash,
                    holder: holder.to_owned(),
                    item: String::new(),
                    get_event: None,
                    substitute: None,
                    presence: crate::world::presence_of(actor),
                    watched: false,
                });
            }
        }
    }
    Ok(out)
}

/// A pack's drop tables. Every pack has `Default`, chests included.
const DROPS: &str = "Component/DropParam/";

/// The placed objects Link breaks or lifts for a drop: their pack has a
/// drop table of its own, and either lifts or is a kind pickups already
/// deals. Grass Link can only cut does neither.
fn breakables(romfs: &Romfs) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for (_, doc) in load_bancs(romfs, banc_files(romfs)?) {
        for actor in actors(&doc?.root)? {
            let name = str_field(actor, GYAML).unwrap_or_default();
            if name.starts_with("Obj_") {
                names.insert(name.to_owned());
            }
        }
    }
    let mut out = BTreeSet::new();
    for name in names {
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
    } else if c.holder.starts_with("Obj_TBox_") {
        "chest"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shuffle_moves_the_keys_and_deals_the_trade_chain() {
        let on = every_shuffle();
        assert_eq!(on.get(settings::KEYS), Some(&"singles".into()));
        assert_eq!(on.get(settings::TRADE_CHAIN), Some(&true.into()));
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
        assert_eq!(
            Debt::of(&ledger),
            BASELINE,
            "the EIE debt moved: update BASELINE"
        );
    }
}
