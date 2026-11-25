//! One-tap songs: with the ocarina out and the songbook open (ZL held), A
//! on a song plays it. The hook keeps ZL from the game so the book
//! closes, then enters the song's notes itself, so the game's own matcher
//! hears the song and does what it does. See
//! `docs/superpowers/specs/2026-11-25-one-tap-songs-design.md`.

use super::Buttons;

/// An ocarina note, by pitch. The N64 buttons, low to high: A, C-down,
/// C-right, C-left, C-up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Note {
    #[default]
    D4,
    F4,
    A4,
    B4,
    D5,
}

impl Note {
    /// The button that plays the note. Y, X and A are from Epona's Song
    /// and Zelda's Lullaby played live (`research/audio-sanity.md`).
    pub const fn button(self) -> Buttons {
        match self {
            Self::D4 => Buttons::L,
            Self::F4 => Buttons::R,
            Self::A4 => Buttons::A,
            Self::B4 => Buttons::Y,
            Self::D5 => Buttons::X,
        }
    }

    /// The note the game stores as `pitch`, in its entered notes and the
    /// Scarecrow's melody: semitones, live (`research/songbook.md`).
    pub const fn from_pitch(pitch: u32) -> Option<Self> {
        Some(match pitch {
            4 => Self::D4,
            7 => Self::F4,
            11 => Self::A4,
            13 => Self::B4,
            16 => Self::D5,
            _ => return None,
        })
    }
}

/// The most notes a song has: Bolero, and any melody the Scarecrow learns.
pub const MAX_NOTES: usize = 8;

/// A song's notes. All zeros is no notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Notes {
    len: u8,
    notes: [Note; MAX_NOTES],
}

impl Notes {
    /// `notes`, cut to [`MAX_NOTES`].
    pub const fn new(notes: &[Note]) -> Self {
        let mut out = Self {
            len: 0,
            notes: [Note::D4; MAX_NOTES],
        };
        while (out.len as usize) < notes.len() && (out.len as usize) < MAX_NOTES {
            out.notes[out.len as usize] = notes[out.len as usize];
            out.len += 1;
        }
        out
    }

    pub fn get(&self, i: usize) -> Option<Note> {
        self.notes[..usize::from(self.len)].get(i).copied()
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The notes the game stores as `pitches`, up to the first 0 (an
    /// unused place), or `None` if one isn't a note.
    pub fn from_pitches(pitches: &[u32]) -> Option<Self> {
        let mut out = Self::default();
        for (note, &pitch) in out.notes.iter_mut().zip(pitches) {
            if pitch == 0 {
                break;
            }
            *note = Note::from_pitch(pitch)?;
            out.len += 1;
        }
        Some(out)
    }
}

/// A song the songbook lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Song {
    ZeldasLullaby,
    EponasSong,
    SariasSong,
    SunsSong,
    SongOfTime,
    SongOfStorms,
    Minuet,
    Bolero,
    Serenade,
    Requiem,
    Nocturne,
    Prelude,
    /// Its notes are the melody this save recorded.
    Scarecrow,
}

impl Song {
    /// Each song, in the songbook's order: its index in the hovered
    /// song's GameData value and the ocarina's owned flags.
    const ALL: [Self; 13] = [
        Self::ZeldasLullaby,
        Self::EponasSong,
        Self::SariasSong,
        Self::SunsSong,
        Self::SongOfTime,
        Self::SongOfStorms,
        Self::Minuet,
        Self::Bolero,
        Self::Serenade,
        Self::Requiem,
        Self::Nocturne,
        Self::Prelude,
        Self::Scarecrow,
    ];

