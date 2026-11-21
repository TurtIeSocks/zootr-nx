//! An animation archive's skeletal animations (FSKA, bfres version 10)
//! resized to another Link's skeleton, in place, so the archive keeps its
//! size. Every clip played as it ships on the other Link stretches his
//! body to the first Link's bone lengths. Resized, it plays at his own
//! (research note, "Animations: the adult archive at child size").
//!
//! Each bone the target has, but for the weapon bones, takes the target's
//! constant translation. A translation curve scales by the bone's length
//! ratio, through its scale and offset. The root bones scale by the leg
//! ratio, since their translation is where the body stands.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, ensure};

use crate::known::field;

const MAGIC: &[u8; 4] = b"FSKA";
/// An animation header's fields: its name, its bone tracks and how many.
const NAME: usize = 0x08;
const TRACKS: usize = 0x28;
const TRACK_COUNT: usize = 0x4c;
/// A bone track's size and fields: its name, curves, base values, flags
/// and curve count.
const TRACK: usize = 0x38;
const CURVES: usize = 0x08;
const BASE: usize = 0x10;
const FLAGS: usize = 0x28;
const CURVE_COUNT: usize = 0x2e;
/// The base values a track's flags carry, in order: scale (3 floats),
/// rotation (4), translation (3).
const HAS_SCALE: u32 = 1 << 3;
const HAS_ROTATION: u32 = 1 << 4;
const HAS_TRANSLATION: u32 = 1 << 5;
/// The flag bits saying which translation axes have curves, X first.
const TRANSLATION_CURVES: u32 = 13;
/// A curve's size and fields: what it animates, its scale and offset.
const CURVE: usize = 0x30;
const TARGET: usize = 0x14;
const SCALE: usize = 0x24;
/// The curve targets of the translation's X, Y and Z.
const TRANSLATIONS: [u32; 3] = [0x10, 0x14, 0x18];
/// The bones whose names start so hold weapons, not the body.
const WEAPON: &str = "Weapon";
/// Shorter than this, a length is no length.
const TINY: f64 = 1e-4;

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn offset_at(d: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?)).ok()
}

fn floats_at<const N: usize>(d: &[u8], at: usize) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    for (i, f) in out.iter_mut().enumerate() {
        *f = f32::from_bits(u32_at(d, at + 4 * i)?);
    }
    Some(out)
}

fn put_floats(d: &mut [u8], at: usize, values: &[f32]) {
    for (i, f) in values.iter().enumerate() {
        d[at + 4 * i..at + 4 * i + 4].copy_from_slice(&f.to_le_bytes());
    }
}

/// The string a bfres offset points at: a u16 length, then its bytes.
fn string_at(d: &[u8], at: usize) -> Option<&str> {
    let len = usize::from(u16_at(d, at)?);
    std::str::from_utf8(d.get(at + 2..at + 2 + len)?).ok()
}

/// In f64, as each value is worked out before it's stored back as f32.
fn length(v: [f32; 3]) -> f64 {
    v.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>().sqrt()
}

/// One bone's track in one animation.
struct Track<'a> {
    name: &'a str,
    /// Where its constant translation sits, and its value, if it has one.
    translation: Option<(usize, [f32; 3])>,
    /// Which translation axes are curves, X in bit 0.
    curved: u32,
    curves: usize,
    curve_count: usize,
}

fn track(d: &[u8], at: usize) -> Option<Track<'_>> {
    let flags = u32_at(d, at + FLAGS)?;
    let base = offset_at(d, at + BASE)?;
    let translation = match flags & HAS_TRANSLATION != 0 && base != 0 {
        true => {
            let scale = if flags & HAS_SCALE != 0 { 12 } else { 0 };
            let rotation = if flags & HAS_ROTATION != 0 { 16 } else { 0 };
            let at = base + scale + rotation;
            Some((at, floats_at::<3>(d, at)?))
        }
        false => None,
    };
    Some(Track {
        name: string_at(d, offset_at(d, at)?)?,
        translation,
        curved: (flags >> TRANSLATION_CURVES) & 0b111,
        curves: offset_at(d, at + CURVES)?,
        curve_count: usize::from(*d.get(at + CURVE_COUNT)?),
    })
}

/// Each animation in the archive: where its header is, and where its bone
/// tracks are and how many. Found by magic, 8-aligned, as the archive's
/// own tables would give them; a match whose name or tracks don't read is
/// not one.
fn animations(d: &[u8]) -> Vec<(usize, usize, usize)> {
    (0..d.len().saturating_sub(0x50))
        .step_by(8)
        .filter(|&at| &d[at..at + 4] == MAGIC)
        .filter_map(|at| {
            string_at(d, offset_at(d, at + NAME)?)?;
            let tracks = offset_at(d, at + TRACKS)?;
            let count = (u32_at(d, at + TRACK_COUNT)? & 0xffff) as usize;
            d.get(tracks..tracks + count * TRACK)?;
            Some((at, tracks, count))
        })
        .collect()
}

/// The archive's skeleton: each bone's constant translation, from the
/// animation that animates the most bones.
fn skeleton(d: &[u8]) -> Result<HashMap<&str, Option<[f32; 3]>>> {
    let (_, tracks, count) = (animations(d).into_iter())
        .max_by_key(|&(_, _, count)| count)
        .context("no skeletal animation in the archive")?;
    (0..count)
        .map(|i| {
            let t = track(d, tracks + i * TRACK).context("a bone track doesn't read")?;
            Ok((t.name, t.translation.map(|(_, v)| v)))
        })
        .collect()
}

