//! How low and how high an item's model reaches once its pose and a turn
//! have it: the one source of every height a seed writes.
//!
//! A holder's item reaches down to where the goods the game put there
//! reached (`rise`). Heights come from a generated table ([`table`],
//! written by `zootr probe heights`): each model's hull, the extreme
//! vertices of its mesh over 2562 directions, and each pose's root
//! transforms, one per frame of the clip it plays. A model's lowest point
//! under any turn is a vertex of its hull, so a row answers every angle.
//! Nothing is measured by hand, and nothing here reads the dump.

mod table;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, BymlDoc};

use crate::known::{Name, field, hash};
use crate::medallions;
use crate::standins::{self, ANIMATION, MODEL, SPIN};
use table::{MODELS, PLANKS, POSES};

/// A turn's rows: the rows of `Rz · Ry · Rx` for Euler X, Y, Z.
pub(crate) type Turn = [[f32; 3]; 3];
/// No turn.
pub(crate) const UPRIGHT: Turn = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
/// A pose's root transform, rows of a 3×4 affine map.
pub(crate) type Frame = [f32; 12];
/// Each model point is stored in millimetres.
const MM: f32 = 1000.0;

/// What a model's row was made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Kind {
    /// Its mesh's vertices.
    Mesh = 0,
    /// Its mesh's vertices, with a shape skinned to more than one bone
    /// posed by the root alone until the model reader returns weights.
    Skinned = 1,
    /// No readable vertices: its `ActorInfo` box's corners.
    Box = 2,
    /// Nothing to read: one point at its origin.
    Missing = 3,
}

impl Kind {
    pub(crate) const fn of(byte: u8) -> Kind {
        match byte {
            0 => Kind::Mesh,
            1 => Kind::Skinned,
            2 => Kind::Box,
            _ => Kind::Missing,
        }
    }
}

/// Which clip a holder plays while it rests: on a shelf the goods' shop
/// pose where an archive has one, elsewhere the common pose. Without
/// either, the root stays where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Role {
    Shelf,
    Rest,
}

impl Role {
    /// The clips it plays, the first an archive list has.
    pub(crate) fn clips(self) -> &'static [&'static Name] {
        match self {
            Role::Shelf => &[field::POSE_SHOP, field::POSE_COMMON],
            Role::Rest => &[field::POSE_COMMON],
        }
    }

    fn word(self) -> &'static str {
        match self {
            Role::Shelf => "shelf",
            Role::Rest => "rest",
        }
    }
}

/// What a holder shows: the model file it draws (`PROJECT.MODEL`) and the
/// animation archives it plays, in the role it plays them. The table's
/// keys, so the probe and the build name a row the same way.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Shown {
    pub model: String,
    pub archives: Vec<String>,
    pub role: Role,
}

impl Shown {
    pub(crate) fn model_key(&self) -> u64 {
        hash(&self.model)
    }

    pub(crate) fn pose_key(&self) -> u64 {
        pose_key(&self.archives, self.role)
    }

    /// Its hull, posed by every frame of the clip it plays. A model the
    /// table lacks (one the seed paints, like a key's copy) takes `item`'s
    /// `ActorInfo` box instead.
    pub(crate) fn hull(&self, actor: &BymlDoc, item: &str) -> Result<Hull> {
        let (kind, rest, points) = match model_row(self.model_key()) {
            Some((kind, rest, points)) => (
                kind,
                rest,
                points.iter().map(|p| p.map(|c| c as f32 / MM)).collect(),
            ),
            None => (Kind::Box, IDENTITY, corners(actor, item)?),
        };
        // A pose with no clip leaves the root bones where the model binds
        // them.
        let frames = match pose_row(self.pose_key()) {
            Some(frames) if !frames.is_empty() => frames,
            _ => std::slice::from_ref(&rest),
        };
        Ok(Hull::posed(kind, &points, frames))
    }
}