    /// The song at the songbook's `index`, or `None` past them (13 is no
    /// song).
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// The song's notes, or `None` for the Scarecrow's, which live in
    /// the save.
    pub const fn notes(self) -> Option<Notes> {
        MELODIES[self as usize]
    }
}

/// Each song's notes, by [`Song`]: plain data, as a match on slices
/// would build a table of pointers the hook can't relocate.
const MELODIES: [Option<Notes>; Song::ALL.len()] = {
    use Note::*;
    [
        Some(Notes::new(&[B4, D5, A4, B4, D5, A4])),
        Some(Notes::new(&[D5, B4, A4, D5, B4, A4])),
        Some(Notes::new(&[F4, A4, B4, F4, A4, B4])),
        Some(Notes::new(&[A4, F4, D5, A4, F4, D5])),
        Some(Notes::new(&[A4, D4, F4, A4, D4, F4])),
        Some(Notes::new(&[D4, F4, D5, D4, F4, D5])),
        Some(Notes::new(&[D4, D5, B4, A4, B4, A4])),
        Some(Notes::new(&[F4, D4, F4, D4, A4, F4, A4, F4])),
        Some(Notes::new(&[D4, F4, A4, A4, B4])),
        Some(Notes::new(&[D4, F4, D4, A4, F4, D4])),
        Some(Notes::new(&[B4, A4, A4, D4, B4, A4, F4])),
        Some(Notes::new(&[D5, A4, D5, A4, B4, D5])),
        None,
    ]
};

/// The songbook as the hook read it this frame. All zeros is `Shut`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C, u8)]
pub enum Book {
    #[default]
    Shut,
    /// Open, with the notes of the song the cursor is on, if any. A
    /// Scarecrow's Song with no melody recorded has none.
    Open(Option<Notes>),
}

/// The songbook as the ocarina's input last saw it, at `ms`. That input
/// runs only while the ocarina is out, so a sighting goes stale once it's
/// put away. All zeros is a shut book.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Seen {
    pub book: Book,
    pub ms: u64,
}

/// How old a sighting may be and still count.
pub const FRESH_MS: u64 = 100;

impl Seen {
    /// The book at `ms`: as seen if that's recent, else shut.
    pub fn at(&self, ms: u64) -> Book {
        if ms.saturating_sub(self.ms) <= FRESH_MS {
            self.book
        } else {
            Book::Shut
        }
    }
}

/// What the hook tells the one-tap songs this frame, with the setting on.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub book: Book,
    /// The system clock, in milliseconds.
    pub ms: u64,
}

/// What the hook does for the one-tap songs this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Out {
    /// Buttons the game mustn't see until they're released.
    pub swallow: Buttons,
    /// The note to press for the game.
    pub press: Buttons,
}

/// How long after the book closes the first note waits.
pub const SETTLE_MS: u64 = 150;
/// How long each note is held, then let go.
pub const HOLD_MS: u64 = 100;
pub const GAP_MS: u64 = 100;
/// How long the book may take to close before the hook gives up.
pub const CLOSE_MS: u64 = 1000;

/// Every button: none reaches the game while the notes go in.
const ALL: Buttons = Buttons(u32::MAX);

/// Waiting for a press, waiting for the book to close since `since`, or
/// entering `notes`, the first at `start`. All zeros is `Idle`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C, u8)]
pub enum OneTap {
    #[default]
    Idle,
    Closing {
        notes: Notes,
        since: u64,
    },
    Playing {
        notes: Notes,
        start: u64,
    },
}

impl OneTap {
    /// Moves on one controller read, with this read's `pressed` buttons
    /// and the `frame`, or `None` with the setting off. In the far code,
    /// out of the cave, which is full.
    #[inline(never)]
    #[cfg_attr(target_os = "none", unsafe(link_section = ".far"))]
    pub fn step(&mut self, pressed: Buttons, frame: Option<Frame>) -> Out {
        let Some(Frame { book, ms }) = frame else {
            *self = Self::Idle;
            return Out::default();
        };
        match *self {
            Self::Idle => {
                let Book::Open(hovered) = book else {
                    return Out::default();
                };
                if !pressed.any(Buttons::A) {
                    return Out::default();
                }
                let Some(notes) = hovered.filter(|n| !n.is_empty()) else {
                    return Out {
                        swallow: Buttons::A,
                        ..Out::default()
                    };
                };
                *self = Self::Closing { notes, since: ms };
                Out {
                    swallow: Buttons::A | Buttons::ZL,
                    ..Out::default()
                }
            }
            Self::Closing { notes, since } => {
                if book == Book::Shut {
                    let start = ms + SETTLE_MS;
                    *self = Self::Playing { notes, start };
                } else if ms.saturating_sub(since) > CLOSE_MS {
                    *self = Self::Idle;
                }
                Out {
                    swallow: Buttons::ZL,
                    ..Out::default()
                }
            }
            Self::Playing { notes, start } => {
                let Some(t) = ms.checked_sub(start) else {
                    return Out {
                        swallow: ALL,
                        ..Out::default()
                    };
                };
                let period = HOLD_MS + GAP_MS;
                let note = usize::try_from(t / period).ok().and_then(|i| notes.get(i));
                if note.is_none() {
                    *self = Self::Idle;
                }
                let press = note
                    .filter(|_| t % period < HOLD_MS)
                    .map_or(Buttons::NONE, Note::button);
                Out {
                    swallow: ALL,
                    press,
                }
            }
        }
    }

