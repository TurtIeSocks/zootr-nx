//! A model's skeleton (FSKL) in its bind pose, and its shapes' vertices
//! moved into the model's space with it.
//!
//! A shape weighed by two or more bones keeps its vertices in the model's
//! space already. One weighed by one keeps them in a bone's: the shape's
//! own bone with no weights (skin 0), or with one (skin 1) the bone its
//! `_i0` names through the skeleton's matrix-to-bone list. A bone's place
//! is its parent's, then its translation, rotation and scale.

use anyhow::{Context, Result, ensure};

use super::names::find_magic;
use super::shape::shapes_of;
use super::{attribute, attributes, bones};
use crate::reader::Reader;

/// The skeleton header: its flags (the rotation mode in `ROTATION_MODE`),
/// its bones and matrix-to-bone list, and its bone, smooth and rigid
/// matrix counts (u16s).
const FLAGS: usize = 0x04;
const BONES: usize = 0x10;
const MATRIX_BONES: usize = 0x18;
const BONE_COUNT: usize = 0x38;
const SMOOTH_COUNT: usize = 0x3a;
const RIGID_COUNT: usize = 0x3c;
const ROTATION_MODE: u32 = 0x7000;
const EULER_XYZ: u32 = 0x1000;
/// A bone: its parent (u16, none at 0xffff), then its scale (3 floats),
/// rotation (4: a quaternion, or Euler XYZ angles and a 1) and
/// translation (3).
const BONE_SIZE: usize = 0x58;
const BONE_PARENT: usize = 0x22;
const BONE_SCALE: usize = 0x30;
const BONE_ROTATION: usize = 0x3c;
const BONE_TRANSLATION: usize = 0x4c;
const NO_PARENT: u16 = 0xffff;

/// An affine transform, three rows of a 4x4 matrix acting on columns.
pub type Affine = [[f32; 4]; 3];

const IDENTITY: Affine = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

/// A model's skeleton in its bind pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    /// Each bone's place in the model.
    pub bones: Vec<Affine>,
    /// The bone each skinning matrix stands for, smooth ones first.
    pub matrices: Vec<usize>,
}

/// The skeleton of the one model in `data`.
pub fn skeleton(data: &[u8]) -> Result<Skeleton> {
    let r = Reader(data);
    let mut found = find_magic(data, b"FSKL", RIGID_COUNT + 2, 8);
    let at = found.next().context("no skeleton")?;
    ensure!(found.next().is_none(), "more than one skeleton");
    let euler = r.u32(at + FLAGS)? & ROTATION_MODE == EULER_XYZ;
    let first = r.ptr(at + BONES)?;
    let mut bones: Vec<Affine> = Vec::new();
    for i in 0..r.u16(at + BONE_COUNT)? as usize {
        let b = first + BONE_SIZE * i;
        let floats = |at: usize, n: usize| -> Result<Vec<f32>> {
            (0..n).map(|k| r.f32(at + 4 * k)).collect()
        };
        let (s, q, t) = (
            floats(b + BONE_SCALE, 3)?,
            floats(b + BONE_ROTATION, 4)?,
            floats(b + BONE_TRANSLATION, 3)?,
        );
        let rotation = match euler {
            true => euler_xyz([q[0], q[1], q[2]]),
            false => quaternion([q[0], q[1], q[2], q[3]]),
        };
        let mut local = IDENTITY;
        for (row, out) in local.iter_mut().enumerate() {
            for (col, value) in out.iter_mut().take(3).enumerate() {
                *value = rotation[row][col] * s[col];
            }
            out[3] = t[row];
        }
        let parent = r.u16(b + BONE_PARENT)?;
        bones.push(match parent {
            NO_PARENT => local,
            p => {
                let p = bones.get(p as usize).context("a bone before its parent")?;
                multiply(p, &local)
            }
        });
    }
    let list = r.ptr(at + MATRIX_BONES)?;
    let count = (r.u16(at + SMOOTH_COUNT)? + r.u16(at + RIGID_COUNT)?) as usize;
    let matrices = (0..count)
        .map(|i| Ok(r.u16(list + 2 * i)? as usize))
        .collect::<Result<_>>()?;
    Ok(Skeleton { bones, matrices })
}

/// The bits of a `bones` value that hold the first index of the shape
/// named `shape`: its `_i0` format's component width, 8 or 16 bits.
fn index_mask(data: &[u8], shape: &str) -> Result<u32> {
    let format = (attributes(data, shape)?.into_iter())
        .find(|a| a.name == "_i0")
        .with_context(|| format!("{shape}: no _i0"))?
        .format;
    match format >> 8 {
        0x02 => Ok(0xff),
        0x0a => Ok(0xffff),
        _ => anyhow::bail!("{shape}: _i0 in format {format:#x}"),
    }
}

