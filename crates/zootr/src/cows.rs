//! Cows as checks: Epona's Song at each cow gives the seed's item once,
//! then milk as before.
//!
//! The game places 8 cows, each once per era's file, so 14 placements. The
//! 5 plain cows share one pack and one song event, so each gets a clone of
//! the pack naming its own event. The 3 named cows already have their own.
//! Every cow's entry asks its own GameData Bool first. How the flows change
//! lives in `flow`, the packs, rows and Bools in `write`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Romfs, pack_path};

use crate::checks::{
    GYAML, ItemCheck, actors, banc_files, flat, hash_of, load_banc, str_field, vec3,
};
use crate::flows;
use crate::gifts;
use crate::known::{Name, actor, field, item};
use crate::standins::{TABLE, component, path_of, resolved};

mod flow;
#[cfg(test)]
mod tests;
mod write;

pub use flow::give;
pub use write::{fields, flags, write};

/// Every cow actor's name starts with this, the plain one's whole name.
const PLAIN: &Name = actor::ANIMAL_COW;
/// The flow every cow's milk runs through.
const FLOW: &Name = actor::ANIMAL_COW;
/// How far apart, measured flat, two files may place the same cow.
const NEAR: f32 = 1.0;
/// What a cow brings to the pool: its milk, which goes in a chest as a new
/// bottle.
pub const VANILLA: &Name = item::BOTTLE_MILK;
pub const HOLDER: &str = "Cow";
/// The song a cow wants played to it.
pub const MELODY: &Name = item::MELODY_EPONA_SONG;
/// Names for the cows that share an area with another, or stand somewhere
/// worth naming, by check hash. Any other cow is plain `HOLDER`, its area
/// enough to tell it apart.
const NAMES: [(u64, &str); 5] = [
    (6253945510651149508, "Grotto Cow"),
    (4825657261695689152, "Grotto Cow"),
    (2139721051512122393, "Lon Lon Ranch Cow 1"),
    (5665703420335364217, "Lon Lon Ranch Cow 2"),
    (11142894434822030727, "Link's House Cow"),
];
/// The pack component naming the event Epona's Song runs.
const REPLACE: &Name = field::REPLACE_EVENT_PARAM;
const EVENT_NAME: &Name = crate::known::flow::EVENT_NAME;

/// One placement of a cow, in one file.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub actor: String,
    /// The Banc file, like `Banc/KakarikoVillage/KakarikoVillage_Child_Dynamic.bcett.byml.zs`.
    pub file: String,
    pub hash: u64,
    pub pos: [f32; 3],
}

/// One cow, with every placement of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Cow {
    /// Its place among the cows, in the order of their check hashes.
    pub index: usize,
    /// By hash.
    pub placements: Vec<Placement>,
    /// The `EventSetting` row Epona's Song runs at it, once written.
    pub event: String,
}

/// Whether `file` is a child map's.
fn child(file: &str) -> bool {
    file.contains("_Child_")
}

impl Cow {
    /// The check's hash: its lowest child-file placement hash, else its
    /// lowest.
    pub fn hash(&self) -> u64 {
        self.check().hash
    }

    /// The placement whose hash is the check's.
    pub fn check(&self) -> &Placement {
        let all = self.placements.iter();
        (all.clone()
            .filter(|p| child(&p.file))
            .min_by_key(|p| p.hash))
        .or_else(|| all.min_by_key(|p| p.hash))
        .expect("a cow has a placement")
    }

    pub fn actor(&self) -> &str {
        &self.placements[0].actor
    }

    /// Its Banc folder.
    pub fn area(&self) -> &str {
        area(&self.placements[0].file)
    }

    /// Whether it's one of the cows sharing the plain pack.
    pub fn plain(&self) -> bool {
        self.actor() == PLAIN
    }

    /// The GameData Bool set once it's given its item.
    pub fn flag(&self) -> String {
        format!("Rando_Cow_{:02}", self.index)
    }

    /// The clone pack a plain cow is swapped to.
    pub fn clone_name(&self) -> String {
        format!("{PLAIN}_Rando_{:02}", self.index)
    }

    pub fn holder(&self) -> String {
        let named = NAMES.iter().find(|(hash, _)| *hash == self.hash());
        named.map_or(HOLDER, |(_, name)| name).to_owned()
    }
}

