//! `zootr target`: adding a target from a shared preset and printing one.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::args::TargetCommand;
use crate::config::{Config, Target};

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
            let Some(target) = config.targets.iter().find(|t| t.name == name) else {
                bail!("no target named {name}");
            };
            println!("{}", target.export());
        }
    }
    Ok(())
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
