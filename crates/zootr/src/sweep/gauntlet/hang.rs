//! Hangs: a load that never ends, a black screen that stays, a game that
//! stops answering or quits. Every hung load players met (#425, #469) went
//! the same way: the triforce stops spinning and disappears, and the
//! screen stays pure black (maintainer, 2026-11-25). `play::watch::Stall`
//! counts a load as never stuck, so it can't see one.

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::dev::Dev;
use crate::play::Input;

/// How long each sign may last before it's a hang.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Pure black, no triforce.
    pub black: Duration,
    /// Loading, or not yet on the map asked for.
    pub load: Duration,
}

/// How long Link must stand in play, the screen lit, for a load to count.
const SETTLED: Duration = Duration::from_secs(3);
/// How often a frame is taken and the game read.
const TICK: Duration = Duration::from_millis(400);
/// How long reads may fail in a row before the game counts as hung.
const DEAD: Duration = Duration::from_secs(5);

/// A channel's level at or under which a pixel is black. Encoders leave
/// pure black at 0 to 2, and a frame's darkest lit wall sits well above.
const DARK: u8 = 24;
/// How long an event plays on the map asked for before A presses
/// through it.
const TALK: Duration = Duration::from_secs(2);

/// How many lit pixels a frame may have and still be black. The triforce
/// covers thousands, even shrunk; a black frame has none.
const LIT: usize = 64;

/// How a hang showed.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum Hang {
    /// Pure black, no triforce: `secs` of black over `frames` frames, `map`
    /// the map read then. Few frames over many seconds mean slow shots: a
    /// game starved of its GPU (a model loaded beside it) loads slowly too.
    Black { secs: f32, frames: u32, map: String },
    /// Still loading after the load limit: `secs` since the teleport.
    Loading { secs: f32, map: String },
    /// Settled on another map than the one asked for.
    WrongMap { map: String, want: String },
    /// An event played through the load limit: `secs` since the teleport.
    Event { secs: f32, map: String },
    /// On the map asked for, never lit in play for 3 s in a row by the
    /// load limit: `secs` since the teleport.
    Unsettled { secs: f32, map: String },
    /// The game refused the teleport, or never answered it.
    Refused { why: String },
    /// Reads of the game failed, or no frame came, for 5 s.
    Unread { why: String },
    /// The game quit: what its log says ended it.
    Crashed { why: String },
}

/// Whether `png` is pure black: no more than [`LIT`] pixels with any colour
/// channel over [`DARK`].
pub fn black(png: &[u8]) -> Result<bool> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let size = reader
        .output_buffer_size()
        .context("the frame is too big")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf)?;
    let samples = info.color_type.samples();
    // Alpha says nothing of light: only the colour channels count.
    let colours = match info.color_type {
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha => samples - 1,
        _ => samples,
    };
    let lit = (buf[..info.buffer_size()].chunks_exact(samples))
        .filter(|px| px[..colours].iter().any(|&c| c > DARK))
        .count();
    Ok(lit <= LIT)
}

/// How a watched load went.
pub struct Watched {
    pub secs: f32,
    /// The last frame taken, kept as evidence.
    pub frame: Option<PathBuf>,
    pub hang: Option<Hang>,
}

/// Watch the game after a teleport to `want` until Link stands on it in
/// play, the screen lit, for [`SETTLED`], or until a hang. Frames go to
/// `frame`, the newest kept.
pub fn settle(
    dev: &Dev,
    input: &mut impl Input,
    want: &str,
    limits: Limits,
    frame: &Path,
) -> Watched {
    let (at, hang) = watch(dev, input, (want, limits), frame, |watch, input, at| {
        if watch.talking(at) {
            let _ = input.press("A");
        }
        watch.settled(at)
    });
    Watched {
        secs: at.as_secs_f32(),
        frame: frame.exists().then(|| frame.to_owned()),
        hang,
    }
}

