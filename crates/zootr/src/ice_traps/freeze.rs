//! The freeze a trap springs: a request for the `Frozen` bad condition
//! alone doesn't freeze Link, so a trap hits him with a frozen attack.
//! The seed adds a condition sensor, the kind a song makes around Link,
//! carrying the Freezard's breath, and an event that makes it. The hook
//! starts that event once Link is free (`zootr_hook::traps`).

use anyhow::{Context, Result};
use ounce_formats::{
    ActorPack, Byml, BymlDoc, BymlMap, EntryPoint, EventKind, ModBuilder, Romfs, pack_file,
    pack_path,
};
use roead::sarc::SarcWriter;
use zootr_hook::traps::{END, EVENT, SENSOR};

use crate::flows::{self, Rewrites, set, string};
use crate::known::{Name, actor, field, flow, scene};
use crate::rsdb::Events;
use crate::standins::{
    COMPONENTS, Tables, actor_path, component, components, doc, path_of, raw, reference, renamed,
    resolved,
};

/// The sensor a trap's event makes is the Song of the Sun's, remade.
const LIKE: &Name = actor::SONG_OF_THE_SUN_SENSOR_ACTOR;
/// Whose breath freezes Link, and whose side the sensor takes.
const FREEZER: &Name = actor::ENEMY_FREEZARD;
/// The breath, the first of the Freezard's attacks.
const BREATH: usize = 0;
/// The sensor's reach about Link, in metres: his own size, so it hits no
/// one else.
const RADIUS: f32 = 0.5;
const HALF_HEIGHT: f32 = 1.0;
/// The layer the Freezard's attack sensors sit on, which Link's receipt
/// takes hits from.
const ENEMY_ATTACK: &Name = field::ENEMY_ATTACK;

/// The flow the event runs from: the map change flow, whose event the
/// hook already starts, gains an entry point named for the event.
const FLOW: &Name = scene::SY_BANC_CHANGE_GO;
/// The flow with the call that makes a sensor, to copy.
const SONGS: &Name = scene::SY_OCARINA;
const MAKE: (&Name, &Name) = (
    field::GAME_SYSTEM_ACTOR,
    flow::EVENT_TRIGGER_REQUEST_CREATE_CONDITION_SENSOR,
);
/// The call that takes the sensor away. The system holds one condition
/// sensor, so it names none.
const UNMAKE: &Name = flow::EVENT_TRIGGER_REQUEST_DELETE_CONDITION_SENSOR;

/// Keys of a controller set, each an array of `{FilePath, Name}`.
const CCOL_SENSORS: &Name = field::C_COL_SENSOR_NAME_PATH_ARY;
const BODY_SENSORS: &Name = field::RIGID_BODY_SENSOR_NAME_PATH_ARY;
const SHAPES: &Name = field::SHAPE_NAME_PATH_ARY;
const FILE_PATH: &Name = field::FILE_PATH;
/// The Freezard's attack sensor, by its name in the set.
const ATK: &str = "Atk";

/// The event's name, without its NUL.
fn name() -> &'static str {
    text(EVENT)
}

/// The ending event's name, without its NUL.
fn end() -> &'static str {
    text(END)
}

