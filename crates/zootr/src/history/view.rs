//! What the app's seed list sees of the history: a [`Summary`] per seed,
//! read again only once the index changes.

use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::Serialize;

use super::{Entry, History, Label, store};
use crate::game::Release;
use crate::generate::Extras;
use crate::icons::icons;
use crate::settings::Settings;
use crate::share;
use crate::shuffle::Fixed;

/// An entry without its spots, for lists.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Summary {
    pub id: u32,
    pub(super) time: u32,
    pub(super) seed: String,
    /// Stand-ins for the seed, settings and extras together.
    pub(super) icons: Vec<String>,
    /// The seed, settings and extras as one line to share.
    pub(super) share: String,
    pub(super) settings: Settings,
    pub(super) label: Label,
    pub(super) chests: u32,
    pub(super) changed: u32,
    /// Built with a test setup: a spawn, items, flags or quest steps.
    pub(super) test: bool,
    /// Built with a plando: items pinned to checks.
    pub(super) plando: bool,
    /// The slot, for a seed opened from an Archipelago `.apznx`.
    pub(super) ap_slot: Option<String>,
    /// The room's server, when the `.apznx` came from its room page.
    pub(super) ap_server: Option<String>,
    /// The slot plays DeathLink.
    pub(super) death_link: bool,
    /// The build of the game the seed was made for.
    pub(super) release: Release,
    /// The colors as the Colors pane asked for them ([`Extras::asked`]).
    pub(super) colors: crate::colors::Draft,
    /// The music as the Audio pane asked for it.
    pub(super) audio: crate::audio::Draft,
    /// The plando: chest hash to the item it holds.
    #[cfg_attr(feature = "specta", specta(type = std::collections::BTreeMap<String, String>))]
    pub(super) place: Fixed,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct View {
    pub(super) installed: Option<Summary>,
    /// Newest first.
    pub(super) entries: Vec<Summary>,
}

impl From<&Entry> for Summary {
    fn from(e: &Entry) -> Self {
        let asked = e.extras.asked();
        Self {
            id: e.id,
            time: e.time,
            seed: e.brief.seed.to_string(),
            icons: icons(e.brief.seed, &e.settings, &e.extras),
            share: share::encode(e.brief.seed, &e.settings, &e.extras),
            settings: e.settings.clone(),
            label: e.label.clone(),
            chests: e.brief.chests,
            changed: e.brief.changed,
            test: !Extras {
                place: Fixed::new(),
                ap: None,
                release: None,
                ..e.extras.cosmetic_free()
            }
            .is_empty(),
            plando: !e.extras.place.is_empty(),
            ap_slot: e.extras.ap.as_ref().map(|a| a.slot.clone()),
            ap_server: (e.extras.ap.as_ref())
                .map(|a| a.server.clone())
                .filter(|s| !s.is_empty()),
            death_link: e.extras.ap.as_ref().is_some_and(|a| a.death_link),
            release: e.brief.release,
            colors: asked.colors,
            audio: asked.audio,
            place: e.extras.place.clone(),
        }
    }
}

/// A file's modified time and length, or `None` when it's missing: what
/// changes when it's written.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Result<Stamp> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(Some((m.modified()?, m.len()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// The saved history's [`View`], parsed again only once the index
/// changes: the app's seed list polls this every few seconds.
pub fn view() -> Result<View> {
    static KEPT: Mutex<Option<(Stamp, View)>> = Mutex::new(None);
    kept(&KEPT, &store::index(&store::dir()?), || {
        Ok(History::load()?.view())
    })
}

/// What `load` gives for `path`, kept in `slot` until `path` changes.
fn kept<V: Clone>(
    slot: &Mutex<Option<(Stamp, V)>>,
    path: &Path,
    load: impl FnOnce() -> Result<V>,
) -> Result<V> {
    let now = stamp(path)?;
    let mut slot = slot.lock().unwrap();
    if let Some((was, v)) = slot.as_ref()
        && *was == now
    {
        return Ok(v.clone());
    }
    let v = load()?;
    *slot = Some((now, v.clone()));
    Ok(v)
}

impl History {
    pub fn view(&self) -> View {
        let installed = self.installed().id.and_then(|id| self.get(id).ok());
        View {
            installed: installed.map(Summary::from),
            entries: self.entries.iter().rev().map(Summary::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::BTreeMap;

    use super::*;
    use crate::history::tests::report;
    use crate::known::item;

    #[test]
    fn a_kept_view_is_read_again_only_once_the_file_changes() {
        let path = std::env::temp_dir().join(format!("zootr-kept-{}", std::process::id()));
        std::fs::write(&path, "a").unwrap();
        let (slot, loads) = (Mutex::new(None), Cell::new(0));
        let load = || {
            loads.set(loads.get() + 1);
            Ok(loads.get())
        };
        assert_eq!(kept(&slot, &path, load).unwrap(), 1);
        assert_eq!(kept(&slot, &path, load).unwrap(), 1);
        std::fs::write(&path, "ab").unwrap();
        assert_eq!(kept(&slot, &path, load).unwrap(), 2);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(kept(&slot, &path, load).unwrap(), 3);
        assert_eq!(kept(&slot, &path, load).unwrap(), 3);
    }

    #[test]
    fn a_plando_badges_apart_from_a_test_setup() {
        let flags = |extras: Extras| {
            let mut history = History::default();
            history.add(Settings::new(), extras, Label::default(), report(1));
            let s = Summary::from(&history.entries[0]);
            (s.test, s.plando)
        };
        let place = Fixed::from([(1, item::HOOKSHOT.to_string())]);
        let give = vec![item::BOMB.to_string()];
        assert_eq!(flags(Extras::default()), (false, false));
        let painted = Extras::default().styled(
            crate::generate::Drafts {
                colors: [(
                    "hair".into(),
                    crate::colors::Pick::Mode(crate::colors::Mode::Random),
                )]
                .into(),
                ..Default::default()
            },
            1,
        );
        assert_eq!(
            flags(painted.unwrap()),
            (false, false),
            "colors aren't a test"
        );
        let plando = Extras {
            place: place.clone(),
            ..Default::default()
        };
        assert_eq!(flags(plando), (false, true));
        let both = Extras {
            place,
            give: give.clone(),
            ..Default::default()
        };
        assert_eq!(flags(both), (true, true));
        assert_eq!(
            flags(Extras {
                give,
                ..Default::default()
            }),
            (true, false)
        );
    }

    #[test]
    fn an_ap_seed_carries_its_slot_and_death_link_and_isnt_a_test() {
        let mut history = History::default();
        let ap = crate::ap::ApSeed {
            world_version: String::new(),
            settings: Settings::new(),
            seed: 1,
            slot: "FakeSlot".into(),
            player: 1,
            server: "FakeRoom:38281".into(),
            start: vec![],
            death_link: true,
            players: BTreeMap::new(),
            placements: BTreeMap::new(),
            hints: vec![],
        };
        history.add(Settings::new(), Extras::ap(ap), Label::default(), report(1));
        let s = Summary::from(&history.entries[0]);
        assert_eq!((s.test, s.ap_slot.as_deref()), (false, Some("FakeSlot")));
        assert_eq!(s.ap_server.as_deref(), Some("FakeRoom:38281"));
        assert!(s.death_link, "the slot's DeathLink");
    }
}
