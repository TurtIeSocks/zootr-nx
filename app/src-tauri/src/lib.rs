//! The zootr window: Tauri commands over the `zootr` library.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, anyhow, bail};
use generating::Generating;
use ounce_formats::Romfs;
use serde::Serialize;
use specta::Type;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_specta::{Event as _, collect_commands, collect_events};
use zootr::config::Config;
use zootr::gamedata::GameData;
use zootr::generate::plando::PlandoOptions;
use zootr::generate::{self, RoundEvent, Spot, Start};
use zootr::history::{History, Label, Mark, Summary, View};
use zootr::inventory::{self, Cell, Inventory, TrackerOptions};
use zootr::platform;
use zootr::progress::{self, Run};
use zootr::recorder::{self, Ask, Event, Hub, Numbered, Snapshot, Watch};
use zootr::savefile::{self, Change};
use zootr::settings::SettingDef;
use zootr::share;
use zootr::tracker::{self, Row, Tracker};

mod dev_panel;
mod generating;
mod save_editor;
mod targets;
mod ui;

/// Where the bindings are written, from this crate's folder.
const BINDINGS: &str = "../src/bindings.ts";

/// Errors reach the UI as the whole chain on one line.
type Res<T> = Result<T, String>;

fn text(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[derive(Serialize, Type)]
struct TargetStatus {
    name: String,
    app: bool,
    mod_dir: bool,
    running: bool,
}

#[derive(Serialize, Type)]
struct Status {
    /// The game dump's path exists. Save already extracted it.
    game: bool,
    targets: Vec<TargetStatus>,
}

#[tauri::command]
#[specta::specta]
fn status() -> Res<Status> {
    let config = Config::load().map_err(text)?;
    let targets = config.targets.iter().map(|t| TargetStatus {
        name: t.name.clone(),
        app: t.app.exists(),
        mod_dir: t.mod_dir.is_dir(),
        running: t.running(),
    });
    Ok(Status {
        game: config.game.is_some_and(|g| g.exists()),
        targets: targets.collect(),
    })
}

#[tauri::command]
#[specta::specta]
fn schema() -> Vec<SettingDef> {
    zootr::settings::shown()
}

#[tauri::command]
#[specta::specta]
fn config() -> Res<Config> {
    Config::load().map_err(text)
}

/// Refuse a changed test setup whose quest step names a quest or step the
/// game doesn't have, saying which. Setups already saved aren't read
/// again, and without a game dump there is nothing to check against.
fn check_steps(old: &Config, config: &Config) -> anyhow::Result<()> {
    let tests = |c: &'_ Config| {
        let saved = c
            .saved_tests
            .iter()
            .map(|(n, t)| (Some(n.clone()), t.clone()));
        std::iter::once((None, c.test.clone()))
            .chain(saved)
            .collect::<Vec<_>>()
    };
    let before = tests(old);
    let changed: Vec<_> = tests(config)
        .into_iter()
        .filter(|t| !t.1.progress.is_empty() && !before.contains(t))
        .collect();
    if changed.is_empty() || config.game.is_none() {
        return Ok(());
    }
    let gd = GameData::load(&romfs(config)?)?;
    for (name, setup) in changed {
        zootr::steps::parse_all(&gd, &setup.progress).with_context(|| match name {
            Some(name) => format!("the saved setup {name}"),
            None => "the test setup".to_owned(),
        })?;
    }
    Ok(())
}

