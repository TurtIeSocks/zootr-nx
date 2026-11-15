//! A play script: a TOML file naming its test start and its steps. See
//! `crates/zootr/play/` for some.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use toml::Spanned;

use crate::config::TestSetup;

/// How long `until` and `expect` wait when the step doesn't say.
const WITHIN: f64 = 10.0;
/// How long `hold` and `stick` last when the step doesn't say.
const MS: u64 = 500;

#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub name: String,
    /// A saved test start, by name.
    pub test: Option<String>,
    /// A test start written in the script.
    pub setup: Option<TestSetup>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// Where the step starts in the script, from 1.
    pub line: usize,
    pub kind: Kind,
    /// A screenshot once the step is done.
    pub shot: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Start an event by its EventSetting name.
    Play(String),
    Press(String),
    /// A button, held for milliseconds.
    Hold(String, u64),
    /// A stick direction, held for milliseconds.
    Stick(String, u64),
    Wait(Duration),
    /// Wait for a `NAME=VALUE` line to hold.
    Until(String, Duration),
    /// Each `NAME=VALUE` line holds by the deadline.
    Expect(Vec<String>, Duration),
    /// Only the screenshot.
    Shot,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Kind::Play(event) => write!(f, "play {event}"),
            Kind::Press(button) => write!(f, "press {button}"),
            Kind::Hold(button, ms) => write!(f, "hold {button} for {ms} ms"),
            Kind::Stick(direction, ms) => write!(f, "stick {direction} for {ms} ms"),
            Kind::Wait(d) => write!(f, "wait {} s", d.as_secs_f64()),
            Kind::Until(line, d) => write!(f, "until {line} within {} s", d.as_secs_f64()),
            Kind::Expect(lines, d) => {
                write!(
                    f,
                    "expect {} within {} s",
                    lines.join(", "),
                    d.as_secs_f64()
                )
            }
            Kind::Shot => write!(f, "screenshot"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: String,
    test: Option<String>,
    setup: Option<TestSetup>,
    #[serde(default)]
    step: Vec<Spanned<Raw>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    play: Option<String>,
    press: Option<String>,
    hold: Option<String>,
    stick: Option<String>,
    ms: Option<u64>,
    wait: Option<f64>,
    until: Option<String>,
    expect: Option<Vec<String>>,
    within: Option<f64>,
    #[serde(default)]
    shot: bool,
}

impl Script {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let file: File = toml::from_str(text)?;
        ensure!(
            file.test.is_none() || file.setup.is_none(),
            "a script has a test or a [setup], not both"
        );
        let mut steps = Vec::new();
        for raw in file.step {
            let line = text[..raw.span().start].matches('\n').count() + 1;
            let step = raw.into_inner().step(line);
            steps.push(step.with_context(|| format!("the step at line {line}"))?);
        }
        Ok(Self {
            name: file.name,
            test: file.test,
            setup: file.setup,
            steps,
        })
    }
}

impl Raw {
    fn step(self, line: usize) -> Result<Step> {
        let ms = self.ms.unwrap_or(MS);
        let within = secs(self.within.unwrap_or(WITHIN))?;
        let mut kinds = Vec::new();
        kinds.extend(self.play.map(Kind::Play));
        kinds.extend(self.press.map(Kind::Press));
        kinds.extend(self.hold.map(|b| Kind::Hold(b, ms)));
        kinds.extend(self.stick.map(|d| Kind::Stick(d, ms)));
        kinds.extend(self.wait.map(secs).transpose()?.map(Kind::Wait));
        if let Some(goal) = self.until {
            name_value(&goal)?;
            kinds.push(Kind::Until(goal, within));
        }
        if let Some(goals) = self.expect {
            ensure!(!goals.is_empty(), "expect lists no lines");
            goals.iter().try_for_each(|g| name_value(g).map(drop))?;
            kinds.push(Kind::Expect(goals, within));
        }
        if kinds.is_empty() && self.shot {
            kinds.push(Kind::Shot);
        }
        let mut kinds = kinds.into_iter();
        let (Some(kind), None) = (kinds.next(), kinds.next()) else {
            bail!("a step is one of play, press, hold, stick, wait, until, expect or shot");
        };
        Ok(Step {
            line,
            kind,
            shot: self.shot,
        })
    }
}

