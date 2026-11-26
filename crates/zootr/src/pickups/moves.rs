//! What each holder whose item changed places: the item's own pickup, or
//! a stand-in built for it, written into its placement field, a step at a
//! time when it shows one.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, ModBuilder, Romfs};

use super::drops::{pickup_for, template_for};
use super::{DROP, Kind, NEW, kind_of, stale, steps};
use crate::checks::{Drop, ItemCheck};
use crate::gifts;
use crate::settings::NewGame;
use crate::standins::{StandIn, Tables, item_pack};

/// A holder whose item changed, and the actor its placement field names.
pub struct Moved<'a> {
    pub hash: u64,
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
