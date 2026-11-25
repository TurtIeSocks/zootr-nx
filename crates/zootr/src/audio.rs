//! Audio: a seed's music shuffled within its kind, as Ship of Harkinian's
//! Audio Editor does. Every track keeps its own graph: a seed repoints the
//! streams its asset nodes play and copies each new stream's call into the
//! track's SLink2 user, since a key the user lacks plays silence. Moving
//! graphs between tracks left them silent in every live test. The ocarina
//! songs aren't in the music table: each is a call of the `Ocarina` user,
//! repointed at another's asset. Romfs files only: the hook patch never
//! differs by music. See
//! `superpowers/specs/2026-11-24-audio-design.md`.

mod files;
pub mod listen;
mod pick;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlDoc, ModBuilder, xlink_call_table, xlink_calls, xlink_copy_call};
use serde::{Deserialize, Serialize};

use crate::ageless_items::SOUNDS;
pub use files::{HELD, OCARINA, STREAMS, TABLE, TRACKS};
pub use pick::{Audio, Draft, PoolMode, VANILLA, args, draft, parse_pool, parse_track, resolve};

/// A track's kind. Tracks shuffle only within their pool.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    clap::ValueEnum,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Pool {
    /// Looping world music: towns, field, dungeons, houses, fountains.
    Area,
    /// Enemy, mini-boss and boss music.
    Battle,
    /// Cutscene music.
    Event,
    /// Gets, the medallion and stone, game over, riddles, sleep.
    Fanfare,
    /// The songs Link plays and the ones he's taught.
    Ocarina,
    /// The silences: never shuffled.
    Fixed,
}

impl Pool {
    /// Where a one-shot's length band ends, in seconds. A pool without
    /// edges has no bands: its tracks loop.
    fn edges(self) -> &'static [f32] {
        match self {
            Pool::Event => &[12.0, 25.0],
            Pool::Fanfare => &[5.0, 12.0],
            // Lessons and the short songs, the longer songs, the warp songs.
            Pool::Ocarina => &[7.0, 14.0],
            Pool::Area | Pool::Battle | Pool::Fixed => &[],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pool::Area => "area",
            Pool::Battle => "battle",
            Pool::Event => "event",
            Pool::Fanfare => "fanfare",
            Pool::Ocarina => "ocarina",
            Pool::Fixed => "fixed",
        }
    }
}

/// One track of the music table, or an ocarina song.
#[derive(Debug)]
pub struct Track {
    /// Ours, as the CLI and the pane name it.
    pub id: &'static str,
    /// In player words.
    pub label: &'static str,
    pub pool: Pool,
    /// The graph's name in the music table, or an ocarina song's call in
    /// the [`OCARINA`] user.
    pub graph: &'static str,
    /// The one stream this track gives another: its only stream, or a
    /// sectioned track's whole-song stream. None: it receives only.
    pub song: Option<&'static str>,
    /// The song's length, or a songless track's longest stream's.
    pub secs: f32,
    /// Whether that stream loops (a songless track: any of its streams).
    pub looping: bool,
}

impl Track {
    /// Its length band in its pool: 0 for a loop or a pool without bands,
    /// else 1 and up, shortest first. A one-shot shuffles within its band.
    pub fn band(&self) -> usize {
        let edges = self.pool.edges();
        match self.looping || edges.is_empty() {
            true => 0,
            false => 1 + edges.iter().filter(|&&e| self.secs >= e).count(),
        }
    }

    /// The band in words, as `audio list` shows it.
    pub fn band_label(&self) -> String {
        let edges = self.pool.edges();
        match self.band() {
            _ if edges.is_empty() => "-".into(),
            0 => "loop".into(),
            1 => format!("<{}s", edges[0]),
            b if b > edges.len() => format!(">{}s", edges[b - 2]),
            b => format!("{}-{}s", edges[b - 2], edges[b - 1]),
        }
    }
}