fn text(event: &'static [u8]) -> &'static str {
    core::str::from_utf8(&event[..event.len() - 1]).expect("an event's name is text")
}

/// The map change flow with an entry point named for the event, which
/// makes the sensor and ends, and one for [`END`], which takes it away. It
/// outlives Link's freeze and would freeze him again after each break.
pub fn flow(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    let songs = flows::load(romfs, SONGS)?.context("the song flow is a timeline")?;
    let at = *flows::running(&songs, MAKE)
        .first()
        .context("the song flow makes no sensor")?;
    let model = flows::model(&songs, MAKE, at)?;
    flows::edit(romfs, rewrites, FLOW, |flow| {
        let (actor, action) = flow.action(&model.actor, model.name);
        let mut params = model.params.clone();
        set(&mut params, field::ACTOR_NAME, string(SENSOR))?;
        let make = flow.push(EventKind::Action {
            next: None,
            actor,
            action,
            params,
        });
        let (actor, action) = flow.action(&model.actor, UNMAKE.get());
        let unmake = flow.push(EventKind::Action {
            next: None,
            actor,
            action,
            params: vec![],
        });
        for (name, main) in [(name(), make), (end(), unmake)] {
            flow.entry_points.push(EntryPoint {
                name: name.into(),
                main: Some(main),
                vars: vec![],
            });
        }
        Ok(())
    })
}

/// The events' rows: the map change's, starting the entry points [`flow`]
/// adds.
pub fn event(events: &mut Events) -> Result<()> {
    events.copy(name(), FLOW)?;
    events.copy(end(), FLOW)
}

/// The sensor's pack and rows: the sun's sensor, on the Freezard's side,
/// hitting with its breath on the enemy attack layer, shrunk to Link.
pub fn sensor(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    remade(romfs)?.write(builder)?;
    tables.copy(SENSOR, LIKE)
}

/// Child Link, who can freeze. The game turns his `Frozen` bad condition
/// off (adult Link's is on), so a frozen hit only hurt him (live,
/// 2026-11-18). His pack carries the frozen reaction all the same.
const CHILD: &Name = actor::PLAYER_CHILD;

/// Write child Link's pack with his `Frozen` bad condition on, and the
/// adult's effects: the frozen shell's break is only in the adult's
/// (live, 2026-11-18).
pub fn thaw(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let pack = ActorPack::load(romfs, CHILD)?;
    let mut writer = pack.writer();
    for (path, old, new) in thawed(&pack)? {
        builder.resize(&path, old.len(), new.len())?;
        writer.add_file(path, new);
    }
    builder.write(&pack_file(CHILD), &writer.to_binary())
}

/// A file of a pack: its path, its bytes, and its bytes edited.
type Edit = (String, Vec<u8>, Vec<u8>);

/// Child Link's conditions and effects files, as [`thaw`] writes them.
fn thawed(pack: &ActorPack) -> Result<[Edit; 2]> {
    let param = param(pack)?;
    let adult = Byml::String(actor::PLAYER_ADULT.get().into());
    Ok([
        edited(
            pack,
            &param,
            field::CONDITION_REF,
            field::IS_ENABLE_FROZEN,
            Byml::Bool(true),
        )?,
        edited(pack, &param, field::E_LINK_REF, field::USER_NAME, adult)?,
    ])
}

/// The file of `pack`'s component `at`, with `key` set to `value`.
fn edited(pack: &ActorPack, param: &BymlMap, at: &Name, key: &Name, value: Byml) -> Result<Edit> {
    let file = component(param, at).with_context(|| format!("child Link has no {}", at.get()))?;
    let path = pack_path(&file);
    let old = raw(pack, &path)?;
    let mut doc = BymlDoc::parse(&old)?;
    (doc.root.as_mut_map()?).insert(key.get().into(), value);
    Ok((path, old, doc.to_binary()))
}

fn remade(romfs: &Romfs) -> Result<Remade> {
    let freezer = ActorPack::load(romfs, FREEZER)?;
    // Read from, while the remade pack holds its own to size against.
    let like = ActorPack::load(romfs, LIKE)?;
    let mut pack = Remade::new(ActorPack::load(romfs, LIKE)?);

    let attack_at = attack(&like)?;
    let attack = pack.add(&attack_at, breath(&freezer)?)?;
    let table_at = path_of(&like, field::GAME_PARAMETER_TABLE_REF)?;
    let table = pack.add(
        &table_at,
        with_component(
            &resolved(&like, &table_at)?,
            field::GAMEBALANCE_ATTACK_PARAM,
            &attack,
        )?,
    )?;

    let set_at = controller_set(&like)?;
    let set = resolved(&like, &set_at)?;
    let receipt = pack.lend(
        &freezer,
        &pack_path(&named(&controller_set_of(&freezer)?, CCOL_SENSORS, ATK)?),
    )?;
    let body_at = pack_path(&first(&set, BODY_SENSORS)?);
    let body = pack.add(&body_at, enemy_attack(resolved(&like, &body_at)?))?;
    let shape_at = pack_path(&first(&set, SHAPES)?);
    let shape = pack.add(&shape_at, shrunk(resolved(&like, &shape_at)?)?)?;
    let mut set = set;
    for (key, path) in [
        (CCOL_SENSORS, &receipt),
        (BODY_SENSORS, &body),
        (SHAPES, &shape),
    ] {
        point_first(&mut set, key, path)?;
    }
    let set = pack.add(&set_at, set)?;
    let physics_at = path_of(&like, field::PHYSICS_REF)?;
    let mut physics = resolved(&like, &physics_at)?;
    physics.insert(field::CONTROLLER_SET_PATH.get().into(), work(&set));
    let physics = pack.add(&physics_at, physics)?;

    let side = component(&param(&freezer)?, field::AFFILIATION_REF)
        .context("the Freezard takes no side")?;
    let side = pack.lend(&freezer, &pack_path(&side))?;

    let mut actor = param_doc(&like)?;
    let mut refs = components(&actor)?;
    for (key, path) in [
        (field::GAME_PARAMETER_TABLE_REF, &table),
        (field::PHYSICS_REF, &physics),
        (field::AFFILIATION_REF, &side),
    ] {
        refs.insert(key.get().into(), reference(path));
    }
    actor.insert(COMPONENTS.into(), Byml::Map(refs));
    pack.add_as(actor_path(SENSOR), &actor_path(&like.name), actor)?;
    Ok(pack)
}

/// A pack being remade from `like` as the sensor: each file it changes
/// renamed for the sensor and sized in the RESTBL like the one it replaces.
struct Remade {
    like: ActorPack,
    writer: SarcWriter,
    /// Each new file's path, the file it's sized like and both sizes.
    sized: Vec<(String, String, usize, usize)>,
}

impl Remade {
    fn new(like: ActorPack) -> Self {
        Self {
            writer: like.writer(),
            like,
            sized: vec![],
        }
    }

    /// `map` as the sensor's own copy of `like`'s file at `at`.
    fn add(&mut self, at: &str, map: BymlMap) -> Result<String> {
        self.add_as(renamed(at, SENSOR), at, map)
    }

    fn add_as(&mut self, path: String, at: &str, map: BymlMap) -> Result<String> {
        let old = raw(&self.like, at)?;
        let data = doc(map, &BymlDoc::parse(&old)?).to_binary();
        self.sized
            .push((path.clone(), at.into(), old.len(), data.len()));
        self.writer.add_file(path.clone(), data);
        Ok(path)
    }

    /// `lender`'s file at `at`, unchanged, under its own path.
    fn lend(&mut self, lender: &ActorPack, at: &str) -> Result<String> {
        self.writer.add_file(at.to_owned(), raw(lender, at)?);
        Ok(at.to_owned())
    }

    fn write(mut self, builder: &mut ModBuilder) -> Result<()> {
        for (path, like, like_size, size) in &self.sized {
            builder.add_entry(path, like, *like_size, *size)?;
        }
        builder.add(
            &format!("Pack/Actor/{SENSOR}.pack.zs"),
            &self.writer.to_binary(),
            &format!("Pack/Actor/{}.pack.zs", self.like.name),
        )
    }
}

fn param(pack: &ActorPack) -> Result<BymlMap> {
    pack.actor_param()?
        .with_context(|| format!("{} has no {}", pack.name, field::ACTOR_PARAM))
}

/// `pack`'s own ActorParam file, unresolved: the sensor keeps its parent.
fn param_doc(pack: &ActorPack) -> Result<BymlMap> {
    let data = raw(pack, &actor_path(&pack.name))?;
    Ok(BymlDoc::parse(&data)?.root.as_map()?.clone())
}

/// A pack file's path as a param names it: `Work/…​.gyml`.
fn work(path: &str) -> Byml {
    let stem = path.strip_suffix(".bgyml").unwrap_or(path);
    Byml::String(format!("Work/{stem}.gyml").into())
}

/// `map` with its `Components`' `key` naming `path`.
fn with_component(map: &BymlMap, key: &Name, path: &str) -> Result<BymlMap> {
    let mut map = map.clone();
    let mut refs = components(&map)?;
    refs.insert(key.get().into(), reference(path));
    map.insert(COMPONENTS.into(), Byml::Map(refs));
    Ok(map)
}

/// The AttackParam file `pack`'s parameter table names.
fn attack(pack: &ActorPack) -> Result<String> {
    let table = resolved(pack, &path_of(pack, field::GAME_PARAMETER_TABLE_REF)?)?;
    let at = component(&table, field::GAMEBALANCE_ATTACK_PARAM)
        .with_context(|| format!("{} has no attack", pack.name))?;
    Ok(pack_path(&at))
}

/// The Freezard's attacks with only the breath left, without its shove:
/// it threw Link a few metres (live, 2026-11-18), and a trap mustn't
/// knock him off a ledge. The breath stays a new attack each frame, as
/// the freeze needs: hitting once only hurt him.
fn breath(freezer: &ActorPack) -> Result<BymlMap> {
    let mut attacks = resolved(freezer, &attack(freezer)?)?;
    let all = attacks
        .get(field::ATTACK_PARAMS.get())
        .context("no attacks")?
        .as_array()?;
    let mut breath = all.get(BREATH).context("no breath")?.as_map()?.clone();
    breath.insert(field::IMPULSE.get().into(), Byml::Float(0.0));
    attacks.insert(
        field::ATTACK_PARAMS.get().into(),
        Byml::Array(vec![Byml::Map(breath)]),
    );
    Ok(attacks)
}

/// The controller set file `pack`'s physics names.
fn controller_set(pack: &ActorPack) -> Result<String> {
    let physics = resolved(pack, &path_of(pack, field::PHYSICS_REF)?)?;
    let at = physics
        .get(field::CONTROLLER_SET_PATH.get())
        .context("no controller set")?
        .as_string()?;
    Ok(pack_path(at))
}

fn controller_set_of(pack: &ActorPack) -> Result<BymlMap> {
    resolved(pack, &controller_set(pack)?)
}

fn entries<'m>(set: &'m BymlMap, key: &Name) -> Result<&'m [Byml]> {
    Ok(set
        .get(key.get())
        .with_context(|| format!("no {key}"))?
        .as_array()?)
}

