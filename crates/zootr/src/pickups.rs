//! Placed actors that hold an item as a check without a chest: freestanding
//! heart pieces, Gold Skulltulas, whose kill drops their token, the
//! pots, crates, rocks, grass, hives and flying pots that drop something
//! when broken, the Gerudo thieves' keys, and the rupees, hearts and other
//! pickups lying loose. One
//! that holds something else gets a pickup for it, a stand-in built on a
//! pickup that gives a major item when no `Obj_*` form exists.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, ModBuilder, Romfs};

use crate::checks::{
    DROP_ACTOR, Drop, GYAML, ItemCheck, Watched, actors, banc_files, hash_of, load_bancs, room_of,
    str_field,
};
use crate::gifts;
use crate::known::{Name, actor, field as fields, item};
use crate::per_dump::PerDump;
use crate::settings::{NPC_GIFTS, NewGame};
use crate::standins::{StandIn, Tables, item_pack};
use crate::world::presence_of;

mod drops;
pub mod groups;
pub mod nodes;
mod shared;
mod stale;
pub mod steps;
pub mod taken;

use drops::{own_drop, pickup_for, template_for};

/// One kind of placed holder.
#[derive(PartialEq)]
pub struct Kind {
    /// The placed actors.
    pub actors: &'static [&'static Name],
    /// What each brings to the pool, or none for what `field` holds.
    pub item: Option<&'static Name>,
    /// The placement field that names what's there.
    field: &'static str,
    /// The ability getting at one needs, beyond reaching it.
    pub needs: Option<&'static str>,
    /// Only the placements named so, which each hold their own actor.
    named: Option<Named>,
}

/// The placements of a kind with a `Name` beginning `prefix`, shown as
/// `holder`.
#[derive(PartialEq)]
struct Named {
    prefix: &'static str,
    holder: &'static str,
}

/// The 14 heart pieces lying in the world. A piece's `Gyaml` is the pickup.
/// The Gerudo Valley one is its own actor.
pub const FREESTANDING: Kind = Kind {
    actors: &[actor::OBJ_HEART_PIECE, actor::OBJ_GERUDO_VALLEY_HEART_PIECE],
    item: Some(actor::OBJ_HEART_PIECE),
    field: GYAML,
    needs: None,
    named: None,
};

/// The 100 Gold Skulltulas. A kill drops the placement's `Drop__DropActor`
/// in place of the token, so the pool holds a token for each and the House
/// of Skulltula's count still adds up.
pub const SKULLTULAS: Kind = Kind {
    actors: &[actor::ENEMY_SKULLTULA_GOLD],
    item: Some(item::TOKEN_GOLD),
    field: DROP,
    needs: Some("gold-skulltula"),
    named: None,
};

/// The 8 boss heart containers, placed in each dungeon's boss room.
/// `logic/bosses.toml` puts each past its boss door.
pub const BOSS_HEARTS: Kind = Kind {
    actors: &[actor::OBJ_HEART_CONTAINER],
    item: Some(actor::OBJ_HEART_CONTAINER),
    field: GYAML,
    needs: None,
    named: None,
};

/// Every pot, each with its [`own_drop`]. The `_Respawn` ones come back
/// with their area. The remix's files stay out ([`REMIX`]), and so do the
/// `_Dream` copies of the pots before Ganondorf: only the boss rush, which
/// reuses that room, places them (the maintainer's call, EIE spec).
pub const POTS: Kind = Kind {
    actors: &[
        actor::OBJ_POT_A_01,
        actor::OBJ_POT_A_02,
        actor::OBJ_POT_A_03,
        actor::OBJ_POT_A_04,
        actor::OBJ_POT_A_05,
        actor::OBJ_POT_A_06,
        actor::OBJ_POT_A_07,
        actor::OBJ_POT_A_08,
        actor::OBJ_POT_A_02_RESPAWN,
        actor::OBJ_POT_A_05_RESPAWN,
        actor::OBJ_POT_A_07_RESPAWN,
        actor::OBJ_POT_A_08_RESPAWN,
    ],
    item: None,
    field: DROP,
    needs: None,
    named: None,
};

/// Wooden crates, which break to a rolling Link. The small ones he lifts
/// ([`actor::OBJ_BROKEN_BOX_A_03`], 4 in the Fire Temple and 2 in
/// Jabu-Jabu's Belly) stay out: they have no Life, so nothing breaks them
/// and nothing they hold could be got (the maintainer's call, EIE row 17).
pub const CRATES: Kind = Kind {
    actors: &[actor::OBJ_BROKEN_BOX_A_01],
    item: None,
    field: DROP,
    needs: None,
    named: None,
};

/// Small rocks Link lifts and throws, both of the light grade. The rock
/// that needs gauntlets stays out.
pub const ROCKS: Kind = Kind {
    actors: &[actor::OBJ_LIFT_ROCK_A_01, actor::OBJ_LIFT_ROCK_A_03],
    item: None,
    field: DROP,
    needs: None,
    named: None,
};