/// A stream by a track's id (its song) or its own name.
pub fn stream_name(name: &str) -> Result<&str> {
    match TRACKS.iter().find(|t| t.id == name) {
        Some(t) => {
            (t.song).with_context(|| format!("{name} has no song: its music is in sections"))
        }
        None => Ok(name),
    }
}

/// The BWAV of `name` (a track's id or a stream's name) from the dump at
/// `game`, with the stream's name.
pub fn stream<'a>(game: &Path, name: &'a str) -> Result<(&'a str, Vec<u8>)> {
    let name = stream_name(name)?;
    let path = format!("{STREAMS}{name}.bwav");
    let found = crate::game::search(game, &path)?;
    let (_, data) = (found.into_iter().find(|(p, _)| *p == path))
        .with_context(|| format!("no stream {name}"))?;
    Ok((name, data))
}

/// `name`'s stream as a stereo WAV.
pub fn wav(game: &Path, name: &str) -> Result<Vec<u8>> {
    let (_, data) = stream(game, name)?;
    let rate = ounce_formats::bwav::header(&data)?.rate;
    Ok(ounce_formats::bwav::wav(
        &ounce_formats::bwav::decode(&data)?,
        rate,
    ))
}

/// Track `id`'s song as a WAV file, decoded once a dump and kept in its
/// cache, since finding a stream walks the whole dump.
pub fn song_wav(game: &Path, id: &str) -> Result<PathBuf> {
    let song = track(id)?
        .song
        .with_context(|| format!("{id} has no song: its music is in sections"))?;
    let file = crate::game::cached(game, "audio")?.join(format!("{song}.wav"));
    if !file.exists() {
        let wav = wav(game, song)?;
        std::fs::create_dir_all(file.parent().context("a cache folder")?)?;
        // Written whole, then renamed: a play racing the decode never reads half.
        let part = file.with_extension("part");
        std::fs::write(&part, wav)?;
        std::fs::rename(&part, &file)?;
    }
    Ok(file)
}

pub fn track(id: &str) -> Result<&'static Track> {
    TRACKS
        .iter()
        .find(|t| t.id == id)
        .with_context(|| format!("no track {id}: `zootr audio list` names them"))
}

/// The spoiler's section: each slot a seed changed and the song it plays.
/// Nothing for a seed in vanilla music.
pub fn spoiler(audio: &Audio) -> String {
    let width = audio.keys().map(|id| id.len()).max().unwrap_or(0);
    let lines = audio.iter().map(|(slot, donor)| {
        let label = track(donor).map_or("?", |t| t.label);
        format!("  {slot:<width$}  {donor} ({label})\n")
    });
    match audio.is_empty() {
        true => String::new(),
        false => format!("\nAudio\n{}", lines.collect::<String>()),
    }
}

/// Play each slot's new song. Writes nothing for a seed in vanilla music.
pub fn write(builder: &mut ModBuilder, audio: &Audio) -> Result<()> {
    let (songs, music): (Audio, Audio) = (audio.clone().into_iter())
        .partition(|(slot, _)| track(slot).is_ok_and(|t| t.pool == Pool::Ocarina));
    if !music.is_empty() {
        write_music(builder, &music)?;
    }
    if !songs.is_empty() {
        write_songs(builder, &songs)?;
    }
    Ok(())
}

/// Each ocarina slot's call takes its donor call's params, all read before
/// any is written, so two songs can trade places.
fn write_songs(builder: &mut ModBuilder, songs: &Audio) -> Result<()> {
    let mut slink = builder.read(SOUNDS)?;
    let names = xlink_calls(&slink, OCARINA)?;
    let calls = xlink_call_table(&slink, OCARINA)?;
    let call = |id: &str| {
        let name = track(id)?.graph;
        let i = (names.iter().position(|n| *n == name))
            .with_context(|| format!("{OCARINA} has no call {name}"))?;
        ensure!(!calls[i].container, "{name} isn't an asset");
        Ok(&calls[i])
    };
    let moves = (songs.iter())
        .map(|(slot, donor)| Ok((call(slot)?.param_at(), call(donor)?.param)))
        .collect::<Result<Vec<_>>>()?;
    for (at, param) in moves {
        slink[at..at + 4].copy_from_slice(&param.to_le_bytes());
    }
    builder.write(SOUNDS, &slink)
}