/// Saves the config, and says which targets' mod folders it moved out of
/// the game's mods root.
#[tauri::command]
#[specta::specta]
async fn set_config(app: AppHandle, config: Config) -> Res<Vec<String>> {
    blocking(move || {
        // A new dump is extracted now, so a bad file fails at Save and the
        // first Generate doesn't wait on it. A dump already cached is reused.
        let old = Config::load()?;
        // The UI's config never carries the tracker's options.
        let mut config = Config {
            tracker: old.tracker.clone(),
            ..config
        };
        let notes = config.fix_mods_roots();
        if let Some(game) = config
            .game
            .as_ref()
            .filter(|&g| old.game.as_ref() != Some(g))
        {
            zootr::game::prepare(game, &Run::silent())?;
        }
        check_steps(&old, &config)?;
        config.save()?;
        if game_art(&old) != game_art(&config) || old.game != config.game {
            app.state::<Items>().emit(&app);
        }
        Ok(notes)
    })
    .await
}

/// The game files from the config's dump.
fn romfs(config: &Config) -> anyhow::Result<Romfs> {
    let game = config.game.as_ref().context("Set the game dump first")?;
    zootr::game::prepare(game, &Run::silent())
}

/// The `zootr shuffle` command that Generate stands for, with the current
/// settings and test setup.
#[tauri::command]
#[specta::specta]
fn shuffle_command() -> Res<String> {
    let config = Config::load().map_err(text)?;
    Ok(zootr::cli::shuffle_command(
        &config.settings,
        &config.extras(),
    ))
}

/// Run `f` off the UI thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

/// Generate and install a seed with the current settings and test setup. A
/// blank seed is random. A share code brings its own settings and extras.
#[tauri::command]
#[specta::specta]
async fn generate(
    app: AppHandle,
    seed: Option<String>,
    steps: Channel<progress::StepEvent>,
) -> Res<Summary> {
    blocking(move || {
        let config = Config::load()?;
        let extras = config.extras();
        let (seed, settings, extras) = match seed.as_deref().map(str::trim) {
            Some(code) if share::is_code(code) => {
                let shared = share::decode(code)?;
                (shared.seed, shared.settings, shared.extras)
            }
            None | Some("") => (generate::time_seed(), config.settings, extras),
            Some(s) => (
                s.parse().map_err(|_| anyhow!("A seed is a whole number"))?,
                config.settings,
                extras,
            ),
        };
        let report = (app.state::<Generating>()).install(steps, seed, &settings, &extras)?;
        History::record(settings, extras, Label::default(), report, true)
    })
    .await
}

/// What the plando draft can pick from with the current settings, and why
/// the deal refuses it, if it does.
#[tauri::command]
#[specta::specta]
async fn plando_options() -> Res<PlandoOptions> {
    blocking(move || {
        let config = Config::load()?;
        generate::plando::options(&romfs(&config)?, &config.settings, &config.place)
    })
    .await
}

/// Reinstall a history entry from its seed and settings.
#[tauri::command]
#[specta::specta]
async fn install(app: AppHandle, id: u32, steps: Channel<progress::StepEvent>) -> Res<()> {
    blocking(move || {
        let mut history = History::load()?;
        let entry = history.get(id)?;
        let generating = app.state::<Generating>();
        generating.install(steps, entry.report.seed, &entry.settings, &entry.extras)?;
        history.install(Some(id));
        history.save()
    })
    .await
}

/// Take the mod off every target so the game runs vanilla.
#[tauri::command]
#[specta::specta]
async fn uninstall() -> Res<()> {
    blocking(|| {
        generate::uninstall(&Config::load()?.targets)?;
        let mut history = History::load()?;
        history.install(None);
        history.save()
    })
    .await
}

#[tauri::command]
#[specta::specta]
fn history() -> Res<View> {
    History::load().map(|h| h.view()).map_err(text)
}

/// Change history and save it.
fn edit_history(f: impl FnOnce(&mut History) -> anyhow::Result<()>) -> Res<()> {
    let mut history = History::load().map_err(text)?;
    f(&mut history).map_err(text)?;
    history.save().map_err(text)
}

#[tauri::command]
#[specta::specta]
fn delete_seed(id: u32) -> Res<()> {
    edit_history(|h| h.remove(id))
}

