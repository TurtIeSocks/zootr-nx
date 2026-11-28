//! The Goron City spinning pot's heart piece as a check (EIE).
//!
//! The pot makes its prize as the map loads, from the heart piece slot of
//! its own holder, and drops it on the heart piece face. Its own code sets
//! the pot's Bool once that held prize is taken, and stops dropping it
//! after (`main` 0x26eec90, 0x26ee010). A world pickup in the slot drops,
//! but is taken by touch, never sets the Bool and comes back on every load
//! (Mac, 2026-11-21). So the slot holds a stand-in built on the heart
//! piece's own actor, which keeps its take: live, it set the Bool and
//! stayed gone after a reload. The drop scene binds the piece by name, so
//! it names the stand-in too.
//!
//! An item shown a step at a time shows its first step: the slot names one
//! actor, and no map file can name another.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{
    ActorPack, BymlDoc, EventKind, Flowchart, ModBuilder, Romfs, Value, pack_file,
};

use crate::checks::{Drop, ItemCheck};
use crate::flows::{self, Rewrites};
use crate::known::{Name, actor, field, flag, map, scene};
use crate::progressive::Lines;
use crate::settings::NewGame;
use crate::standins::{self, StandIn, Tables};
use crate::{gifts, shows};

/// The pot's placement, in child Goron City's static file.
pub const HASH: u64 = 14691060592470890818;
/// The pot.
const POT: &Name = actor::OBJ_GORON_CITY_LARGE_POT_A_01;
/// What it drops: the stand-ins' template.
const PIECE: &Name = actor::OBJ_HEART_PIECE;
/// Set by the pot's own code once its prize is taken.
const TAKEN: &Name = flag::GORON_LARGE_POT_IS_GET_HEART_PIECE;
/// The scene that drops the piece, binding it by name.
const DROP: &Name = scene::GORON_0045;
/// The scene's string params naming an actor.
const ACTOR_NAME: &str = "ActorName";
/// The slot's field naming what it holds.
const ACTOR: &str = "Actor";
/// The stand-in's actor.
const NEW: &str = "Obj_Rando_GoronPot";

/// The Banc file the pot is placed in.
fn file() -> String {
    format!(
        "Banc/{}/{}_Static.bcett.byml.zs",
        flag::GORON_CITY,
        map::GORON_CITY_CHILD
    )
}

/// The pot's check, dealt with the freestanding setting like every other
/// heart piece lying in the world.
pub fn check() -> ItemCheck {
    gifts::check(file(), flag::GORON_CITY, HASH, POT.to_string(), PIECE)
}

/// Whether `check` is the pot's.
pub fn is_pot(check: &ItemCheck) -> bool {
    check.hash == HASH && is_holder(&check.holder)
}

/// Whether `holder` is the pot.
pub fn is_holder(holder: &str) -> bool {
    *holder == **POT
}

/// The Bool the tracker ticks the pot's check by.
pub fn flag_at(hash: u64) -> Option<String> {
    (hash == HASH).then(|| TAKEN.to_string())
}

/// What the pot shows and gives for `item`: its first step when it's shown
/// a step at a time.
fn shown(item: &str, lines: Lines) -> &str {
    match shows::steps(item, lines) {
        Some(steps) => steps[0].0,
        None => item,
    }
}

/// Have the pot drop what the seed deals it, as a stand-in built on the
/// heart piece.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    drops: &HashMap<u64, Drop>,
    new_game: &NewGame,
) -> Result<()> {
    let item = gifts::item_at(drops, HASH, PIECE);
    if *item == **PIECE {
        return Ok(());
    }
    let shown = shown(item, Lines::of(new_game));
    StandIn {
        name: NEW,
        template: ActorPack::load(romfs, PIECE)?,
        item: standins::item_pack(romfs, shown, new_game)?,
        price: None,
        // Its drop has no placement of its own: the body moves.
        stand: standins::Stand::Body,
    }
    .build(builder, tables)
    .with_context(|| format!("the Goron pot's stand-in for {shown}"))?;
    hold(romfs, builder)
}

/// Name the stand-in in the drop scene, when the seed deals the pot
/// something else.
pub fn rename(romfs: &Romfs, rewrites: &mut Rewrites, drops: &HashMap<u64, Drop>) -> Result<()> {
    if *gifts::item_at(drops, HASH, PIECE) == **PIECE {
        return Ok(());
    }
    flows::edit(romfs, rewrites, DROP, bind)
}

/// Bind the stand-in wherever `flow` binds the heart piece: its actor, and
/// the params naming it.
fn bind(flow: &mut Flowchart) -> Result<()> {
    let actor = flow.actors.iter_mut().find(|a| a.name == **PIECE);
    actor
        .with_context(|| format!("{} has no {}", flow.name, PIECE))?
        .name = NEW.into();
    for event in &mut flow.events {
        let EventKind::Action { params, .. } = &mut event.kind else {
            continue;
        };
        for (key, value) in params.iter_mut() {
            if key == ACTOR_NAME && value.as_str() == Some(PIECE) {
                *value = Value::String(vec![NEW.into()]);
            }
        }
    }
    Ok(())
}