/// The table key of an archive list's pose in `role`.
pub(crate) fn pose_key(archives: &[String], role: Role) -> u64 {
    hash(&format!("{}|{}", archives.join("+"), role.word()))
}

pub(crate) const IDENTITY: Frame = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn model_row(key: u64) -> Option<(Kind, Frame, &'static [[i16; 3]])> {
    let at = MODELS.binary_search_by_key(&key, |(n, ..)| n.0).ok()?;
    let (_, kind, rest, points) = MODELS[at];
    Some((Kind::of(kind), rest, points))
}

/// How far the plank under shop slot `slot` (its placement's hash) sits
/// from the slot, read from the shop's collision. `None` where no
/// collision stands under it: the Zora Shop's shelves have none.
pub(crate) fn plank(slot: u64) -> Option<f32> {
    let at = PLANKS.binary_search_by_key(&slot, |(s, _)| *s).ok()?;
    Some(PLANKS[at].1)
}

fn pose_row(key: u64) -> Option<&'static [Frame]> {
    let at = POSES.binary_search_by_key(&key, |(n, _)| n.0).ok()?;
    Some(POSES[at].1)
}

/// What the table made `shown`'s model row from, and whether it holds
/// its pose: `None` for either missing.
#[cfg(test)]
pub(crate) fn rows(shown: &Shown) -> (Option<Kind>, bool) {
    let kind = model_row(shown.model_key()).map(|(kind, ..)| kind);
    (kind, pose_row(shown.pose_key()).is_some())
}

/// The 8 corners of `item`'s `ActorInfo` model box.
fn corners(actor: &BymlDoc, item: &str) -> Result<Vec<[f32; 3]>> {
    let row = standins::row(actor, item)?;
    let corner = |key: &Name| -> Result<[f32; 3]> {
        let map = row.get(key.get()).context("no model box")?.as_map()?;
        let axis = |a: &str| map.get(a).and_then(standins::float).unwrap_or(0.0);
        Ok([axis("X"), axis("Y"), axis("Z")])
    };
    let (lo, hi) = (
        corner(field::MODEL_AABB_MIN)?,
        corner(field::MODEL_AABB_MAX)?,
    );
    Ok((0..8)
        .map(|i| [0, 1, 2].map(|a| if i >> a & 1 == 0 { lo[a] } else { hi[a] }))
        .collect())
}

/// What a holder shows, as the stand-in build binds it: the model and
/// archives of `item` where its pack names them, else `template`'s. A song
/// shows the medallion the seed paints for it. On a shelf it plays the
/// spin, then `pose`.
pub(crate) fn shown(
    template: &ActorPack,
    item: &ActorPack,
    role: Role,
    pose: Option<&str>,
) -> Result<Shown> {
    let names = |key: &Name| -> Result<bool> {
        let param = item.actor_param()?.context("no actor param")?;
        Ok(standins::component(&param, key).is_some())
    };
    let model = match medallions::medallion(&item.name) {
        Some(medal) => format!("{medal}.{medal}"),
        None => {
            let pack = if names(MODEL)? { item } else { template };
            let (project, model) = standins::model_file(pack)?
                .with_context(|| format!("{} shows no model", pack.name))?;
            format!("{project}.{model}")
        }
    };
    let archives = match pose {
        Some(pose) => vec![SPIN.to_string(), pose.to_string()],
        None => archives(if names(ANIMATION)? { item } else { template })?,
    };
    Ok(Shown {
        model,
        archives,
        role,
    })
}

/// The animation archives `pack`'s AnimationParam plays, in order. None
/// when it has no AnimationParam.
pub(crate) fn archives(pack: &ActorPack) -> Result<Vec<String>> {
    let Ok(path) = standins::path_of(pack, ANIMATION) else {
        return Ok(Vec::new());
    };
    let param = standins::resolved(pack, &path)?;
    let Some(list) = param.get(field::ANIMATION_RESOURCES.get()) else {
        return Ok(Vec::new());
    };
    Ok(list
        .as_array()?
        .iter()
        .filter_map(|a| {
            let a = a.as_map().ok()?;
            Some(
                a.get(field::MODEL_PROJECT_NAME.get())?
                    .as_string()
                    .ok()?
                    .to_string(),
            )
        })
        .collect())
}

