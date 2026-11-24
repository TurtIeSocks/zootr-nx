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
        plain(name),
        "a kept save's name is one plain word, not {name:?}"
    );
    Ok(library()?.join(name))
}

/// Whether `name` can name a kept save: one word, not hidden.
fn plain(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.')
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
    let meta = to.join(META);
    std::fs::write(&meta, json).with_context(|| format!("writing {}", meta.display()))?;
    Ok(kept)
}

/// Every kept save by name, sorted.
pub fn list() -> Result<Vec<(String, Kept)>> {
    Ok(list_in(&library()?))
}

/// Every kept save in the library `dir`: its folders a kept save's name
/// can name, so not a stray file or a half-made copy.
fn list_in(dir: &Path) -> Vec<(String, Kept)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut kept: Vec<(String, Kept)> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_str()?.to_owned();
            if !e.file_type().ok()?.is_dir() || !plain(&name) {
                return None;
            }
            let meta = std::fs::read(e.path().join(META)).unwrap_or_default();
            Some((name, serde_json::from_slice(&meta).unwrap_or_default()))
        })
        .collect();
    kept.sort_by(|a, b| a.0.cmp(&b.0));
    kept
}

/// `from`'s save files into a fresh `to`: the top-level `.sav` files, and
/// every `slot_NN` folder or only `slot`'s, renamed `slot_00`. The copy is
/// made beside `to` and replaces it only once whole, so a failed one
/// leaves the old `to` as it was.
fn copy(from: &Path, to: &Path, slot: Option<u8>) -> Result<()> {
    if let Some(n) = slot {
        let slot = from.join(format!("slot_{n:02}"));
        ensure!(slot.is_dir(), "{} has no slot {n}", from.display());
    }
    let entries = std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))?;
    let slots = entries.flatten().filter(|e| {
        let dir = e.file_type().is_ok_and(|t| t.is_dir());
        dir && is_save(&e.file_name().to_string_lossy(), dir)
    });
    ensure!(
        slots.count() > 0,
        "{} holds no slot_NN folder: --from takes the folder the slots are in",
        from.display()
    );
    let name = to.file_name().context("a kept save's folder has no name")?;
    let part = to.with_file_name(format!(".{}.part", name.to_string_lossy()));
    let _ = std::fs::remove_dir_all(&part);
    if let Err(e) = copy_into(from, &part, slot) {
        let _ = std::fs::remove_dir_all(&part);
        return Err(e);
    }
    swap(&part, to).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&part);
    })
}

/// Put the finished `part` in `to`'s place. The old `to` is moved aside
/// first and deleted only once `part` is in, so a failed swap puts it
/// back, or names where it's left when that fails too. A leftover old
/// copy with no `to` is the only one, so it's refused, never deleted.
fn swap(part: &Path, to: &Path) -> Result<()> {
    let name = to.file_name().context("a kept save's folder has no name")?;
    let old = to.with_file_name(format!(".{}.old", name.to_string_lossy()));
    if old.exists() {
        ensure!(
            to.exists(),
            "{} holds the only copy of an earlier keep: move it back to {} or delete it",
            old.display(),
            to.display()
        );
        let _ = std::fs::remove_dir_all(&old);
    }
    let aside = match std::fs::rename(to, &old) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e).with_context(|| format!("moving {} aside", to.display())),
    };
    if let Err(e) = std::fs::rename(part, to) {
        let err = anyhow::Error::new(e).context(format!("moving the copy to {}", to.display()));
        return match aside.then(|| std::fs::rename(&old, to)) {
            Some(Err(back)) => Err(err.context(format!(
                "the earlier keep is left in {} ({back})",
                old.display()
            ))),
            _ => Err(err),
        };
    }
    if aside && let Err(e) = std::fs::remove_dir_all(&old) {
        log::warn!("removing the old copy {}: {e}", old.display());
    }
    Ok(())
}

/// `copy`'s files, into a `to` made for them.
fn copy_into(from: &Path, to: &Path, slot: Option<u8>) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("making {}", to.display()))?;
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let dir = entry.file_type()?.is_dir();
        if !is_save(&name, dir) {
            continue;
        }
        match (dir, slot_of(&name)) {
            (true, Some(n)) => match slot {
                Some(want) if want != n => {}
                Some(_) => copy_dir(&entry.path(), &to.join("slot_00"))?,
                None => copy_dir(&entry.path(), &to.join(&name))?,
            },
            _ => copy_file(&entry.path(), &to.join(&name))?,
        }
    }
    Ok(())
}