    /// Whether a song is on its way, from the press to its last note.
    pub fn busy(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LULLABY: Notes = match Song::ZeldasLullaby.notes() {
        Some(n) => n,
        None => panic!(),
    };

    fn at(book: Book, ms: u64) -> Option<Frame> {
        Some(Frame { book, ms })
    }

    /// A tap on the Lullaby at 0 ms, the book shut at 50 ms.
    fn tapped() -> OneTap {
        let mut tap = OneTap::default();
        tap.step(Buttons::A, at(Book::Open(Some(LULLABY)), 0));
        tap.step(Buttons::NONE, at(Book::Shut, 50));
        tap
    }

    /// What's pressed for the game at `ms`.
    fn press(tap: &mut OneTap, ms: u64) -> Buttons {
        tap.step(Buttons::NONE, at(Book::Shut, ms)).press
    }

    #[test]
    fn every_song_but_the_scarecrow_s_has_its_notes() {
        assert_eq!(Song::Bolero.notes().map(|n| n.len), Some(8));
        assert_eq!(Song::Serenade.notes().map(|n| n.len), Some(5));
        assert_eq!(Song::Scarecrow.notes(), None);
        assert_eq!(Notes::new(&[Note::D4; 9]).len, 8, "cut to the most");
    }

    #[test]
    fn a_in_the_book_keeps_a_and_zl_and_waits_for_the_book_to_close() {
        let mut tap = OneTap::default();
        let out = tap.step(Buttons::A, at(Book::Open(Some(LULLABY)), 0));
        assert_eq!(out.swallow, Buttons::A | Buttons::ZL);
        assert_eq!(out.press, Buttons::NONE);
        assert_eq!(
            tap,
            OneTap::Closing {
                notes: LULLABY,
                since: 0
            }
        );
        let out = tap.step(Buttons::NONE, at(Book::Open(Some(LULLABY)), 20));
        assert_eq!(out.swallow, Buttons::ZL, "ZL stays kept");
        assert!(tap.busy());
    }

    #[test]
    fn the_notes_go_in_one_by_one_after_the_book_closes() {
        let mut tap = tapped();
        assert_eq!(
            press(&mut tap, 50 + SETTLE_MS - 1),
            Buttons::NONE,
            "settling"
        );
        let start = 50 + SETTLE_MS;
        let period = HOLD_MS + GAP_MS;
        let heard: Vec<Buttons> = (0..6)
            .map(|i| press(&mut tap, start + i * period))
            .collect();
        let lullaby: Vec<Buttons> = (0..6).map(|i| LULLABY.get(i).unwrap().button()).collect();
        assert_eq!(heard, lullaby);
        assert_eq!(
            press(&mut tap, start + HOLD_MS),
            Buttons::NONE,
            "let go between notes"
        );
    }

    #[test]
    fn a_repeated_note_is_let_go_between_its_presses() {
        let serenade = Song::Serenade.notes().unwrap();
        let mut tap = OneTap::default();
        tap.step(Buttons::A, at(Book::Open(Some(serenade)), 0));
        tap.step(Buttons::NONE, at(Book::Shut, 0));
        let third = SETTLE_MS + 2 * (HOLD_MS + GAP_MS);
        assert_eq!(press(&mut tap, third), Buttons::A);
        assert_eq!(press(&mut tap, third + HOLD_MS), Buttons::NONE);
        assert_eq!(press(&mut tap, third + HOLD_MS + GAP_MS), Buttons::A);
    }

    #[test]
    fn every_real_button_is_kept_while_the_notes_go_in_then_the_tap_ends() {
        let mut tap = tapped();
        let out = tap.step(Buttons::B, at(Book::Shut, 60));
        assert_eq!(out.swallow, ALL);
        let end = 50 + SETTLE_MS + 6 * (HOLD_MS + GAP_MS);
        let out = tap.step(Buttons::NONE, at(Book::Shut, end));
        assert_eq!(
            out,
            Out {
                swallow: ALL,
                press: Buttons::NONE
            }
        );
        assert_eq!(tap, OneTap::Idle);
        assert_eq!(press(&mut tap, end + 1), Buttons::NONE);
        assert_eq!(
            tap.step(Buttons::A, at(Book::Shut, end + 2)),
            Out::default(),
            "A is the game's again"
        );
    }

    #[test]
    fn a_book_that_never_closes_gives_up() {
        let mut tap = OneTap::default();
        let open = Book::Open(Some(LULLABY));
        tap.step(Buttons::A, at(open, 0));
        tap.step(Buttons::NONE, at(open, CLOSE_MS));
        assert!(tap.busy());
        tap.step(Buttons::NONE, at(open, CLOSE_MS + 1));
        assert_eq!(tap, OneTap::Idle);
    }

    #[test]
    fn a_with_no_song_or_no_melody_does_nothing_but_keeps_the_preview_off() {
        let mut tap = OneTap::default();
        let out = tap.step(Buttons::A, at(Book::Open(None), 0));
        assert_eq!(
            out,
            Out {
                swallow: Buttons::A,
                press: Buttons::NONE
            }
        );
        assert_eq!(tap, OneTap::Idle);
        let empty = Book::Open(Some(Notes::default()));
        assert_eq!(tap.step(Buttons::A, at(empty, 1)).swallow, Buttons::A);
        assert_eq!(tap, OneTap::Idle);
    }

    #[test]
    fn a_outside_the_book_is_the_game_s() {
        let mut tap = OneTap::default();
        assert_eq!(tap.step(Buttons::A, at(Book::Shut, 0)), Out::default());
        let open = Book::Open(Some(LULLABY));
        assert_eq!(
            tap.step(Buttons::Y, at(open, 1)),
            Out::default(),
            "only A taps"
        );
        assert_eq!(tap, OneTap::Idle);
    }

    #[test]
    fn with_the_setting_off_nothing_is_kept_or_pressed() {
        let mut tap = OneTap::default();
        assert_eq!(tap.step(Buttons::A, None), Out::default());
        let mut tap = tapped();
        assert_eq!(tap.step(Buttons::NONE, None), Out::default());
        assert_eq!(tap, OneTap::Idle, "a tap under way stops");
    }

    #[test]
    fn a_melody_reads_up_to_its_first_unused_place() {
        let lullaby = [13, 16, 11, 13, 16, 11, 0, 0];
        assert_eq!(Notes::from_pitches(&lullaby), Some(LULLABY));
        assert_eq!(Notes::from_pitches(&[0; 8]), Some(Notes::default()));
        let eight = Notes::from_pitches(&[4, 7, 11, 13, 16, 4, 7, 11]).unwrap();
        assert_eq!(eight.get(7), Some(Note::A4));
        assert_eq!(Notes::from_pitches(&[4, 5]), None, "5 is no note");
    }

    #[test]
    fn the_songbook_s_index_names_its_song() {
        assert_eq!(Song::from_index(0), Some(Song::ZeldasLullaby));
        assert_eq!(Song::from_index(6), Some(Song::Minuet));
        assert_eq!(Song::from_index(12), Some(Song::Scarecrow));
        assert_eq!(Song::from_index(13), None, "no song");
    }

    #[test]
    fn a_sighting_of_the_book_goes_stale() {
        let seen = Seen {
            book: Book::Open(None),
            ms: 1000,
        };
        assert_eq!(seen.at(1000 + FRESH_MS), Book::Open(None));
        assert_eq!(seen.at(1001 + FRESH_MS), Book::Shut);
        assert_eq!(seen.at(999), Book::Open(None), "a read racing the sighting");
        assert_eq!(Seen::default().at(0), Book::Shut);
    }

    #[test]
    fn each_song_s_place_is_its_number() {
        for (i, song) in Song::ALL.into_iter().enumerate() {
            assert_eq!(song as usize, i, "{song:?}");
        }
        assert_eq!(Song::Bolero.notes().unwrap().get(7), Some(Note::F4));
        assert_eq!(Song::Scarecrow.notes(), None);
    }
}
