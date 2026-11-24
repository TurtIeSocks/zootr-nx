//! The target's control socket: one JSON line a request, its reply matched by `id`.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use super::Memory;

/// How long a reply may take before the game counts as hung.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// What a bridge error says when the game is gone, not a request
/// refused: no game to connect to, the game closing the bridge, or a reply
/// that never came. `sweep::spot::gone` tells a crash by these.
pub const NOT_LISTENING: &str = "no game is listening";
pub const CLOSED: &str = "the game closed the bridge";
pub const HUNG: &str = "didn't answer";
pub const GONE: [&str; 3] = [NOT_LISTENING, CLOSED, HUNG];

/// The most one memory request moves.
const CHUNK: usize = 1 << 20;
/// The most ranges one `mem_read_many` may name: the target refuses more.
const RANGES: usize = 64;
/// The most requests sent before their replies: the target serves up to 8
/// memory requests a frame, all in the same pass.
const FLIGHT: usize = 8;

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

/// A connection to the running game.
pub struct Bridge {
    conn: Mutex<Conn>,
}

struct Conn {
    lines: BufReader<Stream>,
    out: Stream,
    next: u64,
    /// A reply line cut off by a timeout, finished by the next read.
    pending: String,
}

impl Bridge {
    pub fn connect(path: &Path) -> Result<Self> {
        let out = Stream::connect(path)
            .with_context(|| format!("{NOT_LISTENING} on {}", path.display()))?;
        let lines = BufReader::new(out.try_clone()?);
        let conn = Conn {
            lines,
            out,
            next: 1,
            pending: String::new(),
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
        Ok(self.asks(vec![(cmd, args)], within)?.remove(0))
    }

    /// Every one of `asks`, up to [`FLIGHT`] sent before their replies,
    /// each answered within `within`. The replies in `asks`' order, or the
    /// first refusal once all have answered. A `busy` refusal (every memory
    /// slot taken) is asked again until `within` runs out.
    pub fn asks(&self, asks: Vec<(&str, Value)>, within: Duration) -> Result<Vec<Value>> {
        let mut conn = self.conn.lock().unwrap();
        conn.lines.get_ref().set_read_timeout(Some(within))?;
        let start = Instant::now();
        let mut queue: VecDeque<usize> = (0..asks.len()).collect();
        let mut flying = HashMap::new();
        let mut out = vec![Value::Null; asks.len()];
        let mut refused = None;
        loop {
            while flying.len() < FLIGHT
                && let Some(i) = queue.pop_front()
            {
                let (cmd, args) = &asks[i];
                flying.insert(conn.send(cmd, args)?, i);
            }
            let Some(&i) = flying.values().next() else {
                break;
            };
            let reply = conn.reply(asks[i].0, within)?;
            let Some(i) = (reply["id"].as_u64()).and_then(|id| flying.remove(&id)) else {
                continue;
            };
            if reply["ok"] == true {
                out[i] = reply;
                continue;
            }
            let field = |k: &str| reply[k].as_str().unwrap_or_default().to_owned();
            let (error, message) = (field("error"), field("message"));
            if error == "busy" && start.elapsed() < within {
                if flying.is_empty() {
                    std::thread::sleep(Duration::from_millis(2));
                }
                queue.push_front(i);
                continue;
            }
            let cmd = asks[i].0.into();
            refused.get_or_insert(Refused {
                cmd,
                error,
                message,
            });
        }
        match refused {
            Some(r) => Err(r.into()),
            None => Ok(out),
        }
    }

    /// Writes each of `writes`, and reads `reads` in requests of up to
    /// [`RANGES`] ranges and [`CHUNK`] bytes, each read in one pass so its
    /// ranges agree with each other. Every request goes out at once.
    fn exchange(&self, writes: &[(u64, &[u8])], reads: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        let mut asks = Vec::new();
        for &(at, data) in writes {
            for (i, part) in data.chunks(CHUNK).enumerate() {
                let at = at + (i * CHUNK) as u64;
                asks.push(("mem_write", json!({"addr": address(at), "hex": hex(part)})));
            }
        }
        let wrote = asks.len();
        let parts = parts(reads);
        for part in &parts {
            asks.push(match part[..] {
                [(_, at, size)] => ("mem_read", json!({"addr": address(at), "size": size})),
                _ => {
                    let ranges: Vec<Value> = (part.iter())
                        .map(|&(_, at, size)| json!({"addr": address(at), "size": size}))
                        .collect();
                    ("mem_read_many", json!({ "ranges": ranges }))
                }
            });
        }
        let replies = self.asks(asks, PATIENCE)?;
        let mut out = vec![Vec::new(); reads.len()];
        for (part, reply) in parts.iter().zip(&replies[wrote..]) {
            let got = match reply["ranges"].as_array() {
                Some(ranges) => ranges.iter().collect(),
                None => vec![reply],
            };
            ensure!(
                got.len() == part.len(),
                "the game read {} of {} ranges",
                got.len(),
                part.len()
            );
            for (&(i, _, size), r) in part.iter().zip(got) {
                let data = unhex(r["data"].as_str().unwrap_or_default())?;
                ensure!(
                    data.len() == size,
                    "the game read {} of {size} bytes",
                    data.len()
                );
                out[i].extend(data);
            }
        }
        Ok(out)
    }
}

impl Conn {
    /// Sends `cmd` with `args`, saying its `id`.
    fn send(&mut self, cmd: &str, args: &Value) -> Result<u64> {
        let id = self.next;
        self.next += 1;
        let mut line = match args {
            Value::Null => json!({}),
            Value::Object(_) => args.clone(),
            _ => bail!("{cmd}'s arguments aren't an object"),
        };
        line["id"] = id.into();
        line["cmd"] = cmd.into();
        writeln!(self.out, "{line}")?;
        Ok(id)
    }