/// Grass shrubs Link cuts or lifts, like those outside Saria's house.
pub const GRASS: Kind = Kind {
    actors: &[
        actor::OBJ_PLANT_GRASS_A_01,
        actor::OBJ_PLANT_GRASS_A_01_RESPAWN,
        actor::OBJ_PLANT_GRASS_A_03,
        // Hyrule Field's shrubs, whose own table drops nothing.
        actor::LAND_KOKIRI_PLANT_A_02_DROP_FIXED_03,
    ],
    item: None,
    field: DROP,
    needs: None,
    named: None,
};

/// Flying pots, enemies that break against Link and drop as pots do. The
/// pots setting deals them.
pub const FLYING_POTS: Kind = Kind {
    actors: &[
        actor::ENEMY_FLYING_POT_ICE_CAVERN,
        actor::ENEMY_FLYING_POT_SHADOW_TEMPLE,
        actor::ENEMY_FLYING_POT_SPIRIT_TEMPLE,
    ],
    item: None,
    field: DROP,
    needs: None,
    named: None,
};

/// Kakariko's beehives, which something has to knock down.
pub const HIVES: Kind = Kind {
    actors: &[actor::OBJ_KOKIRI_BEEHIVE_A_01],
    item: None,
    field: DROP,
    needs: Some("knock-down"),
    named: None,
};

/// The four keys the Gerudo thieves carry, as the placements the carpenter
/// flow moves to each thief once it's beaten. `logic` has them at the
/// Fortress, for an adult.
pub const FORTRESS_KEYS: Kind = Kind {
    actors: &[
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_01,
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_02,
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_03,
        actor::OBJ_DUNGEON_SMALL_KEY_GERUDO_FORTRESS_04,
    ],
    item: None,
    field: GYAML,
    needs: Some("adult"),
    named: Some(Named {
        prefix: "SmallKeyForEvent_",
        holder: THIEF,
    }),
};

/// The two small keys lying in the open, Bottom of the Well's and the
/// Shadow Temple's, checks whenever keys move as the key chests are.
pub const LOOSE_KEYS: Kind = Kind {
    actors: &[actor::OBJ_DUNGEON_SMALL_KEY],
    item: Some(item::DUNGEON_SMALL_KEY),
    field: GYAML,
    needs: None,
    named: None,
};

/// The Fire Arrows at Lake Hylia, a pickup a logic module spawns after the
/// sun is shot. `logic/gifts.toml` puts them past the bow and Morpha.
pub const FIRE_ARROWS: Kind = Kind {
    actors: &[actor::OBJ_ARROW_FIRE],
    item: Some(item::ARROW_FIRE),
    field: GYAML,
    needs: None,
    named: None,
};

/// Ruto's letter at the bottom of Lake Hylia, spawned by a logic module until
/// the challenge for the Water stone is set. `logic/gifts.toml` puts it past
/// the Silver Scale.
pub const RUTOS_LETTER: Kind = Kind {
    actors: &[actor::OBJ_BOTTLE_LETTER],
    item: Some(item::BOTTLE_LETTER),
    field: GYAML,
    needs: None,
    named: None,
};

/// Rupees, Recovery Hearts and the other pickups lying in the open, each
/// its own pickup, taken once as the game has it (`NoRespawn`): the three
/// hearts under the Deku Tree's water, and the Deku Nut, Seed and Stick on
/// child Lake Hylia's bed, which `logic/hyrule-field.toml` puts past the
/// Silver Scale. The loose items setting deals them.
pub const LOOSE: Kind = Kind {
    actors: &[
        actor::OBJ_RUPEE_GREEN,
        actor::OBJ_RUPEE_BLUE,
        actor::OBJ_RUPEE_RED,
        actor::OBJ_RUPEE_PURPLE,
        actor::OBJ_HEART,
        actor::OBJ_HEART_IN_WATER,
        actor::OBJ_DEKU_NUT,
        actor::OBJ_DEKU_SEED,
        actor::OBJ_DEKU_STICK,
    ],
    item: None,
    field: GYAML,
    needs: None,
    named: None,
};

/// The holders Link breaks for what they drop.
const BREAKABLE: [&Kind; 4] = [&POTS, &CRATES, &ROCKS, &GRASS];

/// Whether one of `kinds` places `holder`.
fn any_holds(kinds: &[&Kind], holder: &str) -> bool {
    kinds.iter().any(|k| k.places(holder))
}

/// Whether the sweep takes what `holder` holds by walking up to it: a
/// pickup lying in the open, which A takes, or one of [`BREAKABLE`].
pub fn within_reach(holder: &str) -> bool {
    any_holds(&[&FREESTANDING, &BOSS_HEARTS, &LOOSE_KEYS, &LOOSE], holder) || broken(holder)
}

/// Whether `holder` is broken for what it drops: a pot, crate, rock or
/// shrub.
pub fn broken(holder: &str) -> bool {
    any_holds(&BREAKABLE, holder)
}

