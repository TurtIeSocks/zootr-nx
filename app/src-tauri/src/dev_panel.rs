//! The dev panel's commands: what it picks from, and the requests it
//! sends the running game.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use zootr::config::{Config, Target};
use zootr::dev::{self, Dev, Place};
use zootr::live::actors::Hidden;
use zootr::live::{self, Memory};
use zootr::warp::{self, Side};

use crate::{Live, Res, blocking, cached, romfs};

/// What the dev panel, the test setup and plando pick from.
#[derive(Serialize, Type)]
pub(crate) struct GameOptions {
    /// Every arrival point, `MAP/SPOT`, both ages.
    places: Vec<String>,
    /// Child `MAP/SPOT`s a new game can start at.
    spawns: Vec<String>,
    /// Pouch item actors.
    items: Vec<String>,
    /// GameData names, sorted.
    names: Vec<String>,
    /// Each quest and its steps, in order.
    quests: BTreeMap<String, Vec<String>>,
}

/// Every arrival point, read from the dump once.
#[derive(Default)]
pub(crate) struct Places(Mutex<Option<Arc<BTreeSet<Place>>>>);

impl Places {
    fn get(&self, config: &Config) -> anyhow::Result<Arc<BTreeSet<Place>>> {
        cached(&self.0, || dev::places(&romfs(config)?))
    }
}

/// The game the panel last attached to, and through which target. The
/// panel polls every second, and attaching each time read and scanned all
/// of `main` on the Mac, and dropped the values' kept places.
static ATTACHED: Mutex<Option<(Target, Arc<Dev>)>> = Mutex::new(None);

