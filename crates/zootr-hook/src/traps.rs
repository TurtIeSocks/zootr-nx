//! Ice traps: each one Link gets freezes him once. The hook starts the
//! seed's [`EVENT`], which makes a sensor hitting him with a Freezard's
//! breath, so the freeze, the break, the damage and his voice are the
//! game's own. See `docs/superpowers/specs/2026-11-18-ice-traps-design.md`.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::{LIST, Shared};

/// The seed's event that springs a trap: it makes [`SENSOR`] on Link.
pub const EVENT: &[u8] = b"Rando_IceTrap\0";
/// The sensor that hits Link with a Freezard's breath.
pub const SENSOR: &str = "Sensor_Rando_IceTrap";

/// Where the count of traps waiting sits in `main`.
pub const TRAPS: u32 = LIST + core::mem::offset_of!(Shared, traps) as u32;

/// Where Link's actor keeps his components, and where among them his bad
/// condition component sits.
pub const COMPONENTS: usize = 0x270;
pub const BAD_CONDITIONS: usize = 0x1b0;
/// The bad conditions on Link now, a bit per type. The `Frozen` bit is up
/// from the hit until he breaks free (live, 2026-11-18).
pub const ON: usize = 0x154;
/// The `Frozen` bad condition's bit: `Burning` is 0, `Frozen` 1.
pub const FROZEN: u32 = 1 << 1;

/// Seconds from one trap to the next. A freeze shows a few frames after
/// its event starts, its bit flickers for about 1.5 s, and the break plays
/// after (live, 2026-11-18), so neither the bit alone nor the event can
/// say he's free.
pub const THAW: u64 = 4;

/// Whether a trap springs now, taking one from `waiting`: one waits, Link,
/// with bad conditions `on`, isn't frozen, and [`THAW`] seconds have gone
/// by `since` the last, a count of `rate` ticks a second (`None` before
/// any). One at a time: the next waits until he's free.
pub fn spring(waiting: &AtomicU32, on: u32, since: Option<u64>, rate: u64) -> bool {
    on & FROZEN == 0
        && since.is_none_or(|ticks| ticks >= THAW * rate)
        && (waiting.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u64 = 1000;

    #[test]
    fn a_trap_waiting_springs_and_is_taken() {
        let waiting = AtomicU32::new(2);
        assert!(spring(&waiting, 1 << 4, None, RATE));
        assert_eq!(waiting.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn none_waiting_leaves_link_alone() {
        let waiting = AtomicU32::new(0);
        assert!(!spring(&waiting, 0, None, RATE));
        assert_eq!(waiting.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_next_trap_waits_while_link_is_frozen() {
        let waiting = AtomicU32::new(1);
        assert!(!spring(&waiting, FROZEN, None, RATE));
        assert_eq!(waiting.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn the_next_trap_waits_until_the_last_has_thawed() {
        let waiting = AtomicU32::new(1);
        assert!(!spring(&waiting, 0, Some(THAW * RATE - 1), RATE));
        assert_eq!(waiting.load(Ordering::Relaxed), 1);
        assert!(spring(&waiting, 0, Some(THAW * RATE), RATE));
        assert_eq!(waiting.load(Ordering::Relaxed), 0);
    }
}
