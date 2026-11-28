//! Sound cues (stage 7): a cue plays another user's cue once that user's
//! bank loads beside its own.

use anyhow::{Context, Result, bail};
use ounce_formats::{Byml, BymlDoc};

use crate::known::field;

/// Every user the bank list gives a bank, by name.
pub fn banked_users(banks: &[u8]) -> Result<Vec<String>> {
    let doc = BymlDoc::parse(banks)?;
    let table = (doc.root.as_map()?.get(field::BARSLIST_TABLE.get()))
        .with_context(|| format!("no {}", field::BARSLIST_TABLE))?
        .as_map()?;
    Ok(table.keys().map(|k| k.to_string()).collect())
}

/// `user`'s banks in the bank list.
pub fn banks_of<'a>(doc: &'a BymlDoc, user: &str) -> Result<Vec<&'a str>> {
    let table = (doc.root.as_map()?.get(field::BARSLIST_TABLE.get()))
        .with_context(|| format!("no {}", field::BARSLIST_TABLE))?
        .as_map()?;
    let row = (table.get(user)).with_context(|| format!("{user} has no banks"))?;
    let Some(Byml::Array(list)) = row.as_map()?.get(field::BARS.get()) else {
        bail!("{user} has no {}", field::BARS);
    };
    list.iter().map(|b| Ok(b.as_string()?.as_str())).collect()
}

/// The bank list at `bytes` with each `(to, from)` pair's `to` loading
/// `from`'s banks beside its own. A pair of one user changes nothing.
pub fn load(bytes: &[u8], pairs: &[(&str, &str)]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(bytes)?;
    let shipped = BymlDoc::parse(bytes)?;
    for &(to, from) in pairs {
        let theirs = banks_of(&shipped, from)?;
        let table = (doc.root.as_mut_map()?.get_mut(field::BARSLIST_TABLE.get()))
            .context("no bank list")?
            .as_mut_map()?;
        let row = (table.get_mut(to)).with_context(|| format!("{to} has no banks"))?;
        let Some(Byml::Array(list)) = row.as_mut_map()?.get_mut(field::BARS.get()) else {
            bail!("{to} has no {}", field::BARS);
        };
        for bank in theirs {
            let bank = Byml::String(bank.into());
            if !list.contains(&bank) {
                list.push(bank);
            }
        }
    }
    Ok(doc.to_binary())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::ageless_items::{BANKS as BANK_LIST, SOUNDS};
    use crate::audio::write::{sfx_gives, sfx_variants};
    use crate::audio::{STREAMS, TRACKS, Track, Via};

    /// The sound cues and their pairs.
    fn cues() -> Vec<(&'static Track, &'static [(&'static str, &'static str)])> {
        (TRACKS.iter())
            .filter_map(|t| match t.via {
                Via::Sfx(pairs) => Some((t, pairs)),
                _ => None,
            })
            .collect()
    }

    /// Every pair's user has banks and plays its stem; each cue's first
    /// pair gives its song first, as long as listed, never looping. Its
    /// given takes are in its user's banks or streamed.
    #[test]
    fn every_sound_cue_matches_the_dump() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let slink = romfs.read(SOUNDS).unwrap();
        let list = BymlDoc::parse(&romfs.read(BANK_LIST).unwrap()).unwrap();
        let mut takes = BTreeMap::new();
        for (t, pairs) in cues() {
            for &(user, stem) in pairs {
                banks_of(&list, user).unwrap_or_else(|e| panic!("{}: {e}", t.id));
                sfx_variants(&slink, user, stem).unwrap_or_else(|e| panic!("{}: {e}", t.id));
            }
            let streams = (sfx_gives(&slink, pairs[0].0, pairs[0].1)
                .unwrap()
                .into_iter())
            .map(|(_, call)| {
                let names = ounce_formats::xlink_asset_names(&slink, call.param).unwrap();
                names
                    .into_iter()
                    .find(|(bit, _)| *bit == 1)
                    .unwrap()
                    .1
                    .to_owned()
            })
            .collect::<Vec<_>>();
            assert_eq!(Some(streams[0].as_str()), t.song, "{}", t.id);
            takes.insert(t.id, streams);
        }
        let wanted: BTreeSet<String> = (cues().iter())
            .flat_map(|(_, pairs)| banks_of(&list, pairs[0].0).unwrap())
            .map(|bank| format!("Sound/Resource/{bank}.zs"))
            .chain(
                takes
                    .values()
                    .flatten()
                    .map(|s| format!("{STREAMS}{s}.bwav")),
            )
            .collect();
        let mut found = BTreeMap::new();
        let game = std::path::Path::new(crate::game::DEV_DUMP);
        crate::game::each_where(
            game,
            |p| wanted.contains(p),
            |path, data| {
                let data = match path.ends_with(".zs") {
                    true => romfs.dicts().decompress(&path, &data)?,
                    false => data,
                };
                found.insert(path, data);
                Ok(())
            },
        )
        .unwrap();
        for (t, _) in cues() {
            let secs = (takes[t.id].iter())
                .map(|name| {
                    let data = match found.get(&format!("{STREAMS}{name}.bwav")) {
                        Some(data) => data.as_slice(),
                        None => (found.values())
                            .find_map(|b| ounce_formats::bars::bwav(b, name).ok().flatten())
                            .unwrap_or_else(|| panic!("{}: no take {name}", t.id)),
                    };
                    let head = ounce_formats::bwav::header(data).unwrap();
                    assert!(head.loop_start.is_none(), "{}: {name} loops", t.id);
                    head.seconds()
                })
                .fold(0.0, f32::max);
            assert!((secs - t.secs).abs() < 0.01, "{}: {secs} s", t.id);
            assert!(!t.looping, "{}", t.id);
        }
    }
}
