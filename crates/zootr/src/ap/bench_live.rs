//! The bench's live metrics, on the running game with a room up: the
//! client's pass, a check from its flag to the server's confirmation, an
//! item from the server's send to Link's pouch, and CPU with and without
//! the client.

use std::process::Command;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ounce_formats::Romfs;

use super::bench::{PASSES, Sample};
use super::client::{self, Book, Game, Note};
use super::room::{Console, Room, SLOTS};
use super::timing;
use crate::live;
use crate::play::Input;

type Dev = crate::dev::Dev<live::Game>;
use crate::tracker::{Flag, Tracker};

/// How many checks and items a run times.
const EACH: usize = 10;
/// The items sent: refills, which logic never needs and no event follows.
const ITEMS: [&str; 4] = [
    "Deku Nuts (3)",
    "Arrows (5)",
    "Red Rupee (20)",
    "Blue Rupee (5)",
];
/// How long a CPU window samples.
const WINDOW: Duration = Duration::from_secs(30);
/// Between one item and the next, so the last one's notice has closed.
const GAP: Duration = Duration::from_secs(3);

/// The live metrics on `room`, whose Link slot the game runs. `client`
/// runs the client, `writer` writes check flags beside it, and `input`
/// closes the get windows a check's own item opens.
pub fn live(
    romfs: &Romfs,
    room: &Room,
    client: live::Game,
    writer: live::Game,
    input: &mut dyn Input,
) -> Result<Vec<Sample>> {
    let gd = Arc::new(crate::recorder::gamedata(romfs)?);
    let client::Installed { book, mut game, .. } = client::installed(romfs, gd.clone(), client)?;
    let pass = (0..PASSES)
        .map(|_| {
            let start = Instant::now();
            game.checked().map(|_| start.elapsed())
        })
        .collect::<Result<Vec<_>>>()?;
    let writer = crate::dev::Dev::new(writer)?;
    let checks = unchecked(&gd, &book, writer.game())?;
    let pids = Pids::find();
    let idle = pids.as_ref().map(|p| p.cpu(WINDOW)).transpose()?;

    let heard: Mutex<Vec<(Instant, Note)>> = Mutex::new(Vec::new());
    let conn = client::Room {
        server: room.address(),
        slot: SLOTS[0].into(),
        password: None,
    };
    let mut console = room.console()?;
    let (stop, stopped) = channel();
    let timed = thread::scope(|s| {
        let (conn, book, game, heard) = (&conn, &book, &mut game, &heard);
        s.spawn(move || {
            client::run(conn, book, game, &stopped, &mut |n| {
                heard.lock().expect("heard").push((Instant::now(), n))
            })
        });
        let seen = || heard.lock().expect("heard").clone();
        let out = (|| -> Result<_> {
            wait(Duration::from_secs(60), || {
                timing::connected(&seen()).then_some(())
            })
            .context("the client didn't connect")?;
            // What the save already holds goes out on connect first.
            thread::sleep(Duration::from_secs(5));
            let busy = pids.as_ref().map(|p| p.cpu(WINDOW)).transpose()?;
            let sends = checks
                .iter()
                .map(|&flag| send(&writer, flag, &seen))
                .collect::<Result<Vec<_>>>()?;
            let receives = (0..EACH)
                .map(|i| {
                    settle(&writer, input)?;
                    receive(&mut console, ITEMS[i % ITEMS.len()], &seen, &writer)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok((busy, sends, receives))
        })();
        drop(stop);
        out
    });
    let (busy, sends, receives) = timed?;
    // Again once the client has stopped: a window either side of it, so a
    // game still settling after boot doesn't read as the client's cost.
    let after = pids.as_ref().map(|p| p.cpu(WINDOW)).transpose()?;

    let mut out = vec![
        Sample::of("pass-live", pass),
        Sample::of("send", sends),
        Sample::of("receive", receives),
    ];
    if let (Some(idle), Some(busy), Some(after)) = (idle, busy, after) {
        out.push(Sample::value("cpu-game-idle%", idle.game));
        out.push(Sample::value("cpu-game-client%", busy.game));
        out.push(Sample::value("cpu-game-after%", after.game));
        out.push(Sample::value("cpu-client%", busy.us));
    }
    Ok(out)
}

/// Writes check `flag` and times it to the server's confirmation.
fn send(writer: &Dev, flag: u32, seen: &impl Fn() -> Vec<(Instant, Note)>) -> Result<Duration> {
    let start = Instant::now();
    writer.game().set(flag, "true")?;
    let limit = Duration::from_secs(10);
    let heard = || since(&seen(), start);
    let name = wait(limit, || timing::sent_after(&seen(), start)).with_context(|| {
        format!(
            "nothing sent after writing #{flag:08x}. The client heard:\n{}",
            heard()
        )
    })?;
    let done = wait(limit, || timing::confirmed(&seen(), &name, start)).with_context(|| {
        format!(
            "the server never confirmed {name}. The client heard:\n{}",
            heard()
        )
    })?;
    thread::sleep(Duration::from_millis(500));
    Ok(done - start)
}

/// Waits out any event, pressing A while one plays: a check holding Link's
/// own item comes back with a get window, and the client gives nothing
/// while it's open.
fn settle(game: &Dev, input: &mut dyn Input) -> Result<()> {
    let start = Instant::now();
    while game.busy()? {
        if start.elapsed() > Duration::from_secs(20) {
            bail!("the game stayed in an event: {:?}", game.state()?);
        }
        input.press("A")?;
        thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// Sends `item` from the room and times it into Link's pouch.
fn receive(
    console: &mut Console,
    item: &str,
    seen: &impl Fn() -> Vec<(Instant, Note)>,
    game: &Dev,
) -> Result<Duration> {
    let start = Instant::now();
    console.send(SLOTS[0], item)?;
    let done =
        wait(Duration::from_secs(30), || timing::received(&seen(), start)).with_context(|| {
            let state = game
                .state()
                .map_or_else(|e| e.to_string(), |s| format!("{s:?}"));
            format!(
                "{item} never reached Link. The game: {state}. The client heard:\n{}",
                since(&seen(), start)
            )
        })?;
    thread::sleep(GAP);
    Ok(done - start)
}

/// The notes heard from `start` on, one a line.
fn since(heard: &[(Instant, Note)], start: Instant) -> String {
    let lines: Vec<String> = (heard.iter())
        .filter(|(at, _)| *at >= start)
        .map(|(at, n)| format!("  +{} ms {n}", (*at - start).as_millis()))
        .collect();
    match lines.is_empty() {
        true => "  nothing".into(),
        false => lines.join("\n"),
    }
}

/// Polls `ready` until it answers or `limit` passes.
fn wait<T>(limit: Duration, mut ready: impl FnMut() -> Option<T>) -> Result<T> {
    let start = Instant::now();
    loop {
        if let Some(t) = ready() {
            return Ok(t);
        }
        if start.elapsed() > limit {
            bail!("gave up after {} s", limit.as_secs());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// The first [`EACH`] chests of the installed seed that AP placed and Link
/// hasn't opened: their opened flags, in AP id order. Chests only: another
/// holder's flag can start its scene, and Link stays busy in it.
fn unchecked(gd: &crate::gamedata::GameData, book: &Book, game: &live::Game) -> Result<Vec<u32>> {
    let history = crate::history::History::load()?;
    let report = history.installed_report()?;
    let spots = &report.spots;
    let tracker = Tracker::new(gd, spots)?;
    let picks: Vec<(i64, u32)> = (spots.iter())
        .filter(|s| !s.given && crate::checks::is_chest(&s.holder))
        .filter_map(|s| match (tracker.flag(s.hash), book.location(s.hash)) {
            (Some(Flag::Bool(h)), Some(id)) => Some((id, h)),
            _ => None,
        })
        .collect();
    let flags = distinct_flags(picks);
    let set = live::bools(game, game.global(), &flags)?;
    let free: Vec<u32> = (flags.into_iter().zip(set))
        .filter(|(_, on)| !on)
        .map(|(f, _)| f)
        .collect();
    if free.len() < EACH {
        bail!("only {} unchecked checks to write", free.len());
    }
    Ok(free[..EACH].to_vec())
}

/// The flags of `(AP id, flag)` picks in AP id order, each flag once (at its
/// lowest id), so no write repeats one already made.
fn distinct_flags(mut picks: Vec<(i64, u32)>) -> Vec<u32> {
    picks.sort_by_key(|&(id, flag)| (flag, id));
    picks.dedup_by_key(|p| p.1);
    picks.sort();
    picks.into_iter().map(|p| p.1).collect()
}

/// CPU over a window, in percent of one core: the game's and ours.
struct Cpu {
    game: f64,
    us: f64,
}

/// The game's process doing the work, and ours.
struct Pids {
    game: u32,
    us: u32,
}

impl Pids {
    /// `None` where the game's process can't be told apart (not the Mac).
    fn find() -> Option<Self> {
        let out = Command::new("pgrep")
            .args(["-f", "Ocarina of Time.app/Contents/MacOS/"])
            .output()
            .ok()?;
        let pids = String::from_utf8_lossy(&out.stdout);
        // The game runs as two processes: the one doing the work holds the
        // most memory.
        let game = (pids.split_whitespace())
            .filter_map(|p| Some((rss(p)?, p.parse().ok()?)))
            .max()
            .map(|(_, p)| p)?;
        Some(Self {
            game,
            us: std::process::id(),
        })
    }

    fn cpu(&self, window: Duration) -> Result<Cpu> {
        let before = (cpu_time(self.game)?, cpu_time(self.us)?);
        thread::sleep(window);
        let after = (cpu_time(self.game)?, cpu_time(self.us)?);
        let pct = |a: f64, b: f64| (b - a) / window.as_secs_f64() * 100.0;
        Ok(Cpu {
            game: pct(before.0, after.0),
            us: pct(before.1, after.1),
        })
    }
}

fn ps(pid: &str, field: &str) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", field, "-p", pid])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn rss(pid: &str) -> Option<u64> {
    ps(pid, "rss=")?.parse().ok()
}

/// A process's CPU time so far, in seconds.
fn cpu_time(pid: u32) -> Result<f64> {
    let time = ps(&pid.to_string(), "time=").context("ps")?;
    parse_time(&time).with_context(|| format!("ps time {time:?}"))
}

/// `ps`'s `[[D-]H:]M:S.ss` as seconds.
fn parse_time(s: &str) -> Option<f64> {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().ok()?, r),
        None => (0.0, s),
    };
    let secs = rest.split(':').try_fold(0.0, |acc, part| {
        Some(acc * 60.0 + part.parse::<f64>().ok()?)
    })?;
    Some(days * 86400.0 + secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_cpu_times_read_as_seconds() {
        assert_eq!(parse_time("0:01.50"), Some(1.5));
        assert_eq!(parse_time("2:03.25"), Some(123.25));
        assert_eq!(parse_time("1:00:00"), Some(3600.0));
        assert_eq!(parse_time("1-00:00:01"), Some(86401.0));
        assert_eq!(parse_time("soon"), None);
    }

    #[test]
    fn a_shared_flag_is_written_once_even_when_not_adjacent_by_id() {
        assert_eq!(distinct_flags(vec![(3, 7), (1, 7), (2, 9)]), [7, 9]);
    }

    #[test]
    fn wait_answers_once_ready_or_gives_up() {
        let mut n = 0;
        assert_eq!(
            wait(Duration::from_secs(1), || {
                n += 1;
                (n == 3).then_some(n)
            })
            .unwrap(),
            3
        );
        assert!(wait(Duration::from_millis(20), || None::<()>).is_err());
    }
}
