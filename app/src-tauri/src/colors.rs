//! The Colors pane's backend: the palettes, their previews at
//! `zootr-colors://localhost/<id>.png`, drawing colours now, and painting
//! the installed seed again.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, bail};
use ounce_formats::Romfs;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use zootr::colors::preview::{Previews, View, sheets};
use zootr::colors::{self, Group, PALETTES, Picked};
use zootr::config::Config;
use zootr::history::History;

use crate::{Res, blocking, icon_dump, romfs};

/// Serves a palette's preview: `<id>.png`, with `t` the sheet's number
/// (0 when left out), `c` the first slot's colour and an `s` for each
/// split slot (both hex, no `#`; vanilla without `c`), and `v=changes` to
/// grey what the palette leaves alone.
pub const SCHEME: &str = "zootr-colors";

/// One palette as the pane lists it.
#[derive(Serialize, Type)]
pub struct PaletteView {
    id: String,
    label: String,
    group: Group,
    /// Each slot's vanilla colour, `#rrggbb`.
    vanilla: Vec<String>,
    /// Each slot's name, when it has more than one: what Split shows.
    names: Vec<String>,
    /// The palette it moves with while left vanilla, by label.
    follows: Option<String>,
    /// How many pictures it previews on: its textures, then a lit sphere
    /// for each material whose colours it writes.
    sheets: u32,
    /// Why this dump can't paint it, when it can't.
    problem: Option<String>,
}

/// The dump the previews read, and the textures decoded from it.
struct Dump {
    romfs: Romfs,
    previews: Previews,
}

/// The previews' dump, opened on first use and again when the config
/// names another.
#[derive(Default)]
pub struct ColorPreviews(Mutex<Option<(PathBuf, Arc<Dump>)>>);

impl ColorPreviews {
    /// Refused while game assets are off, as the icons are.
    fn dump(&self) -> anyhow::Result<Arc<Dump>> {
        let config = Config::load()?;
        let game = icon_dump(&config)?;
        let mut kept = self.0.lock().unwrap();
        if kept.as_ref().is_none_or(|(had, _)| *had != game) {
            let dump = Dump {
                romfs: romfs(&config)?,
                previews: Previews::default(),
            };
            *kept = Some((game, Arc::new(dump)));
        }
        Ok(kept.as_ref().expect("just opened").1.clone())
    }

    /// The PNG for a request's path and query.
    pub fn serve(&self, path: &str, query: &str) -> anyhow::Result<Vec<u8>> {
        let request = Request::parse(path, query)?;
        let dump = self.dump()?;
        dump.previews.png(
            &dump.romfs,
            &request.id,
            request.texture,
            request.picked.as_ref(),
            request.view,
        )
    }
}

/// What a preview URL asks for.
#[derive(Debug, PartialEq)]
struct Request {
    id: String,
    texture: usize,
    picked: Option<Picked>,
    view: View,
}

impl Request {
    fn parse(path: &str, query: &str) -> anyhow::Result<Self> {
        let id = path.trim_start_matches('/');
        let id = id.strip_suffix(".png").context("a preview is a .png")?;
        let mut request = Request {
            id: colors::palette(id)?.id.to_owned(),
            texture: 0,
            picked: None,
            view: View::Paint,
        };
        let (mut first, mut split) = (None, Vec::new());
        let pairs = query.split('&').filter(|p| !p.is_empty());
        for (key, value) in pairs.map(|p| p.split_once('=').unwrap_or((p, ""))) {
            let hex = || colors::parse_hex(value).map(colors::hex);
            match key {
                "t" => request.texture = value.parse().context("t is a number")?,
                "c" => first = Some(hex()?),
                "s" => split.push(hex()?),
                "v" if value == "changes" => request.view = View::Changes,
                _ => bail!("a preview doesn't take {key}={value}"),
            }
        }
        request.picked = match first {
            Some(hex) => Some(Picked { hex, split }),
            None if split.is_empty() => None,
            None => bail!("split colours without a first one"),
        };
        Ok(request)
    }
}

/// Every palette, and why this dump can't paint one, read with game
/// assets on (without them, none is checked).
#[tauri::command]
#[specta::specta]
pub async fn colors_catalog(app: AppHandle) -> Res<Vec<PaletteView>> {
    blocking(move || {
        let dump = app.state::<ColorPreviews>().dump().ok();
        let view = |p: &colors::Palette| PaletteView {
            id: p.id.to_owned(),
            label: p.label.to_owned(),
            group: p.group,
            vanilla: p.vanilla.iter().map(|&c| colors::hex(c)).collect(),
            names: p.names.iter().map(|&n| n.to_owned()).collect(),
            follows: colors::leader(p.id).map(|l| l.label.to_owned()),
            sheets: sheets(p).len() as u32,
            problem: dump.as_ref().and_then(|d| {
                let checked = d.previews.check(&d.romfs, p);
                checked.err().map(|e| format!("{e:#}"))
            }),
        };
        Ok(PALETTES.iter().map(view).collect())
    })
    .await
}

/// A Random colour for each of `ids` now, so the player can see and tweak
/// it before generating.
#[tauri::command]
#[specta::specta]
pub fn colors_draw(ids: Vec<String>) -> Res<BTreeMap<String, String>> {
    let seed = zootr::generate::time_seed();
    ids.into_iter()
        .map(|id| colors::drawn(&id, seed).map(|hex| (id, hex)))
        .collect::<anyhow::Result<_>>()
        .map_err(crate::text)
}

/// Each slot's colour, `#rrggbb`, when palette `id`'s first is `hex` and
/// the rest follow it: where Split starts.
#[tauri::command]
#[specta::specta]
pub fn colors_follow(id: String, hex: String) -> Res<Vec<String>> {
    let first = colors::parse_hex(&hex).map_err(crate::text)?;
    let palette = colors::palette(&id).map_err(crate::text)?;
    let slots = Picked::of(first).slots(palette).map_err(crate::text)?;
    Ok(slots.into_iter().map(colors::hex).collect())
}

/// The installed seed's history number, refused when none is.
pub fn installed() -> anyhow::Result<u32> {
    History::load()?
        .installed()
        .id
        .context("No seed is installed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_url_reads_its_palette_texture_and_colours() {
        let r = Request::parse("/bow.png", "t=0&c=20b060&s=aabbcc&v=changes").unwrap();
        assert_eq!(r.id, "bow");
        assert_eq!(r.view, View::Changes);
        let picked = r.picked.unwrap();
        assert_eq!(
            (picked.hex.as_str(), picked.split),
            ("#20b060", vec!["#aabbcc".into()])
        );
        let vanilla = Request::parse("/kokiri-tunic.png", "t=3").unwrap();
        assert_eq!((vanilla.texture, vanilla.picked), (3, None));
    }

    #[test]
    fn a_bad_preview_url_is_refused() {
        for (path, query) in [
            ("/bow", ""),
            ("/fake-palette.png", ""),
            ("/bow.png", "c=nothex"),
            ("/bow.png", "s=aabbcc"),
            ("/bow.png", "x=1"),
        ] {
            assert!(Request::parse(path, query).is_err(), "{path}?{query}");
        }
    }
}
