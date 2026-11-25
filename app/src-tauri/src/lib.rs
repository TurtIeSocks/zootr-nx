//! The zootr window: Tauri commands over the `zootr` library.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, anyhow, bail};
use generating::Generating;
use live_tab::Live;
use ounce_formats::Romfs;
use serde::Serialize;
use specta::Type;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tauri_specta::{collect_commands, collect_events};
use zootr::build;
use zootr::config::{Config, Target};
use zootr::gamedata::GameData;
use zootr::generate::plando::PlandoOptions;
use zootr::generate::{self, Door, RoundEvent, Spot, Start};
use zootr::history::{self, History, Label, Summary, View};
use zootr::inventory;
use zootr::platform;
use zootr::progress::{self, Run};
use zootr::recorder;
use zootr::settings::favorites::{self, BuiltIn};
use zootr::settings::{SettingDef, Settings};
use zootr::share;

mod app_log;
mod archipelago;
mod audio;
mod checks;
mod colors;
mod dev_panel;
mod generating;
mod items;
mod live_tab;
mod save_editor;
mod targets;
mod ui;

/// Where the bindings are written, from this crate's folder.
const BINDINGS: &str = "../src/bindings.ts";

/// Errors reach the UI as the whole chain on one line.
type Res<T> = Result<T, String>;

/// What a player reads of `e`. The log gets its whole chain.
fn text(e: anyhow::Error) -> String {
    log::error!("{e:#}");
    zootr::plain::shown(&e)
}

#[derive(Serialize, Type)]
struct TargetStatus {
    name: String,
    app: bool,
    mod_dir: bool,
    running: bool,
    /// For a game that bakes the hook patch in at build time, whether its
    /// build has the installed one.
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<Build>,
    /// What to warn when its mod folder holds another seed than the
    /// installed one.
    #[serde(skip_serializing_if = "Option::is_none")]
    foreign: Option<String>,
}

#[derive(Serialize, Type)]
struct Build {
    status: build::Status,
    /// What the user is told, when the build isn't current.
    text: Option<String>,
    /// The target has a rebuild command.
    command: bool,
}

impl Build {
    fn of(target: &Target) -> Option<Build> {
        let status = build::status(target)?;
        Some(Build {
            text: status.text(&target.name),
            status,
            command: target
                .rebuild
                .as_ref()
                .is_some_and(|r| !r.command.is_empty()),
        })
    }
}

#[derive(Serialize, Type)]
struct Status {
    /// The game dump's path exists. Save already extracted it.
    game: bool,
    targets: Vec<TargetStatus>,
}

/// Polled, so off the main thread: it probes processes and user paths.
#[tauri::command]
#[specta::specta]
async fn status() -> Res<Status> {
    blocking(|| {
        let config = Config::load()?;
        let history = History::load().ok();
        let targets = config.targets.iter().map(|t| TargetStatus {
            name: t.name.clone(),
            app: t.app.exists(),
            mod_dir: t.mod_dir.is_dir(),
            running: t.running(),
            build: Build::of(t),
            foreign: history.as_ref().and_then(|h| h.foreign_mod(t)),
        });
        Ok(Status {
            game: config.game.is_some_and(|g| g.exists()),
            targets: targets.collect(),
        })
    })
    .await
}

/// What the sidebar says this build is.
#[derive(Serialize, Type)]
struct BuildInfo {
    version: String,
    /// The short commit it was built from, `*` when dirty. Empty without git.
    commit: String,
    /// A tagged build from the release workflow. Any other is a source build.
    release: bool,
}

#[tauri::command]
#[specta::specta]
fn build_info(app: tauri::AppHandle) -> BuildInfo {
    BuildInfo {
        version: app.package_info().version.to_string(),
        commit: env!("ZNX_COMMIT").into(),
        release: option_env!("ZNX_RELEASE").is_some(),
    }
}

#[tauri::command]
#[specta::specta]
fn schema() -> Vec<SettingDef> {
    zootr::settings::shown()
}

/// The favorites that come with the app, as the Favorites list shows them.
#[tauri::command]
#[specta::specta]
fn built_in_favorites() -> Vec<BuiltIn> {
    favorites::built_in()
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
        // The UI's config never carries the tracker's options, and its
        // settings are current.
        let mut config = Config {
            tracker: old.tracker.clone(),
            settings_version: zootr::settings::SettingsVersion::CURRENT,
            ..config
        };
        favorites::check(&config.favorites)?;
        let mut notes = config.fix_mods_roots();
        notes.extend(zootr::settings::fit_bottles(&mut config.settings)?);
        if let Some(game) = config
            .game
            .as_ref()
            .filter(|&g| old.game.as_ref() != Some(g))
        {
            zootr::game::prepare(game, None, &Run::silent())?;
        }
        check_steps(&old, &config)?;
        config.save()?;
        app_log::set_level(config.dev_tools);
        if game_art(&old) != game_art(&config) || old.game != config.game {
            app.state::<items::Items>().emit(&app);
        }
        Ok(notes)
    })
    .await
}