const KINDS: [&Kind; 14] = [
    &FREESTANDING,
    &SKULLTULAS,
    &BOSS_HEARTS,
    &POTS,
    &CRATES,
    &ROCKS,
    &GRASS,
    &FLYING_POTS,
    &HIVES,
    &FORTRESS_KEYS,
    &LOOSE_KEYS,
    &FIRE_ARROWS,
    &RUTOS_LETTER,
    &LOOSE,
];

/// What a thief's key check is held by, in the spoiler.
pub const THIEF: &str = "Gerudo Thief";

/// The pack of AI graphs no actor carries itself, like the drop graph.
pub const GLOBAL_AI: &str = "Pack/AI.Global.Product.111.pack.zs";
const DROP: &str = "Dynamic.Drop__DropActor";
const NAME: &str = "Name";
/// A drop that's a creature (bugs from some shrubs and a rock), not an
/// item Link gets. Its holder counts as having no fixed drop, as Ship's
/// grass brings a rupee rather than its bugs.
const ANIMAL: &str = "Animal_";
/// The remix's Dream World, outside the story a seed plays.
const REMIX: &str = "Banc/DreamWorld/";

/// Each stand-in's actor is this plus its holder's number.
const NEW: &str = "Obj_Rando_";

impl Kind {
    /// Whether `actor` is one of the placed actors.
    pub(crate) fn places(&self, actor: &str) -> bool {
        self.actors.iter().any(|a| **a == *actor)
    }

    /// Whether a placement with no fixed drop brings its [`own_drop`].
    fn owns_drop(&self) -> bool {
        self.item.is_none() && self.field == DROP
    }

    /// One check per placement, as chests are.
    pub fn checks(&self, romfs: &Romfs) -> Result<Vec<ItemCheck>> {
        let i = (KINDS.iter().position(|k| *k == self)).context("a kind outside KINDS")?;
        Ok(by_kind(romfs)?[i].clone())
    }

    /// The check `actor`, placed in Banc file `file`, is, if it's one of
    /// this kind.
    fn check_of(&self, actor: &Byml, file: &str) -> Result<Option<ItemCheck>> {
        let gyaml = str_field(actor, GYAML).unwrap_or_default();
        if !self.places(gyaml) {
            return Ok(None);
        }
        let dynamic = actor.as_map()?.get(crate::checks::DYNAMIC);
        let held =
            (dynamic.and_then(|d| str_field(d, DROP_ACTOR))).filter(|d| !d.starts_with(ANIMAL));
        let table = dynamic.and_then(|d| str_field(d, fields::DROP_DROP_TABLE));
        let own = self.owns_drop().then(|| own_drop(table, file).get());
        let (holder, item) = match &self.named {
            Some(Named { prefix, holder }) => {
                if !str_field(actor, NAME).is_some_and(|n| n.starts_with(prefix)) {
                    return Ok(None);
                }
                (*holder, gyaml)
            }
            None => {
                // A placement that is its own pickup holds itself.
                let itself = (self.field == GYAML).then_some(gyaml);
                let Some(item) = self.item.map(|i| i.get()).or(held).or(own).or(itself) else {
                    return Ok(None);
                };
                (gyaml, item)
            }
        };
        let hash = hash_of(actor).context("actor without Hash")?;
        let area = crate::checks::area_of(file);
        let mut check = gifts::check(file.into(), area, hash, holder.into(), item);
        // A named holder is there once its event has put it there,
        // which the seed doesn't have to wait for.
        check.presence = presence_of(actor).filter(|_| self.named.is_none());
        check.room = room_of(actor);
        Ok(Some(check))
    }

    pub fn holds(&self, check: &ItemCheck) -> bool {
        let named = self.named.as_ref().map(|n| n.holder);
        self.places(&check.holder) || named == Some(check.holder.as_str())
    }
}

/// The checks of each of [`KINDS`], in its order. Kept per dump: a seed
/// asks for them several times over, and reading every Banc file is slow.
fn by_kind(romfs: &Romfs) -> Result<Arc<Vec<Vec<ItemCheck>>>> {
    static KEPT: PerDump<Arc<Vec<Vec<ItemCheck>>>> = PerDump::new();
    KEPT.get(romfs, || read_kinds(romfs).map(Arc::new))
}

/// [`by_kind`], reading each Banc file once for every kind.
fn read_kinds(romfs: &Romfs) -> Result<Vec<Vec<ItemCheck>>> {
    let mut out = vec![Vec::new(); KINDS.len()];
    let files = (banc_files(romfs)?.into_iter())
        .filter(|f| !f.starts_with(REMIX))
        .collect();
    for (file, doc) in load_bancs(romfs, files) {
        let doc = doc?;
        for actor in actors(&doc.root)? {
            for (kind, checks) in KINDS.iter().zip(&mut out) {
                checks.extend(kind.check_of(actor, &file)?);
            }
        }
    }
    Ok(out)
}