/// The running game, through the picked target: the one attached before
/// while it still answers a read, or a new attach.
fn dev_game() -> anyhow::Result<Arc<Dev>> {
    let target = Config::load()?.target()?;
    let mut kept = ATTACHED.lock().unwrap();
    if let Some((was, dev)) = kept.as_ref()
        && *was == target
        && dev.game().u64(dev.global()).is_ok()
    {
        return Ok(dev.clone());
    }
    *kept = None;
    let dev = Arc::new(Dev::new(live::Game::attach(&target)?)?);
    *kept = Some((target, dev.clone()));
    Ok(dev)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn game_options(app: AppHandle) -> Res<GameOptions> {
    blocking(move || {
        let config = Config::load()?;
        let romfs = romfs(&config)?;
        let places = app.state::<Places>().get(&config)?;
        let gd = app.state::<Live>().gamedata(&config)?;
        let mut names: Vec<String> = gd.names().map(Into::into).collect();
        names.sort_unstable();
        Ok(GameOptions {
            places: places.iter().map(ToString::to_string).collect(),
            spawns: zootr::spawn::destinations(&romfs)?.into_iter().collect(),
            items: zootr::items::scan(&romfs)?
                .into_iter()
                .map(|i| i.actor)
                .collect(),
            names,
            quests: zootr::steps::quests(&gd),
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_state() -> Res<dev::State> {
    blocking(|| dev_game()?.state()).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_teleport(place: String) -> Res<String> {
    blocking(move || Ok(dev_game()?.go(&Place::parse(&place)?)?.into())).await
}

/// One placement the panel's warp lists.
#[derive(Serialize, Type)]
pub(crate) struct DevActor {
    actor: String,
    /// Its instance hash, as text: a u64 doesn't fit a JavaScript number.
    hash: String,
    /// Its own `Name`, telling it from the actor's other placements.
    label: Option<String>,
    /// Where the map's files place it, for sorting by distance.
    pos: [f32; 3],
}

/// Every placement `map` has, for the panel's warp.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_actors(map: String) -> Res<Vec<DevActor>> {
    blocking(move || {
        let placed = warp::actors_in(&romfs(&Config::load()?)?, &map)?;
        Ok(placed
            .into_iter()
            .map(|p| DevActor {
                hash: p.hash.unwrap_or_default().to_string(),
                actor: p.name,
                label: p.label,
                pos: p.pose.at,
            })
            .collect())
    })
    .await
}

/// Where Link stands, for sorting the map's placements by distance.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_link() -> Res<[f32; 3]> {
    blocking(|| dev_game()?.link()).await
}

/// Warp Link to `side` of a placed actor of the map he's in.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_warp(actor: String, side: Side) -> Res<String> {
    blocking(move || warp::beside(&*dev_game()?, &romfs(&Config::load()?)?, &actor, side)).await
}

/// A live actor a placement made, for the panel's Live actors.
#[derive(Serialize, Type)]
pub(crate) struct DevNear {
    /// The placement's hash, as text: a u64 doesn't fit a JS number.
    hash: String,
    actor: String,
    /// The placement's own `Name`, when the map's files have it: an actor
    /// a seed adds has none.
    label: Option<String>,
    distance: f32,
    hidden: Hidden,
}

/// The live actors placements made, nearest Link first.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_nearby() -> Res<Vec<DevNear>> {
    blocking(move || {
        let dev = dev_game()?;
        let near = dev.nearby()?.0;
        let placed = warp::actors_in(&romfs(&Config::load()?)?, &dev.state()?.map)?;
        let labels: BTreeMap<u64, String> = (placed.into_iter())
            .filter_map(|p| Some((p.hash?, p.label?)))
            .collect();
        Ok(near
            .into_iter()
            .map(|n| DevNear {
                hash: n.hash.to_string(),
                label: labels.get(&n.hash).cloned(),
                actor: n.name,
                distance: n.distance,
                hidden: n.hidden,
            })
            .collect())
    })
    .await
}

/// Hide the live actors of these placements, or show them again. Answers
/// the hashes that had one.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_hide(hashes: Vec<String>, on: bool) -> Res<Vec<String>> {
    blocking(move || {
        let hashes = (hashes.iter())
            .map(|h| {
                h.parse()
                    .with_context(|| format!("{h} isn't a placement hash"))
            })
            .collect::<anyhow::Result<Vec<u64>>>()?;
        let found = dev_game()?.hide(&hashes, on)?;
        Ok(found.into_iter().map(|h| h.to_string()).collect())
    })
    .await
}

/// Switch age, and say where Link lands.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_age(app: AppHandle) -> Res<String> {
    blocking(move || {
        let config = Config::load()?;
        let places = app.state::<Places>().get(&config)?;
        dev_game()?.switch_age(&romfs(&config)?, &places)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_give(app: AppHandle, item: String, count: i32) -> Res<String> {
    blocking(move || {
        let gd = app.state::<Live>().gamedata(&Config::load()?)?;
        dev_game()?.give(&gd, &item, count)?;
        Ok(format!(
            "gave {item} ×{count}, open the menu again to see it"
        ))
    })
    .await
}

/// Move quest `challenge` to `step` through the game's quest system, as
/// its own scenes do. Writing its saved step leaves the quest where it was.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_step(app: AppHandle, challenge: String, step: String) -> Res<String> {
    blocking(move || {
        let gd = app.state::<Live>().gamedata(&Config::load()?)?;
        zootr::steps::check(&gd, &challenge, &step)?;
        dev_game()?.step(&challenge, &step)?;
        Ok(format!("{challenge} moved to {step}"))
    })
    .await
}

/// What a GameData name is, the values it takes, its default and what the
/// game holds now. Reads memory only, so it needs no hook.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_value(app: AppHandle, name: String) -> Res<dev::Value> {
    blocking(move || {
        let config = Config::load()?;
        let gd = app.state::<Live>().gamedata(&config)?;
        let dev = dev_game().ok();
        dev::value(&gd, name.trim(), dev.as_deref().map(Dev::game))
    })
    .await
}

/// Write a GameData value, or read it when `value` is empty. Refuses a
/// value the name doesn't take. Says the value it holds after.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_set(app: AppHandle, name: String, value: String) -> Res<String> {
    blocking(move || {
        let gd = app.state::<Live>().gamedata(&Config::load()?)?;
        let value = value.trim();
        if !value.is_empty() {
            gd.info(&name)?.check(&name, value)?;
        }
        let (dev, hash) = (dev_game()?, live::hash(&name)?);
        if !value.is_empty() {
            dev.game().set(hash, value)?;
        }
        Ok(format!("{name} = {}", dev.game().get(&gd, hash)?))
    })
    .await
}

/// Turn the dev switches on or off, and say what the game holds after.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_switches(switches: dev::Switches) -> Res<dev::Switches> {
    blocking(move || {
        let dev = dev_game()?;
        dev.set_switches(switches)?;
        dev.switches()
    })
    .await
}

/// Apply a saved test setup live, saying what it did.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_apply(app: AppHandle, name: String) -> Res<dev::Applied> {
    blocking(move || {
        let config = Config::load()?;
        let setup = (config.saved_tests.get(&name))
            .with_context(|| format!("no saved test setup is named {name}"))?;
        let gd = app.state::<Live>().gamedata(&config)?;
        let game = dev_game()?;
        let near = zootr::warp::near(&romfs(&config)?, setup, || Ok(game.state()?.map))?;
        game.apply(&gd, setup, near)
    })
    .await
}

/// The goals of every saved test setup that has any, read in one attach.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dev_goals(app: AppHandle) -> Res<BTreeMap<String, Vec<dev::Goal>>> {
    blocking(move || {
        let config = Config::load()?;
        let mut tests = config
            .saved_tests
            .iter()
            .filter(|(_, s)| !s.goals.is_empty())
            .peekable();
        if tests.peek().is_none() {
            return Ok(BTreeMap::new());
        }
        let (gd, game) = (app.state::<Live>().gamedata(&config)?, dev_game()?);
        Ok(tests
            .map(|(name, s)| (name.clone(), game.goals(&gd, s)))
            .collect())
    })
    .await
}
