//! The memory the hook keeps in the game: the loaded map's chest list, under
//! a lock, and the dev mailbox and switches beside it.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::Mailbox;

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

/// The chests of the loaded map.
#[repr(C)]
pub struct List {
    count: u32,
    chests: [Chest; CHESTS],
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

    pub fn opened(&mut self, actor: usize) {
        if let Some(c) = self.chests().iter_mut().find(|c| c.actor == actor) {
            c.done = true;
        }
    }
}

/// The list and its lock, as they sit in the game's zeroed memory. Chests
/// are made on several threads at once.
#[repr(C)]
pub struct Shared {
    lock: AtomicU32,
    list: UnsafeCell<List>,
    pub dev: Mailbox,
    /// The dev switches the app turns on, see [`crate::switches`].
    pub switches: AtomicU32,
}

// SAFETY: the list is only reached through `with`, under the lock. The
// mailbox's plain fields are only read after `asked` is.
unsafe impl Sync for Shared {}

impl Shared {
    pub fn with<T>(&self, f: impl FnOnce(&mut List) -> T) -> T {
        self.lock();
        // SAFETY: the lock is held.
        let out = f(unsafe { &mut *self.list.get() });
        self.lock.store(0, Ordering::Release);
        out
    }

    /// Out of line, since every hook takes it and space is short.
    #[inline(never)]
    fn lock(&self) {
        while self
            .lock
            .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    /// Where chest `i`'s name lives, for create.
    pub fn name_at(&self, i: usize) -> *const usize {
        // SAFETY: an address only, `i` is under `CHESTS`.
        unsafe {
            let chests = &raw const (*self.list.get()).chests;
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
