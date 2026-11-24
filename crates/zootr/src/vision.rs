//! A vision model behind its API, Anthropic's Messages or OpenAI's chat
//! shape: a frame and a prompt in, the model's reply out. The sweep's
//! judge and `zootr look` both ask through it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail, ensure};
use clap::ValueEnum;
use serde_json::{Value, json};

/// Which request and reply shape the vision model's API speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Api {
    Anthropic,
    #[value(name = "openai")]
    OpenAi,
}

/// How hard an Anthropic model thinks, sent as `output_config.effort`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    fn name(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}

const ANTHROPIC_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_MODEL: &str = "claude-opus-5-5";
const OPENAI_MODEL: &str = "iq3_xxs";
const ANTHROPIC_VERSION: &str = "2026-10-21";
/// Anthropic's `max_tokens`, whatever [`Ask::max_tokens`] says: thinking
/// counts against it.
const ANTHROPIC_MAX_TOKENS: u32 = 16000;

/// Where the model is and which one.
pub struct Vision {
    pub api: Api,
    /// The API's root, without `/v1/...`.
    pub url: String,
    /// A file holding the API's key, which curl reads itself so the key
    /// never shows in a process list.
    pub key_file: PathBuf,
    pub model: String,
    /// Sent to Anthropic only, and only when given.
    pub effort: Option<Effort>,
}

/// One question about one frame.
pub struct Ask<'a> {
    pub system: &'a str,
    pub text: &'a str,
    pub png: &'a [u8],
    /// OpenAI only: Anthropic's current version rejects it.
    pub temperature: f32,
    /// OpenAI only: Anthropic gets a fixed `ANTHROPIC_MAX_TOKENS`.
    pub max_tokens: u32,
}

impl Vision {
    /// A vision model with the url and model the `api` defaults to filled
    /// in. OpenAI has no default url.
    pub fn new(
        api: Api,
        url: Option<String>,
        model: Option<String>,
        key_file: PathBuf,
        effort: Option<Effort>,
    ) -> Result<Vision> {
        let (default_url, default_model) = match api {
            Api::Anthropic => (Some(ANTHROPIC_URL), ANTHROPIC_MODEL),
            Api::OpenAi => (None, OPENAI_MODEL),
        };
        let url = match url.or(default_url.map(str::to_owned)) {
            Some(url) => url,
            None => bail!("the OpenAI API needs a url"),
        };
        Ok(Vision {
            api,
            url,
            key_file,
            model: model.unwrap_or_else(|| default_model.to_owned()),
            effort,
        })
    }

    /// The model's reply to `ask`, as text.
    pub fn ask(&self, ask: &Ask) -> Result<String> {
        let shot = base64(ask.png);
        let (path, body) = match self.api {
            Api::Anthropic => ("/v1/messages", anthropic_body(self, ask, &shot)),
            Api::OpenAi => ("/v1/chat/completions", openai_body(self, ask, &shot)),
        };
        let url = format!("{}{path}", self.url);
        let mut tries = 1;
        loop {
            let reply = post(self.api, &url, &self.key_file, &body)?;
            match self.api {
                Api::Anthropic => return parse_anthropic(&reply, &self.model),
                Api::OpenAi => match reply.get("error") {
                    Some(error) if out_of_memory(error) && tries < TRIES => {
                        tries += 1;
                        std::thread::sleep(RETRY_AFTER);
                    }
                    _ => return parse_openai(&reply, &self.model),
                },
            }
        }
    }
}

/// Anthropic's Messages body: the frame, then the question.
fn anthropic_body(vision: &Vision, ask: &Ask, base64_png: &str) -> Value {
    let mut body = json!({
        "model": vision.model,
        "max_tokens": ANTHROPIC_MAX_TOKENS,
        "system": ask.system,
        "messages": [
            {"role": "user", "content": [
                {"type": "image", "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": base64_png,
                }},
                {"type": "text", "text": ask.text},
            ]},
        ],
    });
    if let Some(effort) = vision.effort {
        body["output_config"] = json!({"effort": effort.name()});
    }
    body
}

