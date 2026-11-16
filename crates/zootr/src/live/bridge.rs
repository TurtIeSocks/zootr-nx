//! The native build's control socket: one JSON line a request, its reply matched by `id`.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::Memory;

/// How long a reply may take before the game counts as hung.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// The most one memory request moves.
const CHUNK: usize = 1 << 20;

/// The game answered a request with an error.
#[derive(Debug)]
pub struct Refused {
    pub cmd: String,
    /// The protocol's code: `refused`, `out_of_range`, `timeout` and so on.
    pub error: String,
    pub message: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "the game refused {}: {}", self.cmd, self.error)?;
        if !self.message.is_empty() {
            write!(f, " ({})", self.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for Refused {}

/// A connection to the running native build.
pub struct Bridge {
    conn: Mutex<Conn>,
}

struct Conn {
    lines: BufReader<Stream>,
    out: Stream,
    next: u64,
}

impl Bridge {
    pub fn connect(path: &Path) -> Result<Self> {
        let out = Stream::connect(path)
            .with_context(|| format!("no game is listening on {}", path.display()))?;
        let lines = BufReader::new(out.try_clone()?);
        let conn = Conn {
            lines,
            out,
            next: 1,
        };
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Whether a game answers on `path`.
    pub fn listening(path: &Path) -> bool {
        Stream::connect(path).is_ok()
    }

    /// `cmd` with `args`, answered within [`PATIENCE`].
    pub fn request(&self, cmd: &str, args: Value) -> Result<Value> {
        self.ask(cmd, args, PATIENCE)
    }

    /// `cmd` with `args` (an object, or null for none), answered within
    /// `within`. The reply's fields, or [`Refused`].
    pub fn ask(&self, cmd: &str, args: Value, within: Duration) -> Result<Value> {
        let mut conn = self.conn.lock().unwrap();
        let id = conn.next;
        conn.next += 1;
        let mut line = match args {
            Value::Null => json!({}),
            Value::Object(_) => args,
            _ => bail!("{cmd}'s arguments aren't an object"),
        };
        line["id"] = id.into();
        line["cmd"] = cmd.into();
        writeln!(conn.out, "{line}")?;
        conn.lines.get_ref().set_read_timeout(Some(within))?;
        loop {
            let mut text = String::new();
            let read = conn.lines.read_line(&mut text);
            match read {
                Ok(0) => bail!("the game closed the bridge"),
                Err(e) if is_timeout(&e) => bail!("the game didn't answer {cmd} in {within:?}"),
                Err(e) => return Err(e.into()),
                Ok(_) => {}
            }
            let reply: Value = serde_json::from_str(&text)
                .with_context(|| format!("not a reply: {}", text.trim()))?;
            if reply["id"] != id {
                continue;
            }
            if reply["ok"] == true {
                return Ok(reply);
            }
            let field = |k: &str| reply[k].as_str().unwrap_or_default().to_owned();
            let (error, message) = (field("error"), field("message"));
            return Err(Refused {
                cmd: cmd.into(),
                error,
                message,
            }
            .into());
        }
    }

    /// Each range's bytes, read in one pass so they agree with each other.
    pub fn read_many(&self, ranges: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        let ranges: Vec<Value> = ranges
            .iter()
            .map(|&(at, len)| json!({"addr": address(at), "size": len}))
            .collect();
        let reply = self.request("mem_read_many", json!({ "ranges": ranges }))?;
        reply["ranges"]
            .as_array()
            .context("mem_read_many gave no ranges")?
            .iter()
            .map(|r| unhex(r["data"].as_str().unwrap_or_default()))
            .collect()
    }
}

impl Memory for Bridge {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        for start in (0..len).step_by(CHUNK) {
            let size = CHUNK.min(len - start);
            let args = json!({"addr": address(at + start as u64), "size": size});
            let reply = self.request("mem_read", args)?;
            out.extend(unhex(reply["data"].as_str().unwrap_or_default())?);
        }
        if out.len() != len {
            bail!("read {} of {len} bytes at {at:#x}", out.len());
        }
        Ok(out)
    }

    fn write(&self, at: u64, data: &[u8]) -> Result<()> {
        for (i, part) in data.chunks(CHUNK).enumerate() {
            let at = at + (i * CHUNK) as u64;
            self.request("mem_write", json!({"addr": address(at), "hex": hex(part)}))?;
        }
        Ok(())
    }
}

/// An address as a string, since JSON numbers stop being exact past 2^53.
fn address(at: u64) -> String {
    format!("{at:#x}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        bail!("odd hex from the game: {text:.40}");
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).context("bad hex from the game"))
        .collect()
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;

#[cfg(not(unix))]
type Stream = Unsupported;

/// Stands in for the socket where there are no Unix sockets.
#[cfg(not(unix))]
pub struct Unsupported;

#[cfg(not(unix))]
impl Unsupported {
    fn connect(_: &Path) -> std::io::Result<Self> {
        Err(std::io::Error::other("the bridge needs Unix sockets"))
    }

    fn try_clone(&self) -> std::io::Result<Self> {
        unreachable!()
    }

    fn set_read_timeout(&self, _: Option<Duration>) -> std::io::Result<()> {
        unreachable!()
    }
}

#[cfg(not(unix))]
impl std::io::Read for Unsupported {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        unreachable!()
    }
}

#[cfg(not(unix))]
impl std::io::Write for Unsupported {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        unreachable!()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        unreachable!()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::thread;

    use super::*;

    /// A fake game on a fresh socket: `answer` turns each request into its
    /// reply lines, or none to stay silent.
    fn fake(answer: impl Fn(&Value) -> Vec<Value> + Send + 'static) -> Bridge {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("zootr-bridge-{}-{n}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let req: Value = serde_json::from_str(&line.unwrap()).unwrap();
                for reply in answer(&req) {
                    writeln!(out, "{reply}").unwrap();
                }
            }
        });
        let bridge = Bridge::connect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        bridge
    }

    fn ok(req: &Value, fields: Value) -> Value {
        let mut reply = json!({"id": req["id"], "ok": true, "frame": 1});
        reply
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        reply
    }

    #[test]
    fn reads_skipping_events_and_other_replies() {
        let game = fake(|req| {
            assert_eq!(req["cmd"], "mem_read");
            assert_eq!(req["addr"], "0x80000000");
            assert_eq!(req["size"], 4);
            vec![
                json!({"event": "present", "frame": 1}),
                json!({"id": 999, "ok": true}),
                ok(req, json!({"data": "2a000001"})),
            ]
        });
        assert_eq!(game.read(0x8000_0000, 4).unwrap(), [0x2a, 0, 0, 1]);
        assert_eq!(game.u32(0x8000_0000).unwrap(), 0x0100_002a);
    }

    #[test]
    fn reads_many_in_one_request() {
        let game = fake(|req| {
            assert_eq!(req["cmd"], "mem_read_many");
            let ranges = req["ranges"].as_array().unwrap();
            assert_eq!(ranges[1], json!({"addr": "0x20", "size": 1}));
            vec![ok(
                req,
                json!({"ranges": [{"data": "0102"}, {"data": "ff"}]}),
            )]
        });
        let got = game.read_many(&[(0x10, 2), (0x20, 1)]).unwrap();
        assert_eq!(got, [vec![1, 2], vec![0xff]]);
    }

    #[test]
    fn writes_hex() {
        let game = fake(|req| {
            assert_eq!(req["cmd"], "mem_write");
            assert_eq!(req["addr"], "0x1000000000");
            assert_eq!(req["hex"], "2a000000");
            vec![ok(req, json!({"wrote": 4}))]
        });
        game.write(0x10_0000_0000, &[0x2a, 0, 0, 0]).unwrap();
    }

    #[test]
    fn an_error_reply_is_refused() {
        let game = fake(|req| {
            vec![json!({"id": req["id"], "ok": false, "error": "forbidden",
                        "message": "memory is off"})]
        });
        let err = game.read(0, 4).unwrap_err();
        let refused = err.downcast_ref::<Refused>().unwrap();
        assert_eq!(
            (refused.cmd.as_str(), refused.error.as_str()),
            ("mem_read", "forbidden")
        );
    }

    #[test]
    fn silence_times_out() {
        let game = fake(|_| vec![]);
        let err = game
            .ask("status", Value::Null, Duration::from_millis(50))
            .unwrap_err();
        assert!(err.to_string().contains("didn't answer status"), "{err}");
    }

    #[test]
    fn a_game_over_the_bridge_reads_its_global_at_main_plus_gamedata() {
        let global = super::super::NATIVE_MAIN + u64::from(crate::patch::OFFSETS.gamedata);
        let game = fake(move |req| {
            assert_eq!(req["addr"], address(global));
            vec![ok(req, json!({"data": "0000001001000000"}))]
        });
        let game = super::super::Game::over(game).unwrap();
        assert_eq!(game.global(), global);
        assert_eq!(game.u64(global).unwrap(), 0x1_1000_0000);
    }

    #[test]
    fn hex_round_trips() {
        let bytes = [0, 1, 0x7f, 0x80, 0xff];
        assert_eq!(unhex(&hex(&bytes)).unwrap(), bytes);
        assert!(unhex("abc").is_err());
    }
}