/// The game files from the config's dump.
fn romfs(config: &Config) -> anyhow::Result<Romfs> {
    let game = config.game.as_ref().context("Set the game dump first")?;
    zootr::game::prepare(game, None, &Run::silent())
}

/// What `slot` holds, loaded into it by `load` the first time. Kept until
/// the app closes, so a changed dump takes a restart.
fn cached<T>(
    slot: &Mutex<Option<Arc<T>>>,
    load: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<Arc<T>> {
    let mut slot = slot.lock().unwrap();
    if let Some(kept) = &*slot {
        return Ok(kept.clone());
    }
    let loaded = Arc::new(load()?);
    *slot = Some(loaded.clone());
    Ok(loaded)
}

/// The `zootr shuffle` command that Generate stands for, with the current
/// settings and test setup.
#[tauri::command]
#[specta::specta]
fn shuffle_command() -> Res<String> {
    let config = Config::load().map_err(text)?;
    let line = zootr::cli::shuffle_command(&config.settings, &config.extras());
    let colors = zootr::colors::args(&config.colors).into_iter();
    let line = colors.fold(line, |line, c| format!("{line} --color {c}"));
    let audio = zootr::audio::args(&config.audio).into_iter();
    Ok(audio.fold(line, |line, (f, v)| format!("{line} --{f} {v}")))
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

/// Generate a seed with the current settings and test setup, and install
/// it when `install`, else only add it to History. A blank seed is random.
/// A share code brings its own settings and extras.
#[tauri::command]
#[specta::specta]
async fn generate(
    app: AppHandle,
    seed: Option<String>,
    install: bool,
    steps: Channel<progress::StepEvent>,
) -> Res<Summary> {
    blocking(move || {
        let config = Config::load()?;
        let (extras, drafts) = (config.extras(), config.drafts());
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
        // A share code leaves colors and music out: the seed takes this
        // draft's.
        let extras = extras.styled(drafts, seed)?;
        record(&app, steps, seed, settings, extras, install)
    })
    .await
}

/// What a share code builds with, read without building it.
#[derive(Serialize, Type)]
struct SharedBuild {
    settings: Settings,
    /// How many checks its plando pins.
    pins: usize,
    test: zootr::config::TestSetup,
}

/// Reads a share code's settings and extras without generating a seed.
#[tauri::command]
#[specta::specta]
fn share_settings(code: String) -> Res<SharedBuild> {
    let shared = share::decode(&code).map_err(text)?;
    Ok(SharedBuild {
        pins: shared.extras.place.len(),
        test: zootr::config::TestSetup::from_extras(&shared.extras),
        settings: shared.settings,
    })
}

/// Install one slot of an Archipelago multiworld from its `.apznx`. The
/// file brings its own settings, placements and seed.
#[tauri::command]
#[specta::specta]
async fn generate_ap(
    app: AppHandle,
    path: PathBuf,
    steps: Channel<progress::StepEvent>,
) -> Res<Summary> {
    blocking(move || {
        let ap = zootr::ap::ApSeed::read(&path)?;
        let (seed, settings) = (ap.seed, ap.full_settings());
        record(&app, steps, seed, settings, generate::Extras::ap(ap), true)
    })
    .await
}

/// Install a new seed and add it to the history.
fn record(
    app: &AppHandle,
    steps: Channel<progress::StepEvent>,
    seed: u64,
    settings: Settings,
    extras: generate::Extras,
    install: bool,
) -> anyhow::Result<Summary> {
    let built = (app.state::<Generating>()).build(steps, seed, &settings, &extras, install);
    let summary =
        built.and_then(|r| History::record(settings, extras, Label::default(), r, install));
    // A stopped install that took the mod off changed it too.
    if install {
        app.state::<archipelago::Ap>().reset(app);
        app.state::<items::Items>().emit(app);
    }
    summary
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
    blocking(move || reinstall(&app, steps, id, false)).await
}

/// Reinstall the installed seed in the Colors and Audio drafts: the same
/// shuffle, painted and scored again. Its history entry keeps them.
#[tauri::command]
#[specta::specta]
async fn apply_cosmetics(app: AppHandle, steps: Channel<progress::StepEvent>) -> Res<()> {
    blocking(move || reinstall(&app, steps, colors::installed()?, true)).await
}

/// Install history entry `id` again, in the drafts' cosmetics when
/// `restyle`.
fn reinstall(
    app: &AppHandle,
    steps: Channel<progress::StepEvent>,
    id: u32,
    restyle: bool,
) -> anyhow::Result<()> {
    let entry = History::load()?.get(id)?.clone();
    let (seed, settings, mut extras) = (entry.report.seed, entry.settings, entry.extras);
    if restyle {
        extras = extras.styled(Config::load()?.drafts(), seed)?;
    }
    let built = (app.state::<Generating>()).build(steps, seed, &settings, &extras, true);
    // Loaded again: History may have changed during the build.
    let done = built.and_then(|_| {
        let mut history = History::load()?;
        history.get(id)?;
        if restyle {
            history.restyle(id, extras.colors, extras.audio, extras.drafts)?;
        }
        history.install(Some(id));
        history.save()
    });
    // A stopped install that took the mod off changed it too.
    app.state::<archipelago::Ap>().reset(app);
    app.state::<items::Items>().emit(app);
    done
}

/// Rebuild a target's game, its output each a detail of the rebuilding
/// step sent to `steps`. Fails unless the build has the installed hook
/// patch afterwards. Stop Generate stops it too.
#[tauri::command]
#[specta::specta]
async fn rebuild(app: AppHandle, name: String, steps: Channel<progress::StepEvent>) -> Res<()> {
    blocking(move || app.state::<Generating>().rebuild(steps, &name)).await
}

/// Take the mod off every target so the game runs vanilla.
#[tauri::command]
#[specta::specta]
async fn uninstall(app: AppHandle) -> Res<()> {
    blocking(move || {
        generate::uninstall(&Config::load()?.targets)?;
        let mut history = History::load()?;
        history.install(None);
        history.save()?;
        log::info!("Took the mod off every target");
        app.state::<archipelago::Ap>().reset(&app);
        app.state::<items::Items>().emit(&app);
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
async fn history() -> Res<View> {
    blocking(history::view).await
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
    entrances: Vec<Door>,
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
        entrances: report.entrances.clone(),
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
    platform::open_game(&t.app, &args, &env, t.control.as_deref())
        .map_err(|e| format!("Couldn't open {}: {e:#}", t.app.display()))?;
    let t = t.clone();
    zootr::turn::plays(config.turn.clone(), move || t.running());
    Ok(())
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
    let config = config.ok_or_else(|| format!("no {label} window in tauri.conf.json"))?;
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

/// A file served to the UI as `mime`, or a 404 that the log says why of.
fn served(
    body: anyhow::Result<Vec<u8>>,
    mime: &str,
    uri: &tauri::http::Uri,
) -> tauri::http::Response<Vec<u8>> {
    let response = match body {
        // Any origin: the page fetches songs, and a fetch, unlike an <img>,
        // is held to CORS against the page's own scheme.
        Ok(body) => tauri::http::Response::builder()
            .header(tauri::http::header::CONTENT_TYPE, mime)
            .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(body),
        Err(e) => {
            log::debug!("Nothing for {uri}: {e:#}");
            tauri::http::Response::builder().status(404).body(vec![])
        }
    };
    response.expect("a plain response")
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
            build_info,
            schema,
            built_in_favorites,
            config,
            set_config,
            generate,
            share_settings,
            generate_ap,
            install,
            rebuild,
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
            colors::colors_catalog,
            colors::colors_draw,
            colors::colors_follow,
            apply_cosmetics,
            audio::audio_catalog,
            save_editor::save_slots,
            save_editor::save_presets,
            save_editor::save_items,
            save_editor::save_click,
            save_editor::start_items,
            save_editor::start_arsenal,
            save_editor::start_click,
            save_editor::save_stage,
            save_editor::save_flags,
            save_editor::save_write,
            live_tab::live_state,
            live_tab::live_label,
            live_tab::live_mark,
            live_tab::live_mute,
            reveal_log,
            app_log::app_log_state,
            app_log::app_log_write,
            checks::checks_state,
            checks::checks_mark,
            checks::checks_pick,
            items::items_state,
            items::items_options,
            items_icons,
            items_pop_out,
            dev_pop_out,
            archipelago::ap_state,
            archipelago::ap_connect,
            archipelago::ap_disconnect,
            archipelago::apworld_bundled,
            archipelago::save_apworld,
            dev_panel::game_options,
            dev_panel::dev_state,
            dev_panel::dev_teleport,
            dev_panel::dev_actors,
            dev_panel::dev_warp,
            dev_panel::dev_nearby,
            dev_panel::dev_link,
            dev_panel::dev_hide,
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
            live_tab::LiveEvent,
            app_log::AppLogEvent,
            checks::ChecksEvent,
            checks::DoorsEvent,
            items::ItemsEvent,
            archipelago::ApEvent,
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
    // One generated file for every command: the size guard has nothing to split.
    let header = "// file-size-guard: generated by tauri-specta, one file for every command";
    commands().export(
        specta_typescript::Typescript::default().header(header),
        path,
    )
}

pub fn run() -> tauri::Result<()> {
    let config = Config::load();
    app_log::install(config.as_ref().is_ok_and(|c| c.dev_tools));
    app_log::header(&config);
    #[cfg(debug_assertions)]
    if let Err(e) = export_bindings(Path::new(env!("CARGO_MANIFEST_DIR")).join(BINDINGS)) {
        log::warn!("Couldn't write the bindings: {e}");
    }
    let builder = commands();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .register_asynchronous_uri_scheme_protocol(colors::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let uri = request.uri();
                let body = app
                    .state::<colors::ColorPreviews>()
                    .serve(uri.path(), uri.query().unwrap_or_default());
                let mime = body.as_ref().map_or("image/png", |(_, mime)| *mime);
                responder.respond(served(body.map(|(body, _)| body), mime, uri));
            });
        })
        .register_asynchronous_uri_scheme_protocol(ICONS, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let png = (app.state::<Icons>()).read(request.uri().path().trim_start_matches('/'));
                responder.respond(served(png, "image/png", request.uri()));
            });
        })
        .register_asynchronous_uri_scheme_protocol(audio::SCHEME, |_, request, responder| {
            std::thread::spawn(move || {
                let wav = audio::serve(request.uri().path());
                responder.respond(served(wav, "audio/wav", request.uri()));
            });
        })
        .setup(move |app| {
            builder.mount_events(app);
            app_log::attach(app.handle());
            app.manage(archipelago::Ap::default());
            app.manage(checks::Checks::default());
            app.manage(items::Items::default());
            app.manage(Generating::default());
            app.manage(save_editor::Layout::default());
            app.manage(Icons::default());
            app.manage(colors::ColorPreviews::default());
            app.manage(dev_panel::Places::default());
            app.manage(ui::UiFile::default());
            // Built now, so the Items tab rarely waits on a new dump.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                if let Err(e) = handle.state::<Icons>().folder() {
                    log::debug!("Icons not built at startup: {e:#}");
                }
            });
            app.manage(Live::new(app.handle().clone()));
            Ok(app.state::<Live>().watch(app.handle())?)
        })
        .run(context())
}

