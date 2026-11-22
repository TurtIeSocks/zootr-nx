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
                all.into_iter().flatten().all(|l| !l.evidence.is_empty()),
                "a {age} limit has no evidence"
            );
        }
        for (word, l) in limits.range.all() {
            ensure!(!l.evidence.is_empty(), "the {word} range has no evidence");
        }
        Ok(limits)
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