/// Whether `name`, a folder when `dir`, is part of a save folder's save:
/// a `slot_NN` folder or a `.sav` file.
pub(crate) fn is_save(name: &str, dir: bool) -> bool {
    match dir {
        true => slot_of(name).is_some(),
        false => name.ends_with(".sav"),
    }
}

/// The number of a `slot_NN` folder's name: two digits, as the game
/// names them.
fn slot_of(name: &str) -> Option<u8> {
    let n = name.strip_prefix("slot_")?;
    (n.len() == 2 && n.bytes().all(|b| b.is_ascii_digit())).then(|| n.parse().ok())?
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("making {}", to.display()))?;
    let entries = std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))?;
    for entry in entries {
        let entry = entry?;
        copy_file(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    std::fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
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
    #[cfg(unix)]
    fn a_failed_copy_keeps_the_old_keep_whole() {
        let root = std::env::temp_dir().join(format!("zootr-keep-fail-{}", std::process::id()));
        let (from, to) = (root.join("save"), root.join("kept"));
        std::fs::create_dir_all(from.join("slot_00")).unwrap();
        std::fs::write(from.join("slot_00/progress.sav"), "new").unwrap();
        std::os::unix::fs::symlink(root.join("gone"), from.join("broken.sav")).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("option.sav"), "kept before").unwrap();
        let err = format!("{:#}", copy(&from, &to, None).unwrap_err());
        assert!(err.contains("broken.sav"), "{err}");
        assert!(to.join("option.sav").exists());
        assert!(!to.join("slot_00").exists());
        assert!(!root.join(".kept.part").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_failed_swap_puts_the_old_keep_back() {
        let root = std::env::temp_dir().join(format!("zootr-keep-swap-{}", std::process::id()));
        let to = root.join("kept");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("option.sav"), "kept before").unwrap();
        let err = format!("{:#}", swap(&root.join(".kept.part"), &to).unwrap_err());
        assert!(err.contains("moving the copy"), "{err}");
        assert!(to.join("option.sav").exists());
        assert!(!root.join(".kept.old").exists());
        // A whole part goes in, and the old copy goes.
        let part = root.join(".kept.part");
        std::fs::create_dir_all(&part).unwrap();
        std::fs::write(part.join("account.sav"), "new").unwrap();
        swap(&part, &to).unwrap();
        assert!(to.join("account.sav").exists() && !to.join("option.sav").exists());
        assert!(!part.exists() && !root.join(".kept.old").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_leftover_old_keep_is_never_deleted_when_it_is_the_only_copy() {
        let root = std::env::temp_dir().join(format!("zootr-keep-old-{}", std::process::id()));
        let (from, to, old) = (root.join("save"), root.join("kept"), root.join(".kept.old"));
        std::fs::create_dir_all(from.join("slot_00")).unwrap();
        std::fs::write(from.join("slot_00/progress.sav"), "new").unwrap();
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("option.sav"), "kept before").unwrap();
        let err = format!("{:#}", copy(&from, &to, None).unwrap_err());
        assert!(err.contains(".kept.old"), "{err}");
        assert!(old.join("option.sav").exists());
        assert!(!to.exists() && !root.join(".kept.part").exists());
        // Beside a whole keep, a leftover is stale and goes.
        std::fs::create_dir_all(&to).unwrap();
        copy(&from, &to, None).unwrap();
        assert!(to.join("slot_00/progress.sav").exists() && !old.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn slots_are_folders_named_with_two_digits() {
        let root = std::env::temp_dir().join(format!("zootr-keep-slots-{}", std::process::id()));
        let (from, to) = (root.join("save"), root.join("kept"));
        std::fs::create_dir_all(from.join("slot_3")).unwrap();
        std::fs::write(from.join("slot_00"), "a file, not a slot").unwrap();
        let err = copy(&from, &to, None).unwrap_err().to_string();
        assert!(err.contains("no slot_NN folder"), "{err}");
        assert_eq!(slot_of("slot_03"), Some(3));
        assert_eq!(slot_of("slot_+1"), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_list_holds_only_kept_save_folders() {
        let root = std::env::temp_dir().join(format!("zootr-keep-list-{}", std::process::id()));
        std::fs::create_dir_all(root.join("deku")).unwrap();
        std::fs::create_dir_all(root.join(".deku.part")).unwrap();
        std::fs::write(root.join(".DS_Store"), "").unwrap();
        std::fs::write(root.join("notes"), "").unwrap();
        let names: Vec<_> = list_in(&root).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["deku"]);
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
