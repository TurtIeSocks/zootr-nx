//! What the music is set to: each pool Vanilla, Shuffle or Random, each
//! track following its pool, kept vanilla, or given a picked song. A draft
//! keeps the modes; a seed keeps each slot's song, so it reinstalls the
//! same.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{Pool, TRACKS, Track, track};
use crate::rng::Rng;

/// A pool's mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum PoolMode {
    /// Every track plays its own.
    Vanilla,
    /// A permutation within each band: every song plays somewhere.
    Shuffle,
    /// Each slot draws any song in its band, repeats allowed.
    Random,
}

/// A track override's value for "keep this one vanilla".
pub const VANILLA: &str = "vanilla";

/// The pane's draft: each pool's mode (Vanilla when not named), and each
/// track overridden, by id, to [`VANILLA`] or the id of the track whose
/// song it plays.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default, rename = "AudioDraft")]
pub struct Draft {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub pools: BTreeMap<Pool, PoolMode>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tracks: BTreeMap<String, String>,
}

impl Draft {
    pub fn is_empty(&self) -> bool {
        self.pools.is_empty() && self.tracks.is_empty()
    }
}

/// What a seed was built with: slot id to the id of the track whose song
/// it plays. A track not named plays its own.
pub type Audio = BTreeMap<String, String>;

/// `draft` as a seed keeps it: every pool dealt for `seed`, overrides
/// first, each slot playing its own song left out.
pub fn resolve(draft: &Draft, seed: u64) -> Result<Audio> {
    let mut out = Audio::new();
    let mut kept = HashSet::new();
    for (id, value) in &draft.tracks {
        let slot = track(id)?;
        kept.insert(slot.id);
        if value != VANILLA {
            let donor = donor_for(slot, value)?;
            put(&mut out, slot, donor);
        }
    }
    for (&pool, &mode) in &draft.pools {
        ensure!(
            pool != Pool::Fixed || mode == PoolMode::Vanilla,
            "the fixed pool is never shuffled"
        );
        if mode == PoolMode::Vanilla {
            continue;
        }
        let id = crate::build::fnv1a(pool.name().as_bytes()) as u64;
        let mut rng = Rng::new(seed ^ id.rotate_left(32));
        let tracks: Vec<&Track> = TRACKS.iter().filter(|t| t.pool == pool).collect();
        let bands: BTreeSet<usize> = tracks.iter().map(|t| t.band()).collect();
        for band in bands {
            let here: Vec<&Track> = tracks
                .iter()
                .copied()
                .filter(|t| t.band() == band)
                .collect();
            let songs = songs(&here);
            let mut deck = Vec::new();
            for slot in here.iter().filter(|t| !kept.contains(t.id)) {
                let donor = match mode {
                    PoolMode::Shuffle => {
                        if deck.is_empty() {
                            deck = songs.clone();
                            rng.shuffle(&mut deck);
                        }
                        deck.pop()
                    }
                    PoolMode::Random if songs.is_empty() => None,
                    PoolMode::Random => Some(songs[rng.below(songs.len() as u64) as usize]),
                    PoolMode::Vanilla => unreachable!("vanilla pools are skipped"),
                };
                if let Some(donor) = donor {
                    put(&mut out, slot, donor);
                }
            }
        }
    }
    Ok(out)
}

/// The track whose song `slot` may play by `id`: one of its pool, with a
/// song. Any band: a pick may cross them.
fn donor_for(slot: &Track, id: &str) -> Result<&'static Track> {
    let donor = track(id)?;
    ensure!(
        donor.pool == slot.pool && slot.pool != Pool::Fixed,
        "{} ({}) can't play {id} ({})",
        slot.id,
        slot.pool.name(),
        donor.pool.name()
    );
    donor
        .song
        .with_context(|| format!("{id} gives no song: its music is in sections"))?;
    Ok(donor)
}

