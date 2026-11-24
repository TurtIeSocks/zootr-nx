//! Confirmed entries that name a spot on the floor (puzzle links spec):
//! how a room file writes them, and how its links split into room links
//! and spot links.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{Check, Entry, Flag, Knowledge, Link, RoomFile, Spot, Via};
use crate::rules::Expr;
use crate::rules::files::always;

/// A room file as written, before its links split into room links and
/// spot links.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawFile {
    room: u32,
    name: String,
    #[serde(default)]
    link: Vec<RawLink>,
    #[serde(default)]
    check: Vec<Check>,
    #[serde(default)]
    flag: Vec<Flag>,
    #[serde(default)]
    knowledge: Vec<Knowledge>,
    #[serde(default)]
    exit: Vec<Spot>,
    #[serde(default)]
    arrival: Vec<Spot>,
    #[serde(default)]
    floor: Vec<FloorClaim>,
}

/// A link's `to`: a room, or a spot kept as text and parsed after, since
/// an untagged enum drops a variant's own error.
#[derive(Deserialize)]
#[serde(untagged)]
enum To<S> {
    Room(u32),
    Spot(S),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLink {
    from: Option<Point>,
    to: To<String>,
    needs: String,
    via: Via,
    why: String,
    #[serde(default)]
    guess: bool,
    #[serde(default)]
    evidence: Vec<String>,
}

impl TryFrom<RawFile> for RoomFile {
    type Error = anyhow::Error;

    fn try_from(raw: RawFile) -> Result<Self> {
        let (mut link, mut spot_link) = (vec![], vec![]);
        for l in raw.link {
            let RawLink {
                from,
                to,
                needs,
                via,
                why,
                guess,
                evidence,
            } = l;
            let to = match to {
                To::Room(n) => To::Room(n),
                To::Spot(s) => To::Spot(s.parse::<Point>()?),
            };
            match (from, to) {
                (None, To::Room(to)) => link.push(Link {
                    to,
                    needs,
                    via,
                    why,
                    guess,
                    evidence,
                    expr: always(),
                }),
                (Some(from), To::Spot(to)) => spot_link.push(SpotLink {
                    from,
                    to,
                    needs,
                    via,
                    why,
                    guess,
                    evidence,
                    expr: always(),
                }),
                (None, To::Spot(to)) => bail!("link to {to} names no `from` spot"),
                (Some(from), To::Room(to)) => {
                    bail!("link from {from} to room {to}: a spot link names a spot `to`")
                }
            }
        }
        Ok(RoomFile {
            room: raw.room,
            name: raw.name,
            link,
            check: raw.check,
            flag: raw.flag,
            knowledge: raw.knowledge,
            exit: raw.exit,
            arrival: raw.arrival,
            spot_link,
            floor: raw.floor,
        })
    }
}

/// A spot on the floor, written `"x,y,z"`: derive finds its face, room
/// and part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point(pub [f32; 3]);

impl std::str::FromStr for Point {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let xyz: Vec<f32> = (s.split(','))
            .map(|n| n.trim().parse::<f32>())
            .collect::<Result<_, _>>()
            .with_context(|| format!("spot `{s}` isn't x,y,z"))?;
        let [x, y, z] = xyz[..] else {
            bail!("spot `{s}` isn't x,y,z");
        };
        Ok(Point([x, y, z]))
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for Point {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let [x, y, z] = self.0;
        write!(f, "{x},{y},{z}")
    }
}

/// A link between two spots, the first in this file's room.
#[derive(Debug)]
pub struct SpotLink {
    pub from: Point,
    pub to: Point,
    pub needs: String,
    pub via: Via,
    pub why: String,
    pub guess: bool,
    pub evidence: Vec<String>,
    pub expr: Expr,
}

/// Floor walk-joined to `at` that no room had joins this file's room.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorClaim {
    pub at: Point,
    pub why: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

impl Entry for SpotLink {
    fn unproven(&mut self) -> Result<()> {
        self.guess = true;
        Ok(())
    }
    fn key(&self) -> String {
        format!("link from {} to {} by {:?}", self.from, self.to, self.via)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

/// A room's confirmed entries that name a spot. Derive reads them; logic
/// sees them only through the baseline derive writes.
#[derive(Debug, Default)]
pub struct Spots {
    pub link: Vec<SpotLink>,
    /// Flags with `at`.
    pub flag: Vec<Flag>,
    pub floor: Vec<FloorClaim>,
}
