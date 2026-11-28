//! Hangs: a load that never ends, a black screen that stays, a game that
//! stops answering or quits. Every hung load players met (#425, #469) went
//! the same way: the triforce stops spinning and disappears, and the
//! screen stays pure black (maintainer, 2026-11-25). `play::watch::Stall`
//! counts a load as never stuck, so it can't see one.

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;

use crate::dev::{Dev, State};
use crate::live::Memory;
use crate::play::Input;

/// How long each sign may last before it's a hang.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Pure black, no triforce.
    pub black: Duration,
    /// Loading, an event or not yet on the map asked for; [`SETTLED`]
    /// past it, never settled.
    pub load: Duration,
}

/// How long Link must stand in play, the screen lit, for a load to count.
const SETTLED: Duration = Duration::from_secs(3);
/// How often a frame is taken and the game read.
const TICK: Duration = Duration::from_millis(400);
/// How long reads may fail, or frames not come, in a row before the game
/// counts as hung.
const DEAD: Duration = Duration::from_secs(5);

/// A channel's level at or under which a pixel is black. Encoders leave
/// pure black at 0 to 2. A dark room's walls sit only a little above, but
/// its lit pixels number far more than [`LIT`].
const DARK: u8 = 24;
/// How long an event plays on the map asked for before A presses
/// through it.
const TALK: Duration = Duration::from_secs(2);

/// How many lit pixels a frame may have and still be black. The triforce
/// covers thousands; a black frame has none.
const LIT: usize = 64;

/// How a hang showed.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum Hang {
    /// Pure black, no triforce: `secs` of black over `frames` frames, `map`
    /// the last map read (a hung game may answer none after it), empty when
    /// no read worked. Few frames
    /// over many seconds mean slow shots: a game starved of its GPU (a
    /// model loaded beside it) loads slowly too.
    Black { secs: f32, frames: u32, map: String },
    /// Still loading after the load limit. `secs` here and below count
    /// from the watch's start, just after the teleport answered.
    Loading { secs: f32, map: String },
    /// Settled on another map than the one asked for.
    WrongMap { map: String, want: String },
    /// An event played through the load limit.
    Event { secs: f32, map: String },
    /// On the map asked for, never lit in play for [`SETTLED`] in a row by
    /// [`SETTLED`] past the load limit.
    Unsettled { secs: f32, map: String },
    /// The game refused the teleport, or never answered it.
    Refused { why: String },
    /// Reads of the game failed, or no frame came, for [`DEAD`].
    Unread { why: String },
    /// The game quit: what its log says ended it.
    Crashed { why: String },
}

/// Whether `png` is pure black: no more than [`LIT`] pixels with any colour
/// channel over [`DARK`].
fn is_black(png: &[u8]) -> Result<bool> {
    let (buf, info) = crate::vision::frame8(&mut crate::vision::reader8(png)?)?;
    let samples = info.color_type.samples();
    // Alpha says nothing of light: only the colour channels count.
    let colours = match info.color_type {
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha => samples - 1,
        _ => samples,
    };
    let mut lit = (buf[..info.buffer_size()].chunks_exact(samples))
        .filter(|px| px[..colours].iter().any(|&c| c > DARK));
    Ok(lit.nth(LIT).is_none())
}

/// How a watched load went.
pub struct Watched {
    /// Seconds from the watch's start (just after the teleport) to its end.
    pub secs: f32,
    /// The last frame that came, kept as evidence.
    pub frame: Option<PathBuf>,
    /// How it hung, or `None` once Link stood settled.
    pub hang: Option<Hang>,
}

/// Watch the game after a teleport to `want` until Link stands on it in
/// play, the screen lit, for [`SETTLED`], or until a hang, pressing A
/// through arrival text. Frames go to `frame`, the newest kept.
pub fn settle<M: Memory>(
    dev: &Dev<M>,
    input: &mut impl Input,
    want: &str,
    limits: Limits,
    frame: &Path,
) -> Watched {
    // A failed press needs no word here: the run checked the game takes
    // keys, and shots share its control channel, so a dead one shows as
    // Unread.
    let (at, hang) = watch(dev, input, want, limits, frame, |seen, input, at| {
        if seen.talking(at) {
            let _ = input.press("A");
        }
        seen.settled(at)
    });
    Watched {
        secs: at.as_secs_f32(),
        frame: frame.exists().then(|| frame.to_owned()),
        hang,
    }
}

