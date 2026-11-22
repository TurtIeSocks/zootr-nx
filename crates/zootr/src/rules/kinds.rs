//! Object kinds: what each dungeon object needs and does, in
//! `logic/baseline/kinds.toml` from `zootr logic derive --kinds`, with
//! `logic/confirmed/kinds.toml` laid over it (room reach spec).

use anyhow::{Context, Result};
use serde::Deserialize;

use super::Expr;
use super::files::always;
use super::rooms::{Entry, overlay, rule, unique};

/// The file's name in each tier's folder.
pub const FILE: &str = "kinds.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KindFile {
    #[serde(default)]
    kind: Vec<Kind>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "derive and audit read them (room reach build step 4)"
)]
pub struct Kind {
    /// The actor name's hash.
    pub hash: u32,
    pub name: String,
    /// What getting past or using one needs.
    pub uses: String,
    pub sets: Sets,
    pub why: String,
    /// Settled by its name, not the game's data: a live test settles it.
    #[serde(default)]
    pub guess: bool,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(skip, default = "always")]
    pub expr: Expr,
}

/// What a kind does once used.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Sets {
    /// Gone, or open: the way past it is free.
    #[serde(rename = "opens")]
    Opens,
    /// Sets a flag its placement's level logic reads.
    #[serde(rename = "flag")]
    Flag,
    /// The hookshot latches onto it.
    #[serde(rename = "target:hookshot")]
    TargetHookshot,
    /// Carries Link: it moves, rises, sinks or floats.
    #[serde(rename = "platform")]
    Platform,
    #[serde(rename = "none")]
    None,
}

impl Sets {
    pub fn word(self) -> &'static str {
        match self {
            Sets::Opens => "opens",
            Sets::Flag => "flag",
            Sets::TargetHookshot => "target:hookshot",
            Sets::Platform => "platform",
            Sets::None => "none",
        }
    }
}

impl Entry for Kind {
    fn key(&self) -> String {
        format!("kind #{:08x}", self.hash)
    }
    fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

fn parse(text: &str) -> Result<Vec<Kind>> {
    let mut kinds = toml::from_str::<KindFile>(text)?.kind;
    unique(&kinds)?;
    for k in &mut kinds {
        k.expr = rule(&k.uses).with_context(|| k.key())?;
    }
    Ok(kinds)
}

/// The kinds, confirmed over baseline, from the room files' list.
pub(super) fn load(files: &[(&str, &str)]) -> Result<Vec<Kind>> {
    let text = |tier: &str| {
        let path = format!("{tier}/{FILE}");
        files.iter().find(|(n, _)| *n == path).map(|(_, t)| *t)
    };
    let mut kinds = match text("baseline") {
        Some(t) => parse(t).context("logic/baseline/kinds.toml")?,
        None => Vec::new(),
    };
    if let Some(t) = text("confirmed") {
        let confirmed = parse(t).context("logic/confirmed/kinds.toml")?;
        overlay(&mut kinds, confirmed).context("logic/confirmed/kinds.toml")?;
    }
    Ok(kinds)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
        [[kind]]
        hash = 0x5e6f7a8b
        name = "Fake Web A 02"
        uses = "explosives or fire"
        sets = "opens"
        why = "full damage from Bomb, Fire"
        [[kind]]
        hash = 0x00000001
        name = "Fake Eye"
        uses = "slingshot"
        sets = "flag"
        why = "its name"
        guess = true
    "#;

    #[test]
    fn a_confirmed_kind_replaces_its_baseline_entry() {
        let confirmed = r#"
            [[kind]]
            hash = 1
            name = "Fake Eye"
            uses = "slingshot or bow"
            sets = "flag"
            why = "live: bow hit it"
            evidence = ["fake-trace"]
        "#;
        let kinds = load(&[
            ("baseline/kinds.toml", BASE),
            ("confirmed/kinds.toml", confirmed),
        ])
        .unwrap();
        let eye = kinds.iter().find(|k| k.hash == 1).unwrap();
        assert_eq!((eye.uses.as_str(), eye.guess), ("slingshot or bow", false));
        assert_eq!(kinds[0].sets, Sets::Opens);
    }

    #[test]
    fn a_confirmed_kind_the_baseline_lacks_is_an_error() {
        let orphan = r#"
            [[kind]]
            hash = 2
            name = "Fake Gone"
            uses = "always"
            sets = "none"
            why = "gone"
            evidence = ["fake-trace"]
        "#;
        let err = load(&[
            ("baseline/kinds.toml", BASE),
            ("confirmed/kinds.toml", orphan),
        ])
        .unwrap_err();
        assert!(format!("{err:#}").contains("no baseline entry"), "{err:#}");
    }
}
