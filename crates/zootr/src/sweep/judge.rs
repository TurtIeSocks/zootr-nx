//! The judge: a vision model that looks at a spot that didn't resolve by
//! itself and picks the next input, or calls it a bug.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::vision::{self, Ask, Vision};

/// The most turns the judge gets on one spot.
pub const TURNS: usize = 5;
/// How many of the latest lessons ride along in the prompt.
const LESSONS: usize = 40;

/// What the judge is shown.
pub struct Look<'a> {
    /// The spot, as the spoiler names it.
    pub spot: &'a str,
    pub expected: &'a str,
    pub map: &'a str,
    /// An event is playing.
    pub event: bool,
    /// The inputs so far, oldest first.
    pub tried: &'a [String],
    pub shot: &'a Path,
}

/// The judge's answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum Answer {
    Press {
        button: String,
        #[serde(default)]
        why: String,
    },
    Stick {
        direction: String,
        #[serde(default = "step")]
        ms: u64,
        #[serde(default)]
        why: String,
    },
    Wait {
        #[serde(default)]
        why: String,
    },
    /// Play the song the spot teaches, at a song lesson's prompt.
    Play {
        #[serde(default)]
        why: String,
    },
    Bug {
        #[serde(default)]
        why: String,
    },
    /// Not one of the answers above: the raw text, kept for the report.
    #[serde(skip)]
    Unreadable(String),
}

fn step() -> u64 {
    300
}

impl Answer {
    /// The answer in `text`, or [`Answer::Unreadable`] with the text.
    pub fn parse(text: &str) -> Self {
        vision::object(text)
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_else(|| Answer::Unreadable(text.trim().into()))
    }

    /// One line for the report and the lessons.
    pub fn line(&self) -> String {
        match self {
            Answer::Press { button, why } => format!("press {button}: {why}"),
            Answer::Stick { direction, ms, why } => format!("stick {direction} {ms} ms: {why}"),
            Answer::Wait { why } => format!("wait: {why}"),
            Answer::Play { why } => format!("play the song: {why}"),
            Answer::Bug { why } => format!("bug: {why}"),
            Answer::Unreadable(text) => format!("unreadable: {text}"),
        }
    }
}

pub trait Judge {
    fn look(&mut self, look: &Look) -> Result<Answer>;

    /// Remember what worked, for the next spots and the next nights.
    fn learn(&mut self, _lesson: &str) -> Result<()> {
        Ok(())
    }
}

/// A vision model that judges a stalled run from its frame, with the playbook as the
/// system prompt and its lessons file growing as the sweep learns.
pub struct ModelJudge {
    pub vision: Vision,
    pub playbook: PathBuf,
    pub lessons: PathBuf,
}

const RULES: &str = r#"You are watching a test run of a randomizer for The Legend of Zelda: Ocarina of Time on Switch 2. A harness warped Link beside a check and tried to collect it. It stalled, so you pick the next input from the screenshot.

Answer with one JSON object and nothing else. Start it with "see": what is on screen, plainly: is Link visible, and is a chest, a person, a text box or a get-item window there.
{"see":"...","action":"press","button":"A","why":"..."}   buttons: A B X Y L R ZL ZR Plus Minus Up Down Left Right
{"see":"...","action":"stick","direction":"up","ms":300,"why":"..."}   directions: up down left right up-left up-right down-left down-right, moved relative to the camera
{"see":"...","action":"wait","why":"..."}   when something is still animating
{"see":"...","action":"play","why":"..."}   when a staff of notes asks "Play the ...": the harness plays the song, which A alone never passes
{"see":"...","action":"bug","why":"..."}   only for a real fault: a black screen that stays, Link stuck in a wall or the floor, garbled or empty text, a frozen picture. A game over is not a bug: press A to go on.