/// After a cell that didn't give its item: whether the game hangs there,
/// black, quit or unread, as it stands now. The window is long enough for
/// a black that starts with it to show, not one that starts later. A stuck
/// text box isn't a hang here: the cell's own verdict tells of it. The
/// frame at `frame` stays only with a hang, as its evidence.
pub fn probe<M: Memory>(
    dev: &Dev<M>,
    input: &mut impl Input,
    limits: Limits,
    frame: &Path,
) -> Option<Hang> {
    // Never past the load limit: only black, quit and unread can show, so
    // no map is wanted.
    let limits = Limits {
        load: Duration::MAX,
        ..limits
    };
    // From the first look, which a slow shot can make late.
    let window = limits.black.max(DEAD) + TICK * 2;
    let mut first = None;
    let done = move |_: &Watch, _: &mut _, at: Duration| at - *first.get_or_insert(at) >= window;
    let hang = watch(dev, input, "", limits, frame, done).1;
    if hang.is_none() {
        _ = std::fs::remove_file(frame);
    }
    hang
}

/// Look at the game every [`TICK`], a frame into `frame` each time, until
/// a hang or until `done` says so. Gives when it stopped and the hang.
fn watch<M: Memory, I: Input>(
    dev: &Dev<M>,
    input: &mut I,
    want: &str,
    limits: Limits,
    frame: &Path,
    mut done: impl FnMut(&Watch, &mut I, Duration) -> bool,
) -> (Duration, Option<Hang>) {
    // Evidence is this watch's own: never a frame an earlier one left.
    _ = std::fs::remove_file(frame);
    let start = Instant::now();
    let mut seen = Watch::default();
    loop {
        // A game that quit gives no frame: tick names the crash at once
        // rather than after a shot's whole timeout.
        let lit = match input.running() {
            true => seen.framed(shoot(input, frame)),
            false => None,
        };
        // After the shot: one that took its whole timeout to fail mustn't
        // count as that long without a frame.
        let at = start.elapsed();
        if let Some(hang) = seen.tick(at, input, dev.state(), lit, want, limits) {
            return (at, Some(hang));
        }
        if done(&seen, input, at) {
            return (at, None);
        }
        sleep(TICK);
    }
}

/// A frame into `to`, and whether it's lit. It's taken beside `to` and
/// moved there only once it came: an older frame can't pass for a new
/// one, and a failed shot leaves the last good frame as evidence and no
/// half-written one beside it.
fn shoot(input: &mut impl Input, to: &Path) -> Result<bool> {
    let next = to.with_extension("next.png");
    if let Err(e) = std::fs::remove_file(&next)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(e.into());
    }
    let lit = (input.shot(&next))
        .and_then(|()| Ok(!is_black(&std::fs::read(&next)?)?))
        .and_then(|lit| {
            std::fs::rename(&next, to)?;
            Ok(lit)
        });
    if lit.is_err() {
        _ = std::fs::remove_file(&next);
    }
    lit
}

/// What the watch has seen so far: when each sign began.
#[derive(Default)]
struct Watch {
    /// Since when every frame was black.
    black: Option<Duration>,
    /// How many black frames since then.
    dark: u32,
    /// Since when no frame came.
    unframed: Option<Duration>,
    /// Why the last frame didn't come.
    no_frame: String,
    /// Since when reads of the game failed.
    unread: Option<Duration>,
    /// Since when an event played on the map asked for.
    event: Option<Duration>,
    /// The map the last read named.
    map: String,
    /// Since when Link stood in play on a lit frame.
    play: Option<Duration>,
}