/// The file the first of `set`'s `key` entries names.
fn first(set: &BymlMap, key: &Name) -> Result<String> {
    let entry = entries(set, key)?
        .first()
        .with_context(|| format!("no {key}"))?;
    Ok(entry
        .as_map()?
        .get(FILE_PATH.get())
        .context("no path")?
        .as_string()?
        .to_string())
}

/// The file `set`'s `key` entry called `name` names.
fn named(set: &BymlMap, key: &Name, name: &str) -> Result<String> {
    let entry = entries(set, key)?
        .iter()
        .filter_map(|e| e.as_map().ok())
        .find(|e| {
            e.get("Name")
                .and_then(|n| n.as_string().ok())
                .map(|n| n.as_str())
                == Some(name)
        })
        .with_context(|| format!("no {key} {name}"))?;
    Ok(entry
        .get(FILE_PATH.get())
        .context("no path")?
        .as_string()?
        .to_string())
}

/// `set`'s first `key` entry names `path`.
fn point_first(set: &mut BymlMap, key: &Name, path: &str) -> Result<()> {
    let entries = set
        .get_mut(key.get())
        .with_context(|| format!("no {key}"))?
        .as_mut_array()?;
    let entry = entries
        .first_mut()
        .with_context(|| format!("no {key}"))?
        .as_mut_map()?;
    entry.insert(FILE_PATH.get().into(), work(path));
    Ok(())
}

