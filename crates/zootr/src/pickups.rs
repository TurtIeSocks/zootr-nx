//! Placed actors that hold an item as a check without a chest: freestanding
//! heart pieces, Gold Skulltulas, whose kill drops their token, the
//! pots, crates, rocks and hives that drop something when broken, and the
//! Gerudo thieves' keys. One
//! that holds something else gets a pickup for it, a stand-in built on a
//! pickup that gives a major item when no `Obj_*` form exists.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, ModBuilder, Romfs};

use crate::checks::{
    DROP_ACTOR, Drop, GYAML, ItemCheck, actors, banc_files, hash_of, load_banc, str_field,
};
use crate::gifts;
use crate::known::{Name, actor, item};
use crate::settings::{NPC_GIFTS, NewGame};
use crate::standins::{StandIn, Tables};
use crate::world::presence_of;

/// One kind of placed holder.
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

/// Pots, each with the drop it's placed with. The remix's `_Dream` pots
/// stay out.
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
    ],
    item: None,
    field: DROP,
    needs: None,
    named: None,
};

/// Wooden crates, which break to a rolling Link.
pub const CRATES: Kind = Kind {
    actors: &[actor::OBJ_BROKEN_BOX_A_01],
    item: None,
    field: DROP,
    needs: None,
    named: None,
};

/// Small rocks Link lifts and throws. The rock that needs gauntlets stays
/// out.
pub const ROCKS: Kind = Kind {
    actors: &[actor::OBJ_LIFT_ROCK_A_01],
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

/// The holders Link breaks for what they drop.
const BREAKABLE: [&Kind; 3] = [&POTS, &CRATES, &ROCKS];

/// Whether one of `kinds` places `holder`.
fn any_holds(kinds: &[&Kind], holder: &str) -> bool {
    kinds.iter().any(|k| k.places(holder))
}

/// Whether the sweep takes what `holder` holds by walking up to it: a
/// pickup lying in the open, which A takes, or one of [`BREAKABLE`].
pub fn within_reach(holder: &str) -> bool {
    any_holds(&[&FREESTANDING, &BOSS_HEARTS], holder) || broken(holder)
}

/// Whether `holder` is broken for what it drops: a pot, crate or rock.
pub fn broken(holder: &str) -> bool {
    any_holds(&BREAKABLE, holder)
}

const KINDS: [&Kind; 10] = [
    &FREESTANDING,
    &SKULLTULAS,
    &BOSS_HEARTS,
    &POTS,
    &CRATES,
    &ROCKS,
    &HIVES,
    &FORTRESS_KEYS,
    &FIRE_ARROWS,
    &RUTOS_LETTER,
];

/// What a thief's key check is held by, in the spoiler.
pub const THIEF: &str = "Gerudo Thief";

const DROP: &str = "Dynamic.Drop__DropActor";
const NAME: &str = "Name";
/// The remix's Dream World, outside the story a seed plays.
const REMIX: &str = "Banc/DreamWorld/";

/// A pickup that gives a major item with its get, the stand-ins' template.
const TEMPLATE: &Name = actor::OBJ_ARROW_FIRE;
/// The template of a stand-in a holder drops: a pickup that gives with the
/// same get, but falls, bounces and lands as drops do. One built on
/// [`TEMPLATE`], which lies placed and never falls, drops through the floor
/// (Mac, 2026-11-14).
const DROPPED: &Name = actor::OBJ_SHIELD_DEKU;
/// Pickups the game only places, never drops. They have no contact with
/// the ground, so a holder drops a stand-in for them.
const PLACED_ONLY: [&Name; 2] = [actor::OBJ_HEART_PIECE, actor::OBJ_HEART_CONTAINER];
/// Each stand-in's actor is this plus its holder's number.
const NEW: &str = "Obj_Rando_";
/// World pickups already, which a holder gives as they are.
const PICKUP: &str = "Obj_";
/// A chest's rupee, which a holder drops as the floor rupee of its colour:
/// a stand-in rupee pays 1 (Mac, 2026-11-14).
const CHEST_RUPEE: &str = "TBox_Rupee_";
const FLOOR_RUPEE: &str = "Obj_Rupee_";

/// The pickup a holder places for `item` as it is, when there is one that
/// shows where it's put: one that lands, when the holder `dropped` it.
fn pickup_for(item: &str, dropped: bool) -> Option<String> {
    let pickup = match item.strip_prefix(CHEST_RUPEE) {
        Some(colour) => format!("{FLOOR_RUPEE}{colour}"),
        None => item.starts_with(PICKUP).then(|| item.to_owned())?,
    };
    (!dropped || !PLACED_ONLY.iter().any(|p| **p == *pickup)).then_some(pickup)
}

/// The pickup a stand-in for what `field` names is built on.
fn template_for(field: &str) -> &'static str {
    match field {
        DROP => DROPPED,
        _ => TEMPLATE,
    }
}

impl Kind {
    /// Whether `actor` is one of the placed actors.
    fn places(&self, actor: &str) -> bool {
        self.actors.iter().any(|a| **a == *actor)
    }