impl Watch {
    /// One look at `at`: a hang, if this look makes one.
    fn tick(
        &mut self,
        at: Duration,
        input: &impl Input,
        state: Result<State>,
        lit: Option<bool>,
        want: &str,
        limits: Limits,
    ) -> Option<Hang> {
        if !input.running() {
            let why = input.crash().unwrap_or_else(|| "the game quit".into());
            return Some(Hang::Crashed { why });
        }
        let secs = at.as_secs_f32();
        // Play, and an event on the map asked for, last only while every
        // look reaches its own branch: any other return ends them. A
        // missing frame or a failed read keeps their clocks but can't
        // start them.
        let played = self.play.take();
        let evented = self.event.take();
        // Frames first: a black screen is a hang whether or not the game
        // still answers reads. A missing frame neither starts nor ends the
        // black.
        match lit {
            Some(false) => {
                self.black.get_or_insert(at);
                self.dark += 1;
                self.unframed = None;
            }
            Some(true) => (self.black, self.dark, self.unframed) = (None, 0, None),
            None => _ = self.unframed.get_or_insert(at),
        }
        if let Ok(s) = &state {
            self.map.clone_from(&s.map);
        }
        if let Some(since) = self.black.filter(|since| at - *since >= limits.black) {
            let secs = (at - since).as_secs_f32();
            let (frames, map) = (self.dark, self.map.clone());
            return Some(Hang::Black { secs, frames, map });
        }
        if let Some(since) = self.unframed.filter(|since| at - *since >= DEAD) {
            let secs = (at - since).as_secs_f32();
            let why = format!("no frame read for {secs:.1} s: {}", self.no_frame);
            return Some(Hang::Unread { why });
        }
        let state = match state {
            Ok(s) => {
                self.unread = None;
                s
            }
            Err(e) => {
                (self.play, self.event) = (played, evented);
                let since = *self.unread.get_or_insert(at);
                let failed = (at - since).as_secs_f32();
                return (at - since >= DEAD).then(|| Hang::Unread {
                    why: format!("reads failed for {failed:.1} s: {e:#}"),
                });
            }
        };
        let past = at >= limits.load;
        let here = || state.map.clone();
        if state.loading {
            return past.then(|| Hang::Loading { secs, map: here() });
        }
        if state.event && state.map == want {
            self.event = Some(evented.unwrap_or(at));
            return past.then(|| Hang::Event { secs, map: here() });
        }
        // Text on the way elsewhere isn't the arrival's: no A for it, and
        // past the limit it's a mis-arrival, with the map wanted.
        if state.map != want {
            return past.then(|| Hang::WrongMap {
                map: here(),
                want: want.into(),
            });
        }
        self.play = match lit {
            Some(true) => Some(played.unwrap_or(at)),
            Some(false) => None,
            None => played,
        };
        // Settled by now, or never: a screen that keeps flickering dark.
        let unsettled = at >= limits.load.saturating_add(SETTLED) && !self.settled(at);
        unsettled.then(|| Hang::Unsettled { secs, map: here() })
    }

    /// A shot's outcome as [`Watch::tick`] takes it, keeping why it failed.
    fn framed(&mut self, shot: Result<bool>) -> Option<bool> {
        shot.map_err(|e| self.no_frame = format!("{e:#}")).ok()
    }

    /// Whether an event has played on the map asked for since [`TALK`]
    /// before `at`: a text box an arrival opened (a fairy's grace), which
    /// A closes.
    fn talking(&self, at: Duration) -> bool {
        self.event.is_some_and(|since| at - since >= TALK)
    }

    /// Whether Link has stood in play, lit, for [`SETTLED`] by `at`.
    fn settled(&self, at: Duration) -> bool {
        self.play.is_some_and(|since| at - since >= SETTLED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev::Switches;

    fn frame(w: u32, h: u32, paint: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut px = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend(paint(x, y));
            }
        }
        png(w, h, png::ColorType::Rgb, &px)
    }

    /// An 8-bit PNG of `px`, `colour` samples to a pixel.
    fn png(w: u32, h: u32, colour: png::ColorType, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(colour);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(px).unwrap();
        out
    }

    #[test]
    fn pure_black_is_black_and_a_triforce_or_a_dark_room_is_not() {
        assert!(is_black(&frame(320, 180, |_, _| [1, 0, 2])).unwrap());
        // The loading triforce: a small gold patch in a corner.
        let triforce = |x: u32, y: u32| match x > 290 && y > 150 {
            true => [200, 170, 40],
            false => [0, 0, 0],
        };
        assert!(!is_black(&frame(320, 180, triforce)).unwrap());
        // A dark dungeon in play: dim walls and a lit HUD.
        let room = |x: u32, y: u32| match y < 10 && x < 60 {
            true => [220, 30, 30],
            false => [30, 26, 28],
        };
        assert!(!is_black(&frame(320, 180, room)).unwrap());
        // A few stray pixels don't light a black frame.
        let stray = |x: u32, y: u32| if x < 4 && y < 4 { [255; 3] } else { [0; 3] };
        assert!(is_black(&frame(320, 180, stray)).unwrap());
    }

