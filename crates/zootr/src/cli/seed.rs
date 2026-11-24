//! The commands that write a seed as a mod: `shuffle`, and the command line
//! the app shows for a seed it built.

use std::path::Path;

use anyhow::Result;

use super::args::{Install, Shuffle};
use crate::config::Config;
use crate::history::{History, Label};
use crate::progress::{self, About, Run};
use crate::settings::Settings;
use crate::{game, generate, presets, settings, starting};

pub(super) fn print_written(w: &generate::Written) {
    w.defaults.iter().for_each(|l| println!("default {l}"));
    println!("{} placement(s), {} file(s)", w.actors, w.files);
}

/// `arg` as the shell reads it back.
fn quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,".contains(c));
    if plain {
        arg.into()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// The `zootr shuffle` command that builds a seed from `settings` and
/// `extras`, with a random seed. Settings at their default are left out.
pub fn shuffle_command(settings: &Settings, extras: &generate::Extras) -> String {
    let schema = settings::schema();
    let preset = |id: &str| presets::PRESETS.iter().any(|p| p.name == id);
    let mut args = vec!["zootr".to_owned(), "shuffle".into()];
    let mut flag = |name: &str, value: Option<String>| {
        args.push(format!("--{name}"));
        args.extend(value);
    };
    for (id, value) in settings {
        let Some(def) = schema.iter().find(|d| &d.id == id) else {
            continue;
        };
        let default: serde_json::Value = match &def.kind {
            settings::Kind::Toggle { default } => (*default).into(),
            settings::Kind::Choice { default, .. } => default.as_str().into(),
            settings::Kind::Inventory { default } => {
                let start = starting::Start::from_settings(settings).unwrap_or_default();
                if start == *default {
                    continue;
                }
                if start.0.is_empty() {
                    flag("start", Some(starting::NONE.into()));
                }
                for (word, n) in &start.0 {
                    let arg = match n {
                        1 => word.clone(),
                        n => format!("{word}={n}"),
                    };
                    flag("start", Some(arg));
                }
                continue;
            }
        };
        if *value == default {
            continue;
        }
        let on = *value == true;
        let text = value.as_str().map_or(value.to_string(), str::to_owned);
        match id.as_str() {
            id if on && preset(id) => flag("preset", Some(id.into())),
            settings::SKIP_INTRO => flag("keep-intro", None),
            settings::SKIP_ALL => flag("skip", Some("all".into())),
            id if on && id.starts_with("skip-") => flag("skip", Some(id["skip-".len()..].into())),
            settings::CHEST_SIZE => flag("chest-size", Some(text)),
            settings::SHUFFLE_ITEMS if !on => flag("vanilla", None),
            id => flag("setting", Some(format!("{id}={text}"))),
        }
    }
    let generate::Extras {
        defaults,
        place,
        spawn,
        give,
        progress,
        fields,
        release,
        ..
    } = extras;
    spawn.iter().for_each(|s| flag("spawn", Some(s.clone())));
    give.iter().for_each(|g| flag("give", Some(g.clone())));
    progress
        .iter()
        .for_each(|p| flag("progress", Some(p.clone())));
    defaults
        .iter()
        .for_each(|d| flag("default", Some(d.clone())));
    place
        .iter()
        .for_each(|(h, i)| flag("place", Some(format!("{h}={i}"))));
    fields.iter().for_each(|f| flag("field", Some(f.clone())));
    release
        .iter()
        .for_each(|r| flag("release", Some(r.name().into())));
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

pub(super) fn shuffle(game: &Path, args: Shuffle) -> Result<()> {
    let seed = args.seed.unwrap_or_else(generate::time_seed);
    install(game, seed, args.settings(), args.extras(), args.install)
}

/// The mod for one slot of an Archipelago multiworld.
pub(super) fn ap_open(game: &Path, file: &Path, args: Install) -> Result<()> {
    let ap = crate::ap::ApSeed::read(file)?;
    let extras = generate::Extras::ap(ap.clone());
    install(game, ap.seed, ap.full_settings(), extras, args)
}

/// Builds the seed and installs it, or writes `--mod-dir`, then records it.
fn install(
    game: &Path,
    seed: u64,
    settings: Settings,
    extras: generate::Extras,
    args: Install,
) -> Result<()> {
    let held = match args.no_install {
        true => None,
        false => args.out.held()?,
    };
    // The history keeps what it needs to build the seed again, so the mod
    // itself can go.
    let unplaced = args.no_install || held.is_some();
    let about = About {
        seed,
        settings: &settings,
        extras: &extras,
        game,
        folders: match unplaced {
            true => Vec::new(),
            false => args.out.folders()?,
        },
    };
    let run = Run::silent().with(progress::print).logged(&about);
    let built = (|| {
        let romfs = game::prepare(game, extras.release, &run)?;
        let generate = |d: &Path| generate::generate(&romfs, d, seed, &settings, &extras, &run);
        match unplaced {
            true => Ok((generate::throwaway(generate)?, false)),
            false => args.out.write(&run, generate),
        }
    })();
    run.end(&built);
    let (report, installed) = built?;
    if installed {
        super::target::warn_builds(&Config::load()?.targets);
    }
    print_written(&report.written);
    println!(
        "seed {seed}: {} chests, {} changed{}",
        report.spots.len(),
        report.changed(),
        if unplaced {
            ""
        } else {
            ", spoiler.txt beside each mod"
        },
    );
    if installed || unplaced || args.history {
        let label = Label {
            name: args.name,
            note: args.note,
        };
        let id = History::record(settings, extras, label, report, installed)?.id;
        let how = if installed {
            "installed, and"
        } else {
            "not installed,"
        };
        println!("{how} shown in the app's history as #{id}");
        if let Some(game) = held {
            println!("{game} is running: install #{id} from the app's history once it's closed");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::{Cli, Command};
    use crate::known::{field, flow, item};
    use clap::Parser;

    #[test]
    fn no_install_writes_nowhere_so_it_takes_no_mod_dir() {
        let parse = |line: &str| Cli::try_parse_from(line.split(' '));
        let Command::Shuffle(s) = parse("zootr shuffle --no-install").unwrap().command else {
            panic!("not a shuffle");
        };
        assert!(s.install.no_install);
        assert!(parse("zootr shuffle --no-install --mod-dir out").is_err());
        assert!(parse("zootr shuffle --no-install --history").is_err());
    }

    #[test]
    fn a_shuffle_command_parses_back_to_its_settings_and_extras() {
        needs_names!();
        let settings = Settings::from([
            ("kakariko-gate".into(), true.into()),
            ("open-forest".into(), false.into()),
            (settings::SKIP_INTRO.into(), false.into()),
            ("skip-owl".into(), true.into()),
            (settings::CHEST_SIZE.into(), "small".into()),
            (settings::SHUFFLE_ITEMS.into(), false.into()),
            ("progressive-items".into(), false.into()),
            (
                starting::SETTING.into(),
                serde_json::json!({ "bomb-bag": 1, "bombs": 20 }),
            ),
            ("not-a-setting".into(), true.into()),
        ]);
        let extras = generate::Extras {
            defaults: vec!["A=1".into()],
            place: crate::shuffle::Fixed::from([(7, "Item_B".into())]),
            spawn: Some(format!(
                "{}/{}",
                flow::BOTTOM_OF_THE_WELL,
                field::WELL_KAKARIKO_SIDE
            )),
            give: vec![item::GLASS_TRUTH.to_string()],
            progress: vec![
                format!("{}=Step02", flow::SAGE_OF_SPIRIT),
                format!("{}=Step02", flow::GOTO_HYRULE_CASTLE),
            ],
            release: Some(game::Release::V100),
            ..Default::default()
        };
        let line = shuffle_command(&settings, &extras);
        assert!(
            line.starts_with("zootr shuffle --chest-size small"),
            "{line}"
        );
        assert!(line.contains("--setting progressive-items=false"), "{line}");
        let Command::Shuffle(s) = Cli::try_parse_from(line.split(' ')).unwrap().command else {
            panic!()
        };
        let mut expected = settings.clone();
        expected.remove("open-forest");
        expected.remove("not-a-setting");
        assert_eq!(s.settings(), expected);
        assert_eq!(s.extras(), extras);
        assert_eq!(
            shuffle_command(&Settings::new(), &Default::default()),
            "zootr shuffle"
        );
    }

    #[test]
    fn quoting_leaves_plain_args_alone() {
        assert_eq!(quote("Map/Spot"), "Map/Spot");
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }
}