/// A model's points once each frame of its pose has them: the vertices
/// its lowest and highest points come from under any turn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hull {
    pub kind: Kind,
    points: Vec<[f32; 3]>,
}

impl Hull {
    /// `points` through each of `frames`.
    pub(crate) fn posed(kind: Kind, points: &[[f32; 3]], frames: &[Frame]) -> Hull {
        let points = frames
            .iter()
            .flat_map(|f| points.iter().map(move |p| apply(f, *p)))
            .collect();
        Hull { kind, points }
    }

    /// The lowest and highest points once `turn` has it.
    pub(crate) fn span(&self, turn: &Turn) -> (f32, f32) {
        let row = turn[1];
        self.points
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                let y = dot(row, *p);
                (lo.min(y), hi.max(y))
            })
    }

    pub(crate) fn lo(&self, turn: &Turn) -> f32 {
        self.span(turn).0
    }

    /// The lowest and highest any point reaches while the shelf's spin
    /// turns it a full circle about its root's up, `turn` then turning the
    /// whole. A point's height swings by its distance from that axis
    /// times how far `turn` tips the axis, so the swing's ends are exact,
    /// with no angle sampled.
    pub(crate) fn spun(&self, turn: &Turn) -> (f32, f32) {
        let [x, y, z] = turn[1];
        let tip = x.hypot(z);
        self.points
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                let (up, out) = (y * p[1], tip * p[0].hypot(p[2]));
                (lo.min(up - out), hi.max(up + out))
            })
    }

    /// Whether it lies flat: thinner upright than half its narrower side
    /// across.
    pub(crate) fn flat(&self) -> bool {
        self.reach(1) < 0.5 * self.reach(0).min(self.reach(2))
    }

    /// How far it reaches along its own axis `i` (X, Y, Z), unturned.
    pub(crate) fn reach(&self, i: usize) -> f32 {
        let (lo, hi) = (self.points.iter()).fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p[i]), hi.max(p[i]))
        });
        hi - lo
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn apply(f: &Frame, p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| f[r * 4] * p[0] + f[r * 4 + 1] * p[1] + f[r * 4 + 2] * p[2] + f[r * 4 + 3])
}

/// Whether `item`, turned by `place`, fits under a `gap` to the shelf
/// above: its spin's sweep, from its lowest point up.
pub(crate) fn fits(item: &Hull, place: &Turn, gap: f32) -> bool {
    item.spun(place).1 - item.lo(place) <= gap
}

/// How far an item rises over its placement so its lowest point meets the
/// goods': both in the turn they stand in.
pub(crate) fn rise(goods: &Hull, item: &Hull, turn: &Turn) -> f32 {
    goods.lo(turn) - item.lo(turn)
}

/// The rows of `Rz · Ry · Rx` for Euler X, Y, Z in radians: a pose's turn
/// and a placement's alike.
pub(crate) fn euler([x, y, z]: [f32; 3]) -> Turn {
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    [
        [cy * cz, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz],
        [-sy, cy * sx, cy * cx],
    ]
}

/// The Euler X, Y, Z of `turn`, as [`euler`] takes them.
pub(crate) fn angles(turn: &Turn) -> [f32; 3] {
    let sy = (-turn[2][0]).clamp(-1.0, 1.0);
    let y = sy.asin();
    if sy.abs() < 0.999_999 {
        [
            turn[2][1].atan2(turn[2][2]),
            y,
            turn[1][0].atan2(turn[0][0]),
        ]
    } else {
        // Gimbal lock: only X − Z (or X + Z) is fixed, so Z takes 0.
        [turn[0][1].atan2(turn[1][1]) * sy.signum(), y, 0.0]
    }
}