    /// The next line the game sends, waiting on `cmd` for at most `within`.
    /// A timeout keeps what arrived of the line in `pending`, so the stream
    /// stays in step.
    fn reply(&mut self, cmd: &str, within: Duration) -> Result<Value> {
        match self.lines.read_line(&mut self.pending) {
            Ok(0) => bail!(CLOSED),
            Err(e) if is_timeout(&e) => {
                let raw = anyhow::anyhow!("the game {HUNG} {cmd} in {within:?}");
                Err(raw.context(crate::plain::Plain(super::SLOW.into())))
            }
            Err(e) => Err(e.into()),
            Ok(_) => {
                let text = std::mem::take(&mut self.pending);
                serde_json::from_str(&text).with_context(|| format!("not a reply: {}", text.trim()))
            }
        }
    }
}

/// `reads` cut into requests: each a list of (read, address, size), at
/// most [`RANGES`] long and [`CHUNK`] bytes, a read past the cap in pieces.
/// An empty read isn't sent.
fn parts(reads: &[(u64, usize)]) -> Vec<Vec<(usize, u64, usize)>> {
    let mut parts = vec![Vec::new()];
    let mut bytes = 0;
    for (i, &(at, len)) in reads.iter().enumerate() {
        for start in (0..len).step_by(CHUNK) {
            let size = CHUNK.min(len - start);
            let part = parts.last().unwrap();
            if part.len() == RANGES || bytes + size > CHUNK {
                parts.push(Vec::new());
                bytes = 0;
            }
            parts.last_mut().unwrap().push((i, at + start as u64, size));
            bytes += size;
        }
    }
    parts.retain(|p| !p.is_empty());
    parts
}

impl Memory for Bridge {
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        Ok(self.exchange(&[], &[(at, len)])?.remove(0))
    }

    fn write(&self, at: u64, data: &[u8]) -> Result<()> {
        self.exchange(&[(at, data)], &[]).map(drop)
    }

    fn read_many(&self, ranges: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        self.exchange(&[], ranges)
    }

    fn batch(&self, writes: &[(u64, &[u8])], reads: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        self.exchange(writes, reads)
    }
}

/// An address as a string, since JSON numbers stop being exact past 2^53.
fn address(at: u64) -> String {
    format!("{at:#x}")
}

const DIGITS: &[u8; 16] = b"0123456789abcdef";

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(DIGITS[usize::from(b >> 4)].into());
        out.push(DIGITS[usize::from(b & 0xf)].into());
    }
    out
}