/// Each music slot's graph keys repointed at its new song, and its user
/// given the song's call.
fn write_music(builder: &mut ModBuilder, audio: &Audio) -> Result<()> {
    let mut doc = BymlDoc::parse(&builder.read(TABLE)?)?;
    let mut slink = builder.read(SOUNDS)?;
    let graphs = (doc.root.as_mut_map()?.get_mut("BGM"))
        .context("the music table has no BGM")?
        .as_mut_map()?;
    let users = graphs
        .iter()
        .map(|(name, graph)| Ok((name.to_string(), user(graph)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    for (slot, donor) in audio {
        let (slot, donor) = (track(slot)?, track(donor)?);
        let song = donor
            .song
            .with_context(|| format!("{} gives no song", donor.id))?;
        let from = users
            .get(donor.graph)
            .with_context(|| format!("no {}", donor.graph))?;
        ensure!(
            xlink_calls(&slink, from)?.contains(&song),
            "{}: its user {from} has no call {song}",
            donor.id
        );
        let graph = graphs
            .get_mut(slot.graph)
            .with_context(|| format!("no {}", slot.graph))?;
        let to = user(graph)?;
        let own = xlink_calls(&slink, &to)?;
        let replaced = rekey(graph, song)?;
        ensure!(!replaced.is_empty(), "{}: no stream to replace", slot.id);
        // A key its own user lacks: the table isn't the one the dump has.
        if let Some(old) = replaced.iter().find(|k| !own.contains(&k.as_str())) {
            bail!("{}: plays {old}, which its user {to} lacks", slot.id);
        }
        if !own.contains(&song) {
            slink = xlink_copy_call(&slink, from, song, &to, song)?;
        }
    }
    builder.write(SOUNDS, &slink)?;
    builder.write(TABLE, &doc.to_binary())
}

fn user(graph: &Byml) -> Result<String> {
    match graph.as_map()?.get("SLinkUserName") {
        Some(Byml::String(s)) => Ok(s.to_string()),
        _ => bail!("a music graph without an SLinkUserName"),
    }
}

/// A graph's nodes, kept as an array or a map.
fn nodes(graph: &mut Byml) -> Result<Vec<&mut Byml>> {
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

    #[test]
    fn ids_are_unique_and_every_shuffled_track_has_a_band() {
        let mut ids: Vec<_> = TRACKS.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), TRACKS.len());
        for t in &TRACKS {
            assert_eq!(t.pool == Pool::Fixed, t.secs == 0.0, "{}", t.id);
            assert!(t.band() <= t.pool.edges().len() + 1, "{}", t.id);
        }
    }

    #[test]
    fn a_one_shot_takes_its_length_band_and_a_loop_takes_none() {
        let band = |id| track(id).unwrap().band_label();
        assert_eq!(band("item-get-small"), "<5s");
        assert_eq!(band("medal-get"), "5-12s");
        assert_eq!(band("spirit-stone-get"), ">12s");
        assert_eq!(band("goron-city"), "-");
        assert_eq!(band("scene-zora-river-0000"), "loop");
        assert_eq!(band("taught-suns-song"), "<7s");
        assert_eq!(band("played-zeldas-lullaby"), "7-14s");
        assert_eq!(band("played-requiem-of-spirit"), ">14s");
    }

    /// The dump's music table and streams match the catalog: every track
    /// listed and no other, each song a call in its track's user, each
    /// length and loop flag its stream's.
    #[test]
    fn a_song_decodes_to_a_wav_as_long_as_the_catalog_says() {
        if crate::game::dev_dump().is_none() {
            return;
        }
        let game = std::path::Path::new(crate::game::DEV_DUMP);
        let medal = wav(game, "medal-get").unwrap();
        assert_eq!(&medal[..4], b"RIFF");
        // 44 bytes of header, then 4 bytes a stereo frame at 48 kHz.
        let secs = (medal.len() - 44) as f32 / 4.0 / 48_000.0;
        assert!(
            (secs - track("medal-get").unwrap().secs).abs() < 0.01,
            "{secs}"
        );
        assert!(
            wav(game, "goron-city").is_ok(),
            "a song of a sectioned track"
        );
        assert!(wav(game, "market").is_err(), "sections give no song");
    }

    #[test]
    fn the_catalog_matches_the_dump() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = std::path::Path::new(crate::game::DEV_DUMP);
        let doc = BymlDoc::parse(&romfs.read(TABLE).unwrap()).unwrap();
        let slink = romfs.read(SOUNDS).unwrap();
        let graphs = doc.root.as_map().unwrap()["BGM"].as_map().unwrap();
        let mut names: Vec<_> = graphs.keys().map(|k| k.to_string()).collect();
        let (songs, music): (Vec<_>, Vec<_>) = TRACKS.iter().partition(|t| t.pool == Pool::Ocarina);
        let mut listed: Vec<_> = music.iter().map(|t| t.graph.to_owned()).collect();
        names.sort();
        listed.sort();
        assert_eq!(names, listed);
        let streams = crate::game::search(game, STREAMS).unwrap();
        let header = |name: &str| {
            let path = format!("{STREAMS}{name}.bwav");
            let (_, data) = (streams.iter().find(|(p, _)| *p == path))
                .unwrap_or_else(|| panic!("no stream {name}"));
            ounce_formats::bwav::header(data).unwrap()
        };
        let ocarina = xlink_calls(&slink, OCARINA).unwrap();
        let calls = xlink_call_table(&slink, OCARINA).unwrap();
        for t in songs {
            let i = (ocarina.iter().position(|n| *n == t.graph))
                .unwrap_or_else(|| panic!("{OCARINA} lacks {}", t.graph));
            assert!(!calls[i].container, "{}", t.id);
            let head = header(t.song.unwrap());
            assert!(
                (head.seconds() - t.secs).abs() < 0.01,
                "{}: {} s",
                t.id,
                head.seconds()
            );
            assert_eq!(head.loop_start.is_some(), t.looping, "{}", t.id);
        }
        for t in music {
            let mut graph = graphs[t.graph].clone();
            let mut keys: Vec<String> = (nodes(&mut graph).unwrap().into_iter())
                .filter_map(|n| match n.as_map().ok()?.get("Key") {
                    Some(Byml::String(k)) if !HELD.contains(&k.as_str()) => Some(k.to_string()),
                    _ => None,
                })
                .collect();
            keys.dedup();
            if t.pool == Pool::Fixed {
                assert!(keys.is_empty() && t.song.is_none(), "{}", t.id);
                continue;
            }
            let user = user(&graph).unwrap();
            let calls = xlink_calls(&slink, &user).unwrap();
            if let Some(song) = t.song {
                assert!(calls.contains(&song), "{}: {user} lacks {song}", t.id);
            }
            let heard: Vec<_> = t.song.map_or(keys.clone(), |s| vec![s.to_owned()]);
            let heads: Vec<_> = heard.iter().map(|k| header(k)).collect();
            let secs = heads.iter().map(|h| h.seconds()).fold(0.0, f32::max);
            let looping = heads.iter().any(|h| h.loop_start.is_some());
            assert!((secs - t.secs).abs() < 0.01, "{}: {secs} s", t.id);
            assert_eq!(looping, t.looping, "{}", t.id);
        }
    }

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
}
