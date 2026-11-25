//! A seed's music written to its mod. A table slot's graph is rekeyed at
//! its new song and its user given the song's call. An ocarina slot's call
//! is repointed at another call of its user. A call swapped across users
//! played silence both ways live, so a call track only trades within its
//! user.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{
    Byml, BymlDoc, ModBuilder, XlinkCall, xlink_call_table, xlink_calls, xlink_copy_call,
};

use super::{Audio, HELD, TABLE, Track, Via, track};
use crate::ageless_items::SOUNDS;

/// Play each slot's new song. Writes nothing for a seed in vanilla music.
pub fn write(builder: &mut ModBuilder, audio: &Audio) -> Result<()> {
    if audio.is_empty() {
        return Ok(());
    }
    let mut doc = BymlDoc::parse(&builder.read(TABLE)?)?;
    let mut slink = builder.read(SOUNDS)?;
    let graphs = (doc.root.as_mut_map()?.get_mut("BGM"))
        .context("the music table has no BGM")?
        .as_mut_map()?;
    let users = graphs
        .iter()
        .map(|(name, graph)| Ok((name.to_string(), user(graph)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let (mut repoints, mut table) = (vec![], false);
    for (slot, donor) in audio {
        let (slot, donor) = (track(slot)?, track(donor)?);
        let song = (donor.song).with_context(|| format!("{} gives no song", donor.id))?;
        match (slot.via, donor.via) {
            (Via::Table, Via::Table) => {
                let from =
                    (users.get(donor.graph)).with_context(|| format!("no {}", donor.graph))?;
                let graph =
                    (graphs.get_mut(slot.graph)).with_context(|| format!("no {}", slot.graph))?;
                slink = give(slink, graph, slot, from, song)?;
                table = true;
            }
            (Via::Call(to), Via::Call(from)) if to == from => {
                // Read now, written once every song is read, so two can trade.
                repoints.push((to, slot.graph, asset(&slink, from, donor.graph)?.param));
            }
            _ => bail!(
                "{} can't play {}: a call trades only within its user",
                slot.id,
                donor.id
            ),
        }
    }
    for (user, name, param) in repoints {
        let at = asset(&slink, user, name)?.param_at();
        slink[at..at + 4].copy_from_slice(&param.to_le_bytes());
    }
    builder.write(SOUNDS, &slink)?;
    if table {
        builder.write(TABLE, &doc.to_binary())?;
    }
    Ok(())
}

/// A music slot's graph keys repointed at the call `song` of `from`, and
/// its user given that call when it lacks one.
fn give(slink: Vec<u8>, graph: &mut Byml, slot: &Track, from: &str, song: &str) -> Result<Vec<u8>> {
    asset(&slink, from, song)?;
    let to = user(graph)?;
    let own = xlink_calls(&slink, &to)?;
    let replaced = rekey(graph, song)?;
    ensure!(!replaced.is_empty(), "{}: no stream to replace", slot.id);
    // A key its own user lacks: the table isn't the one the dump has.
    if let Some(old) = replaced.iter().find(|k| !own.contains(&k.as_str())) {
        bail!("{}: plays {old}, which its user {to} lacks", slot.id);
    }
    match own.contains(&song) {
        true => Ok(slink),
        false => xlink_copy_call(&slink, from, song, &to, song),
    }
}

/// `user`'s call `name`, which must play an asset of its own.
pub(super) fn asset(slink: &[u8], user: &str, name: &str) -> Result<XlinkCall> {
    let i = (xlink_calls(slink, user)?.iter().position(|n| *n == name))
        .with_context(|| format!("{user} has no call {name}"))?;
    let call = xlink_call_table(slink, user)?.swap_remove(i);
    ensure!(!call.container, "{user}'s {name} isn't an asset");
    Ok(call)
}

pub(super) fn user(graph: &Byml) -> Result<String> {
    match graph.as_map()?.get("SLinkUserName") {
        Some(Byml::String(s)) => Ok(s.to_string()),
        _ => bail!("a music graph without an SLinkUserName"),
    }
}

/// A graph's nodes, kept as an array or a map.
pub(super) fn nodes(graph: &mut Byml) -> Result<Vec<&mut Byml>> {
    Ok(match graph.as_mut_map()?.get_mut("Node") {
        Some(Byml::Array(a)) => a.iter_mut().collect(),
        Some(Byml::Map(m)) => m.values_mut().collect(),
        None => vec![],
        _ => bail!("a music graph's Node is neither array nor map"),
    })
}

/// Point every asset key but the held ones at `song`, returning the keys
/// replaced.
fn rekey(graph: &mut Byml, song: &str) -> Result<Vec<String>> {
    let mut replaced = Vec::new();
    for node in nodes(graph)? {
        if let Ok(map) = node.as_mut_map()
            && let Some(Byml::String(key)) = map.get_mut("Key")
            && !HELD.contains(&key.as_str())
        {
            replaced.push(key.to_string());
            *key = song.into();
        }
    }
    Ok(replaced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::OCARINA;

    /// Goron City's song into Kokiri Forest, a sectioned track whose user
    /// lacks it, and the medallion's into a get with the held build-up.
    #[test]
    fn a_slot_plays_its_donors_song_and_its_user_gains_the_call() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-audio-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let audio = Audio::from([
            ("kokiri-forest".into(), "goron-city".into()),
            ("item-get-small".into(), "medal-get".into()),
        ]);
        write(&mut builder, &audio).unwrap();
        builder.finish().unwrap();
        let doc = BymlDoc::parse(&crate::game::read_built(&romfs, &dir, TABLE)).unwrap();
        let slink = crate::game::read_built(&romfs, &dir, SOUNDS);
        std::fs::remove_dir_all(dir).unwrap();
        let graphs = doc.root.as_map().unwrap()["BGM"].as_map().unwrap();
        for (slot, donor) in &audio {
            let (slot, song) = (track(slot).unwrap(), track(donor).unwrap().song.unwrap());
            let mut graph = graphs[slot.graph].clone();
            let keys: Vec<String> = (nodes(&mut graph).unwrap().into_iter())
                .filter_map(|n| match n.as_map().ok()?.get("Key") {
                    Some(Byml::String(k)) => Some(k.to_string()),
                    _ => None,
                })
                .collect();
            assert!(keys.iter().all(|k| k == song || HELD.contains(&k.as_str())));
            assert!(keys.iter().any(|k| k == song), "{}", slot.id);
            let user = user(&graph).unwrap();
            assert!(xlink_calls(&slink, &user).unwrap().contains(&song));
        }
    }

    /// Two ocarina songs trade places: each call plays the other's asset,
    /// and the music table is left alone.
    #[test]
    fn two_ocarina_songs_trade_places() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-ocarina-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let audio = Audio::from([
            ("played-eponas-song".into(), "played-suns-song".into()),
            ("played-suns-song".into(), "played-eponas-song".into()),
        ]);
        write(&mut builder, &audio).unwrap();
        builder.finish().unwrap();
        let built = crate::game::read_built(&romfs, &dir, SOUNDS);
        let wrote_table = dir.join("romfs").join(TABLE).exists();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(!wrote_table);
        let param = |file: &[u8], id: &str| {
            let names = xlink_calls(file, OCARINA).unwrap();
            let i = names
                .iter()
                .position(|n| *n == track(id).unwrap().graph)
                .unwrap();
            xlink_call_table(file, OCARINA).unwrap()[i].param
        };
        let vanilla = romfs.read(SOUNDS).unwrap();
        for (slot, donor) in &audio {
            assert_eq!(param(&built, slot), param(&vanilla, donor), "{slot}");
        }
    }

    /// A call never trades across users, either way.
    #[test]
    fn a_call_stays_in_its_user() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-refuse-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut refused = |slot: &str, donor: &str| {
            let audio = Audio::from([(slot.into(), donor.into())]);
            write(&mut builder, &audio).is_err()
        };
        assert!(refused("drugstore", "played-eponas-song"));
        assert!(refused("played-eponas-song", "drugstore"));
        std::fs::remove_dir_all(dir).ok();
    }
}
