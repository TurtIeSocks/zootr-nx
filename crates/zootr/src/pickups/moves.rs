//! What each holder whose item changed places: the item's own pickup, or
//! a stand-in built for it, written into its placement field, a step at a
//! time when it shows one.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, ModBuilder, Romfs};

use super::drops::{pickup_for, template_for};
use super::{DROP, Kind, NEW, SKULLTULAS, kind_of, seats, stale, steps};
use crate::checks::{Drop, GYAML, ItemCheck, TRANSLATE};
use crate::known::{actor, item};
use crate::settings::NewGame;
use crate::standins::{StandIn, Tables, item_pack, table_floor};
use crate::{ammo, gifts};

/// A holder whose item changed, and the actor its placement field names.
pub struct Moved<'a> {
    /// Where the item lies: the holder's own placement, a step's copy of
    /// it, or a boulder's seat (`seats`).
    pub hash: u64,
    /// The holder's own placement, its check's hash.
    pub of: u64,
    /// What the holder was.
    pub vanilla: &'a str,
    /// What it now holds.
    pub item: &'a str,
    pub(super) field: &'static str,
    /// The actor placed there: the item's own pickup, or a stand-in built
    /// for it, named by its holder's number.
    pub actor: String,
    /// What a stand-in is built on.
    template: &'static str,
    pub(super) hovering: bool,
    /// Which step of its holder's it is, when the holder shows one a step
    /// at a time.
    pub step: Option<steps::Step>,
}

impl Moved<'_> {
    /// Whether `actor` is a stand-in that has to be built.
    pub(super) fn built(&self) -> bool {
        self.actor.starts_with(NEW) || self.actor.starts_with(stale::STEP)
    }
}

