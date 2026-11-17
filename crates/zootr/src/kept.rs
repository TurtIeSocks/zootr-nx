//! Kept saves: named save folders under `saves/NAME/` beside the config,
//! never in the repo. A boot plays a copy of one, so a kept save never
//! changes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use ounce_formats::murmur3;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::gamedata::GameData;
use crate::known::flag;
use crate::savefile;

/// What `kept.json` says about a kept save.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The folder it was kept from.
    pub from: PathBuf,
    /// The slot kept, when only one was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,
    /// When it was kept.
    pub kept: String,
    /// Where Continue loads: the newest slot's map and age.
    #[serde(default)]
    pub map: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<bool>,
}

const META: &str = "kept.json";

/// The library's folder.
pub fn library() -> Result<PathBuf> {
    Ok(Config::path()?.with_file_name("saves"))
}

/// The kept save `name`, which must exist.
pub fn path(name: &str) -> Result<PathBuf> {
    let path = folder(name)?;
    ensure!(
        path.is_dir(),
        "no kept save {name}: `zootr save kept` lists them"
    );
    Ok(path)
}

fn folder(name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.'),
        "a kept save's name is one plain word, not {name:?}"
    );
    Ok(library()?.join(name))
}

/// Keep the save folder `from` as `name`: every slot, or only `slot`
/// (as `slot_00`, so Continue loads it), with the `.sav` files beside
/// them. Replaces a kept save of the same name.
pub fn keep(
    gd: &GameData,
    name: &str,
    from: &Path,
    slot: Option<u8>,
    note: String,
) -> Result<Kept> {
    let to = folder(name)?;
    copy(from, &to, slot)?;
    let newest = savefile::newest(&to)?;
    let save = newest.as_deref().map(savefile::load).transpose()?;
    let read = |name: &str| {
        let save = save.as_ref()?;
        let hash = murmur3(name.as_bytes());
        let e = save.entries().iter().find(|e| e.hash == hash)?;
        Some(savefile::shown(gd, save, *e))
    };
    let kept = Kept {
        note,
        from: from.to_path_buf(),
        slot,
        kept: jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string(),
        map: read(flag::SEQUENCE_CURRENT_BANC).unwrap_or_default(),
        child: read(flag::SEQUENCE_IS_CHILDHOOD).map(|v| v == "true"),
    };
    let mut json = serde_json::to_vec_pretty(&kept)?;
    json.push(b'\n');
    std::fs::write(to.join(META), json)?;
    Ok(kept)
}

/// Every kept save by name, sorted.
pub fn list() -> Result<Vec<(String, Kept)>> {
    let Ok(entries) = std::fs::read_dir(library()?) else {
        return Ok(vec![]);
    };
    let mut kept: Vec<(String, Kept)> = entries
        .filter_map(|e| {
            let path = e.ok()?.path();
            let meta = std::fs::read(path.join(META)).unwrap_or_default();
            let name = path.file_name()?.to_str()?.to_owned();
            Some((name, serde_json::from_slice(&meta).unwrap_or_default()))
        })
        .collect();
    kept.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(kept)
}

/// `from`'s save files into a fresh `to`: the top-level `.sav` files, and
/// every `slot_NN` folder or only `slot`'s, renamed `slot_00`.
fn copy(from: &Path, to: &Path, slot: Option<u8>) -> Result<()> {
    if let Some(n) = slot {
        let slot = from.join(format!("slot_{n:02}"));
        ensure!(slot.is_dir(), "{} has no slot {n}", from.display());
    }
    let entries = std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))?;
    let slots = entries
        .flatten()
        .filter(|e| slot_of(&e.file_name().to_string_lossy()).is_some());
    ensure!(
        slots.count() > 0,
        "{} holds no slot_NN folder: --from takes the folder the slots are in",
        from.display()
    );
    let _ = std::fs::remove_dir_all(to);
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            let Some(n) = slot_of(&name) else {
                continue;
            };
            match slot {
                Some(want) if want != n => {}
                Some(_) => copy_dir(&entry.path(), &to.join("slot_00"))?,
                None => copy_dir(&entry.path(), &to.join(&name))?,
            }
        } else if name.ends_with(".sav") {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// The number of a `slot_NN` folder's name.
fn slot_of(name: &str) -> Option<u8> {
    name.strip_prefix("slot_")?.parse().ok()
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        std::fs::copy(entry.path(), to.join(entry.file_name()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeping_one_slot_renames_it_slot_00_beside_the_sav_files() {
        let root = std::env::temp_dir().join(format!("zootr-keep-{}", std::process::id()));
        let (from, to) = (root.join("save"), root.join("kept"));
        let write = |rel: &str, text: &str| {
            let path = from.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("option.sav", "options");
        write("account.sav", "account");
        write("slot_00/progress.sav", "manual");
        write("slot_03/progress.sav", "autosave");
        write("slot_03/caption.sav", "caption");
        write("stray/notes.txt", "not a save");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("old.sav"), "from an earlier keep").unwrap();
        copy(&from, &to, Some(3)).unwrap();
        let read = |rel: &str| std::fs::read_to_string(to.join(rel)).ok();
        assert_eq!(read("slot_00/progress.sav").as_deref(), Some("autosave"));
        assert_eq!(read("slot_00/caption.sav").as_deref(), Some("caption"));
        assert_eq!(read("option.sav").as_deref(), Some("options"));
        assert_eq!(read("account.sav").as_deref(), Some("account"));
        assert!(!to.join("slot_03").exists() && !to.join("stray").exists());
        assert_eq!(read("old.sav"), None);
        assert!(
            copy(&from, &to, Some(4))
                .unwrap_err()
                .to_string()
                .contains("no slot 4")
        );
        copy(&from, &to, None).unwrap();
        assert_eq!(read("slot_00/progress.sav").as_deref(), Some("manual"));
        assert!(to.join("slot_03/progress.sav").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_folder_with_no_slots_is_refused_and_the_old_keep_stays() {
        let root = std::env::temp_dir().join(format!("zootr-keep-none-{}", std::process::id()));
        let (from, to) = (root.join("data"), root.join("kept"));
        let nested = from.join("game/save/user/slot_00");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("progress.sav"), "deeper down").unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("option.sav"), "kept before").unwrap();
        let err = copy(&from, &to, None).unwrap_err().to_string();
        assert!(err.contains("no slot_NN folder"), "{err}");
        assert!(to.join("option.sav").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_kept_saves_name_is_one_plain_word() {
        for bad in ["", "../x", "a/b", ".hidden"] {
            assert!(folder(bad).is_err(), "{bad:?}");
        }
        assert!(
            folder("kokiri-deck")
                .unwrap()
                .ends_with("saves/kokiri-deck")
        );
    }
}
