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