/// The app's context. A debug build takes the updater's feed and key from
/// `ZOOTR_UPDATER_ENDPOINT` and `ZOOTR_UPDATER_PUBKEY` when set, to try an
/// install against a local `latest.json`. A release build never reads them,
/// so nothing outside the app can point a player at another feed.
fn context() -> tauri::Context<tauri::Wry> {
    #[allow(unused_mut)]
    let mut context = tauri::generate_context!();
    #[cfg(debug_assertions)]
    if let Some(updater) = context.config_mut().plugins.0.get_mut("updater") {
        if let Ok(url) = std::env::var("ZOOTR_UPDATER_ENDPOINT") {
            updater["endpoints"] = serde_json::json!([url]);
        }
        if let Ok(key) = std::env::var("ZOOTR_UPDATER_PUBKEY") {
            updater["pubkey"] = key.into();
        }
    }
    context
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn an_error_logs_its_whole_chain() {
        app_log::install_bare();
        let shown = text(anyhow!("the FakeFile is gone").context("reading the FakeSave"));
        assert_eq!(shown, "reading the FakeSave: the FakeFile is gone");
        let lines = serde_json::to_value(app_log::app_log_state()).unwrap();
        let mut logged = lines["lines"].as_array().unwrap().iter();
        assert!(logged.any(
            |l| l["text"] == "reading the FakeSave: the FakeFile is gone" && l["level"] == "error"
        ));
    }

    #[test]
    fn a_cached_value_loads_once_and_a_failed_load_keeps_nothing() {
        let slot = Mutex::default();
        assert!(cached(&slot, || Err::<u8, _>(anyhow!("no FakeDump"))).is_err());
        assert!(slot.lock().unwrap().is_none());
        assert_eq!(*cached(&slot, || Ok(1)).unwrap(), 1);
        assert_eq!(*cached(&slot, || Ok(2)).unwrap(), 1);
    }

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

    /// The page fetches songs from another scheme, so a response without
    /// this header fails as "Load failed".
    #[test]
    fn a_served_file_allows_any_origin() {
        let uri = "zootr-audio://localhost/fake-track.wav".parse().unwrap();
        let ok = served(Ok(vec![1]), "audio/wav", &uri);
        assert_eq!(ok.headers()["access-control-allow-origin"], "*");
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