/// The turn `b` then `a`: their product `a · b`.
pub(crate) fn times(a: &Turn, b: &Turn) -> Turn {
    [0, 1, 2].map(|r| [0, 1, 2].map(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

/// A bone's scale, Euler turn and translation as an affine map.
pub(crate) fn frame(scale: [f32; 3], rotate: [f32; 3], translate: [f32; 3]) -> Frame {
    let r = euler(rotate);
    let mut f = [0.0; 12];
    for row in 0..3 {
        for col in 0..3 {
            f[row * 4 + col] = r[row][col] * scale[col];
        }
        f[row * 4 + 3] = translate[row];
    }
    f
}

/// The map that undoes `f`.
pub(crate) fn invert(f: &Frame) -> Frame {
    let m = |r: usize, c: usize| f[r * 4 + c];
    let det = m(0, 0) * (m(1, 1) * m(2, 2) - m(1, 2) * m(2, 1))
        - m(0, 1) * (m(1, 0) * m(2, 2) - m(1, 2) * m(2, 0))
        + m(0, 2) * (m(1, 0) * m(2, 1) - m(1, 1) * m(2, 0));
    let mut out = [0.0; 12];
    for r in 0..3 {
        for c in 0..3 {
            // The cofactor of (c, r), over the determinant.
            let (a, b) = (
                [0, 1, 2]
                    .iter()
                    .filter(|&&i| i != c)
                    .copied()
                    .collect::<Vec<_>>(),
                [0, 1, 2]
                    .iter()
                    .filter(|&&i| i != r)
                    .copied()
                    .collect::<Vec<_>>(),
            );
            let minor = m(a[0], b[0]) * m(a[1], b[1]) - m(a[0], b[1]) * m(a[1], b[0]);
            let sign = if (r + c) % 2 == 0 { 1.0 } else { -1.0 };
            out[r * 4 + c] = sign * minor / det;
        }
    }
    for r in 0..3 {
        out[r * 4 + 3] = -(0..3).map(|k| out[r * 4 + k] * f[k * 4 + 3]).sum::<f32>();
    }
    out
}

/// `parent` after `child`: a child bone's map inside its parent's.
pub(crate) fn chain(parent: &Frame, child: &Frame) -> Frame {
    let mut f = [0.0; 12];
    for row in 0..3 {
        for col in 0..4 {
            let mut v: f32 = (0..3)
                .map(|k| parent[row * 4 + k] * child[k * 4 + col])
                .sum();
            if col == 3 {
                v += parent[row * 4 + 3];
            }
            f[row * 4 + col] = v;
        }
    }
    f
}

/// The 2562 directions of an icosphere split four times, about 4° apart.
pub(crate) fn directions() -> Vec<[f32; 3]> {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut points: Vec<[f32; 3]> = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ]
    .map(unit)
    .to_vec();
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..4 {
        let mut middles = std::collections::HashMap::new();
        let mut middle = |a: usize, b: usize, points: &mut Vec<[f32; 3]>| {
            *middles.entry((a.min(b), a.max(b))).or_insert_with(|| {
                let (p, q) = (points[a], points[b]);
                points.push(unit([0, 1, 2].map(|i| p[i] + q[i])));
                points.len() - 1
            })
        };
        faces = faces
            .into_iter()
            .flat_map(|[a, b, c]| {
                let ab = middle(a, b, &mut points);
                let bc = middle(b, c, &mut points);
                let ca = middle(c, a, &mut points);
                [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]
            })
            .collect();
    }
    points
}

fn unit(p: [f32; 3]) -> [f32; 3] {
    let n = dot(p, p).sqrt();
    p.map(|c| c / n)
}

/// The points of `points` extreme in some direction of `directions`: true
/// vertices of their convex hull, which reach as low as the points do in
/// every sampled direction and within the sampling's spacing between them.
pub(crate) fn extremes(points: &[[f32; 3]], directions: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut at: Vec<usize> = directions
        .iter()
        .filter_map(|d| {
            (0..points.len()).max_by(|&a, &b| dot(*d, points[a]).total_cmp(&dot(*d, points[b])))
        })
        .collect();
    at.sort_unstable();
    at.dedup();
    at.into_iter().map(|i| points[i]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn cube(lo: [f32; 3], hi: [f32; 3]) -> Vec<[f32; 3]> {
        (0..8)
            .map(|i| [0, 1, 2].map(|a| if i >> a & 1 == 0 { lo[a] } else { hi[a] }))
            .collect()
    }

    fn hull(points: &[[f32; 3]]) -> Hull {
        Hull::posed(Kind::Mesh, points, &[IDENTITY])
    }

    #[test]
    fn a_boxs_lowest_point_under_a_turn_about_x_is_its_corner_formula() {
        let (lo, hi) = ([-0.1, -0.2, -0.3], [0.1, 0.4, 0.5]);
        let h = hull(&cube(lo, hi));
        for deg in [0.0f32, 15.0, 33.0, 60.0, 89.0] {
            let t = deg.to_radians();
            let want = lo[1] * t.cos() - hi[2] * t.sin();
            assert!((h.lo(&euler([t, 0.0, 0.0])) - want).abs() < 1e-5, "{deg}");
        }
    }

    #[test]
    fn the_sampled_hull_never_reads_below_the_true_lowest_and_within_a_millimetre() {
        // A 0.7 m blob of points: a sphere's surface, squashed and moved.
        let points: Vec<[f32; 3]> = (0..4000)
            .map(|i| {
                let (a, b) = (i as f32 * 2.399_963, (i as f32 / 4000.0) * 2.0 - 1.0);
                let r = (1.0 - b * b).sqrt();
                [0.35 * r * a.cos(), 0.2 * b + 0.1, 0.25 * r * a.sin()]
            })
            .collect();
        let dirs = directions();
        assert_eq!(dirs.len(), 2562);
        let sampled = hull(&extremes(&points, &dirs));
        let all = hull(&points);
        for i in 0..200 {
            let turn = euler([i as f32 * 0.37, i as f32 * 0.61, i as f32 * 0.23]);
            let (truth, read) = (all.lo(&turn), sampled.lo(&turn));
            assert!(read >= truth - 1e-6, "{i}: {read} below {truth}");
            // 2562 directions leave the worst turn 0.53 mm high on this blob.
            assert!(read - truth < 0.001, "{i}: {read} vs {truth}");
        }
    }

    #[test]
    fn the_same_item_at_its_own_slot_rises_zero() {
        let h = hull(&cube([-0.1, -0.05, -0.1], [0.1, 0.3, 0.1]));
        for turn in [UPRIGHT, euler([0.0, 1.012, 0.0]), euler([0.4, 0.2, 0.1])] {
            assert_eq!(rise(&h, &h, &turn), 0.0);
        }
    }

    #[test]
    fn a_placements_turn_runs_after_the_pose() {
        let quarter = euler([FRAC_PI_2, 0.0, 0.0]);
        let rod = [[0.0, 0.0, -0.2], [0.0, 0.0, 1.0]];
        // A goods actor's turn sits outside its animation pose, so a quarter
        // turn about X sends the end reaching along +Z down under it.
        let (low, high) = hull(&rod).span(&quarter);
        assert!((low + 1.0).abs() < 1e-6);
        assert!((high - 0.2).abs() < 1e-6);
        // The pose's lift is turned with the model, not left upright.
        let back = Hull::posed(
            Kind::Mesh,
            &rod,
            &[frame([1.0; 3], [0.0; 3], [0.0, 0.0, 0.5])],
        );
        let (low, high) = back.span(&quarter);
        assert!((low + 1.5).abs() < 1e-6);
        assert!((high + 0.3).abs() < 1e-6);
        // A pose's own turn runs first: standing the rod up along −Y, a
        // placement's half turn about Z then flips it to +Y.
        let stood = Hull::posed(
            Kind::Mesh,
            &rod,
            &[frame([1.0; 3], [FRAC_PI_2, 0.0, 0.0], [0.0; 3])],
        );
        assert!((stood.lo(&UPRIGHT) + 1.0).abs() < 1e-6);
        assert!((stood.lo(&euler([0.0, 0.0, PI])) + 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_pose_scales_turns_and_lifts_and_a_clip_takes_every_frame() {
        let rod = [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let still = frame([0.5; 3], [-FRAC_PI_2, 0.0, 0.0], [0.0, 0.1, 0.0]);
        let h = Hull::posed(Kind::Mesh, &rod, &[still]);
        assert!((h.span(&UPRIGHT).1 - 0.6).abs() < 1e-6);
        // A bob: the same pose 0.2 m lower on another frame.
        let low = frame([0.5; 3], [-FRAC_PI_2, 0.0, 0.0], [0.0, -0.1, 0.0]);
        let h = Hull::posed(Kind::Mesh, &rod, &[still, low]);
        assert!((h.lo(&UPRIGHT) + 0.1).abs() < 1e-6);
        // A child bone's map inside its parent's.
        let lift = frame([1.0; 3], [0.0; 3], [0.0, 1.0, 0.0]);
        let both = Hull::posed(Kind::Mesh, &rod, &[chain(&lift, &still)]);
        assert!((both.lo(&UPRIGHT) - 1.1).abs() < 1e-6);
    }

    #[test]
    fn a_spun_rod_sweeps_its_tipped_circle() {
        // A rod lying along X, 1 m out: spun about Y it sweeps a disc, and
        // tipped 30° about Z that disc spans ±sin 30°.
        let h = hull(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
        let (lo, hi) = h.spun(&euler([0.0, 0.0, 30f32.to_radians()]));
        assert!((lo + 0.5).abs() < 1e-6 && (hi - 0.5).abs() < 1e-6);
        assert_eq!(h.spun(&UPRIGHT), (0.0, 0.0));
    }

    #[test]
    fn an_item_fits_by_its_spins_sweep_from_its_lowest_point() {
        // A 1 m post on the spin's axis sweeps nothing: it fits a 1 m gap.
        let post = hull(&[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        assert!(fits(&post, &UPRIGHT, 1.0) && !fits(&post, &UPRIGHT, 0.9));
        // An arm out along Z, tipped 30° about X, sweeps up past its own top.
        let arm = hull(&[[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        let tipped = euler([30f32.to_radians(), 0.0, 0.0]);
        assert!(!fits(&arm, &tipped, 0.6) && fits(&arm, &tipped, 1.01));
    }

    #[test]
    fn angles_undo_euler() {
        for a in [[0.3f32, 1.0, -0.2], [0.65, 1.012, 0.0], [-1.2, -0.4, 2.0]] {
            let back = angles(&euler(a));
            let (p, q) = (euler(a), euler(back));
            for r in 0..3 {
                for c in 0..3 {
                    assert!((p[r][c] - q[r][c]).abs() < 1e-5, "{a:?} {back:?}");
                }
            }
        }
    }

    #[test]
    fn a_plate_lies_flat_and_a_rod_or_a_post_does_not() {
        assert!(hull(&cube([-0.2, 0.0, -0.2], [0.2, 0.02, 0.2])).flat());
        assert!(!hull(&cube([-0.02, 0.0, -0.5], [0.02, 0.02, 0.5])).flat());
        assert!(!hull(&cube([-0.05, 0.0, -0.05], [0.05, 0.5, 0.05])).flat());
    }

    #[test]
    fn the_table_is_sorted_and_its_rows_are_whole() {
        assert!(MODELS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(POSES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(PLANKS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(MODELS.iter().all(|(.., p)| !p.is_empty()));
        let text = include_str!("heights/table.rs");
        assert!(text.starts_with("//! Written by `zootr probe heights`"));
    }
}