Name only what you see. "It should give" is what the harness expects, not what is on screen: a get-item window is there only when you see one.
Link not visible: press ZL to bring the camera behind him.
A text box or a get-item window: press A. A choice box: pick the line that buys, accepts or says yes. Link facing away from a chest or a person: turn him with a short stick push toward it, then A."#;

impl ModelJudge {
    fn system(&self) -> String {
        let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
        let lessons = read(&self.lessons);
        let recent: Vec<_> = lessons.lines().rev().take(LESSONS).collect();
        let mut out = format!("{RULES}\n\n{}", read(&self.playbook));
        if !recent.is_empty() {
            out += "\n\nLessons from earlier spots, newest first:\n";
            out += &recent.join("\n");
        }
        out
    }
}

impl Judge for ModelJudge {
    fn look(&mut self, look: &Look) -> Result<Answer> {
        let png = std::fs::read(look.shot)
            .with_context(|| format!("reading the shot {}", look.shot.display()))?;
        let facts = format!(
            "Check: {}\nIt should give: {}\nMap: {}\nAn event is playing: {}\nInputs so far: {}",
            look.spot,
            look.expected,
            look.map,
            look.event,
            if look.tried.is_empty() {
                "none".into()
            } else {
                look.tried.join(", ")
            },
        );
        let ask = Ask {
            system: &self.system(),
            text: &facts,
            png: &png,
            temperature: 0.2,
            max_tokens: 300,
        };
        Ok(Answer::parse(&self.vision.ask(&ask)?))
    }

    fn learn(&mut self, lesson: &str) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.lessons)?;
        writeln!(file, "- {}", lesson.replace('\n', " "))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::Api;

    #[test]
    fn each_answer_parses() {
        let press = Answer::parse(r#"{"action":"press","button":"A","why":"a choice box"}"#);
        assert_eq!(
            press,
            Answer::Press {
                button: "A".into(),
                why: "a choice box".into()
            }
        );
        let stick =
            Answer::parse(r#"Sure: {"see":"Link","action":"stick","direction":"left"} done"#);
        assert_eq!(
            stick,
            Answer::Stick {
                direction: "left".into(),
                ms: 300,
                why: String::new()
            }
        );
        assert_eq!(
            Answer::parse(r#"{"action":"wait"}"#),
            Answer::Wait { why: String::new() }
        );
        let play = Answer::parse(r#"{"action":"play","why":"Play the Nocturne"}"#);
        assert_eq!(play.line(), "play the song: Play the Nocturne");
        let bug = Answer::parse(r#"{"action":"bug","why":"black screen"}"#);
        assert_eq!(bug.line(), "bug: black screen");
    }

    #[test]
    fn a_bad_answer_keeps_its_text() {
        assert_eq!(
            Answer::parse("no idea"),
            Answer::Unreadable("no idea".into())
        );
        let unknown = Answer::parse(r#"{"action":"dance"}"#);
        assert_eq!(unknown, Answer::Unreadable(r#"{"action":"dance"}"#.into()));
    }

    #[test]
    fn the_prompt_carries_the_playbook_and_the_newest_lessons() {
        let dir = std::env::temp_dir().join(format!("zootr-judge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (playbook, lessons) = (dir.join("playbook.md"), dir.join("lessons.md"));
        std::fs::write(&playbook, "Press ZL to recenter the camera.").unwrap();
        let vision = Vision {
            api: Api::Anthropic,
            url: String::new(),
            key_file: PathBuf::new(),
            model: String::new(),
            effort: None,
        };
        let mut judge = ModelJudge {
            vision,
            playbook,
            lessons,
        };
        assert!(!judge.system().contains("Lessons"));
        judge.learn("chests open from the front").unwrap();
        judge.learn("scrubs need rupees").unwrap();
        let system = judge.system();
        assert!(system.contains("recenter the camera"));
        let (a, b) = (
            system.find("scrubs").unwrap(),
            system.find("chests open").unwrap(),
        );
        assert!(a < b, "newest first");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