/// Name a seed and note what it's for.
#[tauri::command]
#[specta::specta]
fn label_seed(id: u32, label: Label) -> Res<()> {
    edit_history(|h| h.relabel(id, label))
}

/// Delete every seed but the installed one.
#[tauri::command]
#[specta::specta]
fn clear_history() -> Res<()> {
    edit_history(|h| {
        h.clear();
        Ok(())
    })
}

/// A seed's spots with their logic, and the checks logic can't show a way into.
#[derive(Serialize, Type)]
struct Spoiler {
    spots: Vec<Spot>,
    events: Vec<RoundEvent>,
    unprovable: Vec<String>,
    start: Start,
}

#[tauri::command]
#[specta::specta]
fn spoiler(id: u32) -> Res<Spoiler> {
    let history = History::load().map_err(text)?;
    let report = &history.get(id).map_err(text)?.report;
    Ok(Spoiler {
        spots: report.spots.clone(),
        events: report.events.clone(),
        unprovable: report.unprovable.clone(),
        start: report.start.clone(),
    })
}

#[tauri::command]
#[specta::specta]
fn play(target: String) -> Res<()> {
    let config = Config::load().map_err(text)?;
    let Some(t) = config.targets.iter().find(|t| t.name == target) else {
        return Err(format!("No target named {target}"));
    };
    let args = t.launch_args(config.game.as_deref()).map_err(text)?;
    let env = t.launch_env().map_err(text)?;
    platform::open_app(&t.app, &args, &env)
        .map_err(|_| format!("Couldn't open {}", t.app.display()))
}

/// Every Live tab event, to whichever page is listening.
#[derive(Clone, Serialize, Type)]
struct LiveEvent(Numbered);

impl tauri_specta::Event for LiveEvent {
    const NAME: &'static str = "live-event";
}

/// The Live tab's sessions, and the GameData names they show, loaded once.
struct Live {
    gd: Mutex<Option<Arc<GameData>>>,
    hub: Arc<Hub>,
    watch: Mutex<Option<Watch>>,
}

impl Live {
    fn new(app: AppHandle) -> Self {
        Self {
            gd: Mutex::default(),
            hub: Hub::new(move |n| {
                if let Event::Row { change, .. } = &n.event {
                    app.state::<Checks>().change(&app, change);
                    app.state::<Items>().change(&app, change);
                }
                let _ = LiveEvent(n.clone()).emit(&app);
            }),
            watch: Mutex::default(),
        }
    }

    fn gamedata(&self, config: &Config) -> anyhow::Result<Arc<GameData>> {
        let mut gd = self.gd.lock().unwrap();
        if gd.is_none() {
            *gd = Some(Arc::new(recorder::gamedata(&romfs(config)?)?));
        }
        Ok(gd.clone().expect("just loaded"))
    }

    /// Records the picked target whenever it runs, for as long as the app
    /// is open.
    fn watch(&self, app: &AppHandle) -> anyhow::Result<()> {
        let app = app.clone();
        let gd = move || {
            let gd = app.state::<Live>().gamedata(&Config::load()?);
            // Ready before the first row, so no flip is missed.
            let _ = app.state::<Checks>().state(&app);
            let _ = app.state::<Items>().state(&app);
            gd
        };
        let target = || Config::load()?.target();
        let logs = recorder::logs()?;
        *self.watch.lock().unwrap() = Some(Watch::start(self.hub.clone(), gd, target, logs));
        Ok(())
    }
}

/// Where the Live tab picks up after a reload.
#[tauri::command]
#[specta::specta]
fn live_state(live: State<Live>) -> Snapshot {
    live.hub.snapshot()
}

/// Label an unnamed value, or drop its label for an empty one.
#[tauri::command]
#[specta::specta]
fn live_label(live: State<Live>, hash: u32, label: String) -> Res<()> {
    zootr::labels::set(&zootr::labels::path().map_err(text)?, hash, &label).map_err(text)?;
    if let Some(gd) = &*live.gd.lock().unwrap() {
        gd.set_label(hash, &label);
    }
    Ok(())
}