    /// One check per placement, as chests are.
    pub fn checks(&self, romfs: &Romfs) -> Result<Vec<ItemCheck>> {
        let mut out = Vec::new();
        for file in banc_files(romfs)?
            .into_iter()
            .filter(|f| !f.starts_with(REMIX))
        {
            let doc = load_banc(romfs, &file)?;
            for actor in actors(&doc.root)? {
                let gyaml = str_field(actor, GYAML).unwrap_or_default();
                if !self.places(gyaml) {
                    continue;
                }
                let held = actor
                    .as_map()?
                    .get("Dynamic")
                    .and_then(|d| str_field(d, DROP_ACTOR));
                let (holder, item) = match &self.named {
                    Some(Named { prefix, holder }) => {
                        if !str_field(actor, NAME).is_some_and(|n| n.starts_with(prefix)) {
                            continue;
                        }
                        (*holder, gyaml)
                    }
                    None => {
                        let Some(item) = self.item.map(|i| i.get()).or(held) else {
                            continue;
                        };
                        (gyaml, item)
                    }
                };
                let hash = hash_of(actor).context("actor without Hash")?;
                let area = file.split('/').nth(1).unwrap_or("?");
                let mut check = gifts::check(file.clone(), area, hash, holder.into(), item);
                // A named holder is there once its event has put it there,
                // which the seed doesn't have to wait for.
                check.presence = presence_of(actor).filter(|_| self.named.is_none());
                out.push(check);
            }
        }
        Ok(out)
    }

    pub fn holds(&self, check: &ItemCheck) -> bool {
        let named = self.named.as_ref().map(|n| n.holder);
        self.places(&check.holder) || named == Some(check.holder.as_str())
    }
}

/// The checks of every kind `new_game` turns on.
pub fn dealt(romfs: &Romfs, new_game: &NewGame) -> Result<Vec<ItemCheck>> {
    let npc_gifts = new_game.on.contains(NPC_GIFTS);
    let on = [
        (new_game.freestanding, &FREESTANDING),
        (new_game.skulltulas, &SKULLTULAS),
        (new_game.boss_hearts, &BOSS_HEARTS),
        (new_game.pots, &POTS),
        (new_game.crates, &CRATES),
        (new_game.rocks, &ROCKS),
        (new_game.hives, &HIVES),
        (new_game.fortress_keys, &FORTRESS_KEYS),
        (npc_gifts, &FIRE_ARROWS),
        (npc_gifts, &RUTOS_LETTER),
    ];
    let mut out = Vec::new();
    for (_, kind) in on.into_iter().filter(|(on, _)| *on) {
        out.extend(kind.checks(romfs)?);
    }
    Ok(out)
}

pub fn kind_of(check: &ItemCheck) -> Option<&'static Kind> {
    KINDS.into_iter().find(|k| k.holds(check))
}

/// Whether `check` is a placed holder's, not a chest's.
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
}

impl Moved<'_> {
    /// Whether `actor` is a stand-in that has to be built.
    fn built(&self) -> bool {
        self.actor.starts_with(NEW)
    }
}

/// Every holder in `checks` whose item in `drops` isn't its own.
pub fn moved<'a>(checks: &'a [ItemCheck], drops: &'a HashMap<u64, Drop>) -> Vec<Moved<'a>> {
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
        let actor = pickup_for(item, field == DROP).unwrap_or_else(|| format!("{NEW}{n:03}"));
        out.push(Moved {
            hash,
            vanilla,
            item,
            field,
            actor,
            template: template_for(field),
        });
    }
    out
}

/// Build a pickup for each holder whose item changed, and return the
/// placement fields, as `HASH.PATH=VALUE`, that place it.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    checks: &[ItemCheck],
    drops: &HashMap<u64, Drop>,
) -> Result<Vec<String>> {
    let mut fields = Vec::new();
    for m in moved(checks, drops) {
        if m.built() {
            let pickup = StandIn {
                name: &m.actor,
                template: ActorPack::load(romfs, m.template)?,
                item: ActorPack::load(romfs, m.item)?,
                price: None,
            };
            pickup
                .build(builder, tables)
                .with_context(|| format!("{} becoming {}", m.hash, m.item))?;
        }
        fields.push(format!("{}.{}={}", m.hash, m.field, m.actor));
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field, item};

    #[test]
    fn a_chest_rupee_drops_as_the_floor_rupee_and_a_major_item_as_a_stand_in() {
        assert_eq!(
            pickup_for(actor::T_BOX_RUPEE_RED, true).as_deref(),
            Some(actor::OBJ_RUPEE_RED.get())
        );
        assert_eq!(
            pickup_for(actor::OBJ_HEART_PIECE, false).as_deref(),
            Some(actor::OBJ_HEART_PIECE.get())
        );
        assert_eq!(pickup_for(item::SHIELD_MIRROR, false), None);
    }

    #[test]
    fn a_heart_piece_a_holder_drops_is_a_stand_in_that_lands() {
        assert_eq!(pickup_for(actor::OBJ_HEART_PIECE, true), None);
        assert_eq!(pickup_for(actor::OBJ_HEART_CONTAINER, true), None);
        assert_eq!(template_for(DROP), DROPPED);
        assert_eq!(template_for(GYAML), TEMPLATE);
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
            (&POTS, 100),
            (&CRATES, 11),
            (&ROCKS, 14),
            (&HIVES, 9),
            (&FORTRESS_KEYS, 4),
            (&FIRE_ARROWS, 1),
            (&RUTOS_LETTER, 1),
        ] {
            let checks = kind.checks(&romfs).unwrap();
            let hashes: BTreeSet<u64> = checks.iter().map(|c| c.hash).collect();
            assert_eq!(hashes.len(), count);
            assert!(
                checks
                    .iter()
                    .all(|c| kind.holds(c) && !c.file.starts_with(REMIX))
            );
            assert!(checks.iter().all(|c| kind.item.is_none_or(|i| c.item == i)));
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
            write(&romfs, builder, tables, &checks, drops).unwrap()
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
        let fields = write(&romfs, &mut builder, &mut tables, &checks, &drops).unwrap();
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
            let raw = std::fs::read(dir.join("romfs").join(&rel)).unwrap();
            let data = romfs.dicts().decompress(&rel, &raw).unwrap();
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
