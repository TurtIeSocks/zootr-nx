//! Skeletal animations in a BFRES: where an animation stands a bone.

use anyhow::{Context, Result, bail, ensure};

use crate::bfres::{find_magic, name_at};
use crate::reader::Reader;

pub const MAGIC: &[u8; 4] = b"FSKA";
/// An animation header's fields: its name, its bone animations and how
/// many.
pub const NAME: usize = 0x08;
pub const BONES: usize = 0x28;
pub const BONE_COUNT: usize = 0x4c;
/// A bone animation's size and fields: its name, base values and flags.
pub const BONE_SIZE: usize = 0x38;
pub const BONE_NAME: usize = 0x00;
pub const BONE_BASE: usize = 0x10;
pub const BONE_FLAGS: usize = 0x28;
/// Which parts a bone's base values hold, in this order: scale (3
/// floats), rotation (4), translation (3).
pub const HAS_SCALE: usize = 1 << 3;
pub const HAS_ROTATE: usize = 1 << 4;
pub const HAS_TRANSLATE: usize = 1 << 5;
const SCALE_FLOATS: usize = 3;
const ROTATE_FLOATS: usize = 4;

/// A bone's values at an animation's first frame. Each is `None` where the
/// animation leaves that part of the bone alone.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Base {
    pub scale: Option<[f32; 3]>,
    /// Euler angles in radians, X then Y then Z, and an unused fourth.
    pub rotate: Option<[f32; 4]>,
    pub translate: Option<[f32; 3]>,
}

/// Each bone animation `anim` moves, by name, with its values at the
/// first frame. `None` when the file has no such animation.
pub fn bases(data: &[u8], anim: &str) -> Result<Option<Vec<(String, Base)>>> {
    let Some(at) = animations(data).find(|&at| name_at(data, at + NAME) == Some(anim.as_bytes()))
    else {
        return Ok(None);
    };
    let r = Reader(data);
    let bones = r.ptr(at + BONES)?;
    let count = r.u16(at + BONE_COUNT)? as usize;
    let mut out = Vec::new();
    for b in (0..count).map(|i| bones + i * BONE_SIZE) {
        let name = name_at(data, b + BONE_NAME).context("a bone without a name")?;
        let name = String::from_utf8_lossy(name).into_owned();
        let flags = r.offset(b + BONE_FLAGS)?;
        let mut cursor = r.ptr(b + BONE_BASE)?;
        if cursor == 0 {
            out.push((name, Base::default()));
            continue;
        }
        let base = Base {
            scale: take::<SCALE_FLOATS>(&r, flags, HAS_SCALE, &mut cursor)?,
            rotate: take::<ROTATE_FLOATS>(&r, flags, HAS_ROTATE, &mut cursor)?,
            translate: take(&r, flags, HAS_TRANSLATE, &mut cursor)?,
        };
        out.push((name, base));
    }
    Ok(Some(out))
}

/// The `N` floats at `cursor`, moving it past them, when `flags` has
/// `has`.
fn take<const N: usize>(
    r: &Reader,
    flags: usize,
    has: usize,
    cursor: &mut usize,
) -> Result<Option<[f32; N]>> {
    if flags & has == 0 {
        return Ok(None);
    }
    let mut v = [0.0; N];
    for (i, f) in v.iter_mut().enumerate() {
        *f = r.f32(*cursor + i * 4)?;
    }
    *cursor += N * 4;
    Ok(Some(v))
}

