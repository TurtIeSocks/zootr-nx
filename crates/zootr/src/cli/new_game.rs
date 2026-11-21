//! The flags for how a new save starts, shared by the commands that make
//! one, and the settings they resolve to.

use anyhow::{Context, Result};

use crate::settings::Settings;
use crate::{presets, savefile, settings, starting};

/// How a new save starts: GameData defaults, and the cutscenes skipped.
#[derive(clap::Args)]
pub(super) struct NewGame {
    /// Flag sets for a new game. Repeatable.
    #[arg(long = "preset", value_parser = presets::parser())]
    presets: Vec<String>,
    /// A new game's starting value for one flag, e.g.
    /// `IsGet.ITEM=true`. Repeatable.
    #[arg(long = "default", value_name = "NAME=VALUE")]
    pub(super) edits: Vec<String>,
    /// Play the opening movie and Navi's wake-up cutscene. Skipped by
    /// default, so a new game starts with Link in control.
    #[arg(long)]
    keep_intro: bool,
    /// Cutscenes to skip: `all`, or a name or area. Repeatable.
    #[arg(long = "skip", value_name = "NAME", value_parser = settings::skip_parser())]
    skips: Vec<String>,
    /// How big each chest is.
    #[arg(long, value_enum)]
    chest_size: Option<settings::ChestSize>,
    /// What a new game starts with, as `WORD` or `WORD=N`: `kokiri-sword`,
    /// `longshot` (which brings the Hookshot), `bombs=20`, `heart-pieces=8`,
    /// `heart-containers=5`, `rupees=99`. Repeatable. Without it, a new game
    /// starts with the Master Sword, and `none` starts it with nothing.
    #[arg(long = "start", value_name = "WORD[=N]", value_parser = starting::parse)]
    start: Vec<(String, i32)>,
    /// Any app setting by id, for those without a flag of their own, e.g.
    /// `progressive-items=false`. Repeatable.
    #[arg(long = "setting", value_name = "ID=VALUE", value_parser = setting)]
    raw: Vec<(String, serde_json::Value)>,
}

/// `ID=VALUE`, where `true` and `false` are toggles and anything else a
/// choice. The id must be one of the app's settings and the value one it
/// takes: an unknown choice would quietly read as the default.
pub(crate) fn setting(arg: &str) -> Result<(String, serde_json::Value)> {
    let (id, value) = savefile::split(arg)?;
    // An old toggle id, read as the setting it became.
    if settings::OLD_TOGGLES.contains(&id) {
        let on = value.parse::<bool>();
        let on = on.with_context(|| format!("{id} is true or false, not {value}"))?;
        let new = settings::renamed_as(id, on).or_else(|| settings::renamed(id));
        let new = new.expect("an old toggle has a new form");
        eprintln!("--setting {id} is now {new}");
        return Ok((id.to_owned(), on.into()));
    }
    let schema = settings::schema();
    let def = schema.iter().find(|def| def.id == id);
    let def = def.with_context(|| format!("no setting is called {id}"))?;
    match &def.kind {
        settings::Kind::Toggle { .. } => anyhow::ensure!(
            value.parse::<bool>().is_ok(),
            "{id} is true or false, not {value}"
        ),
        settings::Kind::Choice { options, .. } => {
            let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
            anyhow::ensure!(
                ids.contains(&value),
                "{id} takes {}, not {value}",
                ids.join(", ")
            );
        }
        settings::Kind::Inventory { .. } => anyhow::bail!("{id} is set with --start"),
    }
    let value = value.parse::<bool>().map_or(value.into(), Into::into);
    Ok((id.to_owned(), value))
}

