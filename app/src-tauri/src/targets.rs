//! The Targets page's commands: sharing a target as a preset, by text or
//! by file, and reading one back in.

use std::path::Path;

use zootr::config::{Config, Target};

use crate::{Res, text};

/// A shared preset as a new target, named so no saved target has its name.
#[tauri::command]
#[specta::specta]
pub(crate) fn import_target(preset: String) -> Res<Target> {
    let mut target = Target::import(&preset).map_err(text)?;
    target.name = target.unique_name(&Config::load().map_err(text)?.targets);
    Ok(target)
}

/// A saved target's preset JSON, to share.
#[tauri::command]
#[specta::specta]
pub(crate) fn export_target(name: String) -> Res<String> {
    let config = Config::load().map_err(text)?;
    (config.targets.iter().find(|t| t.name == name))
        .map(Target::export)
        .ok_or_else(|| format!("No target named {name}"))
}

/// A saved target's preset JSON, written to `file` to share.
#[tauri::command]
#[specta::specta]
pub(crate) fn save_target(name: String, file: String) -> Res<()> {
    std::fs::write(&file, export_target(name)?).map_err(|e| format!("Writing {file}: {e}"))
}

/// The largest file Import reads: a preset is a few hundred bytes, so a
/// bigger one was picked by mistake (a game dump).
const PRESET_MAX: u64 = 64 * 1024;

/// A shared target's file, read for Import to show before it adds it.
#[tauri::command]
#[specta::specta]
pub(crate) fn read_target_file(file: String) -> Res<String> {
    read_preset(Path::new(&file)).map_err(text)
}

fn read_preset(file: &Path) -> anyhow::Result<String> {
    let size = std::fs::metadata(file)?.len();
    anyhow::ensure!(
        size <= PRESET_MAX,
        "{} is too big for a target",
        file.display()
    );
    Ok(std::fs::read_to_string(file)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_file_reads_and_a_huge_one_is_refused() {
        let dir = std::env::temp_dir().join(format!("zootr-targets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let small = dir.join("target.json");
        std::fs::write(&small, "{}").unwrap();
        assert_eq!(read_preset(&small).unwrap(), "{}");
        let huge = dir.join("game.nup");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(PRESET_MAX + 1)
            .unwrap();
        let err = read_preset(&huge).unwrap_err().to_string();
        assert!(err.contains("too big"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
