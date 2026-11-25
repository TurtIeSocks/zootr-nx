//! The Colors pane's backend: the palettes, their previews at
//! `zootr-colors://localhost/<id>.png`, drawing colors now, and painting
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
/// (0 when left out), `c` the first slot's color and an `s` for each
/// split slot (both hex, no `#`; vanilla without `c`), and `v=changes` to
/// grey what the palette leaves alone; or with `a` naming one of its
/// model's albedos in place of `t`. `<id>.model`, with `v` the view's
/// number, is that model's mesh ([`colors::model::mesh`]).
pub const SCHEME: &str = "zootr-colors";

/// One palette as the pane lists it.
#[derive(Serialize, Type)]
pub struct PaletteView {
    id: String,
    label: String,
    group: Group,
    /// Each slot's vanilla color, `#rrggbb`.
    vanilla: Vec<String>,
    /// Each slot's name, when it has more than one: what Split shows.
    names: Vec<String>,
    /// How many pictures it previews on: its textures, then a lit sphere
    /// for each material whose colors it writes.
    sheets: u32,
    /// The views its Model tab shows it on, by label: none without one.
    views: Vec<String>,
    /// Why this dump can't paint it, when it can't.
    problem: Option<String>,
}

/// The dump the previews read, and the textures decoded from it.
struct Dump {
    romfs: Romfs,
    previews: Previews,
    /// Its game folder, for the textures the cache doesn't hold.
    game: PathBuf,
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
                game: game.clone(),
            };
            *kept = Some((game, Arc::new(dump)));
        }
        Ok(kept.as_ref().expect("just opened").1.clone())
    }

    /// The body for a request's path and query, and its media type.
    pub fn serve(&self, path: &str, query: &str) -> anyhow::Result<(Vec<u8>, &'static str)> {
        let request = Request::parse(path, query)?;
        let dump = self.dump()?;
        let (romfs, picked) = (&dump.romfs, request.picked.as_ref());
        let png = |body| Ok((body, "image/png"));
        match request.what {
            What::Sheet(sheet) => {
                png(dump
                    .previews
                    .png(romfs, &request.id, sheet, picked, request.view)?)
            }
            What::Albedo(name) => png(dump.previews.albedo(romfs, &request.id, &name, picked)?),
            What::Model(view) => {
                let (mesh, albedos) = colors::model::mesh(romfs, &request.id, view)?;
                dump.previews.fetch(romfs, &dump.game, &albedos)?;
                Ok((mesh, "application/octet-stream"))
            }
        }
    }
}

/// What a preview URL asks for.
#[derive(Debug, PartialEq)]
struct Request {
    id: String,
    what: What,
    picked: Option<Picked>,
    view: View,
}

/// A sheet by number, a model's albedo by name, or a model's view by
/// number.
#[derive(Debug, PartialEq)]
enum What {
    Sheet(usize),
    Albedo(String),
    Model(usize),
}

impl Request {
    fn parse(path: &str, query: &str) -> anyhow::Result<Self> {
        let path = path.trim_start_matches('/');
        let (id, model) = match (path.strip_suffix(".png"), path.strip_suffix(".model")) {
            (Some(id), _) => (id, false),
            (_, Some(id)) => (id, true),
            _ => bail!("a preview is a .png or a .model"),
        };
        let mut request = Request {
            id: colors::palette(id)?.id.to_owned(),
            what: if model {
                What::Model(0)
            } else {
                What::Sheet(0)
            },
            picked: None,
            view: View::Paint,
        };
        let (mut first, mut split) = (None, Vec::new());
        let pairs = query.split('&').filter(|p| !p.is_empty());
        for (key, value) in pairs.map(|p| p.split_once('=').unwrap_or((p, ""))) {
            let hex = || colors::parse_hex(value).map(colors::hex);
            let number = || value.parse().with_context(|| format!("{key} is a number"));
            match (key, model) {
                ("t", false) => request.what = What::Sheet(number()?),
                ("a", false) => request.what = What::Albedo(value.to_owned()),
                ("v", true) => request.what = What::Model(number()?),
                ("c", false) => first = Some(hex()?),
                ("s", false) => split.push(hex()?),
                ("v", false) if value == "changes" => request.view = View::Changes,
                _ => bail!("a preview doesn't take {key}={value}"),
            }
        }
        request.picked = match first {
            Some(hex) => Some(Picked { hex, split }),
            None if split.is_empty() => None,
            None => bail!("split colors without a first one"),
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
            sheets: sheets(p).len() as u32,
            views: colors::model::labels(p.id)
                .into_iter()
                .map(Into::into)
                .collect(),
            problem: dump.as_ref().and_then(|d| {
                let checked = d.previews.check(&d.romfs, p);
                checked.err().map(|e| format!("{e:#}"))
            }),
        };
        // A palette that follows another (the brows) isn't picked on its own.
        let own = PALETTES.iter().filter(|p| colors::leader(p.id).is_none());
        Ok(own.map(view).collect())
    })
    .await
}

/// A Random color for each of `ids` now, so the player can see and tweak
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

/// Each slot's color, `#rrggbb`, when palette `id`'s first is `hex` and
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
    fn a_preview_url_reads_its_palette_texture_and_colors() {
        let r = Request::parse("/bow.png", "t=0&c=20b060&s=aabbcc&v=changes").unwrap();
        assert_eq!(r.id, "bow");
        assert_eq!(r.view, View::Changes);
        let picked = r.picked.unwrap();
        assert_eq!(
            (picked.hex.as_str(), picked.split),
            ("#20b060", vec!["#aabbcc".into()])
        );
        let vanilla = Request::parse("/kokiri-tunic.png", "t=3").unwrap();
        assert_eq!((vanilla.what, vanilla.picked), (What::Sheet(3), None));
    }

    #[test]
    fn a_model_url_reads_its_view_and_an_albedo_url_its_texture() {
        let model = Request::parse("/kokiri-tunic.model", "v=1").unwrap();
        assert_eq!(model.what, What::Model(1));
        let albedo = Request::parse("/bow.png", "a=Mt_FakeBow_Alb&c=20b060").unwrap();
        assert_eq!(albedo.what, What::Albedo("Mt_FakeBow_Alb".into()));
        assert!(Request::parse("/bow.model", "c=20b060").is_err());
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