impl NewGame {
    /// The presets and the skips as the app's settings. `--default` edits
    /// have no setting, so they stay apart.
    pub(super) fn settings(&self) -> Settings {
        let mut out: Settings = self
            .presets
            .iter()
            .map(|p| (p.clone(), true.into()))
            .collect();
        if self.keep_intro {
            out.insert(settings::SKIP_INTRO.into(), false.into());
        }
        if let Some(size) = self.chest_size {
            out.insert(
                settings::CHEST_SIZE.into(),
                settings::choice_id(size).into(),
            );
        }
        out.extend(
            self.skips
                .iter()
                .map(|s| (settings::skip_id(s), true.into())),
        );
        let mut start = starting::Start::default();
        for (word, n) in self.start.iter().filter(|(w, _)| w != starting::NONE) {
            start.add(word, *n).expect("--start checks its words");
        }
        if !self.start.is_empty() {
            out.insert(
                starting::SETTING.into(),
                serde_json::to_value(start).expect("a map"),
            );
        }
        out.extend(self.raw.iter().cloned());
        // Old preset names and toggle ids, as the settings they became.
        settings::upgrade(&mut out);
        out
    }

    pub(super) fn resolve(&self) -> Result<settings::NewGame> {
        settings::NewGame::from_settings(&self.settings(), &self.edits)
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::super::args::{Cli, Command};
    use super::*;

    /// A command taking only the new game's flags.
    #[derive(Parser)]
    struct Flags {
        #[command(flatten)]
        new_game: NewGame,
    }

    #[test]
    fn the_old_bridge_and_trials_names_become_the_win_condition() {
        let settings = |extra: &[&str]| {
            let args = [&["zootr"], extra].concat();
            Flags::try_parse_from(args).unwrap().new_game.settings()
        };
        let is = |id: &str, v: &str| Settings::from([(id.into(), v.into())]);
        let bridge = settings::BRIDGE;
        assert_eq!(
            settings(&["--preset", "rainbow-bridge"]),
            is(bridge, "open")
        );
        let trials = settings(&["--preset", "trials-done"]);
        assert_eq!(trials, is(settings::TRIALS, "done"));
        let six = settings(&["--setting", "bridge-medallions=true"]);
        assert_eq!(six, is(bridge, "six-medallions"));
        assert!(setting("bridge-medallions=maybe").is_err());
        let open = settings(&["--setting", "rainbow-bridge=open"]);
        assert_eq!(open, is(bridge, "open"));
        let save = ["zootr", "save", "set", "1", "--preset", "trials-done"];
        let Command::Save(_) = Cli::try_parse_from(save).unwrap().command else {
            panic!()
        };
        let err = presets::for_save(&["trials-done".into()], &[]).unwrap_err();
        assert!(err.to_string().contains("ganons-trials=done"), "{err}");
    }

    #[test]
    fn the_old_door_names_become_the_door_of_time() {
        let settings = |extra: &[&str]| {
            let args = [&["zootr"], extra].concat();
            Flags::try_parse_from(args).unwrap().new_game.settings()
        };
        let is = |v: &str| Settings::from([(settings::DOOR_OF_TIME.into(), v.into())]);
        assert_eq!(settings(&["--preset", "door-of-time"]), is("open"));
        let closed = settings(&["--setting", "open-door-of-time=false"]);
        assert_eq!(closed, is("closed"));
        assert_eq!(
            settings(&["--setting", "door-of-time=closed"]),
            is("closed")
        );
    }

    #[test]
    fn a_setting_must_be_one_the_app_knows() {
        assert!(setting("keys=start").is_ok());
        let error = setting("minigames=true").unwrap_err().to_string();
        assert!(error.contains("minigames"), "{error}");
    }

    #[test]
    fn a_value_must_be_one_the_setting_takes() {
        let error = setting("keys=keysanity").unwrap_err().to_string();
        assert!(error.contains("singles"), "{error}");
        let error = setting("chests=maybe").unwrap_err().to_string();
        assert!(error.contains("true or false"), "{error}");
        assert!(setting("keys=singles").is_ok());
        assert!(setting("chests=false").is_ok());
    }
}
