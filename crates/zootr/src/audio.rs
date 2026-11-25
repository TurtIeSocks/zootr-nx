//! Audio: a seed's music shuffled within its kind, as Ship of Harkinian's
//! Audio Editor does. Every track keeps its own graph: a seed repoints the
//! streams its asset nodes play and copies each new stream's call into the
//! track's SLink2 user, since a key the user lacks plays silence. Moving
//! graphs between tracks left them silent in every live test. The ocarina
//! songs and Link's voice aren't in the music table: each is a call of an
//! SLink2 user, repointed at another's asset within that user. Romfs files
//! only: the hook patch never differs by music. See
//! `superpowers/specs/2026-11-24-audio-design.md`.

mod files;
pub mod listen;
mod pick;
mod write;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

pub use files::{BANKS, HELD, OCARINA, STREAMS, TABLE, TRACKS};
pub use pick::{Audio, Draft, PoolMode, VANILLA, args, draft, parse_pool, parse_track, resolve};
pub use write::write;

/// Where a track's music plays from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    /// Its graph in the music table.
    Table,
    /// The call of this SLink2 user named its graph (an ocarina song). It
    /// trades only within the user: across users, a swap played silence.
    Call(&'static str),
    /// The voice calls of this SLink2 user named its graph and a number
    /// (`SwordAttack_S_01`...), which the game calls by name: a cue of
    /// variants. A slot's variants take the donor's in turn, within the
    /// user.
    Cue(&'static str),
}

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
    /// Adult Link's voice: his yells, grunts and breaths, by cue.
    #[serde(rename = "voice-adult")]
    VoiceAdult,
    /// Child Link's voice.
    #[serde(rename = "voice-child")]
    VoiceChild,
    /// The silences: never shuffled.
    Fixed,
}

impl Pool {
    /// Where a one-shot's length band ends, in seconds. A pool without
    /// edges has no bands: its tracks loop, or are all a moment long.
    fn edges(self) -> &'static [f32] {
        match self {
            Pool::Event => &[12.0, 25.0],
            Pool::Fanfare => &[5.0, 12.0],
            // Lessons and the short songs, the longer songs, the warp songs.
            Pool::Ocarina => &[7.0, 14.0],
            Pool::Area | Pool::Battle | Pool::VoiceAdult | Pool::VoiceChild | Pool::Fixed => &[],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pool::Area => "area",
            Pool::Battle => "battle",
            Pool::Event => "event",
            Pool::Fanfare => "fanfare",
            Pool::Ocarina => "ocarina",
            Pool::VoiceAdult => "voice-adult",
            Pool::VoiceChild => "voice-child",
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
    /// The graph's name in the music table, or its call's.
    pub graph: &'static str,
    pub via: Via,
    /// The one stream this track gives another: its only stream, or a
    /// sectioned track's whole-song stream. None: it receives only.
    pub song: Option<&'static str>,
    /// The song's length, or a songless track's longest stream's.
    pub secs: f32,
    /// Whether that stream loops (a songless track: any of its streams).
    pub looping: bool,
}

impl Track {
    /// Why it gives no song, in player words. None when it gives one.
    pub fn gives_none(&self) -> Option<&'static str> {
        (self.song.is_none())
            .then_some("Its music is in sections, so it plays others' songs but gives none")
    }

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
    if let Some((_, data)) = found.into_iter().find(|(p, _)| *p == path) {
        return Ok((name, data));
    }
    let romfs = crate::game::prepare(game, None, &crate::progress::Run::silent())?;
    for bank in banks(&romfs, game)? {
        if let Some(data) = ounce_formats::bars::bwav(&bank, name)? {
            return Ok((name, data.to_vec()));
        }
    }
    bail!("no stream {name}")
}

/// Link's banks from the dump at `game`, decompressed, in [`BANKS`] order.
pub fn banks(romfs: &Romfs, game: &Path) -> Result<Vec<Vec<u8>>> {
    let mut found = vec![];
    crate::game::each_where(
        game,
        |p| BANKS.contains(&p),
        |path, data| {
            found.push((path.clone(), romfs.dicts().decompress(&path, &data)?));
            Ok(())
        },
    )?;
    found.sort_by_key(|(path, _)| BANKS.iter().position(|b| b == path));
    ensure!(found.len() == BANKS.len(), "the dump lacks Link's banks");
    Ok(found.into_iter().map(|(_, bank)| bank).collect())
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

#[cfg(test)]
mod tests {
    use ounce_formats::{Byml, BymlDoc, xlink_calls};

    use super::*;
    use crate::ageless_items::SOUNDS;

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
        assert!(track("gerudo-fortress").unwrap().gives_none().is_some());
        assert!(track("drugstore").unwrap().gives_none().is_none());
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
        let (called, music): (Vec<_>, Vec<_>) = TRACKS.iter().partition(|t| t.via != Via::Table);
        let mut listed: Vec<_> = music.iter().map(|t| t.graph.to_owned()).collect();
        names.sort();
        listed.sort();
        assert_eq!(names, listed);
        let streams = crate::game::search(game, STREAMS).unwrap();
        let banks = banks(&romfs, game).unwrap();
        let header = |name: &str| {
            let path = format!("{STREAMS}{name}.bwav");
            let data = match streams.iter().find(|(p, _)| *p == path) {
                Some((_, data)) => data.as_slice(),
                None => (banks.iter())
                    .find_map(|b| ounce_formats::bars::bwav(b, name).unwrap())
                    .unwrap_or_else(|| panic!("no stream {name}")),
            };
            ounce_formats::bwav::header(data).unwrap()
        };
        let (cues, called): (Vec<_>, Vec<_>) = called
            .into_iter()
            .partition(|t| matches!(t.via, Via::Cue(_)));
        for t in cues {
            let Via::Cue(user) = t.via else {
                unreachable!()
            };
            let heard: Vec<_> = (write::variants(&slink, user, t.graph).unwrap().iter())
                .map(|(_, call)| {
                    let names = ounce_formats::xlink_asset_names(&slink, call.param).unwrap();
                    let (_, stream) = names.into_iter().find(|(bit, _)| *bit == 1).unwrap();
                    (stream.to_owned(), header(stream))
                })
                .collect();
            assert_eq!(heard[0].0, t.song.unwrap(), "{}", t.id);
            let secs = heard.iter().map(|(_, h)| h.seconds()).fold(0.0, f32::max);
            assert!(
                (secs - t.secs).abs() < 0.01 && secs < 6.0,
                "{}: {secs} s",
                t.id
            );
            assert!(
                heard.iter().all(|(_, h)| h.loop_start.is_none()),
                "{}",
                t.id
            );
        }
        for t in called {
            let Via::Call(user) = t.via else {
                unreachable!("{}", t.id)
            };
            let call = write::asset(&slink, user, t.graph).unwrap();
            // Field 1 of an asset's param record names the stream it plays.
            let names = ounce_formats::xlink_asset_names(&slink, call.param).unwrap();
            assert!(names.contains(&(1, t.song.unwrap())), "{}: {names:?}", t.id);
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
            let mut keys: Vec<String> = (write::nodes(&mut graph).unwrap().into_iter())
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
            let user = write::user(&graph).unwrap();
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
}
