//! The dev mailbox: the one request the app leaves for the hook, what it
//! asks and how the hook answers, and the arguments a request passes.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, AtomicU64};

use crate::{LIST, Shared};

/// Where the mailbox sits in `main`.
pub const MAILBOX: u32 = LIST + core::mem::offset_of!(Shared, dev) as u32;
/// A mailbox name's size, its NUL included.
pub const NAME: usize = 64;
/// The arguments a quest step request passes, which a seed's map change flow
/// reads, each with its NUL: whether it is a step, then the two names.
pub const ARG_STEP: &[u8] = b"ZootrStep\0";
pub const ARG_CHALLENGE: &[u8] = b"ZootrChallenge\0";
pub const ARG_STEP_NAME: &[u8] = b"ZootrStepName\0";
/// The arguments an item request passes, read the same way: whether it is a
/// give, the item's actor, and whether to equip it.
pub const ARG_GIVE: &[u8] = b"ZootrGive\0";
pub const ARG_ACTOR: &[u8] = b"ZootrActor\0";
pub const ARG_EQUIP: &[u8] = b"ZootrEquip\0";
/// The arguments a notice request passes: whether it is a notice, then
/// which banner, with the item in `ZootrActor`.
pub const ARG_NOTICE: &[u8] = b"ZootrNotice\0";
pub const ARG_SUMMARY: &[u8] = b"ZootrSummary\0";
pub const ARG_MIXED: &[u8] = b"ZootrMixed\0";

/// One dev request at a time, from the app to the hook. The app writes the
/// request, then bumps `asked`. On Link's next update the hook runs it,
/// writes `outcome`, then sets `answered` to `asked`.
#[repr(C)]
pub struct Mailbox {
    pub asked: AtomicU32,
    /// The request a thread has claimed, so only one runs it.
    pub taken: AtomicU32,
    pub answered: AtomicU32,
    pub outcome: AtomicU32,
    pub ask: u32,
    pub hour: u32,
    /// The system tick at the last teleport.
    pub last: AtomicU64,
    pub map: [u8; NAME],
    pub spot: [u8; NAME],
    /// After a [`Ask::GiveLevel`], the hook writes the item it gave here,
    /// so the app can name what Link got.
    pub arrive: UnsafeCell<[u8; NAME]>,
    /// Where a warp puts Link.
    pub at: [f32; 3],
    /// Where Link stands, as a where request found him.
    pub link: UnsafeCell<[f32; 3]>,
}

/// A `repr(u32)` enum and its reading from a mailbox word.
macro_rules! word_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $n:literal,)* }) => {
        $(#[$doc])*
        #[repr(u32)]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum $name {
            $($(#[$vdoc])* $variant = $n,)*
        }

        impl $name {
            pub fn from_word(word: u32) -> Option<Self> {
                match word {
                    $($n => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

word_enum! {
    /// What the app asks of the hook.
    Ask {
        /// To `map`, `spot`, with `arrive` as the arrival event.
        Teleport = 1,
        /// Flip the age, then teleport as above.
        Age = 2,
        /// Set the clock to `hour`.
        Time = 3,
        /// Move the quest `map` to its step `spot`, with the event a
        /// seed's map change flow runs for it.
        Step = 4,
        /// Give Link the item actor `map`, equipped when `hour` is not 0,
        /// with the grant a seed's map change flow runs for it.
        Give = 5,
        /// Start the event `map` names, with no arguments.
        Play = 6,
        /// Move Link to `at`.
        Warp = 7,
        /// Write where Link stands to `link`.
        Where = 8,
        /// [`Ask::Give`], of the level of `map`'s progressive line Link
        /// needs next, as a chest picks it. With `spot` naming what fires
        /// `map`, `arrive` instead until Link owns it, as a chest's swap does.
        GiveLevel = 9,
        /// Show the banner of `hour`'s [`Notice`] for the item actor `map`,
        /// with the popup for an item, as a seed's map change flow runs it.
        Notice = 10,
        /// Swap Link's age where he stands, with no area load.
        AgeHere = 11,
    }
}

word_enum! {
    /// Which received-item banner a notice shows. Its words come from
    /// GameData the app writes first: the item, the sender, the count.
    Notice {
        /// "ITEM from SENDER", with the pickup popup for `map`.
        Item = 0,
        /// "+COUNT more from SENDER".
        Summary = 1,
        /// "+COUNT more", for a burst from several senders.
        Mixed = 2,
    }
}

word_enum! {
    /// How the hook answered.
    Outcome {
        Done = 1,
        /// The event manager took the request but answered 0.
        Unanswered = 2,
        /// Within [`GUARD`] seconds of the last teleport, or a swap is
        /// already under way.
        Soon = 3,
        /// The event manager or GameData isn't up yet.
        NoGame = 4,
        Event = 5,
        Loading = 6,
        /// The request block didn't take the event or its arguments.
        NotBuilt = 7,
        NoAge = 8,
        NoClock = 9,
        /// Not a request this hook knows.
        Unknown = 10,
        /// Link's character controller isn't where a warp looks.
        NoController = 11,
        /// No area has loaded since the hook came in, so it hasn't met
        /// the resident actors yet.
        NoResident = 12,
        /// The new Link never came: the swap went back to the old age.
        Stuck = 13,
    }
}

/// The event a play request puts in the block: `map` up to its NUL, the
/// NUL included. `None` for an empty or unended name.
pub fn play_event(map: &[u8; NAME]) -> Option<&[u8]> {
    let end = map.iter().position(|&b| b == 0).filter(|&n| n > 0)?;
    Some(&map[..=end])
}

/// Seconds between teleports, so a second never lands mid map change.
pub const GUARD: u64 = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_words_read_back_and_strangers_are_none() {
        assert_eq!(Ask::from_word(Ask::Time as u32), Some(Ask::Time));
        assert_eq!(Ask::from_word(Ask::Step as u32), Some(Ask::Step));
        assert_eq!(Ask::from_word(Ask::Give as u32), Some(Ask::Give));
        assert_eq!(Ask::from_word(7), Some(Ask::Warp));
        assert_eq!(Ask::from_word(8), Some(Ask::Where));
        assert_eq!(Ask::from_word(10), Some(Ask::Notice));
        assert_eq!(Notice::from_word(2), Some(Notice::Mixed));
        assert_eq!(Notice::from_word(3), None);
        assert!(
            [
                ARG_STEP,
                ARG_CHALLENGE,
                ARG_STEP_NAME,
                ARG_GIVE,
                ARG_ACTOR,
                ARG_EQUIP,
                ARG_NOTICE,
                ARG_SUMMARY,
                ARG_MIXED
            ]
            .iter()
            .all(|a| a.ends_with(b"\0") && !a[..a.len() - 1].contains(&0))
        );
        assert_eq!(Outcome::from_word(9), Some(Outcome::NoClock));
        assert_eq!(Outcome::from_word(11), Some(Outcome::NoController));
        assert_eq!(Ask::from_word(0), None);
        assert_eq!(Outcome::from_word(99), None);
        assert_eq!(MAILBOX % 8, 0);
    }

    #[test]
    fn a_play_request_names_the_mailboxs_event() {
        let mut map = [0; NAME];
        map[..9].copy_from_slice(b"FakeEvent");
        assert_eq!(Ask::from_word(6), Some(Ask::Play));
        assert_eq!(play_event(&map), Some(&b"FakeEvent\0"[..]));
        assert_eq!(play_event(&[0; NAME]), None);
        assert_eq!(play_event(&[b'A'; NAME]), None);
    }
}