/// Bytes from the game's hex, taken a byte at a time so nothing in the
/// text can panic it.
fn unhex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        bail!("odd hex from the game: {text:.40}");
    }
    let nibble = |c: u8| (c as char).to_digit(16).context("bad hex from the game");
    let (pairs, _) = text.as_bytes().as_chunks::<2>();
    (pairs.iter())
        .map(|&[hi, lo]| -> Result<u8> { Ok(((nibble(hi)? << 4) | nibble(lo)?) as u8) })
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

#[cfg(windows)]
type Stream = super::pipe::Pipe;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::thread;

    use super::*;

    /// A fake game on a fresh socket: `answer` turns each request into its
    /// reply lines, or none to stay silent.
    fn fake(answer: impl Fn(&Value) -> Vec<Value> + Send + 'static) -> Bridge {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("zootr-bridge-{}-{n}", std::process::id());
        let (path, accept) = listen(&name);
        thread::spawn(move || {
            let (stream, mut out) = accept();
            for line in BufReader::new(stream).lines() {
                let req: Value = serde_json::from_str(&line.unwrap()).unwrap();
                for reply in answer(&req) {
                    writeln!(out, "{reply}").unwrap();
                }
            }
        });
        let bridge = Bridge::connect(&path).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(&path).unwrap();
        bridge
    }

    /// A fresh place for a fake game to listen, and what waits for its one
    /// client: the stream to read requests from and the one to answer on.
    type Accept<R, W> = Box<dyn FnOnce() -> (R, W) + Send>;

    #[cfg(unix)]
    fn listen(
        name: &str,
    ) -> (
        std::path::PathBuf,
        Accept<std::os::unix::net::UnixStream, std::os::unix::net::UnixStream>,
    ) {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let accept = move || {
            let (stream, _) = listener.accept().unwrap();
            (stream.try_clone().unwrap(), stream)
        };
        (path, Box::new(accept))
    }

    #[cfg(windows)]
    fn listen(name: &str) -> (std::path::PathBuf, Accept<std::fs::File, std::fs::File>) {
        let server = super::super::pipe::tests::serve(name);
        let accept = move || {
            let server = server.accept();
            (server.try_clone().unwrap(), server)
        };
        (name.into(), Box::new(accept))
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
    fn reads_many_in_requests_the_game_takes() {
        let sizes = Arc::new(Mutex::new(Vec::new()));
        let seen = sizes.clone();
        let game = fake(move |req| {
            let Some(ranges) = req["ranges"].as_array() else {
                seen.lock().unwrap().push(1);
                return vec![ok(req, json!({"data": "07"}))];
            };
            seen.lock().unwrap().push(ranges.len());
            let data = vec![json!({"data": "07"}); ranges.len()];
            vec![ok(req, json!({ "ranges": data }))]
        });
        let ranges: Vec<_> = (0..RANGES as u64 + 1).map(|at| (at, 1)).collect();
        let got = game.read_many(&ranges).unwrap();
        assert_eq!(got.len(), RANGES + 1);
        assert_eq!(*sizes.lock().unwrap(), [RANGES, 1]);
    }

    #[test]
    fn reads_many_under_the_game_s_byte_cap() {
        let sizes = Arc::new(Mutex::new(Vec::new()));
        let seen = sizes.clone();
        let game = fake(move |req| {
            let sized = |r: &Value| r["size"].as_u64().unwrap() as usize;
            let Some(ranges) = req["ranges"].as_array() else {
                seen.lock().unwrap().push(vec![sized(req)]);
                return vec![ok(req, json!({"data": "07".repeat(sized(req))}))];
            };
            seen.lock()
                .unwrap()
                .push(ranges.iter().map(sized).collect::<Vec<_>>());
            let data: Vec<_> = (ranges.iter())
                .map(|r| json!({"data": "07".repeat(sized(r))}))
                .collect();
            vec![ok(req, json!({ "ranges": data }))]
        });
        let half = CHUNK / 2;
        let got = game
            .read_many(&[(0, half), (0, 0), (0, half), (0, 1)])
            .unwrap();
        assert_eq!(
            got.iter().map(Vec::len).collect::<Vec<_>>(),
            [half, 0, half, 1]
        );
        // An empty range isn't sent, and a request stops at the cap.
        assert_eq!(*sizes.lock().unwrap(), [vec![half, half], vec![1]]);
    }

    #[test]
    fn keeps_the_game_s_flight_full_and_matches_replies_out_of_order() {
        const ASKS: usize = 20;
        let held = Arc::new(Mutex::new((Vec::new(), 0)));
        let game = fake(move |req| {
            let (stash, got) = &mut *held.lock().unwrap();
            stash.push(req.clone());
            *got += 1;
            assert!(stash.len() <= FLIGHT, "{} in flight", stash.len());
            if stash.len() < FLIGHT && *got < ASKS {
                return vec![];
            }
            let replies = stash.drain(..).rev();
            replies.map(|r| ok(&r, json!({"n": r["n"]}))).collect()
        });
        let asks = (0..ASKS).map(|n| ("status", json!({ "n": n }))).collect();
        let got = game.asks(asks, PATIENCE).unwrap();
        let ns: Vec<_> = got.iter().map(|r| r["n"].as_u64().unwrap()).collect();
        assert_eq!(ns, (0..ASKS as u64).collect::<Vec<_>>());
    }

    #[test]
    fn a_busy_game_is_asked_again() {
        let tries = Arc::new(AtomicU32::new(0));
        let seen = tries.clone();
        let game = fake(move |req| match seen.fetch_add(1, Ordering::Relaxed) {
            0 => vec![json!({"id": req["id"], "ok": false, "error": "busy"})],
            _ => vec![ok(req, json!({"data": "2a"}))],
        });
        assert_eq!(game.read(0, 1).unwrap(), [0x2a]);
        assert_eq!(tries.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_batch_sends_its_writes_and_reads_together() {
        let cmds = Arc::new(Mutex::new(Vec::new()));
        let seen = cmds.clone();
        let game = fake(move |req| {
            seen.lock()
                .unwrap()
                .push(req["cmd"].as_str().unwrap().to_owned());
            match req["cmd"].as_str() {
                Some("mem_write") => vec![ok(req, json!({"wrote": 1}))],
                _ => vec![ok(req, json!({"data": "05"}))],
            }
        });
        let got = Memory::batch(&game, &[(0, &[1]), (8, &[2])], &[(4, 1)]).unwrap();
        assert_eq!(got, [vec![5]]);
        assert_eq!(
            *cmds.lock().unwrap(),
            ["mem_write", "mem_write", "mem_read"]
        );
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
        assert!(
            format!("{err:#}").contains(&format!("{HUNG} status")),
            "{err:#}"
        );
        assert_eq!(err.to_string(), super::super::SLOW);
    }

    #[test]
    fn a_game_over_the_bridge_reads_its_global_at_main_plus_gamedata() {
        let global = super::super::NATIVE_MAIN + u64::from(crate::patch::OFFSETS.got.gamedata);
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
        assert!(unhex("+f").is_err());
        assert!(unhex("0\u{e9}0").is_err());
        assert_eq!(unhex("AbcD").unwrap(), [0xab, 0xcd]);
    }

    #[test]
    fn a_reply_cut_off_by_a_timeout_is_finished_by_the_next_read() {
        let name = format!("zootr-bridge-torn-{}", std::process::id());
        let (path, accept) = listen(&name);
        thread::spawn(move || {
            let (stream, mut out) = accept();
            let mut lines = BufReader::new(stream).lines();
            let mut next =
                || serde_json::from_str::<Value>(&lines.next().unwrap().unwrap()).unwrap();
            let line = ok(&next(), json!({"n": 1})).to_string();
            let (head, tail) = line.split_at(line.len() / 2);
            write!(out, "{head}").unwrap();
            out.flush().unwrap();
            let second = next();
            writeln!(out, "{tail}").unwrap();
            writeln!(out, "{}", ok(&second, json!({"n": 2}))).unwrap();
        });
        let game = Bridge::connect(&path).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(&path).unwrap();
        let quick = Duration::from_millis(100);
        assert!(game.ask("status", Value::Null, quick).is_err());
        // The first reply, finished now, is skipped by its id.
        assert_eq!(game.request("status", Value::Null).unwrap()["n"], 2);
    }
}