/// The checks of every kind `new_game` turns on.
pub fn dealt(romfs: &Romfs, new_game: &NewGame) -> Result<Vec<ItemCheck>> {
    let npc_gifts = new_game.on.contains(NPC_GIFTS);
    let on = [
        (new_game.freestanding, &FREESTANDING),
        (new_game.loose_items, &LOOSE),
        (new_game.skulltulas, &SKULLTULAS),
        (new_game.boss_hearts, &BOSS_HEARTS),
        (new_game.pots, &POTS),
        (new_game.pots, &FLYING_POTS),
        (new_game.crates, &CRATES),
        (new_game.rocks, &ROCKS),
        (new_game.grass, &GRASS),
        (new_game.hives, &HIVES),
        (new_game.fortress_keys.moved(), &FORTRESS_KEYS),
        // Always, so logic counts them: with vanilla keys they stay put
        // like the key chests.
        (true, &LOOSE_KEYS),
        (npc_gifts, &FIRE_ARROWS),
        // With King Zora moved, the letter is dealt as a bottle, so its
        // spot always deals, or it would hand over a letter nobody reads.
        (
            npc_gifts || new_game.on.contains(crate::presets::KING_ZORA_MOVED),
            &RUTOS_LETTER,
        ),
    ];
    let mut out = Vec::new();
    for (_, kind) in on.into_iter().filter(|(on, _)| *on) {
        out.extend(kind.checks(romfs)?);
    }
    watch(romfs, &mut out)?;
    Ok(out)
}

/// Mark each of `checks` a logic module makes appear or watches.
fn watch(romfs: &Romfs, checks: &mut [ItemCheck]) -> Result<()> {
    let mut watched: HashMap<u64, Watched> = HashMap::new();
    for c in crate::gates::creators(romfs)? {
        let by = if c.quiet {
            Watched::Quiet
        } else {
            Watched::Loud
        };
        let was = watched.entry(c.hash).or_insert(by);
        if by == Watched::Loud {
            *was = by;
        }
    }
    for c in checks {
        c.watched = watched.get(&c.hash).copied().unwrap_or_default();
    }
    Ok(())
}

/// Every pickup check of every kind a logic module makes appear or watches.
pub fn watched(romfs: &Romfs) -> Result<Vec<ItemCheck>> {
    let mut checks = by_kind(romfs)?.concat();
    watch(romfs, &mut checks)?;
    checks.retain(|c| c.watched != Watched::No);
    Ok(checks)
}

pub fn kind_of(check: &ItemCheck) -> Option<&'static Kind> {
    KINDS.into_iter().find(|k| k.holds(check))
}

/// Whether `check` is a holder its event names and puts in place, as a
/// thief's key.
pub fn is_named(check: &ItemCheck) -> bool {
    kind_of(check).is_some_and(|k| k.named.is_some())
}

/// Whether `check` is a placed holder's, not a chest's. Every one can show
/// a line's level or ammo a step at a time ([`steps`]), holding whatever
/// it's dealt. A logic module watching it runs each step's copy too
/// ([`groups`], [`nodes`]), and a thief's call names each step's key
/// (`fortress::give`).
pub fn is_pickup(check: &ItemCheck) -> bool {
    kind_of(check).is_some()
}

/// A holder whose item changed, and the actor its placement field names.
pub struct Moved<'a> {
    pub hash: u64,
    /// What the holder was.
    pub vanilla: &'a str,
    /// What it now holds.
    pub item: &'a str,
    field: &'static str,
    /// The actor placed there: the item's own pickup, or a stand-in built
    /// for it, named by its holder's number.
    pub actor: String,
    /// What a stand-in is built on.
    template: &'static str,
    /// Which step of its holder's it is, when the holder shows one a step
    /// at a time.
    pub step: Option<steps::Step>,
}

impl Moved<'_> {
    /// Whether `actor` is a stand-in that has to be built.
    fn built(&self) -> bool {
        self.actor.starts_with(NEW) || self.actor.starts_with(stale::STEP)
    }
}

/// Every holder in `checks` whose item in `drops` isn't its own, and each
/// step's placement past a holder's own when it shows them a step at a
/// time.
pub fn moved<'a>(
    checks: &'a [ItemCheck],
    drops: &'a HashMap<u64, Drop>,
    progressive: crate::progressive::Lines,
) -> Vec<Moved<'a>> {
    let holders: BTreeSet<(u64, &str, &str)> = checks
        .iter()
        .filter_map(|c| Some((c.hash, c.item.as_str(), kind_of(c)?.field)))
        .collect();
    let mut out = Vec::new();
    for (n, (hash, vanilla, field)) in holders.into_iter().enumerate() {
        let item = gifts::item_at(drops, hash, vanilla);
        if item == vanilla {
            continue;
        }
        let at = |hash, item, actor: String, step| Moved {
            hash,
            vanilla,
            item,
            field,
            actor,
            template: template_for(field),
            step,
        };
        let dropped = field == DROP;
        let Some(steps) = steps::of(hash, item, progressive) else {
            let actor = pickup_for(item, dropped).unwrap_or_else(|| format!("{NEW}{n:03}"));
            out.push(at(hash, item, actor, None));
            continue;
        };
        for (k, (placement, shows, step)) in steps.into_iter().enumerate() {
            let built = || format!("{}{n:03}_{k}", stale::STEP);
            let actor = pickup_for(shows, dropped).unwrap_or_else(built);
            out.push(at(placement, shows, actor, Some(step)));
        }
    }
    out
}