fn area(file: &str) -> &str {
    file.split('/').nth(1).unwrap_or("?")
}

/// Placements of one actor in one area within `NEAR` of each other are one
/// cow. Numbers the cows in the order of their check hashes.
fn group(placed: Vec<Placement>) -> Vec<Cow> {
    let mut groups: Vec<Vec<Placement>> = Vec::new();
    for p in placed {
        let same = |g: &&mut Vec<Placement>| {
            g[0].actor == p.actor
                && area(&g[0].file) == area(&p.file)
                && flat(g[0].pos, p.pos) <= NEAR
        };
        match groups.iter_mut().find(same) {
            Some(g) if g.iter().any(|q| q.hash == p.hash) => {}
            Some(g) => g.push(p),
            None => groups.push(vec![p]),
        }
    }
    let mut cows: Vec<Cow> = groups
        .into_iter()
        .map(|mut placements| {
            placements.sort_by_key(|p| p.hash);
            Cow {
                index: 0,
                placements,
                event: String::new(),
            }
        })
        .collect();
    cows.sort_by_key(Cow::hash);
    for (i, cow) in cows.iter_mut().enumerate() {
        cow.index = i;
    }
    cows
}

/// The `EventSetting` row Epona's Song runs at `pack`'s actor.
fn event_of(pack: &ActorPack) -> Result<String> {
    let table = resolved(pack, &path_of(pack, TABLE)?)?;
    let replace =
        component(&table, REPLACE).with_context(|| format!("{} has no {REPLACE}", pack.name))?;
    let replace = resolved(pack, &pack_path(&replace))?;
    let name = replace
        .get(&**EVENT_NAME)
        .and_then(|n| n.as_string().ok())
        .with_context(|| format!("{} names no event", pack.name))?;
    let stem = name.rsplit('/').next().unwrap_or(name);
    Ok(stem.split('.').next().unwrap_or(stem).to_owned())
}

/// Every cow the game places, each with the event its song will run.
pub fn find(romfs: &Romfs) -> Result<Vec<Cow>> {
    let mut placed = Vec::new();
    for file in banc_files(romfs)? {
        let doc = load_banc(romfs, &file)?;
        for a in actors(&doc.root)? {
            let Some(actor) = str_field(a, GYAML).filter(|g| g.starts_with(&**PLAIN)) else {
                continue;
            };
            placed.push(Placement {
                actor: actor.to_owned(),
                file: file.clone(),
                hash: hash_of(a).context("a cow without a hash")?,
                pos: vec3(a, "Translate").unwrap_or_default(),
            });
        }
    }
    let mut cows = group(placed);
    let mut events: HashMap<String, String> = HashMap::new();
    for cow in &mut cows {
        cow.event = match cow.plain() {
            true => format!("GiveMilk_Rando_{:02}", cow.index),
            false => match events.get(cow.actor()) {
                Some(event) => event.clone(),
                None => {
                    let event = event_of(&ActorPack::load(romfs, cow.actor())?)?;
                    events.insert(cow.actor().to_owned(), event.clone());
                    event
                }
            },
        };
    }
    Ok(cows)
}

/// Whether `holder` is a cow's: `HOLDER`, last or before a number.
pub fn is_holder(holder: &str) -> bool {
    let name = holder
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();
    name == HOLDER || name.ends_with(&format!(" {HOLDER}"))
}

pub fn is_cow(check: &ItemCheck) -> bool {
    is_holder(&check.holder)
}

/// One check per cow.
pub fn checks(cows: &[Cow]) -> Vec<ItemCheck> {
    cows.iter()
        .map(|c| gifts::check(flows::rel(FLOW), c.area(), c.hash(), c.holder(), VANILLA))
        .collect()
}

/// The cow checks a new game deals: all of them with the setting on.
pub fn dealt(romfs: &Romfs, on: bool) -> Result<Vec<ItemCheck>> {
    match on {
        true => Ok(checks(&find(romfs)?)),
        false => Ok(Vec::new()),
    }
}

/// The item the seed gives at `cow`.
fn item<'a>(drops: &'a HashMap<u64, crate::checks::Drop>, cow: &Cow) -> &'a str {
    gifts::item_at(drops, cow.hash(), VANILLA)
}