/// Mark the session's log with what happened. Blank marks it "Mark".
#[tauri::command]
#[specta::specta]
fn live_mark(live: State<Live>, what: String) -> Res<()> {
    live.hub.ask(Ask::Mark(what)).map_err(text)
}

/// Leave a value out of the session from now on.
#[tauri::command]
#[specta::specta]
fn live_mute(live: State<Live>, hash: u32) -> Res<()> {
    live.hub.ask(Ask::Mute(hash)).map_err(text)
}

/// A spot checked or unchecked in the installed seed.
#[derive(Clone, Serialize, Type)]
struct ChecksEvent {
    hash: String,
    checked: bool,
}

impl tauri_specta::Event for ChecksEvent {
    const NAME: &'static str = "checks-event";
}

#[derive(Serialize, Type)]
struct ChecksState {
    /// The installed seed's history entry, if one is installed.
    id: Option<u32>,
    rows: Vec<Row>,
}

/// The installed seed's tracker, kept current by the watcher's rows.
#[derive(Default)]
struct Checks(Mutex<Option<(u32, Tracker)>>);

impl Checks {
    /// Builds the tracker for a newly installed seed from the last save,
    /// first finding places for a seed saved before they were kept.
    fn state(&self, app: &AppHandle) -> anyhow::Result<ChecksState> {
        let mut history = History::load()?;
        let Some(id) = history.installed else {
            *self.0.lock().unwrap() = None;
            return Ok(ChecksState {
                id: None,
                rows: vec![],
            });
        };
        let mut tracker = self.0.lock().unwrap();
        if tracker.as_ref().is_none_or(|(had, _)| *had != id) {
            let config = Config::load()?;
            if history
                .get(id)?
                .report
                .spots
                .iter()
                .all(|s| s.place.is_empty())
            {
                history.place(id, &romfs(&config)?)?;
                history.save()?;
            }
            let gd = app.state::<Live>().gamedata(&config)?;
            let mut t = Tracker::new(&gd, &history.get(id)?.report.spots)?;
            // Only a save played since this seed went in: an older one
            // holds another seed's checks.
            let save = savefile::newest(&config.save_dir()?)?
                .filter(|s| savefile::written_since(s, history.installed_at));
            if let Some(save) = save {
                t.load(tracker::in_save(&savefile::load(&save)?));
            }
            *tracker = Some((id, t));
        }
        let (_, t) = tracker.as_ref().expect("just built");
        let gd = app.state::<Live>().gd.lock().unwrap().clone();
        Ok(ChecksState {
            id: Some(id),
            rows: t.rows(history.get(id)?, |f| gd.as_ref()?.label(f)),
        })
    }

    fn change(&self, app: &AppHandle, change: &Change) {
        let Some((_, t)) = &mut *self.0.lock().unwrap() else {
            return;
        };
        let flips = t.change(change);
        let events = flips.checked.iter().map(|h| (h, true));
        for (hash, checked) in events.chain(flips.unchecked.iter().map(|h| (h, false))) {
            let hash = hash.to_string();
            let _ = ChecksEvent { hash, checked }.emit(app);
        }
    }
}

/// The Checks tab's rows for the installed seed.
#[tauri::command]
#[specta::specta]
async fn checks_state(app: AppHandle) -> Res<ChecksState> {
    blocking(move || app.state::<Checks>().state(&app)).await
}

/// Mark a check in a seed, or clear its mark.
#[tauri::command]
#[specta::specta]
fn checks_mark(id: u32, hash: String, mark: Option<Mark>) -> Res<()> {
    let hash = hash.parse().map_err(|_| format!("{hash} isn't a spot"))?;
    edit_history(|h| h.mark(id, hash, mark))
}