/// A sensor body on the enemy attack layer, as the Freezard's are, with
/// nothing more to its mask.
fn enemy_attack(mut body: BymlMap) -> BymlMap {
    body.insert(
        field::LAYER_SENSOR.get().into(),
        Byml::String(ENEMY_ATTACK.get().into()),
    );
    body.remove(field::ENABLE_LAYER_HIT_MASK.get());
    body.remove(field::SUB_LAYER_SENSOR.get());
    body
}

/// A cylinder shape shrunk to Link's size.
fn shrunk(mut shape: BymlMap) -> Result<BymlMap> {
    let cylinders = shape
        .get_mut(field::CYLINDER.get())
        .context("no cylinder")?
        .as_mut_array()?;
    for c in cylinders {
        let c = c.as_mut_map()?;
        c.insert(field::RADIUS.get().into(), Byml::Float(RADIUS));
        for (key, y) in [
            (field::CENTER_A, HALF_HEIGHT),
            (field::CENTER_B, -HALF_HEIGHT),
        ] {
            let centre = c
                .get_mut(key.get())
                .with_context(|| format!("no {key}"))?
                .as_mut_map()?;
            centre.insert("Y".into(), Byml::Float(y));
        }
    }
    // The bounds the game takes from the shape.
    if let Some(auto) = shape.get_mut(field::AUTO_CALC.get()) {
        let auto = auto.as_mut_map()?;
        for (key, sign) in [("Max", 1.0), ("Min", -1.0)] {
            if let Some(b) = auto.get_mut(key) {
                let b = b.as_mut_map()?;
                b.insert("X".into(), Byml::Float(sign * RADIUS));
                b.insert("Y".into(), Byml::Float(sign * HALF_HEIGHT));
                b.insert("Z".into(), Byml::Float(sign * RADIUS));
            }
        }
    }
    Ok(shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::running;
    use crate::game::dev_dump;
    use crate::standins::row;

    #[test]
    fn one_event_makes_the_sensor_and_one_takes_it_away() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        flow(&romfs, &mut rewrites).unwrap();
        let flow = flows::current(&romfs, &rewrites, FLOW).unwrap();
        let entry = flow.entry_points.iter().find(|e| e.name == name()).unwrap();
        let [make] = running(&flow, MAKE)[..] else {
            panic!("one sensor made")
        };
        assert_eq!(entry.main, Some(make));
        let entry = flow.entry_points.iter().find(|e| e.name == end()).unwrap();
        let [unmake] = running(&flow, (MAKE.0, UNMAKE))[..] else {
            panic!("one sensor taken away")
        };
        assert_eq!(entry.main, Some(unmake));
        let mut events = Events::load(&romfs).unwrap();
        event(&mut events).unwrap();
        for name in [name(), end()] {
            row(&events.setting, name).unwrap();
            row(&events.analyzed, name).unwrap();
        }
    }

    #[test]
    fn child_link_can_freeze_and_shows_it() {
        let Some(romfs) = dev_dump() else { return };
        let pack = ActorPack::load(&romfs, CHILD).unwrap();
        let [(_, old, new), (_, old_fx, new_fx)] = thawed(&pack).unwrap();
        let read = |data: &[u8], key: &Name| {
            let doc = BymlDoc::parse(data).unwrap();
            doc.root.as_map().unwrap().get(key.get()).unwrap().clone()
        };
        let frozen = field::IS_ENABLE_FROZEN;
        assert_eq!(
            read(&old, frozen),
            Byml::Bool(false),
            "the game's child Link can't"
        );
        assert_eq!(read(&new, frozen), Byml::Bool(true));
        let adult = Byml::String(actor::PLAYER_ADULT.get().into());
        assert_ne!(read(&old_fx, field::USER_NAME), adult);
        assert_eq!(read(&new_fx, field::USER_NAME), adult);
    }

    #[test]
    fn the_sensor_is_the_freezard_s_breath_on_its_side_at_link_s_size() {
        let Some(romfs) = dev_dump() else { return };
        let mut remade = remade(&romfs).unwrap();
        let pack = ActorPack::from_bytes(SENSOR, remade.writer.to_binary()).unwrap();
        let param = param(&pack).unwrap();
        assert!(
            component(&param, field::AFFILIATION_REF).is_some(),
            "takes a side"
        );
        let attacks = resolved(&pack, &attack(&pack).unwrap()).unwrap();
        let attacks = attacks
            .get(field::ATTACK_PARAMS.get())
            .unwrap()
            .as_array()
            .unwrap();
        let freezer = ActorPack::load(&romfs, FREEZER).unwrap();
        let breath = breath(&freezer).unwrap();
        let theirs = breath
            .get(field::ATTACK_PARAMS.get())
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(attacks, theirs);
        let once = attacks[0].as_map().unwrap();
        let impulse = once.get(field::IMPULSE.get()).unwrap();
        assert_eq!(impulse.as_float().unwrap(), 0.0, "no shove");
        let set = resolved(&pack, &controller_set(&pack).unwrap()).unwrap();
        let body = resolved(&pack, &pack_path(&first(&set, BODY_SENSORS).unwrap())).unwrap();
        let layer = body
            .get(field::LAYER_SENSOR.get())
            .unwrap()
            .as_string()
            .unwrap();
        assert_eq!(layer.as_str(), ENEMY_ATTACK.get());
        let shape = resolved(&pack, &pack_path(&first(&set, SHAPES).unwrap())).unwrap();
        let cylinder = &shape
            .get(field::CYLINDER.get())
            .unwrap()
            .as_array()
            .unwrap()[0];
        let radius = cylinder.as_map().unwrap().get(field::RADIUS.get()).unwrap();
        assert_eq!(radius.as_float().unwrap(), RADIUS);
    }
}