/// After a cell that didn't give its item: whether the game hung there,
/// black, quit or unread, watched long enough for each to show. A stuck
/// text box isn't a hang here: the cell's own verdict tells of it.
pub fn probe(dev: &Dev, input: &mut impl Input, limits: Limits, frame: &Path) -> Option<Hang> {
    let want = dev.state().map(|s| s.map).unwrap_or_default();
    // Never past the load limit: only black, quit and unread can show.
    let limits = Limits {
        load: Duration::MAX,
        ..limits
    };
    let window = limits.black.max(DEAD) + TICK * 2;
    watch(dev, input, (&want, limits), frame, |_, _, at| at >= window).1
}

/// Look at the game every [`TICK`], a frame into `frame` each time, until
/// a hang or until `done` says so. Gives when it stopped and the hang.
fn watch<I: Input>(
    dev: &Dev,
    input: &mut I,
    (want, limits): (&str, Limits),
    frame: &Path,
    mut done: impl FnMut(&Watch, &mut I, Duration) -> bool,
) -> (Duration, Option<Hang>) {
    let start = Instant::now();
    let mut watch = Watch::default();
    loop {
        let at = start.elapsed();
        let lit = watch.framed(shoot(input, frame));
        if let Some(hang) = watch.tick(at, input, dev.state(), lit, want, limits) {
            return (at, Some(hang));
        }
        if done(&watch, input, at) {
            return (at, None);
        }
        sleep(TICK);
    }
}