/// What the Items tab and the pop out draw.
#[derive(Clone, Serialize, Type)]
struct ItemsState {
    cells: Vec<Cell>,
    options: TrackerOptions,
    /// The icons come from the dump: game assets are on and a dump is set.
    game_art: bool,
}

/// The grid changed, or its options did.
#[derive(Clone, Serialize, Type)]
struct ItemsEvent(ItemsState);

impl tauri_specta::Event for ItemsEvent {
    const NAME: &'static str = "items-event";
}

/// What Link has, from the last save and then the watcher's rows.
#[derive(Default)]
struct Items(Mutex<Option<Inventory>>);

impl Items {
    fn state(&self, app: &AppHandle) -> anyhow::Result<ItemsState> {
        let config = Config::load()?;
        let mut items = self.0.lock().unwrap();
        if items.is_none() {
            let gd = app.state::<Live>().gamedata(&config)?;
            let mut inv = Inventory::new(&gd, &romfs(&config)?)?;
            if let Some(save) = savefile::newest(&config.save_dir()?)? {
                inv.load(inventory::in_save(&savefile::load(&save)?));
            }
            *items = Some(inv);
        }
        Ok(ItemsState {
            cells: items.as_ref().expect("just built").cells(),
            game_art: game_art(&config),
            options: config.tracker.tidy(),
        })
    }

    fn change(&self, app: &AppHandle, change: &Change) {
        let changed = (self.0.lock().unwrap().as_mut()).is_some_and(|inv| inv.change(change));
        if changed {
            self.emit(app);
        }
    }

    /// Tells every window, so the tab and the pop out never disagree.
    fn emit(&self, app: &AppHandle) {
        if let Ok(state) = self.state(app) {
            let _ = ItemsEvent(state).emit(app);
        }
    }
}

#[tauri::command]
#[specta::specta]
async fn items_state(app: AppHandle) -> Res<ItemsState> {
    blocking(move || app.state::<Items>().state(&app)).await
}

#[tauri::command]
#[specta::specta]
async fn items_options(app: AppHandle, options: TrackerOptions) -> Res<()> {
    blocking(move || {
        let config = Config {
            tracker: options.tidy(),
            ..Config::load()?
        };
        config.save()?;
        app.state::<Items>().emit(&app);
        Ok(())
    })
    .await
}

/// The window OBS captures: the grid alone, from `tauri.conf.json`.
const TRACKER: &str = "tracker";
/// The dev panel's window, from `tauri.conf.json`.
const DEV: &str = "dev";

/// Opens the window `tauri.conf.json` names `label`, or brings it forward.
fn pop_out(app: &AppHandle, label: &str) -> Res<()> {
    if let Some(window) = app.get_webview_window(label) {
        return window.set_focus().map_err(|e| e.to_string());
    }
    let config = app.config().app.windows.iter().find(|w| w.label == label);
    let config = config.ok_or(format!("no {label} window in tauri.conf.json"))?;
    tauri::WebviewWindowBuilder::from_config(app, config)
        .and_then(|b| b.build())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Opens the items pop out, or brings it forward.
#[tauri::command]
#[specta::specta]
async fn items_pop_out(app: AppHandle) -> Res<()> {
    pop_out(&app, TRACKER)
}

#[tauri::command]
#[specta::specta]
async fn dev_pop_out(app: AppHandle) -> Res<()> {
    pop_out(&app, DEV)
}

/// Serves `<icon>.png` from the icons built out of the dump, at
/// `zootr-icon://localhost/<icon>.png`.
const ICONS: &str = "zootr-icon";

/// Whether the icons come from the dump: game assets on, and a dump set.
fn game_art(config: &Config) -> bool {
    config.game_assets && config.game.is_some()
}

/// The dump to read icons from, refused while game assets are off.
fn icon_dump(config: &Config) -> anyhow::Result<PathBuf> {
    if !config.game_assets {
        bail!("Turn on game assets in Settings first");
    }
    config.game.clone().context("Set the game dump first")
}

/// The icons folder, kept with the dump it was built from.
#[derive(Default)]
struct Icons(Mutex<Option<(PathBuf, PathBuf)>>);

impl Icons {
    /// The dump's icon folder, built on first use. Callers wait while
    /// another builds it. Refused while game assets are off.
    fn folder(&self) -> anyhow::Result<PathBuf> {
        let config = Config::load()?;
        let game = icon_dump(&config)?;
        let mut cached = self.0.lock().unwrap();
        if cached.as_ref().is_none_or(|(had, _)| *had != game) {
            let dir = zootr::sprites::folder(&romfs(&config)?, &game)?;
            *cached = Some((game, dir));
        }
        Ok(cached.as_ref().expect("just built").1.clone())
    }

    fn read(&self, name: &str) -> anyhow::Result<Vec<u8>> {
        let icon = name.strip_suffix(".png").unwrap_or_default();
        if icon.is_empty() || !icon.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            bail!("{name} isn't an icon");
        }
        Ok(std::fs::read(self.folder()?.join(name))?)
    }
}

