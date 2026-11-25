//! The Audio pane's backend: the tracks, and each song's preview at
//! `zootr-audio://localhost/<track>.wav`.

use anyhow::{Context, ensure};
use serde::Serialize;
use specta::Type;
use zootr::audio::{Pool, TRACKS, Track};
use zootr::config::Config;

use crate::icon_dump;

/// Serves track `<id>.wav`: its song, decoded from the dump. Refused while
/// game assets are off, as the icons are.
pub const SCHEME: &str = "zootr-audio";

/// One track as the pane lists it.
#[derive(Serialize, Type)]
pub struct TrackView {
    id: String,
    label: String,
    pool: Pool,
    /// Its length band in its pool: 0 for a loop or a pool without bands.
    band: u32,
    /// The band in words: `<5s`, `5-12s`, `loop`, or `-` without bands.
    band_label: String,
    /// The song's length in milliseconds.
    ms: u32,
    looping: bool,
    /// Whether it gives a song another track can play.
    song: bool,
    /// Why it gives none, in player words: it only receives.
    gives_none: Option<String>,
}

impl From<&Track> for TrackView {
    fn from(t: &Track) -> Self {
        TrackView {
            id: t.id.to_owned(),
            label: t.label.to_owned(),
            pool: t.pool,
            band: t.band() as u32,
            band_label: t.band_label(),
            ms: (t.secs * 1000.0).round() as u32,
            looping: t.looping,
            song: t.song.is_some(),
            gives_none: t.gives_none().map(str::to_owned),
        }
    }
}

/// Every track a seed can change: the fixed pool is left out.
#[tauri::command]
#[specta::specta]
pub fn audio_catalog() -> Vec<TrackView> {
    (TRACKS.iter())
        .filter(|t| t.pool != Pool::Fixed)
        .map(TrackView::from)
        .collect()
}

/// The WAV for a request's path: `/<track id>.wav`.
pub fn serve(path: &str) -> anyhow::Result<Vec<u8>> {
    let id = path.trim_start_matches('/');
    let id = id.strip_suffix(".wav").context("a song is a .wav")?;
    // Checked before the config: a bad URL never reads it.
    let track = zootr::audio::track(id)?;
    ensure!(track.song.is_some(), "{id} has no song of its own");
    let game = icon_dump(&Config::load()?)?;
    Ok(std::fs::read(zootr::audio::song_wav(&game, id)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_leaves_out_the_fixed_pool() {
        let catalog = audio_catalog();
        assert!(catalog.iter().all(|t| t.pool != Pool::Fixed));
        let kokiri = catalog.iter().find(|t| t.id == "kokiri-forest").unwrap();
        assert!(kokiri.song && kokiri.looping && kokiri.band_label == "-");
    }

    #[test]
    fn a_bad_song_url_is_refused() {
        for path in ["/kokiri-forest", "/fake-track.wav", "/market.wav"] {
            assert!(serve(path).is_err(), "{path}");
        }
    }
}