/// A bone animation's curves and how many.
pub const BONE_CURVES: usize = 0x08;
pub const BONE_CURVE_COUNT: usize = 0x2e;
/// A curve's size and fields: its frames, its keys, its formats, how many
/// keys, what it animates, then how its frames and values scale.
pub const CURVE_SIZE: usize = 0x30;
const CURVE_FRAMES: usize = 0x00;
const CURVE_KEYS: usize = 0x08;
const CURVE_FLAGS: usize = 0x10;
const CURVE_KEY_COUNT: usize = 0x12;
const CURVE_TARGET: usize = 0x14;
/// What a stored frame is worth in frames, 0 meaning 1: the item spin's
/// archive stores its 360 frames as 0 and 180, at 2.
const CURVE_FRAME_SCALE: usize = 0x20;
/// A value's scale and offset over its quantized step.
const CURVE_SCALE: usize = 0x24;
const CURVE_OFFSET: usize = 0x28;
/// What a curve animates, as an offset into a bone's values: scale X, Y,
/// Z, then translation, then rotation, each 4 bytes.
const TARGET_SCALE: u32 = 0x04;
const TARGET_TRANSLATE: u32 = 0x10;
const TARGET_ROTATE: u32 = 0x20;
/// The one curve kind this game's archives use: Hermite.
const CUBIC: usize = 0;
/// A rotation curve whose keys are whole quaternions, four floats each,
/// on the bosses' rigs. Not read.
const QUATERNION: usize = 1 << 7;
/// Points sampled inside each stretch between two keys.
const SAMPLES: usize = 8;

/// A bone's values at every frame `anim` keys for it and at points between
/// them, its curves over its base values. Just the base where it has no
/// curves. `None` when the file has no such animation or bone.
pub fn frames(data: &[u8], anim: &str, bone: &str) -> Result<Option<Vec<Base>>> {
    let Some(at) = animations(data).find(|&at| name_at(data, at + NAME) == Some(anim.as_bytes()))
    else {
        return Ok(None);
    };
    let Some(base) = bases(data, anim)?
        .and_then(|bones| bones.into_iter().find(|(n, _)| n == bone).map(|(_, b)| b))
    else {
        return Ok(None);
    };
    let r = Reader(data);
    let bones = r.ptr(at + BONES)?;
    let count = r.u16(at + BONE_COUNT)? as usize;
    let Some(track) = (0..count)
        .map(|i| bones + i * BONE_SIZE)
        .find(|&b| name_at(data, b + BONE_NAME) == Some(bone.as_bytes()))
    else {
        return Ok(None);
    };
    let curves = r.ptr(track + BONE_CURVES)?;
    let n = r.u8(track + BONE_CURVE_COUNT)? as usize;
    let curves = (0..n)
        .map(|i| curve(&r, curves + i * CURVE_SIZE))
        .collect::<Result<Vec<_>>>()?;
    let mut times: Vec<f32> = curves.iter().flat_map(Curve::times).collect();
    times.sort_by(f32::total_cmp);
    times.dedup();
    if times.is_empty() {
        return Ok(Some(vec![base]));
    }
    Ok(Some(
        times
            .into_iter()
            .map(|t| {
                let mut b = base;
                for c in &curves {
                    c.set(&mut b, c.at(t));
                }
                b
            })
            .collect(),
    ))
}

