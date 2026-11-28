//! The songbook as the one-tap songs need it: whether it's open, the song
//! the cursor is on, and the melody the Scarecrow learned. The ocarina's
//! input reads them each frame the ocarina is out, and the controller
//! reads take the last sighting. See `ui::songs` and
//! `docs/research/songbook.md`.

use core::slice;

use super::dev::{clock, single};
use super::refill::{entry, get_int};
use super::{hooked, shared};
use crate::switches::{INT, UINT_ARRAY};
use crate::ui::songs::{Book, MAX_NOTES, Notes, Seen, Song};
use crate::{ONE_TAP_SONGS, Table};

/// The ocarina's songbook state: 0 shut, 1 open. Its input handler reads
/// 2 as open too.
const BOOK: usize = 0x278;
/// The ocarina's owned flags, one byte a song, by songbook index.
const OWNED: usize = 0x264;
/// GameData's Int of the song the songbook's cursor is on (13 for none),
/// and UIntArray of the Scarecrow's melody (pitches, 0 past its end).
const HOVERED: u32 = 0x8566_048e;
const SCARECROW: u32 = 0xcc53_5536;

/// The clock for this controller read, or `None` in a seed without the
/// one-tap songs.
pub(super) fn ms(table: &Table) -> Option<u64> {
    (table.flags & ONE_TAP_SONGS != 0).then(now)
}

/// The system clock, in milliseconds.
fn now() -> u64 {
    let (now, rate) = clock();
    now / (rate / 1000).max(1)
}

/// The ocarina's input, each frame the ocarina is out: note what the
/// songbook shows.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".far")]
extern "C" fn ocarina_input(ocarina: usize) {
    hooked(|main, table| {
        if table.flags & ONE_TAP_SONGS == 0 {
            return;
        }
        // SAFETY: the game hands its live ocarina.
        let book = unsafe { book(main, table, ocarina) };
        let seen = Seen { book, ms: now() };
        shared(main).ui.with(|s| s.book = seen);
    })
}

/// The songbook, as `ocarina` has it.
///
/// # Safety
/// `ocarina` is the game's ocarina.
unsafe fn book(main: usize, table: &Table, ocarina: usize) -> Book {
    match unsafe { *((ocarina + BOOK) as *const u32) } {
        1 | 2 => Book::Open(unsafe { hovered(main, table, ocarina) }),
        _ => Book::Shut,
    }
}

/// The notes of the song the cursor is on, if Link has it.
///
/// # Safety
/// As [`book`].
unsafe fn hovered(main: usize, table: &Table, ocarina: usize) -> Option<Notes> {
    let gd = unsafe { single(main, table.offsets.gamedata)? };
    let index = unsafe { get_int(gd, HOVERED)? };
    let song = Song::from_index(usize::try_from(index).ok()?)?;
    let owned = unsafe { *((ocarina + OWNED + song as usize) as *const u8) };
    if owned != 1 {
        return None;
    }
    song.notes().or_else(|| unsafe { scarecrow(gd) })
}

/// The melody the Scarecrow learned, empty if none, or `None` if a pitch
/// isn't a note.
///
/// # Safety
/// `gd` is GameData.
unsafe fn scarecrow(gd: usize) -> Option<Notes> {
    let at = unsafe { entry(gd, UINT_ARRAY, SCARECROW)? };
    let len = unsafe { *(at as *const u64) } as usize;
    let values = unsafe { *((at + 8) as *const *const u32) };
    if values.is_null() {
        return None;
    }
    let pitches = unsafe { slice::from_raw_parts(values, len.min(MAX_NOTES)) };
    Notes::from_pitches(pitches)
}
