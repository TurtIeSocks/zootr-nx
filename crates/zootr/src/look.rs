//! `zootr look`: questions about the game's picture, answered by a vision
//! model, so a player reading the answers needn't read the frame.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::play::Input;
use crate::vision::{self, Ask, Vision};

/// The widest frame the model is shown, as the bench measured it.
const WIDTH: u32 = 960;

/// A question ending in this asks for words instead of yes or no.
pub const IN_WORDS: &str = "?:";

const SYSTEM: &str = r#"You look at one frame of a 3D Zelda game for a player who can't see it. Answer every numbered question. Reply with one JSON object whose keys are the question numbers as strings ("1", "2", ...) and nothing else. A yes or no question takes true or false. A question ending in "?:" takes a short answer in words, or "none" when what it asks about isn't on screen. Say only what you see."#;

/// The answers, by question, and how they were had.
#[derive(Debug, Serialize)]
pub struct Looked {
    pub answers: Map<String, Value>,
    pub model: String,
    pub secs: f32,
    /// The frame, when kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<PathBuf>,
}

/// The questions, numbered one per line.
fn prompt(questions: &[String]) -> String {
    let lines = questions.iter().enumerate();
    let lines = lines.map(|(i, q)| format!("{}: {q}", i + 1));
    lines.collect::<Vec<_>>().join("\n")
}

/// Each question with its answer in `reply`, null where it has none.
fn answers(questions: &[String], reply: &str) -> Map<String, Value> {
    let got: Map<String, Value> = vision::object(reply)
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();
    (questions.iter().enumerate())
        .map(|(i, q)| {
            let a = got.get(&(i + 1).to_string()).cloned();
            (q.clone(), a.unwrap_or(Value::Null))
        })
        .collect()
}

/// Take a frame from `game` and ask `vision` every question about it. The
/// frame lands in `keep` when given, else it's dropped after.
pub fn look(
    vision: &Vision,
    game: &mut dyn Input,
    questions: &[String],
    keep: Option<&Path>,
) -> Result<Looked> {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S%.3f");
    let dir = keep.map_or_else(std::env::temp_dir, Path::to_path_buf);
    std::fs::create_dir_all(&dir)?;
    let frame = dir.join(format!("look-{stamp}.png"));
    let looked = game
        .shot(&frame)
        .and_then(|()| look_at(vision, &frame, questions));
    // A frame left behind in the temp folder never hides the look's own
    // answer or error.
    if keep.is_none() {
        let _ = std::fs::remove_file(&frame);
    }
    let looked = looked?;
    Ok(Looked {
        frame: keep.map(|_| frame),
        ..looked
    })
}

/// Ask `vision` every question about the saved `frame`: a sweep's shot,
/// read without opening it.
pub fn look_at(vision: &Vision, frame: &Path, questions: &[String]) -> Result<Looked> {
    let png = std::fs::read(frame).with_context(|| format!("reading {}", frame.display()))?;
    let png = vision::fit(&png, WIDTH)?;
    let words = questions.iter().any(|q| q.ends_with(IN_WORDS));
    let ask = Ask {
        system: SYSTEM,
        text: &prompt(questions),
        png: &png,
        temperature: 0.0,
        max_tokens: if words { 400 } else { 200 },
    };
    let start = Instant::now();
    let reply = vision.ask(&ask)?;
    Ok(Looked {
        answers: answers(questions, &reply),
        model: vision.model.clone(),
        secs: start.elapsed().as_secs_f32(),
        frame: Some(frame.to_path_buf()),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::vision::Api;

    fn asked() -> Vec<String> {
        vec![
            "Is a text box open?".into(),
            "What does it say?:".into(),
            "Is Link climbing?".into(),
        ]
    }

    #[test]
    fn a_saved_frame_that_is_missing_is_named_before_any_ask() {
        let vision = Vision {
            api: Api::OpenAi,
            url: "http://127.0.0.1:9".into(),
            key_file: PathBuf::from("key"),
            model: "m".into(),
            effort: None,
        };
        let gone = Path::new("frames/none-such.png");
        let err = look_at(&vision, gone, &asked()).unwrap_err();
        assert!(
            format!("{err:#}").contains("frames/none-such.png"),
            "{err:#}"
        );
    }

    #[test]
    fn questions_are_numbered() {
        assert_eq!(
            prompt(&asked()),
            "1: Is a text box open?\n2: What does it say?:\n3: Is Link climbing?"
        );
    }

    #[test]
    fn answers_come_back_by_question_and_a_missing_one_is_null() {
        let reply = r#"Here: {"1": true, "2": "Thank you, brother!"}"#;
        let got = answers(&asked(), reply);
        assert_eq!(
            Value::Object(got),
            json!({
                "Is a text box open?": true,
                "What does it say?:": "Thank you, brother!",
                "Is Link climbing?": null,
            })
        );
        let none = answers(&asked(), "I can't tell");
        assert!(none.values().all(Value::is_null));
    }
}