/// One curve, decoded: what it animates, and each key's frame, value and
/// slope per frame.
///
/// This game's curves aren't the documented layout. Measured against
/// 369,177 segments of two archives (2026-11-27): the value scale and
/// offset sit at 0x24 and 0x28, 0x20 scales the frames, and the key block
/// opens with two floats, `a` and `b`, then holds each key as an unsigned
/// value step and slope step, side by side. A value is `offset + scale *
/// q`, a slope `a * q + b` per frame, and each stretch is a Hermite curve.
/// Decoded so, the item spin is exactly 0 to 2π at 2π over its length.
#[derive(Debug, Clone, PartialEq)]
struct Curve {
    target: u32,
    keys: Vec<Key>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Key {
    frame: f32,
    value: f32,
    slope: f32,
}

fn curve(r: &Reader, at: usize) -> Result<Curve> {
    let flags = r.u16(at + CURVE_FLAGS)? as usize;
    ensure!(
        flags >> 4 & 7 == CUBIC,
        "a curve of kind {}, not Hermite",
        flags >> 4 & 7
    );
    ensure!(flags & QUATERNION == 0, "a quaternion curve");
    let n = r.u16(at + CURVE_KEY_COUNT)? as usize;
    let (scale, offset) = (r.f32(at + CURVE_SCALE)?, r.f32(at + CURVE_OFFSET)?);
    let step = match r.f32(at + CURVE_FRAME_SCALE)? {
        0.0 => 1.0,
        s => s,
    };
    let frames_at = r.ptr(at + CURVE_FRAMES)?;
    let frame = |i: usize| -> Result<f32> {
        Ok(step
            * match flags & 3 {
                0 => r.f32(frames_at + i * 4)?,
                // Fixed point, five fraction bits.
                1 => r.u16(frames_at + i * 2)? as i16 as f32 / 32.0,
                _ => r.u8(frames_at + i)? as f32,
            })
    };
    let keys_at = r.ptr(at + CURVE_KEYS)?;
    let (a, b) = (r.f32(keys_at)?, r.f32(keys_at + 4)?);
    let steps = keys_at + 8;
    let q = |i: usize| -> Result<f32> {
        Ok(match flags >> 2 & 3 {
            1 => r.u16(steps + i * 2)? as f32,
            2 => r.u8(steps + i)? as f32,
            kind => bail!("a curve of key kind {kind}"),
        })
    };
    let keys = (0..n)
        .map(|k| {
            Ok(Key {
                frame: frame(k)?,
                value: offset + scale * q(2 * k)?,
                slope: a * q(2 * k + 1)? + b,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Curve {
        target: r.u32(at + CURVE_TARGET)?,
        keys,
    })
}

impl Curve {
    /// Each key's frame and [`SAMPLES`] points after it, up to the next.
    fn times(&self) -> Vec<f32> {
        let mut out = Vec::new();
        for (i, k) in self.keys.iter().enumerate() {
            out.push(k.frame);
            if let Some(next) = self.keys.get(i + 1) {
                let span = next.frame - k.frame;
                out.extend((1..SAMPLES).map(|s| k.frame + span * s as f32 / SAMPLES as f32));
            }
        }
        out
    }

    /// Its value at frame `t`: the Hermite stretch it falls in, held past
    /// the ends.
    fn at(&self, t: f32) -> f32 {
        let i = self.keys.iter().rposition(|k| k.frame <= t).unwrap_or(0);
        let k = self.keys[i];
        let Some(next) = self.keys.get(i + 1).filter(|n| n.frame > k.frame) else {
            return k.value;
        };
        let span = next.frame - k.frame;
        let u = ((t - k.frame) / span).clamp(0.0, 1.0);
        let (u2, u3) = (u * u, u * u * u);
        (2.0 * u3 - 3.0 * u2 + 1.0) * k.value
            + (u3 - 2.0 * u2 + u) * span * k.slope
            + (-2.0 * u3 + 3.0 * u2) * next.value
            + (u3 - u2) * span * next.slope
    }

    /// Put `v` where it animates in `b`.
    fn set(&self, b: &mut Base, v: f32) {
        let t = self.target;
        let slot = |from: u32| ((t - from) / 4) as usize;
        if (TARGET_SCALE..TARGET_SCALE + 12).contains(&t) {
            b.scale.get_or_insert([1.0; 3])[slot(TARGET_SCALE)] = v;
        } else if (TARGET_TRANSLATE..TARGET_TRANSLATE + 12).contains(&t) {
            b.translate.get_or_insert([0.0; 3])[slot(TARGET_TRANSLATE)] = v;
        } else if (TARGET_ROTATE..TARGET_ROTATE + 16).contains(&t) {
            b.rotate.get_or_insert([0.0, 0.0, 0.0, 1.0])[slot(TARGET_ROTATE)] = v;
        }
    }
}

/// Every skeletal animation's name.
pub fn names(data: &[u8]) -> Vec<String> {
    animations(data)
        .filter_map(|at| name_at(data, at + NAME))
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

/// Where a bone's base translation sits, given its `flags` and its base
/// values at `base`: past the scale and rotation the flags say come first.
/// `None` when it has no base translation.
pub fn translation_at(flags: usize, base: usize) -> Option<usize> {
    if flags & HAS_TRANSLATE == 0 || base == 0 {
        return None;
    }
    let floats = |has, n: usize| if flags & has != 0 { n * 4 } else { 0 };
    Some(base + floats(HAS_SCALE, SCALE_FLOATS) + floats(HAS_ROTATE, ROTATE_FLOATS))
}

/// Every skeletal animation header's offset, by magic, 8-aligned.
pub fn animations(data: &[u8]) -> impl Iterator<Item = usize> + '_ {
    // Its last field read is the 16-bit bone count.
    find_magic(data, MAGIC, BONE_COUNT + 2, 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One animation, `Pose`, with bone `Root` (scale, rotate, translate)
    /// and bone `Leg` (translate only).
    fn file() -> Vec<u8> {
        let mut d = vec![0u8; 0x200];
        let put = |d: &mut Vec<u8>, at: usize, b: &[u8]| d[at..at + b.len()].copy_from_slice(b);
        let string = |d: &mut Vec<u8>, at: usize, s: &str| {
            put(d, at, &(s.len() as u16).to_le_bytes());
            put(d, at + 2, s.as_bytes());
        };
        let (anim, bones, base_root, base_leg) = (0x10, 0x80, 0x100, 0x140);
        put(&mut d, anim, MAGIC);
        put(&mut d, anim + NAME, &0x180u64.to_le_bytes());
        put(&mut d, anim + BONES, &(bones as u64).to_le_bytes());
        put(&mut d, anim + BONE_COUNT, &2u32.to_le_bytes());
        string(&mut d, 0x180, "Pose");
        string(&mut d, 0x190, "Root");
        string(&mut d, 0x1a0, "Leg");
        for (i, (name, base, flags)) in [
            (0x190u64, base_root, HAS_SCALE | HAS_ROTATE | HAS_TRANSLATE),
            (0x1a0, base_leg, HAS_TRANSLATE),
        ]
        .into_iter()
        .enumerate()
        {
            let b = bones + i * BONE_SIZE;
            put(&mut d, b + BONE_NAME, &name.to_le_bytes());
            put(&mut d, b + BONE_BASE, &(base as u64).to_le_bytes());
            put(&mut d, b + BONE_FLAGS, &(flags as u32).to_le_bytes());
        }
        let floats = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>();
        put(
            &mut d,
            base_root,
            &floats(&[1., 1., 1., 0., 0., 0., 1., 0.5, 0.18, -0.5]),
        );
        put(&mut d, base_leg, &floats(&[0., 0.3, 0.]));
        d
    }

    #[test]
    fn a_bones_translation_skips_its_scale_and_rotation() {
        let d = file();
        let bones = bases(&d, "Pose").unwrap().unwrap();
        let translate = |name: &str| {
            bones
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, b)| b.translate)
        };
        assert_eq!(translate("Root"), Some(Some([0.5, 0.18, -0.5])));
        assert_eq!(translate("Leg"), Some(Some([0., 0.3, 0.])));
        assert_eq!(translate("Arm"), None);
        assert!(bases(&d, "Walk").unwrap().is_none());
        assert_eq!(names(&d), ["Pose"]);
    }

    /// One animation, `Spin`, with bone `Root` turning about Y on one
    /// curve: the item spin's own bytes, 360 frames stored as 0 and 180 at
    /// a frame scale of 2, keys stepping 0 to 255 at 2π/255.
    fn spin() -> Vec<u8> {
        let mut d = vec![0u8; 0x300];
        let put = |d: &mut Vec<u8>, at: usize, b: &[u8]| d[at..at + b.len()].copy_from_slice(b);
        let floats = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>();
        let (anim, bones, base, curve, frames, keys) = (0x10, 0x80, 0x100, 0x140, 0x180, 0x188);
        put(&mut d, anim, MAGIC);
        put(&mut d, anim + NAME, &0x200u64.to_le_bytes());
        put(&mut d, anim + BONES, &(bones as u64).to_le_bytes());
        put(&mut d, anim + BONE_COUNT, &1u32.to_le_bytes());
        put(&mut d, 0x200, &4u16.to_le_bytes());
        put(&mut d, 0x202, b"Spin");
        put(&mut d, 0x210, &4u16.to_le_bytes());
        put(&mut d, 0x212, b"Root");
        put(&mut d, bones + BONE_NAME, &0x210u64.to_le_bytes());
        put(&mut d, bones + BONE_CURVES, &(curve as u64).to_le_bytes());
        put(&mut d, bones + BONE_BASE, &(base as u64).to_le_bytes());
        put(
            &mut d,
            bones + BONE_FLAGS,
            &((HAS_ROTATE) as u32).to_le_bytes(),
        );
        d[bones + BONE_CURVE_COUNT] = 1;
        put(&mut d, base, &floats(&[0.0, 0.0, 0.0, 1.0]));
        put(&mut d, curve + CURVE_FRAMES, &(frames as u64).to_le_bytes());
        put(&mut d, curve + CURVE_KEYS, &(keys as u64).to_le_bytes());
        put(&mut d, curve + CURVE_FLAGS, &0x4c0au16.to_le_bytes());
        put(&mut d, curve + CURVE_KEY_COUNT, &2u16.to_le_bytes());
        put(&mut d, curve + CURVE_TARGET, &0x24u32.to_le_bytes());
        put(&mut d, curve + 0x1c, &floats(&[360.0]));
        put(
            &mut d,
            curve + CURVE_FRAME_SCALE,
            &floats(&[2.0, TAU / 255.0, 0.0]),
        );
        put(&mut d, frames, &[0, 180]);
        put(&mut d, keys, &floats(&[1.0 / 255.0, PI / 180.0]));
        put(&mut d, keys + 8, &[0, 0, 255, 0]);
        d
    }

    use std::f32::consts::{PI, TAU};

    #[test]
    fn the_item_spin_turns_once_evenly_over_its_length() {
        let d = spin();
        let got = frames(&d, "Spin", "Root").unwrap().unwrap();
        // Two keys, 360 frames apart, sampled eight times between them.
        assert_eq!(got.len(), 9);
        for (i, f) in got.iter().enumerate() {
            let want = TAU * i as f32 / 8.0;
            assert!((f.rotate.unwrap()[1] - want).abs() < 1e-4, "{i}: {f:?}");
        }
        assert!(frames(&d, "Spin", "Leg").unwrap().is_none());
        assert!(frames(&d, "Walk", "Root").unwrap().is_none());
    }

    #[test]
    fn a_hermite_stretch_meets_its_keys_and_eases_by_its_slopes() {
        let k = |frame, value, slope| Key {
            frame,
            value,
            slope,
        };
        let c = Curve {
            target: TARGET_TRANSLATE + 4,
            keys: vec![k(0.0, 1.0, 0.0), k(10.0, 2.0, 0.0), k(20.0, 2.0, 0.0)],
        };
        assert_eq!(c.at(0.0), 1.0);
        assert!((c.at(10.0) - 2.0).abs() < 1e-6);
        // Flat slopes ease in and out: the middle is halfway.
        assert!((c.at(5.0) - 1.5).abs() < 1e-6);
        assert!(c.at(2.5) < 1.25);
        // Held past the last key.
        assert_eq!(c.at(99.0), 2.0);
        let mut b = Base::default();
        c.set(&mut b, 0.5);
        assert_eq!(b.translate, Some([0.0, 0.5, 0.0]));
    }

    #[test]
    fn a_translation_sits_past_the_scale_and_rotation_its_flags_carry() {
        let all = HAS_SCALE | HAS_ROTATE | HAS_TRANSLATE;
        assert_eq!(translation_at(all, 0x100), Some(0x100 + 12 + 16));
        assert_eq!(
            translation_at(HAS_ROTATE | HAS_TRANSLATE, 0x100),
            Some(0x110)
        );
        assert_eq!(translation_at(HAS_TRANSLATE, 0x100), Some(0x100));
        assert_eq!(translation_at(HAS_SCALE | HAS_ROTATE, 0x100), None);
        assert_eq!(translation_at(all, 0), None);
    }
}
