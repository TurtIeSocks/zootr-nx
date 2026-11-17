//! The memory the hook keeps in the game: the loaded map's chest list, under
//! a lock, the dev mailbox and switches beside it, and the cycled slot's
//! store under a lock of its own.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::Mailbox;
use crate::ui::Store;

/// How many chests a map's list holds.
pub const CHESTS: usize = 64;

/// A loaded chest whose content is a level of a line.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chest {
    pub actor: usize,
    pub holder: usize,
    /// The level it holds, as its drop name in the table. Create is handed
    /// this field's address, so it outlives the call.
    pub name: usize,
    /// The slot in the holder's `Content` entry.
    pub index: u32,
    pub line: u16,
    /// Opened, or failed its check: never recreated again.
    pub done: bool,
}

/// The chests of the loaded map, and the last key copy credited.
#[repr(C)]
pub struct List {
    count: u32,
    chests: [Chest; CHESTS],
    credited: Credited,
}

/// A key copy credited: its name's hash, whether the pickup window
/// credited it (else the pouch add), and when, in counter ticks.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Credited {
    pub name: u32,
    pub picked: bool,
}

impl List {
    pub fn clear(&mut self) {
        self.count = 0;
    }

    pub fn chests(&mut self) -> &mut [Chest] {
        let (listed, _) = self.chests.split_at_mut((self.count as usize).min(CHESTS));
        listed
    }

    /// Add `chest`, or update it if its actor is listed. A full list skips.
    pub fn made(&mut self, chest: Chest) {
        if let Some(c) = self.chests().iter_mut().find(|c| c.actor == chest.actor) {
            *c = chest;
        } else if let Some(c) = self.chests.get_mut(self.count as usize) {
            *c = chest;
            self.count += 1;
        }
    }

    /// Whether to credit `now`. A chest or gift both adds to the pouch and
    /// opens the pickup window, so each hook sees its get: when the other
    /// hook credited the same name within `same_get` ticks, this is that
    /// get's other half. A paired credit is forgotten, so the next get of
    /// the same name counts.
    pub fn first_credit(&mut self, now: Credited) -> bool {
        let last = self.credited;
        let pair =
            last != Credited::default() && last.name == now.name && last.picked != now.picked;
        self.credited = if pair { Credited::default() } else { now };
        !pair
    }

    /// No event plays: the get that made the last credit is over, so no
    /// later one pairs with it.
    pub fn idle(&mut self) {
        self.credited = Credited::default();
    }

    pub fn opened(&mut self, actor: usize) {
        if let Some(c) = self.chests().iter_mut().find(|c| c.actor == actor) {
            c.done = true;
        }
    }
}

/// A value and its spin lock, as they sit in the game's zeroed memory.
#[repr(C)]
pub struct Locked<T> {
    lock: AtomicU32,
    value: UnsafeCell<T>,
}

// SAFETY: the value is only reached through `with`, under the lock.
unsafe impl<T: Send> Sync for Locked<T> {}

impl<T> Locked<T> {
    pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        lock(&self.lock);
        // SAFETY: the lock is held.
        let out = f(unsafe { &mut *self.value.get() });
        self.lock.store(0, Ordering::Release);
        out
    }
}

/// Out of line, since every hook takes it and space is short.
#[inline(never)]
fn lock(lock: &AtomicU32) {
    while lock
        .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

/// The hook's memory. Chests are made on several threads at once, and the
/// controller is read on other threads than the menu runs on.
#[repr(C)]
pub struct Shared {
    list: Locked<List>,
    pub dev: Mailbox,
    /// The dev switches the app turns on, see [`crate::switches`].
    pub switches: AtomicU32,
    pub ui: Locked<Store>,
}

// SAFETY: the mailbox's plain fields are only read after `asked` is.
unsafe impl Sync for Shared {}

impl Shared {
    pub fn with<T>(&self, f: impl FnOnce(&mut List) -> T) -> T {
        self.list.with(f)
    }

    /// Where chest `i`'s name lives, for create.
    pub fn name_at(&self, i: usize) -> *const usize {
        // SAFETY: an address only, `i` is under `CHESTS`.
        unsafe {
            let chests = &raw const (*self.list.value.get()).chests;
            &raw const (*chests.cast::<Chest>().add(i)).name
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chest(actor: usize) -> Chest {
        Chest {
            actor,
            ..Chest::default()
        }
    }

    fn list() -> Box<Shared> {
        // SAFETY: zeroed, as the game's memory starts.
        unsafe { Box::new_zeroed().assume_init() }
    }

    #[test]
    fn one_get_seen_by_both_hooks_credits_once_in_either_order() {
        let shared = list();
        let add = |name| Credited {
            name,
            picked: false,
        };
        let window = |name| Credited { name, picked: true };
        shared.with(|l| {
            // A chest's get event: the add, then the window.
            assert!(l.first_credit(add(7)));
            assert!(!l.first_credit(window(7)));
            l.idle();
            // The window first, then the add.
            assert!(l.first_credit(window(7)));
            assert!(!l.first_credit(add(7)));
            l.idle();
            // Two pots in a row, the window alone each time.
            assert!(l.first_credit(window(7)));
            l.idle();
            assert!(l.first_credit(window(7)));
            l.idle();
            // A gift with no window, then a pot of the same name, however
            // soon: a frame with no event plays between them.
            assert!(l.first_credit(add(7)));
            l.idle();
            assert!(l.first_credit(window(7)));
            l.idle();
            // Another name between the halves isn't their pair.
            assert!(l.first_credit(add(8)));
            assert!(l.first_credit(window(7)));
        });
    }

    #[test]
    fn chests_are_added_updated_and_opened() {
        let shared = list();
        shared.with(|l| {
            l.made(chest(1));
            l.made(chest(2));
            l.made(Chest {
                index: 3,
                ..chest(1)
            });
            l.opened(2);
            l.opened(9);
            assert_eq!(
                l.chests(),
                [
                    Chest {
                        index: 3,
                        ..chest(1)
                    },
                    Chest {
                        done: true,
                        ..chest(2)
                    }
                ]
            );
            l.clear();
            assert!(l.chests().is_empty());
        });
    }

    #[test]
    fn a_full_list_skips() {
        let shared = list();
        shared.with(|l| {
            (0..=CHESTS).for_each(|a| l.made(chest(a + 1)));
            assert_eq!(l.chests().len(), CHESTS);
            assert_eq!(l.chests()[CHESTS - 1].actor, CHESTS);
        });
    }

    #[test]
    fn a_chest_is_32_bytes() {
        assert_eq!(size_of::<Chest>(), 32);
    }
}