/// Resolves once the icons are built, so the grid can say it's waiting.
#[tauri::command]
#[specta::specta]
async fn items_icons(app: AppHandle) -> Res<()> {
    blocking(move || app.state::<Icons>().folder().map(|_| ())).await
}

/// Show a session log in the system's file browser.
#[tauri::command]
#[specta::specta]
fn reveal_log(path: PathBuf) -> Res<()> {
    if !path.starts_with(recorder::logs().map_err(text)?) {
        return Err(format!("{} isn't a session log", path.display()));
    }
    platform::reveal(&path).map_err(|_| format!("Couldn't show {}", path.display()))
}

fn commands() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::new()
        .commands(collect_commands![
            status,
            schema,
            config,
            set_config,
            generate,
            install,
            generating::stop_generate,
            generating::generate_log,
            uninstall,
            history,
            delete_seed,
            label_seed,
            clear_history,
            spoiler,
            play,
            targets::import_target,
            targets::export_target,
            targets::save_target,
            targets::read_target_file,
            shuffle_command,
            plando_options,
            save_editor::save_slots,
            save_editor::save_presets,
            save_editor::save_items,
            save_editor::save_click,
            save_editor::save_stage,
            save_editor::save_flags,
            save_editor::save_write,
            live_state,
            live_label,
            live_mark,
            live_mute,
            reveal_log,
            checks_state,
            checks_mark,
            items_state,
            items_options,
            items_icons,
            items_pop_out,
            dev_pop_out,
            dev_panel::game_options,
            dev_panel::dev_state,
            dev_panel::dev_teleport,
            dev_panel::dev_actors,
            dev_panel::dev_warp,
            dev_panel::dev_age,
            dev_panel::dev_give,
            dev_panel::dev_step,
            dev_panel::dev_value,
            dev_panel::dev_set,
            dev_panel::dev_switches,
            dev_panel::dev_apply,
            dev_panel::dev_goals,
            ui::ui_load,
            ui::ui_save,
        ])
        .events(collect_events![
            LiveEvent,
            ChecksEvent,
            ItemsEvent,
            ui::UiShared
        ])
        .constant("TRACKER_COLUMNS", inventory::COLUMNS)
        .constant("STEPS", progress::names())
        // Only setting values (JSON numbers) and small counts reach this: the
        // u64 seeds and hashes go over as strings.
        .dangerously_cast_bigints_to_number()
        // One TS type per Rust type: the UI sends back what it was given.
        .disable_serde_phases()
}

/// Write `src/bindings.ts`, the commands and their types in TypeScript.
pub fn export_bindings(path: impl AsRef<Path>) -> Result<(), specta_typescript::Error> {
    commands().export(specta_typescript::Typescript::default(), path)
}