/// The songs among `tracks`, each once: tracks sharing a stream give it as
/// the first of them.
fn songs(tracks: &[&'static Track]) -> Vec<&'static Track> {
    let mut seen = HashSet::new();
    (tracks.iter().copied())
        .filter(|t| t.song.is_some_and(|s| seen.insert(s)))
        .collect()
}

/// `slot` plays `donor`'s song, unless that's its own.
fn put(out: &mut Audio, slot: &Track, donor: &Track) {
    match slot.song.is_some() && slot.song == donor.song {
        true => out.remove(slot.id),
        false => out.insert(slot.id.to_owned(), donor.id.to_owned()),
    };
}

/// `draft` as CLI flags, each `(flag, value)`: `--audio POOL=MODE`, then
/// `--track SLOT=SONG`.
pub fn args(draft: &Draft) -> Vec<(&'static str, String)> {
    let pools = (draft.pools.iter()).map(|(p, m)| ("audio", format!("{}={}", p.name(), mode(*m))));
    let tracks = (draft.tracks.iter()).map(|(s, v)| ("track", format!("{s}={v}")));
    pools.chain(tracks).collect()
}

fn mode(mode: PoolMode) -> &'static str {
    match mode {
        PoolMode::Vanilla => "vanilla",
        PoolMode::Shuffle => "shuffle",
        PoolMode::Random => "random",
    }
}

/// The draft that builds `audio` again.
pub fn draft(audio: &Audio) -> Draft {
    Draft {
        pools: BTreeMap::new(),
        tracks: audio.clone(),
    }
}

/// One `--audio POOL=vanilla|shuffle|random`.
pub fn parse_pool(arg: &str) -> Result<(Pool, PoolMode)> {
    let (pool, mode) = arg
        .split_once('=')
        .context("--audio takes POOL=vanilla, shuffle or random")?;
    Ok((value("pool", pool)?, value("mode", mode)?))
}

fn value<T: clap::ValueEnum>(what: &str, s: &str) -> Result<T> {
    T::from_str(s, true).map_err(|_| anyhow::anyhow!("no {what} {s}"))
}

/// One `--track SLOT=SONG|vanilla`, SONG the id of the track whose song
/// the slot plays.
pub fn parse_track(arg: &str) -> Result<(String, String)> {
    let (slot, value) = arg
        .split_once('=')
        .context("--track takes SLOT=SONG or SLOT=vanilla")?;
    let slot = track(slot)?;
    if value != VANILLA {
        donor_for(slot, value)?;
    }
    Ok((slot.id.to_owned(), value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(mode: PoolMode) -> Draft {
        let pools = [
            Pool::Area,
            Pool::Battle,
            Pool::Event,
            Pool::Fanfare,
            Pool::Ocarina,
        ];
        Draft {
            pools: pools.into_iter().map(|p| (p, mode)).collect(),
            tracks: BTreeMap::new(),
        }
    }

    #[test]
    fn a_draft_reads_as_the_pane_writes_it() {
        let json =
            r#"{"pools":{"area":"shuffle","fanfare":"random"},"tracks":{"goron-city":"vanilla"}}"#;
        let draft: Draft = serde_json::from_str(json).unwrap();
        assert_eq!(draft.pools[&Pool::Area], PoolMode::Shuffle);
        assert_eq!(draft.tracks["goron-city"], VANILLA);
        let back: Draft = serde_json::from_str(&serde_json::to_string(&draft).unwrap()).unwrap();
        assert_eq!(back, draft);
        assert!(serde_json::from_str::<Draft>("{}").unwrap().is_empty());
    }

    #[test]
    fn every_slot_plays_a_song_of_its_own_pool_and_band() {
        for mode in [PoolMode::Shuffle, PoolMode::Random] {
            let audio = resolve(&all(mode), 11).unwrap();
            assert!(!audio.is_empty());
            for (slot, donor) in &audio {
                let (slot, donor) = (track(slot).unwrap(), track(donor).unwrap());
                assert_eq!((slot.pool, slot.band()), (donor.pool, donor.band()));
                assert!(donor.song.is_some() && donor.song != slot.song);
            }
        }
    }

    #[test]
    fn shuffle_plays_every_song_of_a_band() {
        let audio = resolve(&all(PoolMode::Shuffle), 5).unwrap();
        let mut heard: HashSet<&str> = audio
            .values()
            .map(|d| track(d).unwrap().song.unwrap())
            .collect();
        // A slot left playing its own song is left out of `audio`.
        let own = TRACKS
            .iter()
            .filter(|t| t.pool != Pool::Fixed && !audio.contains_key(t.id));
        heard.extend(own.filter_map(|t| t.song));
        let all = TRACKS
            .iter()
            .filter(|t| t.pool != Pool::Fixed)
            .filter_map(|t| t.song);
        assert!(all.into_iter().all(|s| heard.contains(s)));
    }

    #[test]
    fn the_same_seed_deals_the_same_and_vanilla_deals_nothing() {
        let draft = all(PoolMode::Shuffle);
        assert_eq!(resolve(&draft, 3).unwrap(), resolve(&draft, 3).unwrap());
        assert_ne!(resolve(&draft, 3).unwrap(), resolve(&draft, 4).unwrap());
        assert!(resolve(&all(PoolMode::Vanilla), 3).unwrap().is_empty());
    }

    #[test]
    fn overrides_win_over_their_pool() {
        let mut draft = all(PoolMode::Random);
        draft.tracks.insert("goron-city".into(), VANILLA.into());
        draft.tracks.insert("market".into(), "zora-domain".into());
        for seed in 0..20 {
            let audio = resolve(&draft, seed).unwrap();
            assert!(!audio.contains_key("goron-city"));
            assert_eq!(audio["market"], "zora-domain");
        }
    }

    #[test]
    fn a_pick_must_be_a_song_of_the_same_pool() {
        assert!(parse_track("market=zora-domain").is_ok());
        assert!(parse_track("market=vanilla").is_ok());
        assert!(parse_track("market=medal-get").is_err());
        assert!(
            parse_track("goron-city=market").is_err(),
            "sections give no song"
        );
        assert!(parse_track("fake-track=market").is_err());
        assert_eq!(
            parse_pool("area=shuffle").unwrap(),
            (Pool::Area, PoolMode::Shuffle)
        );
        assert!(parse_pool("fixed=bogus").is_err());
    }

    #[test]
    fn a_draft_comes_back_from_its_args() {
        let mut draft = all(PoolMode::Shuffle);
        draft.tracks.insert("market".into(), "zora-domain".into());
        let mut back = Draft::default();
        for (flag, value) in args(&draft) {
            match flag {
                "audio" => drop(
                    back.pools
                        .insert(parse_pool(&value).unwrap().0, parse_pool(&value).unwrap().1),
                ),
                _ => drop(back.tracks.insert(
                    parse_track(&value).unwrap().0,
                    parse_track(&value).unwrap().1,
                )),
            }
        }
        assert_eq!(back, draft);
    }
}