/// A frame into `to`, and whether it's lit, with no older frame left at
/// `to` to pass for it when none comes.
fn shoot(input: &mut impl Input, to: &Path) -> Result<bool> {
    match std::fs::remove_file(to) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    input.shot(to)?;
    let png = std::fs::read(to)?;
    Ok(!black(&png)?)
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
        state: Result<crate::dev::State>,
        lit: Option<bool>,
        want: &str,
        limits: Limits,
    ) -> Option<Hang> {
        if !input.running() {
            let why = input.crash().unwrap_or_else(|| "the game quit".into());
            return Some(Hang::Crashed { why });
        }
        let secs = at.as_secs_f32();
        // Frames first: a black screen is a hang whether or not the game
        // still answers reads. A missing frame neither starts nor ends the
        // black.
        match lit {
            Some(false) => {
                self.black.get_or_insert(at);
                self.dark += 1;
            }
            Some(true) => (self.black, self.dark) = (None, 0),
            None => {}
        }
        match lit {
            Some(_) => self.unframed = None,
            None => _ = self.unframed.get_or_insert(at),
        }
        // Play needs a lit frame on every look since it began.
        match lit {
            Some(true) => {}
            _ => self.play = None,
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
            let why = format!("no frame came for {secs:.1} s: {}", self.no_frame);
            return Some(Hang::Unread { why });
        }
        let state = match state {
            Ok(s) => {
                self.unread = None;
                s
            }
            Err(e) => {
                let since = *self.unread.get_or_insert(at);
                self.play = None;
                return (at - since >= DEAD).then(|| Hang::Unread {
                    why: format!("{e:#}"),
                });
            }
        };
        let past = at >= limits.load;
        let map = || state.map.clone();
        if state.loading {
            self.play = None;
            return past.then(|| Hang::Loading { secs, map: map() });
        }
        if state.event {
            self.play = None;
            // Text on the way elsewhere isn't the arrival's: no A for it.
            if state.map == want {
                self.event.get_or_insert(at);
            } else {
                self.event = None;
            }
            return past.then(|| Hang::Event { secs, map: map() });
        }
        self.event = None;
        if state.map != want {
            self.play = None;
            return past.then(|| Hang::WrongMap {
                map: map(),
                want: want.into(),
            });
        }
        if lit == Some(true) {
            self.play.get_or_insert(at);
        }
        // Settled by now, or never: a screen that keeps flickering dark.
        let unsettled = at >= limits.load.saturating_add(SETTLED) && !self.settled(at);
        unsettled.then(|| Hang::Unsettled { secs, map: map() })
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
    use crate::dev::{State, Switches};

    fn frame(w: u32, h: u32, paint: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut px = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend(paint(x, y));
            }
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
        out
    }

    #[test]
    fn pure_black_is_black_and_a_triforce_or_a_dark_room_is_not() {
        assert!(black(&frame(320, 180, |_, _| [1, 0, 2])).unwrap());
        // The loading triforce: a small gold patch in a corner.
        let triforce = |x: u32, y: u32| match x > 290 && y > 150 {
            true => [200, 170, 40],
            false => [0, 0, 0],
        };
        assert!(!black(&frame(320, 180, triforce)).unwrap());
        // A dark dungeon in play: dim walls and a lit HUD.
        let room = |x: u32, y: u32| match y < 10 && x < 60 {
            true => [220, 30, 30],
            false => [30, 26, 28],
        };
        assert!(!black(&frame(320, 180, room)).unwrap());
        // A few stray pixels don't light a black frame.
        let stray = |x: u32, y: u32| if x < 4 && y < 4 { [255; 3] } else { [0; 3] };
        assert!(black(&frame(320, 180, stray)).unwrap());
    }

    /// An input that runs, or quit with a log line.
    struct Up(bool);

    impl Input for Up {
        fn running(&self) -> bool {
            self.0
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
        fn shot(&mut self, _: &Path) -> Result<()> {
            Ok(())
        }
        fn crash(&self) -> Option<String> {
            Some("exit 139".into())
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
    fn two_seconds_of_black_hang_even_while_loading() {
        let mut w = Watch::default();
        let up = Up(true);
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
        let up = Up(true);
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
    }

    #[test]
    fn a_black_screen_hangs_even_when_reads_fail() {
        let mut w = Watch::default();
        let up = Up(true);
        let fail = || Err(anyhow::anyhow!("fake read failure"));
        assert_eq!(
            w.tick(secs(0.0), &up, fail(), Some(false), "FakeMap", LIMITS),
            None
        );
        let hang = w.tick(secs(2.0), &up, fail(), Some(false), "FakeMap", LIMITS);
        assert!(matches!(hang, Some(Hang::Black { .. })), "{hang:?}");
    }

    #[test]
    fn a_missing_frame_restarts_play_and_flicker_never_settles() {
        let mut w = Watch::default();
        let up = Up(true);
        let mut look = |at, lit| {
            w.tick(
                secs(at),
                &up,
                state("FakeMap", false, false),
                lit,
                "FakeMap",
                LIMITS,
            )
        };
        assert_eq!(look(0.0, Some(true)), None);
        assert_eq!(look(1.0, None), None);
        assert_eq!(look(2.0, Some(true)), None);
        assert!(!w.settled(secs(3.5)));
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
    fn a_probe_with_no_load_limit_never_overflows() {
        let mut w = Watch::default();
        let none = Limits {
            load: Duration::MAX,
            ..LIMITS
        };
        let up = Up(true);
        let state = || state("FakeMap", false, false);
        assert_eq!(
            w.tick(secs(0.0), &up, state(), Some(true), "FakeMap", none),
            None
        );
        assert_eq!(
            w.tick(secs(9.0), &up, state(), Some(false), "FakeMap", none),
            None
        );
    }

    #[test]
    fn an_rgba_frame_ignores_its_alpha() {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 16, 16);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let px = [0u8, 0, 0, 255].repeat(256);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
        assert!(black(&out).unwrap());
    }

    #[test]
    fn no_frame_never_settles_and_hangs_after_five_seconds() {
        let mut w = Watch::default();
        let up = Up(true);
        let mut look = |at| {
            w.tick(
                secs(at),
                &up,
                state("FakeMap", false, false),
                None,
                "FakeMap",
                LIMITS,
            )
        };
        assert_eq!(look(0.0), None);
        assert_eq!(look(4.0), None);
        assert!(matches!(look(5.0), Some(Hang::Unread { why }) if why.contains("no frame")));
        assert!(!w.settled(secs(5.0)));
    }

    #[test]
    fn black_that_lights_again_is_a_normal_load() {
        let mut w = Watch::default();
        let up = Up(true);
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
        let up = Up(true);
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
        let quit = w.tick(
            secs(1.0),
            &Up(false),
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
        let up = Up(true);
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