/// Every holder in `checks` whose item in `drops` isn't its own, and each
/// step's placement past a holder's own when it shows them a step at a
/// time. Ammo still steps when a holder keeps its own drop, and a
/// boulder's item always lies in its seat.
pub fn moved<'a>(
    checks: &'a [ItemCheck],
    drops: &'a HashMap<u64, Drop>,
    progressive: crate::progressive::Lines,
) -> Vec<Moved<'a>> {
    let holders: BTreeSet<_> = checks
        .iter()
        .filter_map(|c| {
            Some((
                c.hash,
                c.item.as_str(),
                kind_of(c)?.field,
                seats::holds(c),
                SKULLTULAS.holds(c),
            ))
        })
        .collect();
    let mut out = Vec::new();
    for (n, (of, vanilla, field, seated, hovering)) in holders.into_iter().enumerate() {
        let item = gifts::item_at(drops, of, vanilla);
        if item == vanilla && !seated && ammo::need(item, progressive).is_none() {
            continue;
        }
        let lies = |placement, k| match seated {
            true => seats::seat(of, k),
            false => placement,
        };
        let at = |hash, item, actor: String, step| Moved {
            hash,
            of,
            vanilla,
            item,
            field,
            actor,
            template: if hovering {
                actor::OBJ_TOKEN_GOLD
            } else {
                template_for(field)
            },
            hovering,
            step,
        };
        // A Skulltula always uses a hovering version of its reward, even
        // if the item has a pickup actor of its own that normally falls.
        let pickup = |item| {
            (!hovering)
                .then(|| pickup_for(item, field == DROP))
                .flatten()
        };
        let Some(steps) = steps::of(of, item, progressive) else {
            let prefix = if hovering { stale::HOVER } else { NEW };
            let actor = pickup(item).unwrap_or_else(|| format!("{prefix}{n:03}"));
            out.push(at(lies(of, 0), item, actor, None));
            continue;
        };
        for (k, (placement, shows, step)) in steps.into_iter().enumerate() {
            let prefix = if hovering {
                stale::HOVER_STEP
            } else {
                stale::STEP
            };
            let actor = pickup(shows).unwrap_or_else(|| format!("{prefix}{n:03}_{k}"));
            out.push(at(lies(placement, k), shows, actor, Some(step)));
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
    built.map(|m| (m.of, m.actor)).collect()
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
    let lines = crate::progressive::Lines::of(new_game);
    let mut fields = Vec::new();
    let moved = moved(checks, drops, lines);
    let drop_ai = moved
        .iter()
        .any(|m| m.hovering)
        .then(|| ActorPack::load(romfs, super::drops::DROPPED))
        .transpose()?;
    for m in moved {
        if m.built() {
            let pickup = StandIn {
                name: &m.actor,
                template: ActorPack::load(romfs, m.template)?,
                item: item_pack(romfs, m.item, new_game)?,
                price: None,
            };
            pickup
                .build_with_drop_ai(builder, tables, drop_ai.as_ref().filter(|_| m.hovering))
                .with_context(|| format!("{} becoming {}", m.hash, m.item))?;
        }
        fields.push(format!("{}.{}={}", m.hash, m.field, m.actor));
        // A placed token's model extends below its origin. Raise it by
        // the model's depth, as the shelf placement does.
        if m.field == GYAML && m.item == item::TOKEN_GOLD {
            let rise = -table_floor(&tables.actor, m.item)?;
            fields.push(format!("{}.{TRANSLATE}.1+={rise}", m.hash));
        }
    }
    // A holder keeping its own drop needs a fixed drop because its table
    // goes (`tabled`). Ammo's drops were already written as steps above.
    for c in checks
        .iter()
        .filter(|c| kind_of(c).is_some_and(Kind::owns_drop))
    {
        let item = gifts::item_at(drops, c.hash, &c.item);
        if item == c.item
            && ammo::need(item, lines).is_none()
            && pickup_for(item, true).as_deref() == Some(item)
        {
            fields.push(format!("{}.{DROP}={item}", c.hash));
        }
    }
    // A boulder's item lies in its seat, so the boulder drops nothing.
    let boulders: BTreeSet<u64> = (checks.iter().filter(|c| seats::drops(c)))
        .map(|c| c.hash)
        .collect();
    fields.extend(boulders.into_iter().map(seats::no_drop));
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
    use ounce_formats::{Byml, BymlMap, pack_path};
    use roead::sarc::Sarc;

    use super::super::drops::LANDS;
    use super::super::{
        ANIMAL, FIRE_ARROWS, FREESTANDING, GRASS, LOOSE, POTS, RUTOS_LETTER, SKULLTULAS,
    };
    use super::*;
    use crate::checks::GYAML;
    use crate::game::dev_dump;
    use crate::known::{Name, actor, field, item};
    use crate::standins::{
        COUNT_CHECK, FALLS, MODEL, NO_RESPAWN, PARENT, PHYSICS, POUCH, RESPAWN, TABLE, actor_path,
        component,
    };

    #[test]
    fn skulltula_rewards_use_the_tokens_hovering_body() {
        let Some(romfs) = dev_dump() else { return };
        let checks = SKULLTULAS.checks(&romfs).unwrap();
        let hash = 17067594426901953848;
        let dir = std::env::temp_dir().join(format!("zootr-hover-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let drops = HashMap::from([(hash, Drop::from(actor::OBJ_HEART.to_string()))]);
        let fields = write(
            &romfs,
            &mut builder,
            &mut tables,
            &checks,
            &drops,
            &NewGame::rings(),
        )
        .unwrap();
        builder.finish().unwrap();
        let prefix = format!("{hash}.{DROP}=");
        let reward = fields.iter().find_map(|f| f.strip_prefix(&prefix)).unwrap();
        assert_ne!(reward, actor::OBJ_HEART.get(), "the stock heart falls");
        let heap = |name: &str| {
            crate::standins::row(&tables.actor, name).unwrap()[field::INSTANCE_HEAP_SIZE.get()]
                .as_int::<u32>()
                .unwrap()
        };
        assert!(
            heap(reward) >= heap(actor::OBJ_TOKEN_GOLD) + heap(actor::OBJ_HEART),
            "the game needs room to load both the token body and the heart",
        );
        let rel = format!("Pack/Actor/{reward}.pack.zs");
        let pack =
            ActorPack::from_bytes(reward, crate::game::read_built(&romfs, &dir, &rel)).unwrap();
        let params = pack.actor_param().unwrap().unwrap();
        let physics = component(&params, PHYSICS).unwrap();
        let tokens = ActorPack::load(&romfs, actor::OBJ_TOKEN_GOLD).unwrap();
        assert_eq!(
            component(&tokens.actor_param().unwrap().unwrap(), PHYSICS),
            Some(physics)
        );
        let heart = ActorPack::load(&romfs, actor::OBJ_HEART).unwrap();
        let heart_params = heart.actor_param().unwrap().unwrap();
        assert_eq!(
            component(&params, field::E_LINK_REF),
            component(&heart_params, field::E_LINK_REF)
        );
        let effect_user = |name: &str| {
            crate::standins::row(&tables.actor, name)
                .unwrap()
                .get(field::E_LINK_USER_NAME.get())
                .cloned()
        };
        assert_eq!(effect_user(reward), effect_user(actor::OBJ_HEART));
        let pouch = pack
            .resolve(&pack_path(&component(&params, POUCH).unwrap()))
            .unwrap()
            .unwrap();
        assert_eq!(
            pouch[field::POUCH_ACTOR.get()],
            crate::standins::pouch_actor(actor::OBJ_HEART)
        );
        std::fs::remove_dir_all(dir).unwrap();
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
        let prefix = format!("{green}.{GYAML}=");
        let standin = fields.iter().find_map(|f| f.strip_prefix(&prefix));
        let standin = standin.filter(|a| a.starts_with(NEW)).expect("a stand-in");
        assert!(fields.contains(&format!("{blue}.{GYAML}={}", actor::OBJ_RUPEE_GREEN)));
        builder.finish().unwrap();
        // The Hookshot lies quiet: no Fire Arrow flames or crackle from
        // the template it's built on.
        let rel = format!("Pack/Actor/{standin}.pack.zs");
        let pack =
            ActorPack::from_bytes(standin, crate::game::read_built(&romfs, &dir, &rel)).unwrap();
        let params = pack.actor_param().unwrap().unwrap();
        let row = crate::standins::row(&tables.actor, standin).unwrap();
        for (link, user) in [
            (field::E_LINK_REF, field::E_LINK_USER_NAME),
            (field::S_LINK_REF, field::S_LINK_USER_NAME),
        ] {
            assert_eq!(component(&params, link), None, "{}", link.get());
            assert_eq!(row.get(user.get()), None, "{}", user.get());
        }
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
        let pots = POTS.checks(&romfs).unwrap();
        let (pot, centred) = (pots[0].hash, pots[1].hash);
        checks.extend([pots[0].clone(), pots[1].clone()]);
        let drops = HashMap::from([
            (first, Drop::from(item::HOOKSHOT.to_string())),
            (second, Drop::from(actor::OBJ_SHIELD_HYLIAN.to_string())),
            (pot, Drop::from(item::BOOMERANG.to_string())),
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
        assert!(set(pot, POTS.field).starts_with(NEW));

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

        // The pots' drops: built on a pickup that lands, keeping how
        // it falls and bounces over the boomerang's table.
        let dropped = set(pot, POTS.field);
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
            (&set(centred, POTS.field), -0.152_150_5),
        ] {
            let (physics, bottom) = body(name);
            assert!(physics.contains(name.as_str()), "{physics}");
            assert!((bottom - base).abs() < 1e-4, "{name}: {bottom}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