fn secs(s: f64) -> Result<Duration> {
    Duration::try_from_secs_f64(s).with_context(|| format!("{s} isn't a time in seconds"))
}

fn name_value(line: &str) -> Result<(&str, &str)> {
    line.split_once('=')
        .filter(|(n, _)| !n.trim().is_empty())
        .with_context(|| format!("{line} isn't NAME=VALUE"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag, flow, scene};

    fn example() -> String {
        format!(
            r#"
name = "Zelda's chain"

[setup]
spawn = "{}/{}"

[[step]]
play = "{}"

[[step]]
until = "{}.{}=true"
within = 20
shot = true

[[step]]
press = "A"

[[step]]
hold = "ZL"
ms = 800

[[step]]
stick = "up-left"

[[step]]
wait = 1.5

[[step]]
shot = true

[[step]]
expect = ["{}.{}=true"]
"#,
            flag::HYRULE_CASTLE,
            field::HYRULE_CASTLE_ENTRANCE,
            flow::GOTO_HYRULE_CASTLE,
            flag::HAVE_PLAYED_EVENT,
            scene::HRL_CASTLE_0050,
            flag::HAVE_PLAYED_EVENT,
            scene::HRL_CASTLE_0060
        )
    }

    #[test]
    fn a_script_reads_its_setup_and_steps() {
        let script = Script::parse(&example()).unwrap();
        assert_eq!(script.name, "Zelda's chain");
        let spawn = script.setup.unwrap().spawn;
        assert_eq!(
            spawn.as_deref(),
            Some(format!("{}/{}", flag::HYRULE_CASTLE, field::HYRULE_CASTLE_ENTRANCE).as_str())
        );
        let kinds: Vec<_> = script.steps.iter().map(|s| s.kind.clone()).collect();
        assert_eq!(
            kinds,
            [
                Kind::Play(flow::GOTO_HYRULE_CASTLE.to_string()),
                Kind::Until(
                    format!(
                        "{}.{}=true",
                        flag::HAVE_PLAYED_EVENT,
                        scene::HRL_CASTLE_0050
                    ),
                    Duration::from_secs(20)
                ),
                Kind::Press("A".into()),
                Kind::Hold("ZL".into(), 800),
                Kind::Stick("up-left".into(), MS),
                Kind::Wait(Duration::from_millis(1500)),
                Kind::Shot,
                Kind::Expect(
                    vec![format!(
                        "{}.{}=true",
                        flag::HAVE_PLAYED_EVENT,
                        scene::HRL_CASTLE_0060
                    )],
                    Duration::from_secs(10)
                ),
            ]
        );
        let lines: Vec<_> = script.steps.iter().map(|s| s.line).collect();
        assert_eq!(lines, [7, 10, 15, 18, 22, 25, 28, 31]);
        assert!(script.steps[1].shot && !script.steps[2].shot);
    }

    #[test]
    fn the_checked_in_scripts_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("play");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let script = Script::load(&path).unwrap();
            assert!(!script.steps.is_empty(), "{}", path.display());
        }
    }

    fn error(text: &str) -> String {
        format!("{:#}", Script::parse(text).unwrap_err())
    }

    #[test]
    fn a_bad_step_names_its_line() {
        let two = "name = \"x\"\n\n[[step]]\npress = \"A\"\n\n[[step]]\npress = \"A\"\nwait = 1\n";
        let err = error(two);
        assert!(err.contains("line 6") && err.contains("one of"), "{err}");
        let none = "name = \"x\"\n[[step]]\nshot = false\n";
        assert!(error(none).contains("line 2"), "{}", error(none));
        let goal = "name = \"x\"\n[[step]]\nuntil = \"Flag\"\n";
        let err = error(goal);
        assert!(
            err.contains("line 2") && err.contains("NAME=VALUE"),
            "{err}"
        );
        let unknown = "name = \"x\"\n[[step]]\npress = \"A\"\npres = \"B\"\n";
        let err = error(unknown);
        assert!(err.contains("line 4") && err.contains("pres"), "{err}");
        let late = "name = \"x\"\n[[step]]\nwait = -1\n";
        assert!(error(late).contains("line 2"), "{}", error(late));
        let both = "name = \"x\"\ntest = \"t\"\n[setup]\nspawn = \"A/B\"\n";
        assert!(error(both).contains("not both"));
    }
}
