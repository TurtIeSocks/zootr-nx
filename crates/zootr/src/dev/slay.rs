//! Beating enemies for real. A Life written to 0 on an awake enemy runs its
//! own death: its drop, token, key or defeat event. A dormant one (out of
//! its room, a Gold Skulltula by day) resets its Life as it wakes, or sits
//! at 0 and never dies, so the write repeats on a reset and gives up after
//! a while, putting the Life back (`research/enemy-life.md`).

use std::fmt;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;
use serde::Serialize;

use super::Dev;
use crate::enemies::ENEMY;
use crate::live::Memory;
use crate::live::actors::{self, Actor, Life};
use crate::warp;

/// How long an enemy gets to die, and how often it's looked at.
const PATIENCE: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_millis(250);

/// What to beat: the live actor of a placement, or every actor of a name.
#[derive(Debug, PartialEq)]
pub enum Target {
    Placement(u64),
    Name(String),
    /// The actor of a name nearest a point: one seen there just now.
    At(String, [f32; 3]),
}

impl Target {
    pub fn parse(s: &str) -> Self {
        s.parse()
            .map_or_else(|_| Self::Name(s.into()), Self::Placement)
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Placement(h) => write!(f, "placement {h}"),
            Self::Name(n) => f.write_str(n),
            Self::At(n, [x, y, z]) => write!(f, "{n} at {x:.1},{y:.1},{z:.1}"),
        }
    }
}

/// An enemy near Link, with the Life it has left.
#[derive(Debug, Serialize)]
pub struct Foe {
    pub name: String,
    pub distance: f32,
    /// Where it is now.
    pub at: [f32; 3],
    pub left: i32,
    pub most: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fate {
    /// Its own death ran: it left the actor table, or its defeat event
    /// started.
    Died,
    /// Its Life stayed at 0 and it never died, so it's back at full.
    NeverWoke,
}

#[derive(Debug, Serialize)]
pub struct Slain {
    pub name: String,
    pub distance: f32,
    pub fate: Fate,
}

/// A list that prints one line an entry.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Lines<T>(pub Vec<T>);

impl<T: fmt::Display> fmt::Display for Lines<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let lines: Vec<String> = self.0.iter().map(T::to_string).collect();
        f.write_str(&lines.join("\n"))
    }
}

impl fmt::Display for Foe {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Foe {
            name,
            distance,
            at: [x, y, z],
            left,
            most,
        } = self;
        write!(
            f,
            "{name} {distance:.1} m at {x:.1},{y:.1},{z:.1}, life {left}/{most}"
        )
    }
}

impl fmt::Display for Slain {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let fate = match self.fate {
            Fate::Died => "died",
            Fate::NeverWoke => "never woke, life put back",
        };
        write!(f, "{} {:.1} m: {fate}", self.name, self.distance)
    }
}

/// What one look at an enemy after the write says.
#[derive(Debug, PartialEq)]
enum Look {
    Gone,
    /// It woke and its Life reset: write it again.
    Reset(Life),
    Waiting,
}

/// `alive` is whether it's still in the actor table, `event` whether an
/// event plays (a mini-boss's defeat starts one).
fn look(alive: bool, event: bool, life: Option<Life>) -> Look {
    match life {
        _ if !alive || event => Look::Gone,
        Some(l) if l.left > 0 => Look::Reset(l),
        Some(_) => Look::Waiting,
        None => Look::Gone,
    }
}

pub(crate) fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

impl<M: Memory> Dev<M> {
    /// The enemies within `within` of Link that have a Life, nearest first.
    pub fn foes(&self, within: f32) -> Result<Lines<Foe>> {
        let link = self.link()?;
        let mut found = Vec::new();
        for a in actors::actors(&self.game, self.main)? {
            let d = distance(link, a.pos);
            if !a.name.starts_with(ENEMY) || d > within {
                continue;
            }
            if let Some(Life { left, most, .. }) = a.life(&self.game, self.main)? {
                found.push(Foe {
                    name: a.name,
                    distance: d,
                    at: a.pos,
                    left,
                    most,
                });
            }
        }
        found.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        Ok(Lines(found))
    }

    /// Beat `target` within `within` of Link: a placement's actor (the
    /// nearest of its name to its spot), the one nearest a point, or every
    /// actor of a name.
    pub fn slay(&self, romfs: &Romfs, target: &Target, within: f32) -> Result<Lines<Slain>> {
        let link = self.link()?;
        let all = actors::actors(&self.game, self.main)?;
        let (name, spot) = match target {
            Target::Name(name) => (name.clone(), None),
            Target::At(name, at) => (name.clone(), Some(*at)),
            Target::Placement(hash) => {
                let map = self.state()?.map;
                let placed = (warp::placements_in(romfs, &map)?.into_iter())
                    .find(|p| p.hash == Some(*hash))
                    .with_context(|| format!("{map} has no placement {hash}"))?;
                (placed.name, Some(placed.pose.at))
            }
        };
        // NPCs have a Life too, and a write leaves them at 0 until it's put back.
        ensure!(name.starts_with(ENEMY), "{name} isn't an enemy");
        let near = (all.iter()).filter(|a| a.name == name && distance(link, a.pos) <= within);
        let picked: Vec<&Actor> = match spot {
            None => near.collect(),
            Some(at) => (near.min_by(|a, b| distance(a.pos, at).total_cmp(&distance(b.pos, at))))
                .into_iter()
                .collect(),
        };
        let mut lives = Vec::new();
        for a in picked {
            if let Some(life) = a.life(&self.game, self.main)? {
                lives.push((a, life));
            }
        }
        ensure!(
            !lives.is_empty(),
            "no {target} with a Life within {within} m of Link"
        );
        let slain = lives.into_iter().map(|(a, life)| {
            Ok(Slain {
                name: a.name.clone(),
                distance: distance(link, a.pos),
                fate: self.beat(a, life)?,
            })
        });
        Ok(Lines(slain.collect::<Result<_>>()?))
    }