/// OpenAI's chat body: the question, then the frame as a data URL.
fn openai_body(vision: &Vision, ask: &Ask, base64_png: &str) -> Value {
    let shot = format!("data:image/png;base64,{base64_png}");
    json!({
        "model": vision.model,
        "stream": false,
        "max_tokens": ask.max_tokens,
        "temperature": ask.temperature,
        // A thinking model looks for a minute instead of seconds, and
        // sees no better for it here.
        "reasoning_effort": "none",
        "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": ask.system},
            {"role": "user", "content": [
                {"type": "text", "text": ask.text},
                {"type": "image_url", "image_url": {"url": shot}},
            ]},
        ],
    })
}

/// A reply that answered but held no text to read, such as one cut off
/// while thinking. Its message says why.
#[derive(Debug)]
pub struct NoText(String);

impl std::fmt::Display for NoText {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NoText {}

/// The text of an Anthropic reply's first `text` block.
pub(crate) fn parse_anthropic(reply: &Value, model: &str) -> Result<String> {
    if reply["type"] == "error" {
        let message = reply["error"]["message"].as_str().unwrap_or("no message");
        bail!("{model} refused: {message}");
    }
    if reply["stop_reason"] == "refusal" {
        bail!("{model} refused: stop_reason refusal");
    }
    let blocks = reply["content"].as_array().map(Vec::as_slice);
    let text = blocks
        .unwrap_or_default()
        .iter()
        .find(|block| block["type"] == "text")
        .and_then(|block| block["text"].as_str());
    match (text, reply["stop_reason"].as_str()) {
        (Some(text), _) => Ok(text.to_owned()),
        // Only a reply that stopped and held blocks answered at all.
        (None, Some(stop)) if blocks.is_some() => {
            Err(NoText(format!("{model} gave no text: stop_reason {stop}")).into())
        }
        (None, _) => bail!("{model} gave no reply: {reply}"),
    }
}

/// The message of an OpenAI reply's first choice.
fn parse_openai(reply: &Value, model: &str) -> Result<String> {
    if let Some(error) = reply.get("error") {
        bail!("{model} refused: {error}");
    }
    let choice = &reply["choices"][0];
    match choice["message"]["content"].as_str() {
        Some(text) => Ok(text.to_owned()),
        // Only a choice with a message answered at all.
        None if choice["message"].is_object() => {
            let finish = choice["finish_reason"].as_str().unwrap_or("none");
            Err(NoText(format!("{model} gave no content: finish_reason {finish}")).into())
        }
        None => bail!("{model} gave no choices: {reply}"),
    }
}

/// How many times a question is asked while the model is out of memory.
/// The game shares the GPU, so its use moves and a later try often fits.
const TRIES: u32 = 3;
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether the model refused for want of GPU memory, which passes.
fn out_of_memory(error: &Value) -> bool {
    error.to_string().contains("out of memory")
}

/// `png` shrunk by a whole factor to at most `width` pixels wide, each
/// pixel the average of the block it stands for. A full Mac frame is
/// 2560 wide, which a vision model reads slowly or fails on.
pub fn fit(png: &[u8], width: u32) -> Result<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let by = reader.info().width.div_ceil(width);
    if by <= 1 {
        return Ok(png.to_vec());
    }
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .context("the frame is too big")?
    ];
    let info = reader.next_frame(&mut buf)?;
    let (w, h) = (info.width / by, info.height / by);
    let samples = info.color_type.samples();
    let mut out = Vec::with_capacity((w * h) as usize * samples);
    for y in 0..h {
        for x in 0..w {
            for c in 0..samples {
                let block = (0..by).flat_map(|dy| (0..by).map(move |dx| (dx, dy)));
                let sum: u32 = block
                    .map(|(dx, dy)| {
                        let (px, py) = ((x * by + dx) as usize, (y * by + dy) as usize);
                        u32::from(buf[py * info.line_size + px * samples + c])
                    })
                    .sum();
                out.push((sum / (by * by)) as u8);
            }
        }
    }
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, w, h);
    encoder.set_color(info.color_type);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&out)?;
    Ok(encoded)
}

/// The JSON object in `text`: from its first `{` to its last `}`.
pub fn object(text: &str) -> Option<&str> {
    let (a, b) = text.find('{').zip(text.rfind('}'))?;
    text.get(a..=b)
}

