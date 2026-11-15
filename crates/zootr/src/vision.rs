//! A vision model behind OpenAI's chat API on the paddock: a frame and a
//! prompt in, the model's reply out. The sweep's judge and `zootr look`
//! both ask through it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

/// Where the model is and which one.
pub struct Vision {
    /// The API's root, without `/v1/...`.
    pub url: String,
    /// A file holding the paddock's bearer key, which curl reads itself so
    /// the key never shows in a process list.
    pub key_file: PathBuf,
    pub model: String,
}

/// One question about one frame.
pub struct Ask<'a> {
    pub system: &'a str,
    pub text: &'a str,
    pub png: &'a [u8],
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Vision {
    /// The model's reply to `ask`, as text.
    pub fn ask(&self, ask: &Ask) -> Result<String> {
        let shot = format!("data:image/png;base64,{}", base64(ask.png));
        let body = json!({
            "model": self.model,
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
        });
        let url = format!("{}/v1/chat/completions", self.url);
        let mut tries = 1;
        loop {
            let reply = post(&url, &self.key_file, &body)?;
            match reply.get("error") {
                Some(error) if out_of_memory(error) && tries < TRIES => {
                    tries += 1;
                    std::thread::sleep(RETRY_AFTER);
                }
                Some(error) => bail!("{} refused: {error}", self.model),
                None => {
                    let text = reply["choices"][0]["message"]["content"].as_str();
                    return Ok(text.unwrap_or_default().to_owned());
                }
            }
        }
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
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .context("the frame is too big")?
    ];
    let info = reader.next_frame(&mut buf)?;
    let by = info.width.div_ceil(width);
    if by <= 1 {
        return Ok(png.to_vec());
    }
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
/// has, and read the JSON reply. curl reads the bearer key from
/// `key_file` (curl 8.3 or later).
fn post(url: &str, key_file: &Path, body: &Value) -> Result<Value> {
    let key = format!("key@{}", key_file.display());
    let mut child = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "600",
            "-H",
            "content-type: application/json",
            "-H",
            "x-paddock-max-wait: 600s",
        ])
        .args(["--variable", &key])
        .args(["--expand-header", "authorization: Bearer {{key:trim}}"])
        .args(["--data-binary", "@-", url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running curl")?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.to_string().as_bytes())?;
    let out = child.wait_with_output()?;
    ensure!(
        out.status.success(),
        "the vision model didn't answer: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
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
}