pub fn run() -> tauri::Result<()> {
    #[cfg(debug_assertions)]
    if let Err(e) = export_bindings(Path::new(env!("CARGO_MANIFEST_DIR")).join(BINDINGS)) {
        eprintln!("couldn't write the bindings: {e}");
    }
    let builder = commands();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .register_asynchronous_uri_scheme_protocol(ICONS, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let response = match app
                    .state::<Icons>()
                    .read(request.uri().path().trim_start_matches('/'))
                {
                    Ok(png) => tauri::http::Response::builder()
                        .header(tauri::http::header::CONTENT_TYPE, "image/png")
                        .body(png),
                    Err(_) => tauri::http::Response::builder().status(404).body(vec![]),
                };
                responder.respond(response.expect("a plain response"));
            });
        })
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(Checks::default());
            app.manage(Items::default());
            app.manage(Generating::default());
            app.manage(save_editor::Layout::default());
            app.manage(Icons::default());
            app.manage(dev_panel::Places::default());
            app.manage(ui::UiFile::default());
            // Built now, so the Items tab rarely waits on a new dump.
            let handle = app.handle().clone();
            std::thread::spawn(move || handle.state::<Icons>().folder());
            app.manage(Live::new(app.handle().clone()));
            Ok(app.state::<Live>().watch(app.handle())?)
        })
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn a_changed_setup_with_a_bad_quest_step_is_refused_when_saved() {
        let dump = Path::new("../../dump/oot.nup");
        let setup = |line: &str| zootr::config::TestSetup {
            progress: vec![line.into()],
            ..Default::default()
        };
        let old = Config::default();
        let with = |test: zootr::config::TestSetup| Config {
            game: dump.exists().then(|| dump.to_path_buf()),
            test,
            ..Default::default()
        };
        // Nothing to read, or nothing new: nothing is checked.
        // Text: no dump is open yet, so a name can't be read here.
        let good = with(setup("SageOfSpirit=Step02"));
        check_steps(&old, &good).unwrap();
        check_steps(&good, &good).unwrap();
        check_steps(&old, &Config::default()).unwrap();
        if !dump.exists() {
            return;
        }
        let bad = with(setup("SageOfSpirit=Step99"));
        let err = format!("{:#}", check_steps(&old, &bad).unwrap_err());
        assert!(
            err.contains("the test setup") && err.contains("Step99"),
            "{err}"
        );
        check_steps(&bad, &bad).unwrap();
        let saved = Config {
            saved_tests: BTreeMap::from([("Card".into(), setup("Nope=Step02"))]),
            ..with(Default::default())
        };
        let err = format!("{:#}", check_steps(&old, &saved).unwrap_err());
        assert!(
            err.contains("the saved setup Card") && err.contains("isn't a quest"),
            "{err}"
        );
    }

    #[test]
    fn icons_come_from_the_dump_only_with_game_assets_on() {
        let dump = PathBuf::from("oot.nup");
        let config = |game_assets, game: Option<&PathBuf>| Config {
            game_assets,
            game: game.cloned(),
            ..Default::default()
        };
        let off = config(false, Some(&dump));
        assert!(!game_art(&off));
        let err = format!("{:#}", icon_dump(&off).unwrap_err());
        assert!(err.contains("game assets"), "{err}");
        let no_dump = config(true, None);
        assert!(!game_art(&no_dump));
        assert!(icon_dump(&no_dump).is_err());
        let on = config(true, Some(&dump));
        assert!(game_art(&on));
        assert_eq!(icon_dump(&on).unwrap(), dump);
    }

    /// Keeps the checked-in bindings in step with the commands.
    #[test]
    fn bindings_are_current() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(BINDINGS);
        let before = std::fs::read_to_string(&path).unwrap_or_default();
        export_bindings(&path).unwrap();
        assert_eq!(
            before,
            std::fs::read_to_string(&path).unwrap(),
            "bindings were stale; rerun"
        );
    }
}