    /// Write 0 to `a`'s Life until it dies, again on each reset. One that
    /// never dies gets its Life back, so it isn't left alive at 0, and so
    /// does one whose fight ends on an error.
    fn beat(&self, a: &Actor, mut life: Life) -> Result<Fate> {
        let fight = self.fight(a, &mut life);
        kept(fight, || {
            (self.game).write(life.left_at(), &life.most.to_le_bytes())
        })
    }

    /// `beat` without the Life put back: `life` follows each reset.
    fn fight(&self, a: &Actor, life: &mut Life) -> Result<Fate> {
        let zero = 0_i32.to_le_bytes();
        self.game.write(life.left_at(), &zero)?;
        let start = Instant::now();
        while start.elapsed() < PATIENCE {
            std::thread::sleep(TICK);
            let table = actors::actors(&self.game, self.main)?;
            let alive = table.iter().any(|b| b.at == a.at && b.name == a.name);
            let now = match alive {
                true => a.life(&self.game, self.main)?,
                false => None,
            };
            match look(alive, self.event()?, now) {
                Look::Gone => return Ok(Fate::Died),
                Look::Reset(reset) => {
                    *life = reset;
                    self.game.write(life.left_at(), &zero)?;
                }
                Look::Waiting => {}
            }
        }
        Ok(Fate::NeverWoke)
    }
}

/// A fight's fate, with the Life put back unless the enemy died: on giving
/// up and on any error, which stays the one reported.
fn kept(fight: Result<Fate>, put_back: impl FnOnce() -> Result<()>) -> Result<Fate> {
    match fight {
        Ok(Fate::Died) => Ok(Fate::Died),
        Ok(Fate::NeverWoke) => put_back().map(|()| Fate::NeverWoke),
        Err(e) => match put_back() {
            Ok(()) => Err(e),
            Err(p) => Err(e.context(format!("and the Life wasn't put back: {p:#}"))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_end_without_a_blank_one() {
        let slain = |fate| Slain {
            name: "Enemy_FakeBat".into(),
            distance: 2.0,
            fate,
        };
        let both = Lines(vec![slain(Fate::Died), slain(Fate::NeverWoke)]);
        let want = "Enemy_FakeBat 2.0 m: died\nEnemy_FakeBat 2.0 m: never woke, life put back";
        assert_eq!(both.to_string(), want);
    }

    #[test]
    fn a_target_is_a_hash_or_a_name() {
        assert_eq!(
            Target::parse("9866222048459639395"),
            Target::Placement(9866222048459639395)
        );
        assert_eq!(
            Target::parse("Enemy_FakeBat"),
            Target::Name("Enemy_FakeBat".into())
        );
    }

    #[test]
    fn the_life_goes_back_unless_the_enemy_died() {
        let back = std::cell::Cell::new(0);
        let put = || {
            back.set(back.get() + 1);
            Ok(())
        };
        assert_eq!(kept(Ok(Fate::Died), put).unwrap(), Fate::Died);
        assert_eq!(back.get(), 0);
        assert_eq!(kept(Ok(Fate::NeverWoke), put).unwrap(), Fate::NeverWoke);
        assert_eq!(back.get(), 1);
        let failed = kept(Err(anyhow::anyhow!("read failed")), put).unwrap_err();
        assert_eq!(back.get(), 2);
        assert_eq!(failed.to_string(), "read failed");
    }

    #[test]
    fn a_failed_put_back_keeps_the_first_error() {
        let failed = kept(Err(anyhow::anyhow!("read failed")), || {
            Err(anyhow::anyhow!("write failed"))
        })
        .unwrap_err();
        assert_eq!(failed.root_cause().to_string(), "read failed");
        assert!(format!("{failed:#}").contains("write failed"));
    }

    #[test]
    fn a_look_tells_dead_from_reset_from_waiting() {
        let life = |left| {
            Some(Life {
                at: 0x10,
                left,
                most: 18,
            })
        };
        assert_eq!(look(false, false, life(0)), Look::Gone);
        assert_eq!(look(true, true, life(0)), Look::Gone);
        assert_eq!(look(true, false, None), Look::Gone);
        assert_eq!(look(true, false, life(18)), Look::Reset(life(18).unwrap()));
        assert_eq!(look(true, false, life(0)), Look::Waiting);
    }
}
