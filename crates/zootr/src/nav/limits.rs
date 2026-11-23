//! How far Link reaches, from `logic/limits.toml`: each value measured or
//! read from his own parameters, with the evidence for it.

use std::sync::LazyLock;

use anyhow::{Result, ensure};
use serde::Deserialize;

/// The file in `logic/`, and its text.
pub const FILE: &str = "limits.toml";
const TEXT: &str = include_str!("../../logic/limits.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// The share of a limit glitchless logic counts.
    pub margin: f32,
    pub child: Reach,
    pub adult: Reach,
    pub range: Range,
}

/// How far each weapon reaches from Link, keyed by its ability word.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Range {
    pub slingshot: Limit,
    pub bow: Limit,
    pub boomerang: Limit,
    pub hookshot: Limit,
    pub longshot: Limit,
    pub dins_fire: Limit,
}

impl Range {
    /// Each weapon with its ability word, the hookshot before the longshot.
    fn all(&self) -> [(&'static str, &Limit); 6] {
        [
            ("slingshot", &self.slingshot),
            ("bow", &self.bow),
            ("boomerang", &self.boomerang),
            ("hookshot", &self.hookshot),
            ("longshot", &self.longshot),
            ("dins-fire", &self.dins_fire),
        ]
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Reach {
    pub step_up: Limit,
    pub drop: Limit,
    pub level_jump: Option<Limit>,
    pub edge_jump: Option<Limit>,
    /// How far from each kind of holder he takes it by hand.
    #[serde(default)]
    pub hand: Hand,
}

/// How far from a holder Link takes it by hand, per holder kind: `None`
/// until measured.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Hand {
    pub small_chest: Option<Limit>,
    pub big_chest: Option<Limit>,
    pub pot: Option<Limit>,
}

/// A kind of holder Link takes by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    SmallChest,
    BigChest,
    Pot,
}

impl Holder {
    pub fn word(self) -> &'static str {
        match self {
            Holder::SmallChest => "small chest",
            Holder::BigChest => "big chest",
            Holder::Pot => "pot",
        }
    }
}

impl Hand {
    pub fn get(&self, holder: Holder) -> Option<&Limit> {
        match holder {
            Holder::SmallChest => self.small_chest.as_ref(),
            Holder::BigChest => self.big_chest.as_ref(),
            Holder::Pot => self.pot.as_ref(),
        }
    }

    fn all(&self) -> [Option<&Limit>; 3] {
        [Holder::SmallChest, Holder::BigChest, Holder::Pot].map(|h| self.get(h))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    pub value: f32,
    /// The drop a jump's reach was measured at.
    pub drop: Option<f32>,
    pub evidence: Vec<String>,
}

impl Limits {
    fn parse(text: &str) -> Result<Self> {
        let limits: Limits = toml::from_str(text)?;
        ensure!(
            (0.0..=1.0).contains(&limits.margin),
            "margin {} isn't a share",
            limits.margin
        );
        for (age, reach) in [("child", &limits.child), ("adult", &limits.adult)] {
            let all = [
                Some(&reach.step_up),
                Some(&reach.drop),
                reach.level_jump.as_ref(),
                reach.edge_jump.as_ref(),
            ];
            ensure!(
                (all.into_iter().chain(reach.hand.all()))
                    .flatten()
                    .all(|l| !l.evidence.is_empty()),
                "a {age} limit has no evidence"
            );
        }
        for (word, l) in limits.range.all() {
            ensure!(!l.evidence.is_empty(), "the {word} range has no evidence");
        }
        Ok(limits)
    }

    /// How far from `holder` Link at his age takes it by hand, `None`
    /// until measured.
    pub fn hand(&self, child: bool, holder: Holder) -> Option<f32> {
        let reach = if child { &self.child } else { &self.adult };
        reach.hand.get(holder).map(|l| l.value)
    }

    /// The highest ledge either age gets onto: the graph doesn't know who
    /// walks it, so it takes the lower.
    pub fn step_up(&self) -> f32 {
        self.child.step_up.value.min(self.adult.step_up.value)
    }

    /// The widest level gap glitchless logic links: `margin` of the
    /// lower age's level jump, of those measured. 0 with none.
    pub fn level_jump(&self) -> f32 {
        let both = [&self.child.level_jump, &self.adult.level_jump];
        let least = both.into_iter().flatten().map(|l| l.value).reduce(f32::min);
        least.map_or(0.0, |v| v * self.margin)
    }