/// Name the stand-in in the pot's heart piece slot.
fn hold(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let pack = ActorPack::load(romfs, POT)?;
    let path = standins::path_of(&pack, field::ACTOR_HOLDER_REF)?;
    let old = standins::raw(&pack, &path)?;
    let new = held(&old)?;
    builder.resize(&path, old.len(), new.len())?;
    let mut writer = pack.writer();
    writer.add_file(path, new);
    builder.write(&pack_file(POT), &writer.to_binary())
}

/// The pot's holder file `old`, its heart piece slot naming the stand-in.
fn held(old: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(old)?;
    let slot = (doc.root.as_mut_map()?)
        .get_mut(&**field::ACTOR_HOLD_SETTINGS)
        .and_then(|s| s.as_mut_map().ok())
        .and_then(|s| s.get_mut(&**field::PRIZE_HEART_PIECE))
        .with_context(|| format!("{POT} has no {}", field::PRIZE_HEART_PIECE))?;
    slot.as_mut_map()?
        .insert(ACTOR.into(), standins::pouch_actor(NEW));
    Ok(doc.to_binary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{GYAML, actors, hash_of, str_field};
    use crate::game::dev_dump;
    use crate::known::item;

    #[test]
    fn the_pot_stands_where_its_check_says() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let doc = BymlDoc::parse(&romfs.read(&file()).unwrap()).unwrap();
        let actors = actors(&doc.root).unwrap();
        let placed = actors.iter().find(|a| hash_of(a) == Some(HASH));
        assert_eq!(str_field(placed.unwrap(), GYAML), Some(POT.get()));
    }

    #[test]
    fn the_slot_names_the_stand_in() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let pack = ActorPack::load(&romfs, POT).unwrap();
        let path = standins::path_of(&pack, field::ACTOR_HOLDER_REF).unwrap();
        let new = held(&standins::raw(&pack, &path).unwrap()).unwrap();
        let doc = BymlDoc::parse(&new).unwrap();
        let slot = &doc.root.as_map().unwrap()[&**field::ACTOR_HOLD_SETTINGS]
            .as_map()
            .unwrap()[&**field::PRIZE_HEART_PIECE];
        let actor = slot.as_map().unwrap()[ACTOR].as_string().unwrap();
        assert!(actor.contains(&format!("/{NEW}.")), "{actor}");
    }

    #[test]
    fn the_drop_scene_binds_the_stand_in() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, DROP).unwrap().unwrap();
        bind(&mut flow).unwrap();
        let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert!(flow.actors.iter().any(|a| a.name == NEW));
        assert!(!flow.actors.iter().any(|a| a.name == **PIECE));
        let named = |e: &ounce_formats::Event| match &e.kind {
            EventKind::Action { params, .. } => (params.iter())
                .filter(|(k, _)| k == ACTOR_NAME)
                .filter_map(|(_, v)| v.as_str().map(str::to_owned))
                .collect(),
            _ => vec![],
        };
        let names: Vec<String> = flow.events.iter().flat_map(named).collect();
        assert!(names.iter().any(|n| n == NEW), "{names:?}");
        assert!(!names.iter().any(|n| n == PIECE.get()), "{names:?}");
    }

    #[test]
    fn freestanding_deals_it_and_the_tracker_knows_its_flag() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let dealt = |freestanding| {
            let new_game = NewGame {
                freestanding,
                ..NewGame::default()
            };
            let checks = crate::checks::dealt(&romfs, &new_game).unwrap();
            checks.iter().any(|c| c.hash == HASH)
        };
        assert!(dealt(true));
        assert!(!dealt(false));
        assert_eq!(flag_at(HASH), Some(TAKEN.to_string()));
        assert_eq!(flag_at(HASH + 1), None);
    }

    #[test]
    fn the_shuffle_takes_it_with_any_item() {
        needs_names!();
        let open = crate::narrowing::Let::of(&NewGame::default());
        assert!(crate::narrowing::shuffleable(&check(), open));
        for item in [item::HOOKSHOT, item::BOOMERANG, item::DUNGEON_SMALL_KEY] {
            assert!(crate::narrowing::allowed(&check(), item, true), "{item}");
        }
    }

    #[test]
    fn a_line_shows_its_first_step() {
        needs_names!();
        let lines = Lines::widest(true);
        assert_eq!(shown(item::LONGSHOT, lines), item::HOOKSHOT.get());
        assert_eq!(shown(item::HOOKSHOT, lines), item::HOOKSHOT.get());
        assert_eq!(shown(item::BOOMERANG, lines), item::BOOMERANG.get());
    }
}
