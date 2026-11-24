//! Skeletal animations in a BFRES: where an animation stands a bone.

use anyhow::{Context, Result};

use crate::bfres::{find_magic, name_at};
use crate::reader::Reader;

const MAGIC: &[u8; 4] = b"FSKA";
const NAME: usize = 0x08;
const BONES: usize = 0x28;
const BONE_COUNT: usize = 0x4c;
const BONE_SIZE: usize = 0x38;
const BONE_NAME: usize = 0x00;
const BONE_BASE: usize = 0x10;
const BONE_FLAGS: usize = 0x28;
/// Which parts a bone's base values hold, in this order.
const HAS_SCALE: usize = 1 << 3;
const HAS_ROTATE: usize = 1 << 4;
const HAS_TRANSLATE: usize = 1 << 5;

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
            scale: take(&r, flags, HAS_SCALE, &mut cursor)?,
            rotate: take(&r, flags, HAS_ROTATE, &mut cursor)?,
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

/// Every skeletal animation's name.
pub fn names(data: &[u8]) -> Vec<String> {
    animations(data)
        .filter_map(|at| name_at(data, at + NAME))
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

/// Every skeletal animation header's offset.
fn animations(data: &[u8]) -> impl Iterator<Item = usize> + '_ {
    find_magic(data, MAGIC, BONE_COUNT + 4, 8)
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
}
