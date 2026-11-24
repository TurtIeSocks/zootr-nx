//! `zootr save`: reading, comparing, watching and editing a save slot by
//! flag name.

use std::path::Path;

use anyhow::Result;
use ounce_formats::Romfs;

use super::args::SaveCommand;
use super::running::{named, stamp};
use crate::config::Config;
use crate::inventory::{Inventory, Pouch};
use crate::{kept, presets, recorder, savefile, starting};

/// `zootr save`: a save slot read, compared, watched or edited.
pub(super) fn save_command(romfs: &Romfs, cmd: SaveCommand) -> Result<()> {
    let gd = recorder::gamedata(romfs)?;
    let wanted = |grep: Option<&str>, e: ounce_formats::SaveEntry| named(&gd, grep, e.hash);
    match cmd {
        SaveCommand::Show { slot, grep, all } => {
            let save = savefile::load(&savefile::path(&slot)?)?;
            for &e in save.entries() {
                if wanted(grep.as_deref(), e) && (all || savefile::changed(&gd, &save, e)) {
                    println!(
                        "{}",
                        savefile::line(&gd, e.hash, e.kind, &savefile::shown(&gd, &save, e))
                    );
                }
            }
        }
        SaveCommand::Diff { a, b, grep } => {
            let a = savefile::load(&savefile::path(&a)?)?;
            let b = savefile::load(&savefile::path(&b)?)?;
            for c in savefile::diff(&gd, &a, &b, |e| wanted(grep.as_deref(), e)) {
                println!("{c}");
            }
        }
        SaveCommand::Watch { slot, grep } => {
            let mut watcher = savefile::Watcher::default();
            let start = std::time::Instant::now();
            println!("watching, Ctrl-C stops");
            loop {
                let paths = savefile::watched(slot.as_deref())?;
                for (path, lines) in watcher.look(&gd, &paths, |e| wanted(grep.as_deref(), e)) {
                    let name = path.parent().and_then(Path::file_name).unwrap_or_default();
                    println!("{} {} changed", stamp(start), name.display());
                    lines.iter().for_each(|c| println!("  {c}"));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
        SaveCommand::Keep {
            name,
            slot,
            from,
            note,
        } => {
            let from = match from {
                Some(from) => from,
                None => Config::load()?.save_dir()?,
            };
            let kept = kept::keep(&gd, &name, &from, slot, note)?;
            println!("kept {name}: {}", shown(&kept));
        }
        SaveCommand::Kept => {
            for (name, kept) in kept::list()? {
                println!("{name}: {}", shown(&kept));
            }
        }
        SaveCommand::Set {
            slot,
            edits,
            presets,
        } => {
            let edits = presets::for_save(&presets, &edits)?;
            let (log, backup) = savefile::edit(&gd, &savefile::path(&slot)?, &edits)?;
            log.iter().for_each(|l| println!("{l}"));
            println!("backup at {}", backup.display());
        }
        SaveCommand::Give { slot, words } => {
            let mut start = starting::Start::default();
            for (word, n) in words.iter().filter(|(w, _)| w != starting::NONE) {
                start.add(word, *n)?;
            }
            let (inv, pouch) = (Inventory::new(&gd, romfs)?, Pouch::load(romfs)?);
            let path = savefile::path(&slot)?;
            let mut save = savefile::load(&path)?;
            let log = starting::onto(&gd, &inv, &pouch, &mut save, &start)?;
            log.iter().for_each(|l| println!("{l}"));
            println!("backup at {}", savefile::write(&path, &save)?.display());
        }
    }
    Ok(())
}

/// A kept save's map, age, time and note on one line.
fn shown(kept: &kept::Kept) -> String {
    let age = match kept.child {
        Some(true) => " (child)",
        Some(false) => " (adult)",
        None => "",
    };
    let map = if kept.map.is_empty() { "?" } else { &kept.map };
    let note = match kept.note.is_empty() {
        true => String::new(),
        false => format!(", {}", kept.note),
    };
    format!("{map}{age}, kept {}{note}", kept.kept)
}
