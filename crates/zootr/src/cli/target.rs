//! `zootr target`: adding a target from a shared preset, printing one, and
//! rebuilding one's game.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::args::TargetCommand;
use crate::build;
use crate::config::{Config, Target};
use crate::progress::{self, Run};

pub(super) fn target_command(cmd: TargetCommand) -> Result<()> {
    match cmd {
        TargetCommand::Import { file } => {
            let text = read(&file)?;
            let mut target = Target::import(&text).context("not a target preset")?;
            if let Some(note) = target.fix_mods_root() {
                eprintln!("{note}");
            }
            let mut config = Config::load()?;
            let name = config.add_imported(target);
            config.save()?;
            println!("{name}");
        }
        TargetCommand::Export { name } => {
            let config = Config::load()?;
            println!("{}", named(&config, &name)?.export());
        }
        TargetCommand::Rebuild { name } => {
            let config = Config::load()?;
            let target = named(&config, &name)?;
            let run = Run::silent().with(build::echo).with(progress::print);
            let done = build::rebuild(target, config.game.as_deref(), &run);
            run.end(&done);
            done?;
            println!("{name}'s build has the installed hook patch");
        }
    }
    Ok(())
}

fn named<'a>(config: &'a Config, name: &str) -> Result<&'a Target> {
    match config.targets.iter().find(|t| t.name == name) {
        Some(target) => Ok(target),
        None => bail!("no target named {name}"),
    }
}

/// Each target's build status line on stderr, for a game whose build hasn't
/// seen the hook patch just installed.
pub(super) fn warn_builds(targets: &[Target]) {
    for t in targets {
        if let Some(text) = build::status(t).and_then(|s| s.text(&t.name)) {
            eprintln!("{text}");
        }
    }
}

/// The file's text, or stdin's for `-`.
fn read(file: &Path) -> Result<String> {
    if file == Path::new("-") {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        return Ok(text);
    }
    std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))
}