/// Each vertex of the shape named `shape`, in the model's space, with
/// its normal turned the same way.
pub fn model_vertices(
    data: &[u8],
    skeleton: &Skeleton,
    shape: &str,
) -> Result<Vec<([f32; 3], [f32; 3])>> {
    let info = shapes_of(data)?
        .into_iter()
        .find(|s| s.name == shape)
        .with_context(|| format!("no shape {shape}"))?;
    let positions = attribute(data, shape, "_p0")?;
    let normals = attribute(data, shape, "_n0")?;
    let bone_of = |m: usize| -> Result<&Affine> {
        let bone = *skeleton.matrices.get(m).context("a matrix past the list")?;
        skeleton.bones.get(bone).context("a bone past the skeleton")
    };
    let each: Vec<Option<&Affine>> = match info.skin {
        0 => {
            let bone = skeleton
                .bones
                .get(info.bone)
                .context("a bone past the skeleton")?;
            vec![Some(bone); positions.len()]
        }
        // The first index names each vertex's matrix.
        1 => {
            let mask = index_mask(data, shape)?;
            (bones(data, shape)?.into_iter())
                .map(|m| bone_of((m & mask) as usize).map(Some))
                .collect::<Result<_>>()?
        }
        _ => vec![None; positions.len()],
    };
    Ok((positions.iter().zip(&normals).zip(each))
        .map(|((p, n), bone)| {
            let (p, n) = ([p[0], p[1], p[2]], [n[0], n[1], n[2]]);
            match bone {
                Some(m) => (apply(m, p, 1.0), apply(m, n, 0.0)),
                None => (p, n),
            }
        })
        .collect())
}

/// `m` applied to `v`, moved by its translation when `w` is 1.
fn apply(m: &Affine, v: [f32; 3], w: f32) -> [f32; 3] {
    std::array::from_fn(|row| {
        m[row][0] * v[0] + m[row][1] * v[1] + m[row][2] * v[2] + m[row][3] * w
    })
}

/// `a` after `b`.
fn multiply(a: &Affine, b: &Affine) -> Affine {
    std::array::from_fn(|row| {
        std::array::from_fn(|col| {
            let turn: f32 = (0..3).map(|k| a[row][k] * b[k][col]).sum();
            turn + if col == 3 { a[row][3] } else { 0.0 }
        })
    })
}

/// The rotation turning about X, then Y, then Z.
fn euler_xyz([x, y, z]: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx, sy, cy, sz, cz) = (x.sin(), x.cos(), y.sin(), y.cos(), z.sin(), z.cos());
    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz],
        [-sy, sx * cy, cx * cy],
    ]
}

/// The rotation of the unit quaternion `x, y, z, w`.
fn quaternion([x, y, z, w]: [f32; 4]) -> [[f32; 3]; 3] {
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    #[test]
    fn euler_xyz_turns_about_x_first() {
        let half = std::f32::consts::FRAC_PI_2;
        // A quarter turn about X takes Y to Z; then about Z, Z stays.
        let r = euler_xyz([half, 0.0, half]);
        let m = [
            [r[0][0], r[0][1], r[0][2], 0.0],
            [r[1][0], r[1][1], r[1][2], 0.0],
            [r[2][0], r[2][1], r[2][2], 0.0],
        ];
        assert!(close(apply(&m, [0.0, 1.0, 0.0], 0.0), [0.0, 0.0, 1.0]));
        // And X goes to Y, by the turn about Z alone.
        assert!(close(apply(&m, [1.0, 0.0, 0.0], 0.0), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn a_quaternion_and_its_euler_angles_agree() {
        let (a, s) = (0.3f32, (0.3f32 / 2.0).sin());
        let q = quaternion([0.0, s, 0.0, (a / 2.0).cos()]);
        let e = euler_xyz([0.0, a, 0.0]);
        for row in 0..3 {
            assert!(close(q[row], e[row]));
        }
    }

    #[test]
    fn a_child_bone_sits_on_its_parent() {
        let mut parent = IDENTITY;
        parent[0][3] = 2.0;
        let mut child = IDENTITY;
        child[1][3] = 3.0;
        let both = multiply(&parent, &child);
        assert!(close(apply(&both, [0.0; 3], 1.0), [2.0, 3.0, 0.0]));
    }
}