    /// The weapons that hit a target `d` m from Link, within `margin` of
    /// their range. The longshot only where the hookshot falls short, as
    /// every longshot is a hookshot too.
    pub fn reaching(&self, d: f32) -> Vec<&'static str> {
        let within = |l: &Limit| d <= l.value * self.margin;
        let hookshot = within(&self.range.hookshot);
        (self.range.all().into_iter())
            .filter(|(w, l)| within(l) && !(*w == "longshot" && hookshot))
            .map(|(w, _)| w)
            .collect()
    }

    /// The deepest drop either age lands well.
    pub fn drop(&self) -> f32 {
        self.child.drop.value.min(self.adult.drop.value)
    }

    /// How far past an edge a running jump lands for a fall of `h` m,
    /// within `margin`: the arc d(h) = L/2 (1 + sqrt(1 + h/u)) through the
    /// child's measured level jump (L at h = 0) and edge jump (at its
    /// drop), the only ages measured. 0 without both.
    pub fn leap(&self, h: f32) -> f32 {
        let (Some(level), Some(edge)) = (&self.child.level_jump, &self.child.edge_jump) else {
            return 0.0;
        };
        let Some(drop) = edge.drop.filter(|d| *d > 0.0) else {
            return 0.0;
        };
        let ratio = 2.0 * edge.value / level.value - 1.0;
        let apex = drop / (ratio * ratio - 1.0);
        level.value / 2.0 * (1.0 + (1.0 + h.max(0.0) / apex).sqrt()) * self.margin
    }
}

/// The limits, read once.
pub fn limits() -> &'static Limits {
    static LIMITS: LazyLock<Limits> =
        LazyLock::new(|| Limits::parse(TEXT).expect("logic/limits.toml"));
    &LIMITS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_loads_and_the_graph_takes_the_lower_age() {
        let l = limits();
        assert_eq!(
            l.step_up(),
            l.child.step_up.value.min(l.adult.step_up.value)
        );
        assert!(l.drop() >= 4.65, "a child lands a measured 4.65 m drop");
    }

    #[test]
    fn a_target_takes_the_weapons_whose_range_reaches_it() {
        let l = limits();
        let at = |d| l.reaching(d);
        assert_eq!(
            at(5.0),
            ["slingshot", "bow", "boomerang", "hookshot", "dins-fire"]
        );
        // The Deku basement vine gold skulltula, past Din's Fire.
        assert_eq!(at(7.7), ["slingshot", "bow", "boomerang", "hookshot"]);
        assert_eq!(at(12.0), ["slingshot", "bow", "longshot"]);
        assert!(at(30.0).is_empty());
    }

    #[test]
    fn a_leap_runs_through_both_measured_jumps() {
        let l = limits();
        let level = l.child.level_jump.as_ref().unwrap().value;
        let edge = l.child.edge_jump.as_ref().unwrap();
        assert!((l.leap(0.0) - level * l.margin).abs() < 1e-4);
        assert!((l.leap(edge.drop.unwrap()) - edge.value * l.margin).abs() < 1e-4);
        // A 2.1 m fall: about 3.6 m.
        assert!((3.5..3.75).contains(&l.leap(2.1)), "{}", l.leap(2.1));
    }

    #[test]
    fn hand_reach_is_per_holder_and_age_and_none_until_measured() {
        let l = limits();
        assert_eq!(l.hand(true, Holder::SmallChest), Some(1.25));
        assert_eq!(l.hand(true, Holder::BigChest), Some(1.35));
        assert_eq!(l.hand(false, Holder::BigChest), None);
        // A hand reach needs its evidence like any limit. A Windows
        // checkout ends lines with CRLF.
        let text = TEXT.replace("\r\n", "\n");
        let bare = text.replacen(
            "evidence = [\n    \"measured: Kakariko pot, +90, 180 and -90 degrees: shown 1.04, 1.05, 1.10 (run-pot-child2)\",\n]",
            "evidence = []",
            1,
        );
        assert_ne!(bare, text);
        assert!(Limits::parse(&bare).is_err());
    }

    #[test]
    fn a_limit_without_evidence_is_refused() {
        let bare = TEXT.replacen(
            "evidence = [\"static: StartJumpClamberHeight 0.99\"]",
            "evidence = []",
            1,
        );
        assert!(
            Limits::parse(&bare)
                .unwrap_err()
                .to_string()
                .contains("no evidence")
        );
    }
}
