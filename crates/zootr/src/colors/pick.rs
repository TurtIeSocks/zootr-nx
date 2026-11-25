//! What a palette is set to: Vanilla, Random, or a colour. A draft keeps
//! Random as Random; a seed keeps the colour it drew, so it reinstalls the
//! same.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::oklch::{self, Lch};
use super::paint::Colour;
use super::{Palette, palette};
use crate::rng::Rng;

/// A palette left alone or drawn when the seed is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Vanilla,
    Random,
}

/// A colour picked for a palette's first slot, as `#rrggbb`, and with
/// Split on, one for each slot after it. Without, the other slots follow
/// the first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Picked {
    pub hex: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub split: Vec<String>,
}

/// One palette in a draft.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(untagged)]
pub enum Pick {
    Mode(Mode),
    Colour(Picked),
}

/// The pane's draft: palette id to its pick. A palette not named is
/// Vanilla.
pub type Draft = BTreeMap<String, Pick>;

/// What a seed was built with: palette id to its colour, Random already
/// drawn. A palette not named is Vanilla.
pub type Colors = BTreeMap<String, Picked>;

pub fn parse_hex(hex: &str) -> Result<Colour> {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    ensure!(
        digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()),
        "{hex} isn't a colour like #2f8f3a"
    );
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).expect("hex digits");
    Ok([byte(0), byte(2), byte(4)])
}

pub fn hex([r, g, b]: Colour) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

impl Picked {
    pub fn of(colour: Colour) -> Self {
        Self {
            hex: hex(colour),
            split: Vec::new(),
        }
    }

    /// The colour of each of `palette`'s slots: the first picked, the rest
    /// split or following it.
    pub fn slots(&self, palette: &Palette) -> Result<Vec<Colour>> {
        let first = parse_hex(&self.hex)?;
        let rest = palette.vanilla.len() - 1;
        if self.split.is_empty() {
            let from = palette.vanilla[0];
            let follow = palette.vanilla[1..]
                .iter()
                .map(|&v| oklch::follow(first, from, v));
            return Ok([first].into_iter().chain(follow).collect());
        }
        ensure!(
            self.split.len() == rest,
            "{}: {} split colours for its {rest} other slots",
            palette.id,
            self.split.len()
        );
        let split = self.split.iter().map(|h| parse_hex(h));
        [Ok(first)].into_iter().chain(split).collect()
    }
}

/// Random's range: any hue, chroma enough to read as a colour, lightness
/// near the vanilla colour's so a part stays readable.
const CHROMA: (f32, f32) = (0.08, 0.22);
const LIGHTNESS: f32 = 0.15;

/// A colour for `palette`, drawn from its own stream: the seed and the
/// palette's id, so it never moves the shuffle, and the same seed draws
/// the same.
fn draw(palette: &Palette, seed: u64) -> Colour {
    let id = crate::build::fnv1a(palette.id.as_bytes()) as u64;
    let mut rng = Rng::new(seed ^ id.rotate_left(32));
    let mut unit = || rng.below(1 << 24) as f32 / (1 << 24) as f32;
    let vanilla = oklch::lch(palette.vanilla[0]);
    let (h, c, l) = (unit(), unit(), unit());
    oklch::colour(Lch {
        l: (vanilla.l + (2.0 * l - 1.0) * LIGHTNESS).clamp(0.1, 0.95),
        c: CHROMA.0 + c * (CHROMA.1 - CHROMA.0),
        h,
    })
}

/// `draft` as a seed keeps it: Vanilla left out, Random drawn for `seed`,
/// every colour and palette checked.
pub fn resolve(draft: &Draft, seed: u64) -> Result<Colors> {
    let mut out = Colors::new();
    for (id, pick) in draft {
        let palette = palette(id)?;
        let picked = match pick {
            Pick::Mode(Mode::Vanilla) => continue,
            Pick::Mode(Mode::Random) => Picked::of(draw(palette, seed)),
            Pick::Colour(picked) => picked.clone(),
        };
        picked
            .slots(palette)
            .with_context(|| format!("palette {id}"))?;
        out.insert(id.clone(), picked);
    }
    Ok(out)
}