/// The length of a skeleton's leg, thigh to foot: the size of the body.
fn leg(skeleton: &HashMap<&str, Option<[f32; 3]>>) -> Result<f64> {
    [field::LEG_2_L, field::ANKLE_L]
        .iter()
        .map(|bone| {
            let v = skeleton.get(bone.get()).copied().flatten();
            Ok(length(
                v.with_context(|| format!("no {bone} in the skeleton"))?,
            ))
        })
        .sum()
}

/// What [`resize`] changed.
#[derive(Debug, PartialEq)]
pub struct Resized {
    pub clips: usize,
    pub bones: usize,
    pub curves: usize,
}

/// Resize every animation in `archive` to the skeleton of `target`, an
/// archive of the other Link's. Values shared between animations change
/// once.
pub fn resize(archive: &mut [u8], target: &[u8]) -> Result<Resized> {
    let to = skeleton(target)?;
    let from = skeleton(archive)?;
    let legs = leg(&to)? / leg(&from)?;
    let roots = [field::ROOT, field::SKL_ROOT];
    let all = animations(archive);
    let (mut bones, mut curves) = (0, 0);
    let mut done = HashSet::new();
    for &(_, tracks, count) in &all {
        for i in 0..count {
            let t = track(archive, tracks + i * TRACK).context("a bone track doesn't read")?;
            let (Some(target), Some((at, now))) = (to.get(t.name), t.translation) else {
                continue;
            };
            if t.name.starts_with(WEAPON) || !done.insert(at) {
                continue;
            }
            let root = roots.iter().any(|r| r.get() == t.name);
            let ratio = match target {
                Some(v) if !root && length(now) >= TINY && length(*v) >= TINY => {
                    length(*v) / length(now)
                }
                _ => legs,
            };
            let new: Vec<f32> = (0..3)
                .map(|axis| match target {
                    Some(v) if t.curved >> axis & 1 == 0 => v[axis],
                    _ => (f64::from(now[axis]) * ratio) as f32,
                })
                .collect();
            let scaled: Vec<usize> = (0..t.curve_count)
                .map(|j| t.curves + j * CURVE)
                .filter(|&c| u32_at(archive, c + TARGET).is_some_and(|k| TRANSLATIONS.contains(&k)))
                .filter(|&c| done.insert(c))
                .collect();
            put_floats(archive, at, &new);
            bones += 1;
            for c in scaled {
                let [scale, offset] =
                    floats_at::<2>(archive, c + SCALE).context("a curve doesn't read")?;
                let [scale, offset] = [scale, offset].map(|v| (f64::from(v) * ratio) as f32);
                put_floats(archive, c + SCALE, &[scale, offset]);
                curves += 1;
            }
        }
    }
    Ok(Resized {
        clips: all.len(),
        bones,
        curves,
    })
}

/// Rename the archive `from` to `to`, a name of the same length, wherever
/// the archive names itself. How many times it did.
pub fn rename(archive: &mut [u8], from: &str, to: &str) -> Result<usize> {
    let (from, to) = (from.as_bytes(), to.as_bytes());
    ensure!(from.len() == to.len(), "a rename keeps the name's length");
    let mut renamed = 0;
    let mut at = 0;
    while let Some(i) = archive[at..].windows(from.len()).position(|w| w == from) {
        archive[at + i..at + i + to.len()].copy_from_slice(to);
        at += i + to.len();
        renamed += 1;
    }
    ensure!(renamed > 0, "the archive doesn't name itself");
    Ok(renamed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rename_replaces_every_copy_and_keeps_the_length() {
        let mut d = b"..FakeA_01..FakeA_01.".to_vec();
        assert_eq!(rename(&mut d, "FakeA_01", "FakeB_01").unwrap(), 2);
        assert_eq!(d, b"..FakeB_01..FakeB_01.");
        assert!(rename(&mut d, "FakeB_01", "FakeLonger").is_err());
        assert!(rename(&mut d, "FakeC_01", "FakeD_01").is_err());
    }
}

#[cfg(test)]
mod dump_tests {
    use super::*;
    use crate::known::field;

    #[test]
    fn an_adult_archive_resized_to_the_child_has_his_legs_and_keeps_its_size() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let read =
            |name: &crate::known::Name| romfs.read(&format!("Model/{name}.anim.bfres.zs")).unwrap();
        let child = read(field::PLAYER_CHILD_01_ANIMATION);
        let adult = read(field::PLAYER_ADULT_02_H_ANIMATION);
        let mut resized = adult.clone();
        let done = resize(&mut resized, &child).unwrap();
        // The research note's counts, from the spike's script.
        let counts = (done.clips, done.bones, done.curves);
        assert_eq!(counts, (525, 116_025, 35_026));
        assert_eq!(resized.len(), adult.len());
        let leg_of = |d: &[u8]| leg(&skeleton(d).unwrap()).unwrap();
        assert!((leg_of(&resized) - leg_of(&child)).abs() < 1e-4);
        assert!(
            leg_of(&adult) > leg_of(&child) * 1.5,
            "the adult's are longer"
        );
    }
}
