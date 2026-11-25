//! `zootr probe text`: the game's messages that match, by file and id.

use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{Msbt, Part, Romfs, messages};
use roead::sarc::Sarc;

use crate::names::TEXT;

pub(super) fn text(romfs: &Romfs, pattern: &str, mod_dir: Option<&Path>, tags: bool) -> Result<()> {
    let data = match mod_dir {
        Some(dir) => {
            let path = dir.join("romfs").join(TEXT);
            let raw =
                std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            romfs.dicts().decompress(TEXT, &raw)?
        }
        None => romfs.read(TEXT)?,
    };
    let sarc = Sarc::new(&data).with_context(|| format!("reading {TEXT}"))?;
    let want = pattern.to_lowercase();
    for f in sarc.files() {
        let Some(name) = f.name().filter(|n| n.ends_with(".msbt")) else {
            continue;
        };
        let mut found: Vec<_> = messages(f.data())
            .with_context(|| name.to_owned())?
            .into_iter()
            .filter(|(id, t)| [id, t].iter().any(|s| s.to_lowercase().contains(&want)))
            .collect();
        found.sort();
        let msbt = tags.then(|| Msbt::parse(f.data())).transpose()?;
        for (id, t) in found {
            let t = match &msbt {
                Some(m) => tagged(&m.get(&id)?.unwrap_or_default()),
                None => t,
            };
            println!("{name}:{id} {}", t.replace('\n', " / "));
        }
    }
    Ok(())
}

/// `parts` as text with each tag's bytes in place.
fn tagged(parts: &[Part]) -> String {
    let hex = |raw: &[u8]| raw.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>();
    (parts.iter())
        .map(|p| match p {
            Part::Text(t) => t.clone(),
            Part::Tag(raw) => format!("<{}>", hex(raw).join(" ")),
        })
        .collect()
}