/// `draft` as `--color` values, each `ID=VALUE`, Vanilla left out.
pub fn args(draft: &Draft) -> Vec<String> {
    let value = |pick: &Pick| match pick {
        Pick::Mode(Mode::Vanilla) => None,
        Pick::Mode(Mode::Random) => Some("random".to_owned()),
        Pick::Colour(p) => Some(
            [p.hex.as_str()]
                .into_iter()
                .chain(p.split.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(","),
        ),
    };
    draft
        .iter()
        .filter_map(|(id, pick)| Some(format!("{id}={}", value(pick)?)))
        .collect()
}

/// The draft that builds `colors` again.
pub fn draft(colors: &Colors) -> Draft {
    colors
        .iter()
        .map(|(id, p)| (id.clone(), Pick::Colour(p.clone())))
        .collect()
}

/// One `--color ID=#rrggbb|random|vanilla`, or `ID=#rrggbb,#rrggbb` for a
/// colour per slot.
pub fn parse_arg(arg: &str) -> Result<(String, Pick)> {
    let (id, value) = arg
        .split_once('=')
        .context("--color takes ID=#rrggbb or ID=random")?;
    let pick = match value {
        "random" => Pick::Mode(Mode::Random),
        "vanilla" => Pick::Mode(Mode::Vanilla),
        hexes => {
            let mut all = hexes.split(',').map(|h| parse_hex(h).map(hex));
            let first = all.next().context("no colour")??;
            let split = all.collect::<Result<_>>()?;
            let picked = Picked { hex: first, split };
            picked.slots(palette(id)?)?;
            Pick::Colour(picked)
        }
    };
    palette(id)?;
    Ok((id.to_owned(), pick))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_reads_as_the_pane_writes_it() {
        let json =
            r##"{"kokiri-tunic":"random","zora-tunic":"vanilla","goron-tunic":{"hex":"#3050c8"}}"##;
        let draft: Draft = serde_json::from_str(json).unwrap();
        assert_eq!(draft["kokiri-tunic"], Pick::Mode(Mode::Random));
        assert_eq!(
            draft["goron-tunic"],
            Pick::Colour(Picked::of([0x30, 0x50, 0xc8]))
        );
        let back: Draft = serde_json::from_str(&serde_json::to_string(&draft).unwrap()).unwrap();
        assert_eq!(back, draft);
    }

    #[test]
    fn random_draws_the_same_for_a_seed_and_vanilla_drops_out() {
        let draft = Draft::from([
            ("kokiri-tunic".into(), Pick::Mode(Mode::Random)),
            ("zora-tunic".into(), Pick::Mode(Mode::Vanilla)),
        ]);
        let a = resolve(&draft, 7).unwrap();
        assert_eq!(a, resolve(&draft, 7).unwrap());
        assert_ne!(a, resolve(&draft, 8).unwrap());
        assert_eq!(a.keys().collect::<Vec<_>>(), ["kokiri-tunic"]);
    }

    #[test]
    fn random_stays_near_the_vanillas_lightness() {
        let tunic = palette("kokiri-tunic").unwrap();
        let vanilla = oklch::lch(tunic.vanilla[0]).l;
        for seed in 0..200 {
            let l = oklch::lch(draw(tunic, seed)).l;
            assert!(
                (l - vanilla).abs() <= LIGHTNESS + 0.02,
                "seed {seed}: {l} by {vanilla}"
            );
        }
    }

    #[test]
    fn a_draft_comes_back_from_its_args() {
        let draft = Draft::from([
            ("kokiri-tunic".into(), Pick::Mode(Mode::Random)),
            ("zora-tunic".into(), Pick::Mode(Mode::Vanilla)),
            ("bow".into(), Pick::Colour(Picked::of([1, 2, 3]))),
        ]);
        let back: Draft = args(&draft).iter().map(|a| parse_arg(a).unwrap()).collect();
        let mut want = draft.clone();
        want.remove("zora-tunic");
        assert_eq!(back, want);
    }

    #[test]
    fn a_bad_colour_or_palette_is_refused() {
        assert!(parse_arg("kokiri-tunic=#12345").is_err());
        assert!(parse_arg("fake-palette=#123456").is_err());
        assert!(parse_arg("kokiri-tunic").is_err());
        assert_eq!(parse_hex("#2F8f3a").unwrap(), [0x2f, 0x8f, 0x3a]);
        assert_eq!(hex([0x2f, 0x8f, 0x3a]), "#2f8f3a");
    }
}