    #[test]
    fn a_frame_is_black_up_to_its_lit_limit() {
        // Grayscale, the arm without alpha: `n` lit pixels on the first rows.
        let grey = |n: usize| {
            let px: Vec<u8> = (0..320 * 180)
                .map(|i| if i < n { 200 } else { 0 })
                .collect();
            png(320, 180, png::ColorType::Grayscale, &px)
        };
        assert!(is_black(&grey(LIT)).unwrap());
        assert!(!is_black(&grey(LIT + 1)).unwrap());
    }

    /// An input that runs, or quit with a log line. Its frames are pure
    /// black, or torn: not a PNG.
    #[derive(Clone, Copy)]
    struct Up {
        running: bool,
        torn: bool,
    }

    /// An input that runs, its frames black.
    const UP: Up = Up {
        running: true,
        torn: false,
    };

    impl Input for Up {
        fn running(&self) -> bool {
            self.running
        }
        fn launch(&mut self, _: Option<&Path>) -> Result<()> {
            Ok(())
        }
        fn keys(&self) -> Result<()> {
            Ok(())
        }
        fn hold(&mut self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn stick(&mut self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn shot(&mut self, to: &Path) -> Result<()> {
            let png = match self.torn {
                true => b"torn".to_vec(),
                false => frame(16, 16, |_, _| [0; 3]),
            };
            Ok(std::fs::write(to, png)?)
        }
        fn crash(&self) -> Option<String> {
            (!self.running).then(|| "exit 139".into())
        }
    }

    fn state(map: &str, loading: bool, event: bool) -> Result<State> {
        Ok(State {
            map: map.into(),
            child: true,
            loading,
            event,
            switches: Switches::default(),
        })
    }

    const LIMITS: Limits = Limits {
        black: Duration::from_secs(2),
        load: Duration::from_secs(45),
    };

    fn secs(s: f32) -> Duration {
        Duration::from_secs_f32(s)
    }

    #[test]
    fn a_probe_of_a_black_game_hangs_and_keeps_its_frame() {
        let dir = std::env::temp_dir().join(format!("zootr-hang-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let shot = dir.join("hang.png");
        // A game whose reads all fail and whose frames are all black: the
        // black shows at 2 s, before the reads' 5 s.
        let hang = probe(&crate::nav::sim::dev(), &mut { UP }, LIMITS, &shot);
        assert!(matches!(hang, Some(Hang::Black { .. })), "{hang:?}");
        assert!(shot.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_shot_that_fails_keeps_the_last_frame_and_leaves_no_other() {
        let dir = std::env::temp_dir().join(format!("zootr-shoot-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let to = dir.join("frame.png");
        assert!(!shoot(&mut { UP }, &to).unwrap());
        assert!(shoot(&mut Up { torn: true, ..UP }, &to).is_err());
        assert!(is_black(&std::fs::read(&to).unwrap()).unwrap());
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["frame.png"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_seconds_of_black_hang_even_while_loading() {
        let mut w = Watch::default();
        let up = UP;
        let look = |w: &mut Watch, at, lit| {
            w.tick(
                secs(at),
                &up,
                state("FakeMap", true, false),
                lit,
                "FakeMap",
                LIMITS,
            )
        };
        assert_eq!(look(&mut w, 0.0, Some(true)), None);
        assert_eq!(look(&mut w, 1.0, Some(false)), None);
        // A missing frame doesn't end the black.
        assert_eq!(look(&mut w, 2.0, None), None);
        // The black began at 1 s: it's been black 2 s, not 3.
        assert_eq!(
            look(&mut w, 3.0, Some(false)),
            Some(Hang::Black {
                secs: 2.0,
                frames: 2,
                map: "FakeMap".into()
            })
        );
    }

    #[test]
    fn an_event_on_the_map_asked_for_is_talked_through_after_two_seconds() {
        let mut w = Watch::default();
        let up = UP;
        let look = |w: &mut Watch, at, map| {
            w.tick(
                secs(at),
                &up,
                state(map, false, true),
                Some(true),
                "FakeMap",
                LIMITS,
            )
        };
        // An event on the way, on another map, isn't talk.
        assert_eq!(look(&mut w, 0.0, "FakeOther"), None);
        assert_eq!(look(&mut w, 1.0, "FakeMap"), None);
        assert!(!w.talking(secs(2.0)));
        assert!(w.talking(secs(3.0)));
        // Text on the way elsewhere afterwards ends it.
        assert_eq!(look(&mut w, 3.4, "FakeOther"), None);
        assert!(!w.talking(secs(3.4)));
        // So does a load on the map asked for.
        assert_eq!(look(&mut w, 4.0, "FakeMap"), None);
        let load = state("FakeMap", true, false);
        assert_eq!(
            w.tick(secs(6.5), &up, load, Some(true), "FakeMap", LIMITS),
            None
        );
        assert!(!w.talking(secs(6.5)));
    }

    #[test]
    fn an_event_past_the_load_limit_hangs_and_a_failed_read_keeps_its_clock() {
        let mut w = Watch::default();
        let up = UP;
        let event = || state("FakeMap", false, true);
        assert_eq!(
            w.tick(secs(0.0), &up, event(), Some(true), "FakeMap", LIMITS),
            None
        );
        let fail = Err(anyhow::anyhow!("fake read failure"));
        assert_eq!(
            w.tick(secs(1.0), &up, fail, Some(true), "FakeMap", LIMITS),
            None
        );
        // The event's clock ran from 0 s through the failed read.
        assert!(w.talking(secs(2.0)));
        let late = w.tick(secs(46.0), &up, event(), Some(true), "FakeMap", LIMITS);
        assert!(matches!(late, Some(Hang::Event { .. })), "{late:?}");
    }

    #[test]
    fn a_black_screen_hangs_even_when_reads_fail() {
        let mut w = Watch::default();
        let up = UP;
        let fail = || Err(anyhow::anyhow!("fake read failure"));
        assert_eq!(
            w.tick(secs(0.0), &up, fail(), Some(false), "FakeMap", LIMITS),
            None
        );
        let hang = w.tick(secs(2.0), &up, fail(), Some(false), "FakeMap", LIMITS);
        assert!(matches!(hang, Some(Hang::Black { .. })), "{hang:?}");
    }

    #[test]
    fn a_missing_frame_neither_starts_nor_ends_play_and_flicker_never_settles() {
        let mut w = Watch::default();
        let up = UP;
        let look = |w: &mut Watch, at, lit| {
            let state = state("FakeMap", false, false);
            w.tick(secs(at), &up, state, lit, "FakeMap", LIMITS)
        };
        // A shot that failed mid-play keeps the clock from 0 s.
        assert_eq!(look(&mut w, 0.0, Some(true)), None);
        assert_eq!(look(&mut w, 1.0, None), None);
        assert_eq!(look(&mut w, 2.0, Some(true)), None);
        assert!(w.settled(secs(3.0)));
        // Missing frames alone never start it.
        let mut w = Watch::default();
        assert_eq!(look(&mut w, 0.0, None), None);
        assert_eq!(look(&mut w, 1.0, None), None);
        assert!(!w.settled(secs(4.0)));
        // Lit and black in turn, each under the black limit, to the end.
        let mut w = Watch::default();
        let mut hang = None;
        for i in 0..100 {
            let at = i as f32;
            hang = w.tick(
                secs(at),
                &up,
                state("FakeMap", false, false),
                Some(i % 2 == 0),
                "FakeMap",
                LIMITS,
            );
            if hang.is_some() {
                assert_eq!(at, 48.0);
                break;
            }
        }
        assert!(matches!(hang, Some(Hang::Unsettled { .. })), "{hang:?}");
    }

    #[test]
    fn a_watch_with_no_load_limit_never_overflows() {
        let mut w = Watch::default();
        let none = Limits {
            load: Duration::MAX,
            ..LIMITS
        };
        let up = UP;
        let state = || state("FakeMap", false, false);
        assert_eq!(
            w.tick(secs(0.0), &up, state(), Some(true), "FakeMap", none),
            None
        );
        assert_eq!(
            w.tick(secs(9.0), &up, state(), Some(false), "FakeMap", none),
            None
        );
        // Lit again at the end of time: still no sum past MAX.
        let end = Duration::MAX - Duration::from_secs(1);
        assert_eq!(w.tick(end, &up, state(), Some(true), "FakeMap", none), None);
    }

    #[test]
    fn a_frame_with_alpha_ignores_it() {
        let small = |colour, px: &[u8]| png(16, 16, colour, px);
        let rgba = |grey| small(png::ColorType::Rgba, &[grey, grey, grey, 255].repeat(256));
        let grey_alpha = |grey| small(png::ColorType::GrayscaleAlpha, &[grey, 255].repeat(256));
        assert!(is_black(&rgba(0)).unwrap());
        assert!(!is_black(&rgba(200)).unwrap());
        assert!(is_black(&grey_alpha(0)).unwrap());
        assert!(!is_black(&grey_alpha(200)).unwrap());
    }

    #[test]
    fn no_frame_never_settles_and_hangs_after_five_seconds() {
        let mut w = Watch::default();
        let up = UP;
        let look = |w: &mut Watch, at| {
            let lit = w.framed(Err(anyhow::anyhow!("fake shot failure")));
            let state = state("FakeMap", false, false);
            w.tick(secs(at), &up, state, lit, "FakeMap", LIMITS)
        };
        assert_eq!(look(&mut w, 0.0), None);
        assert_eq!(look(&mut w, 4.0), None);
        let hang = look(&mut w, 5.0);
        let why = |w: &str| w.contains("no frame") && w.contains("fake shot failure");
        assert!(
            matches!(&hang, Some(Hang::Unread { why: w }) if why(w)),
            "{hang:?}"
        );
        assert!(!w.settled(secs(5.0)));
    }

    #[test]
    fn black_that_lights_again_is_a_normal_load() {
        let mut w = Watch::default();
        let up = UP;
        let mut look = |at, loading, lit| {
            w.tick(
                secs(at),
                &up,
                state("FakeMap", loading, false),
                lit,
                "FakeMap",
                LIMITS,
            )
        };
        assert_eq!(look(0.0, true, Some(false)), None);
        assert_eq!(look(1.5, true, Some(true)), None);
        assert_eq!(look(3.0, true, Some(false)), None);
        assert_eq!(look(4.0, false, Some(true)), None);
        assert!(!w.settled(secs(5.0)));
        assert_eq!(
            w.tick(
                secs(7.0),
                &up,
                state("FakeMap", false, false),
                Some(true),
                "FakeMap",
                LIMITS
            ),
            None
        );
        assert!(w.settled(secs(7.0)));
    }

    #[test]
    fn a_load_past_its_limit_a_wrong_map_and_a_quit_hang() {
        let up = UP;
        let mut w = Watch::default();
        let late = w.tick(
            secs(46.0),
            &up,
            state("FakeMap", true, false),
            Some(true),
            "FakeMap",
            LIMITS,
        );
        assert!(matches!(late, Some(Hang::Loading { .. })));
        let mut w = Watch::default();
        let wrong = w.tick(
            secs(46.0),
            &up,
            state("FakeOther", false, false),
            Some(true),
            "FakeMap",
            LIMITS,
        );
        assert!(matches!(wrong, Some(Hang::WrongMap { .. })));
        let mut w = Watch::default();
        let text_there = w.tick(
            secs(46.0),
            &up,
            state("FakeOther", false, true),
            Some(true),
            "FakeMap",
            LIMITS,
        );
        assert!(matches!(text_there, Some(Hang::WrongMap { .. })));
        let mut w = Watch::default();
        let quit = w.tick(
            secs(1.0),
            &Up {
                running: false,
                ..UP
            },
            state("FakeMap", false, false),
            Some(true),
            "FakeMap",
            LIMITS,
        );
        assert_eq!(
            quit,
            Some(Hang::Crashed {
                why: "exit 139".into()
            })
        );
    }

    #[test]
    fn reads_failing_five_seconds_hang() {
        let up = UP;
        let mut w = Watch::default();
        let fail = || Err(anyhow::anyhow!("no answer"));
        assert_eq!(
            w.tick(secs(0.0), &up, fail(), Some(true), "FakeMap", LIMITS),
            None
        );
        assert!(matches!(
            w.tick(secs(5.0), &up, fail(), Some(true), "FakeMap", LIMITS),
            Some(Hang::Unread { .. })
        ));
    }
}