/// Each stand-in built for one holder, by the holder whose gates its
/// take feeds (`taken`).
pub fn stand_ins(
    checks: &[ItemCheck],
    drops: &HashMap<u64, Drop>,
    progressive: crate::progressive::Lines,
) -> Vec<(u64, String)> {
    let moved = moved(checks, drops, progressive).into_iter();
    let built = moved.filter(Moved::built);
    built
        .map(|m| (m.step.as_ref().map_or(m.hash, |s| s.of), m.actor))
        .collect()
}

/// Build a pickup for each holder whose item changed, and return the
/// placement fields, as `HASH.PATH=VALUE`, that place it.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    checks: &[ItemCheck],
    drops: &HashMap<u64, Drop>,
    new_game: &NewGame,
) -> Result<Vec<String>> {
    let mut fields = Vec::new();
    for m in moved(checks, drops, crate::progressive::Lines::of(new_game)) {
        if m.built() {
            let pickup = StandIn {
                name: &m.actor,
                template: ActorPack::load(romfs, m.template)?,
                item: item_pack(romfs, m.item, new_game)?,
                price: None,
            };
            pickup
                .build(builder, tables)
                .with_context(|| format!("{} becoming {}", m.hash, m.item))?;
        }
        fields.push(format!("{}.{}={}", m.hash, m.field, m.actor));
    }
    // One keeping its own drop drops it as dealt too, since its table goes
    // ([`tabled`]). A heart piece lying in place keeps its own field.
    for c in checks
        .iter()
        .filter(|c| kind_of(c).is_some_and(Kind::owns_drop))
    {
        let item = gifts::item_at(drops, c.hash, &c.item);
        if item == c.item && pickup_for(item, true).as_deref() == Some(item) {
            fields.push(format!("{}.{DROP}={item}", c.hash));
        }
    }
    Ok(fields)
}