/// POST `body` as JSON with curl, which every machine the sweep runs on
/// has, and read the JSON reply. curl reads the key from `key_file`
/// (curl 8.3 or later).
fn post(api: Api, url: &str, key_file: &Path, body: &Value) -> Result<Value> {
    let key = format!("key@{}", key_file.display());
    let version = format!("anthropic-version: {ANTHROPIC_VERSION}");
    let headers: [&str; 4] = match api {
        Api::Anthropic => ["--expand-header", "x-api-key: {{key:trim}}", "-H", &version],
        Api::OpenAi => [
            "--expand-header",
            "authorization: Bearer {{key:trim}}",
            "-H",
            "x-paddock-max-wait: 600s",
        ],
    };
    let mut child = crate::platform::command("curl")
        .args([
            "-sS",
            "--max-time",
            "600",
            "-H",
            "content-type: application/json",
        ])
        .args(["--variable", &key])
        .args(headers)
        .args(["--data-binary", "@-", url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running curl")?;
    // A curl that quits before reading the body says why on stderr, which
    // beats the broken pipe: wait for it either way, and check it first.
    let mut stdin = child.stdin.take().context("curl's stdin")?;
    let sent = stdin.write_all(body.to_string().as_bytes());
    drop(stdin);
    let out = child.wait_with_output()?;
    ensure!(
        out.status.success(),
        "the vision model didn't answer: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    sent.context("sending the request to curl")?;
    serde_json::from_slice(&out.stdout).context("the vision model's reply isn't JSON")
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(match i <= chunk.len() {
                true => ABC[(n >> (18 - 6 * i)) as usize & 63] as char,
                false => '=',
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_running_out_of_memory_is_tried_again() {
        let oom = json!({"message": "cudaMalloc failed: out of memory"});
        assert!(out_of_memory(&oom));
        assert!(!out_of_memory(&json!({"message": "model not found"})));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    fn frame(width: u32, height: u32, rgb: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                data.extend(rgb(x, y));
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&data)
            .unwrap();
        out
    }

    fn size(png: &[u8]) -> (u32, u32) {
        let info = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        (info.info().width, info.info().height)
    }

    #[test]
    fn a_big_frame_shrinks_by_a_whole_factor_and_averages() {
        let checker = frame(2560, 1440, |x, _| {
            if x % 2 == 0 {
                [0, 0, 0]
            } else {
                [200, 100, 50]
            }
        });
        let small = fit(&checker, 960).unwrap();
        assert_eq!(size(&small), (853, 480));
        let mut decoder = png::Decoder::new(std::io::Cursor::new(&small[..]));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        // Columns 0 to 2 hold two black pixels and one colored one.
        assert_eq!(&buf[..3], &[66, 33, 16]);
        let small_already = frame(800, 450, |_, _| [1, 2, 3]);
        assert_eq!(fit(&small_already, 960).unwrap(), small_already);
    }

    #[test]
    fn the_object_is_found_inside_chatter() {
        assert_eq!(
            object(r#"Sure: {"a": {"b": 1}} done"#),
            Some(r#"{"a": {"b": 1}}"#)
        );
        assert_eq!(object("no braces"), None);
    }

    fn vision(api: Api, effort: Option<Effort>) -> Vision {
        Vision {
            api,
            url: String::new(),
            key_file: PathBuf::new(),
            model: "m".into(),
            effort,
        }
    }

    fn ask() -> Ask<'static> {
        Ask {
            system: "be brief",
            text: "what is on screen?",
            png: b"png",
            temperature: 0.2,
            max_tokens: 300,
        }
    }

    #[test]
    fn the_anthropic_body_has_the_image_first_and_no_temperature() {
        let body = anthropic_body(&vision(Api::Anthropic, None), &ask(), "cG5n");
        assert_eq!(body["system"], "be brief");
        assert_eq!(body["max_tokens"], 16000);
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["type"], "base64");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[0]["source"]["data"], "cG5n");
        assert_eq!(
            content[1],
            json!({"type": "text", "text": "what is on screen?"})
        );
        for absent in ["temperature", "thinking", "output_config", "stream"] {
            assert!(body.get(absent).is_none(), "{absent}");
        }
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn effort_is_sent_only_when_given() {
        let body = anthropic_body(&vision(Api::Anthropic, Some(Effort::Low)), &ask(), "");
        assert_eq!(body["output_config"], json!({"effort": "low"}));
    }

    #[test]
    fn the_openai_body_keeps_its_shape() {
        let body = openai_body(&vision(Api::OpenAi, None), &ask(), "cG5n");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "be brief");
        let user = &body["messages"][1]["content"];
        assert_eq!(user[0]["text"], "what is on screen?");
        assert_eq!(user[1]["image_url"]["url"], "data:image/png;base64,cG5n");
        assert_eq!(body["max_tokens"], 300);
        assert!((body["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn an_anthropic_reply_is_its_first_text_block() {
        let reply = json!({"stop_reason": "end_turn", "content": [
            {"type": "thinking", "thinking": "hm"},
            {"type": "text", "text": "first"},
            {"type": "text", "text": "second"},
        ]});
        assert_eq!(parse_anthropic(&reply, "m").unwrap(), "first");
    }

    #[test]
    fn a_reply_with_no_text_is_an_error() {
        let thought = json!({"stop_reason": "max_tokens", "content": [
            {"type": "thinking", "thinking": "hm"},
        ]});
        let err = parse_anthropic(&thought, "m").unwrap_err();
        assert!(err.to_string().contains("max_tokens"), "{err}");
        assert!(err.is::<NoText>());
        let empty = json!({"choices": [{"message": {"content": null}, "finish_reason": "length"}]});
        let err = parse_openai(&empty, "m").unwrap_err();
        assert!(err.to_string().contains("length"), "{err}");
        assert!(err.is::<NoText>());
    }

    #[test]
    fn a_body_that_is_no_reply_is_no_text_to_judge() {
        // A wrong URL or a server error answers with a JSON body too.
        for body in [
            json!({"object": "error", "message": "x"}),
            json!({"detail": "Not Found"}),
            json!({"choices": []}),
        ] {
            let err = parse_openai(&body, "m").unwrap_err();
            assert!(!err.is::<NoText>(), "{body} is no reply");
        }
        for body in [
            json!({"detail": "Not Found"}),
            json!({"stop_reason": "end_turn"}),
            json!({"content": []}),
        ] {
            let err = parse_anthropic(&body, "m").unwrap_err();
            assert!(!err.is::<NoText>(), "{body} is no reply");
        }
    }

    #[test]
    fn an_anthropic_refusal_and_error_are_errors() {
        let refusal = json!({"stop_reason": "refusal", "content": []});
        let err = parse_anthropic(&refusal, "m").unwrap_err();
        assert!(err.to_string().contains("refusal"), "{err}");
        assert!(!err.is::<NoText>(), "a refusal is no reply to judge");
        let error = json!({"type": "error", "error": {"type": "invalid_request_error", "message": "bad key"}});
        let err = parse_anthropic(&error, "m").unwrap_err();
        assert!(err.to_string().contains("bad key"), "{err}");
    }

    #[test]
    fn an_openai_reply_is_its_message_or_its_error() {
        let good = json!({"choices": [{"message": {"content": "hi"}}]});
        assert_eq!(parse_openai(&good, "m").unwrap(), "hi");
        let err = parse_openai(&json!({"error": {"message": "nope"}}), "m").unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn the_defaults_follow_the_api() {
        let key = || PathBuf::from("key");
        let a = Vision::new(Api::Anthropic, None, None, key(), None).unwrap();
        assert_eq!(a.url, "https://api.anthropic.com");
        assert_eq!(a.model, "claude-opus-5-5");
        assert!(Vision::new(Api::OpenAi, None, None, key(), None).is_err());
        let o = Vision::new(Api::OpenAi, Some("http://h".into()), None, key(), None).unwrap();
        assert_eq!((o.url.as_str(), o.model.as_str()), ("http://h", "iq3_xxs"));
        let own = Vision::new(
            Api::Anthropic,
            Some("http://p".into()),
            Some("x".into()),
            key(),
            None,
        )
        .unwrap();
        assert_eq!((own.url.as_str(), own.model.as_str()), ("http://p", "x"));
    }
}