/// The holders in `checks` whose drop table the seed takes away, so each
/// drops exactly what it was dealt: every one whose kind brings an
/// [`own_drop`].
pub fn tabled(checks: &[ItemCheck]) -> HashSet<u64> {
    (checks.iter())
        .filter(|c| kind_of(c).is_some_and(Kind::owns_drop))
        .map(|c| c.hash)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::drops::{DROPPED, PLACED_ONLY, TEMPLATE};
    use super::*;
    use crate::known::{actor, field, item};

    #[test]
    fn king_zora_moved_deals_the_letter_spot_without_npc_gifts() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let letters = |new_game: &NewGame| {
            let dealt = dealt(&romfs, new_game).unwrap();
            dealt.iter().filter(|c| RUTOS_LETTER.holds(c)).count()
        };
        assert_eq!(letters(&NewGame::default()), 0);
        let moved = NewGame {
            on: [crate::presets::KING_ZORA_MOVED.to_owned()].into(),
            ..NewGame::default()
        };
        assert_eq!(letters(&moved), 1);
    }

    #[test]
    fn a_pot_keeping_its_tables_drop_drops_it_and_loses_the_table() {
        let Some(romfs) = dev_dump() else { return };
        let checks = POTS.checks(&romfs).unwrap();
        let item = |hash: u64| checks.iter().find(|c| c.hash == hash).unwrap().item.clone();
        // Zora's River's pot on the Deku Stick table, and Water Temple pots
        // on the range weapon table and on one picking what Link lacks.
        let (river, range, supply) = (
            13505325356541728049,
            12146339761792216385,
            7331502814510362649,
        );
        assert_eq!(item(river), actor::OBJ_DEKU_STICK.get());
        assert_eq!(item(range), actor::OBJ_ARROW_BUNDLE_SMALL.get());
        assert_eq!(item(supply), actor::OBJ_HEART.get());
        let dir = std::env::temp_dir().join(format!("zootr-own-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let none = HashMap::new();
        let fields = write(
            &romfs,
            &mut builder,
            &mut tables,
            &checks,
            &none,
            &NewGame::rings(),
        );
        let fields = fields.unwrap();
        let stick = format!("{river}.{DROP}={}", actor::OBJ_DEKU_STICK.get());
        assert!(fields.contains(&stick));
        assert!(
            [river, range, supply]
                .iter()
                .all(|h| tabled(&checks).contains(h))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_shrub_dropping_bugs_brings_a_heart_and_drops_it() {
        let Some(romfs) = dev_dump() else { return };
        let checks = GRASS.checks(&romfs).unwrap();
        // A Hyrule Field shrub whose placement drops bugs.
        let shrub = 493105369468884447;
        let check = checks.iter().find(|c| c.hash == shrub).unwrap();
        assert_eq!(check.item, actor::OBJ_HEART.get());
        assert!(checks.iter().all(|c| !c.item.starts_with(ANIMAL)));
        let dir = std::env::temp_dir().join(format!("zootr-bugs-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let none = HashMap::new();
        let fields = write(
            &romfs,
            &mut builder,
            &mut tables,
            &checks,
            &none,
            &NewGame::rings(),
        );
        let heart = format!("{shrub}.{DROP}={}", actor::OBJ_HEART.get());
        assert!(fields.unwrap().contains(&heart));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_a_dropped_template_and_the_drops_lands() {
        let Some(romfs) = dev_dump() else { return };
        let lands = |actor: &str| {
            let pack = ActorPack::load(&romfs, actor).unwrap();
            pack.files().iter().any(|f| f.starts_with(LANDS))
        };
        assert!(lands(DROPPED) && lands(actor::OBJ_RUPEE_GREEN));
        assert!(!lands(TEMPLATE));
        assert!(PLACED_ONLY.iter().all(|p| !lands(p)));
    }
    use crate::game::dev_dump;
    use crate::standins::{
        COUNT_CHECK, FALLS, MODEL, NO_RESPAWN, PARENT, PHYSICS, POUCH, RESPAWN, TABLE, actor_path,
        component,
    };
    use ounce_formats::{Byml, BymlMap, pack_path};
    use roead::sarc::Sarc;

    /// The pack files of a pickup that lands: its contact with the ground.
    const LANDS: &str = "Phive/ContactCollectionEntityParam/HitOnlyGroundAndObject";

    #[test]
    fn every_kind_finds_its_holders() {
        let Some(romfs) = dev_dump() else { return };
        for (kind, count) in [
            (&FREESTANDING, 14),
            (&SKULLTULAS, 100),
            (&BOSS_HEARTS, 8),
            (&POTS, 518),
            (&CRATES, 38),
            (&ROCKS, 167),
            (&GRASS, 160),
            (&FLYING_POTS, 23),
            (&HIVES, 21),
            (&FORTRESS_KEYS, 4),
            (&LOOSE_KEYS, 2),
            (&FIRE_ARROWS, 1),
            (&RUTOS_LETTER, 1),
            (&LOOSE, 140),
        ] {
            let checks = kind.checks(&romfs).unwrap();
            let hashes: BTreeSet<u64> = checks.iter().map(|c| c.hash).collect();
            assert_eq!(hashes.len(), count, "{}", kind.actors[0].get());
            assert!(
                checks
                    .iter()
                    .all(|c| kind.holds(c) && !c.file.starts_with(REMIX))
            );
            assert!(checks.iter().all(|c| kind.item.is_none_or(|i| c.item == i)));
        }
    }

    #[test]
    fn quiet_and_loud_groups_watch_holders_that_step_alike() {
        let Some(romfs) = dev_dump() else { return };
        // Each kind's checks quiet groups watch, then loud ones, then all.
        let watched = |kind: &Kind| {
            let mut checks = kind.checks(&romfs).unwrap();
            watch(&romfs, &mut checks).unwrap();
            let by = |w| checks.iter().filter(|c| c.watched == w).count();
            assert!(checks.iter().all(is_pickup));
            (by(Watched::Quiet), by(Watched::Loud), checks.len())
        };
        // Each boss's heart, the Fire Arrows and Ruto's letter appear once
        // a boss falls or a flag is set, by groups that set flags too.
        assert_eq!(watched(&BOSS_HEARTS), (0, 8, 8));
        assert_eq!(watched(&FIRE_ARROWS), (0, 1, 1));
        assert_eq!(watched(&RUTOS_LETTER), (0, 1, 1));
        assert_eq!(watched(&FREESTANDING), (0, 0, 26));
        // The all-day, nighttime, swaying tree and broken box modules only
        // read state and make the Skulltula and its web.
        assert_eq!(watched(&SKULLTULAS), (101, 3, 114));
        // Most watched pots sit in rooms whose groups set flags or play.
        assert_eq!(watched(&POTS), (25, 24, 518));
    }

    /// The Deku Tree's regrowing shrubs (rooms C and H) are watched quiet:
    /// their modules write only their own autogen GameData. Watched loud,
    /// a twin named beside its holder, kept out by its Presence, hung the
    /// map's load (research: Fire Temple crate freeze).
    #[test]
    fn regrowing_grass_is_watched_quiet() {
        let Some(romfs) = dev_dump() else { return };
        let mut checks = GRASS.checks(&romfs).unwrap();
        watch(&romfs, &mut checks).unwrap();
        for hash in [12536486041203664147, 2540384747854371104] {
            let check = checks.iter().find(|c| c.hash == hash).unwrap();
            assert_eq!(check.watched, Watched::Quiet, "{hash}");
        }
    }

    #[test]
    fn the_lake_pickups_swap_their_actor_only_when_moved() {
        let Some(romfs) = dev_dump() else { return };
        let mut checks = FIRE_ARROWS.checks(&romfs).unwrap();
        checks.extend(RUTOS_LETTER.checks(&romfs).unwrap());
        let [arrows, letter] = [&checks[0], &checks[1]].map(|c| c.hash);
        assert_eq!(checks.len(), 2);
        assert_eq!((arrows, letter), (4598657944866228526, 474136620438465634));
        let dir = std::env::temp_dir().join(format!("zootr-lake-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let moved = |drops: &HashMap<u64, Drop>, builder: &mut ModBuilder, tables: &mut Tables| {
            write(&romfs, builder, tables, &checks, drops, &NewGame::rings()).unwrap()
        };
        assert!(moved(&HashMap::new(), &mut builder, &mut tables).is_empty());
        let same = HashMap::from([
            (arrows, Drop::from(item::ARROW_FIRE.to_string())),
            (letter, Drop::from(item::BOTTLE_LETTER.to_string())),
        ]);
        assert!(moved(&same, &mut builder, &mut tables).is_empty());
        let drops = HashMap::from([
            (arrows, Drop::from(item::HOOKSHOT.to_string())),
            (letter, Drop::from(actor::OBJ_SHIELD_HYLIAN.to_string())),
        ]);
        let fields = moved(&drops, &mut builder, &mut tables);
        assert_eq!(fields.len(), 2, "{fields:?}");
        let arrows_at = format!("{arrows}.{GYAML}={NEW}");
        assert!(
            fields.iter().any(|f| f.starts_with(&arrows_at)),
            "{fields:?}"
        );
        assert!(fields.contains(&format!("{letter}.{GYAML}={}", actor::OBJ_SHIELD_HYLIAN)));
        builder.finish().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Each pickup lying loose is a check holding itself, dealt only with
    /// the loose items setting on: 192 rupee and heart placements, the
    /// Deku Tree's three underwater hearts and Lake Hylia's nut, seed and
    /// stick.
    #[test]
    fn a_loose_pickup_holds_itself_and_its_setting_deals_it() {
        let Some(romfs) = dev_dump() else { return };
        let loose = LOOSE.checks(&romfs).unwrap();
        assert_eq!(loose.len(), 198);
        assert!(loose.iter().all(|c| c.item == c.holder));
        let deals = |on: bool| {
            let mut settings = crate::ledger::every_shuffle();
            settings.insert(crate::settings::LOOSE_ITEMS.into(), on.into());
            let new_game = NewGame::from_settings(&settings, &[]).unwrap();
            let dealt = dealt(&romfs, &new_game).unwrap();
            dealt.iter().filter(|c| LOOSE.holds(c)).count()
        };
        assert_eq!(deals(false), 0);
        assert_eq!(deals(true), 198);
    }

    /// A loose rupee dealt the Hookshot lies as a stand-in, and one dealt
    /// a Green Rupee lies as that rupee. Each keeps its placement hash, so
    /// the game's own key still marks it taken.
    #[test]
    fn a_loose_spot_swaps_its_actor_under_its_own_hash() {
        let Some(romfs) = dev_dump() else { return };
        let checks = LOOSE.checks(&romfs).unwrap();
        let of = |holder: &Name| {
            checks
                .iter()
                .find(|c| c.holder == holder.get())
                .unwrap()
                .hash
        };
        let (green, blue) = (of(actor::OBJ_RUPEE_GREEN), of(actor::OBJ_RUPEE_BLUE));
        let drops = HashMap::from([
            (green, Drop::from(item::HOOKSHOT.to_string())),
            (blue, Drop::from(actor::OBJ_RUPEE_GREEN.to_string())),
        ]);
        let dir = std::env::temp_dir().join(format!("zootr-loose-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let fields = write(
            &romfs,
            &mut builder,
            &mut tables,
            &checks,
            &drops,
            &NewGame::rings(),
        )
        .unwrap();
        let standin = format!("{green}.{GYAML}={NEW}");
        assert!(fields.iter().any(|f| f.starts_with(&standin)), "{fields:?}");
        assert!(fields.contains(&format!("{blue}.{GYAML}={}", actor::OBJ_RUPEE_GREEN)));
        builder.finish().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_changed_holder_gets_a_pickup_of_its_own() {
        let Some(romfs) = dev_dump() else { return };
        let mut checks = FREESTANDING.checks(&romfs).unwrap();
        let mut hashes: Vec<u64> = checks.iter().map(|c| c.hash).collect();
        hashes.sort();
        hashes.dedup();
        let (first, second) = (hashes[0], hashes[1]);
        let skulltulas = SKULLTULAS.checks(&romfs).unwrap();
        let (skulltula, centred) = (skulltulas[0].hash, skulltulas[1].hash);
        checks.extend(skulltulas);
        let drops = HashMap::from([
            (first, Drop::from(item::HOOKSHOT.to_string())),
            (second, Drop::from(actor::OBJ_SHIELD_HYLIAN.to_string())),
            (skulltula, Drop::from(item::BOOMERANG.to_string())),
            (centred, Drop::from(item::HOOKSHOT.to_string())),
        ]);
        let dir = std::env::temp_dir().join(format!("zootr-free-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let fields = write(
            &romfs,
            &mut builder,
            &mut tables,
            &checks,
            &drops,
            &NewGame::rings(),
        )
        .unwrap();
        tables.write(&mut builder).unwrap();
        builder.finish().unwrap();
        assert_eq!(fields.len(), 4, "{fields:?}");
        let set = |hash: u64, field: &str| -> String {
            let prefix = format!("{hash}.{field}=");
            let f = fields.iter().find(|f| f.starts_with(&prefix));
            f.unwrap_or_else(|| panic!("no {prefix}"))
                .replacen(&prefix, "", 1)
        };
        let name = set(first, GYAML);
        assert!(name.starts_with(NEW), "{name}");
        assert_eq!(set(second, GYAML), actor::OBJ_SHIELD_HYLIAN);
        assert!(set(skulltula, SKULLTULAS.field).starts_with(NEW));

        let built = |name: &str| {
            let rel = format!("Pack/Actor/{name}.pack.zs");
            let data = crate::game::read_built(&romfs, &dir, &rel);
            Sarc::new(data).unwrap()
        };
        let param = |sarc: &Sarc, path: &str| -> BymlMap {
            let file = pack_path(path);
            let data = sarc.get_data(&file).unwrap_or_else(|| panic!("no {file}"));
            Byml::from_binary(data).unwrap().as_map().unwrap().clone()
        };
        let sarc = built(&name);
        let actor = param(&sarc, &actor_path(&name));
        let refs = |key: &str| component(&actor, key).unwrap();
        assert!(refs(MODEL).contains(&**item::HOOKSHOT));
        assert_eq!(refs(RESPAWN), NO_RESPAWN);
        let table = param(&sarc, &refs(TABLE));
        let parent = table[PARENT].as_string().unwrap().to_string();
        assert!(parent.contains(&**item::HOOKSHOT), "{parent}");
        param(&sarc, &parent);
        let pouch = param(&sarc, &refs(POUCH));
        assert_eq!(pouch.get(COUNT_CHECK.get()), Some(&Byml::Bool(false)));
        // Every take plays its get, so the event graph marks it (`taken`).
        let many = field::PLAY_GET_EVENT_MANY_TIMES.get();
        assert_eq!(pouch.get(many), Some(&Byml::Bool(true)));

        // The Skulltula's drops: built on a pickup that lands, keeping how
        // it falls and bounces over the boomerang's table.
        let dropped = set(skulltula, SKULLTULAS.field);
        let sarc = built(&dropped);
        assert!(
            sarc.files()
                .any(|f| f.name.is_some_and(|n| n.starts_with(LANDS)))
        );
        let actor = param(&sarc, &actor_path(&dropped));
        let table = param(&sarc, &component(&actor, TABLE).unwrap());
        for part in FALLS {
            assert!(component(&table, part).is_some(), "no {part}");
        }
        // Each drop's body is lowered till its lowest point meets its
        // model's base, a centred Hookshot's further than a boomerang's.
        let body = |name: &str| -> (String, f32) {
            let sarc = built(name);
            let physics = component(&param(&sarc, &actor_path(name)), PHYSICS).unwrap();
            let set = param(&sarc, &physics)[&**field::CONTROLLER_SET_PATH]
                .as_string()
                .unwrap()
                .clone();
            let shapes = param(&sarc, &set)[&**field::SHAPE_NAME_PATH_ARY]
                .as_array()
                .unwrap()
                .to_vec();
            let shape = shapes
                .iter()
                .map(|s| s.as_map().unwrap())
                .find(|s| s["Name"].as_string().unwrap().starts_with("Body"));
            let shape = param(
                &sarc,
                shape.unwrap()[field::FILE_PATH.get()].as_string().unwrap(),
            );
            let sphere = shape["Sphere"].as_array().unwrap()[0]
                .as_map()
                .unwrap()
                .clone();
            let y = sphere["Center"].as_map().unwrap()["Y"].as_float().unwrap();
            (physics, y - sphere["Radius"].as_float().unwrap())
        };
        for (name, base) in [
            (&dropped, -0.030_685_425),
            (&set(centred, SKULLTULAS.field), -0.152_150_5),
        ] {
            let (physics, bottom) = body(name);
            assert!(physics.contains(name.as_str()), "{physics}");
            assert!((bottom - base).abs() < 1e-4, "{name}: {bottom}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
