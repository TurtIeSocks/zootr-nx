//! The age swap in place (`Ask::AgeHere`) over faked game memory: the
//! resident actor manager, its lists, the camera and both Links, with the
//! game's routines stubbed and logged. Started through the dev mailbox
//! from Link's update, stepped a frame at a time through the UI's update.
//! Spec: age swap in place (private notes).

use std::collections::BTreeMap;
use std::mem::offset_of;

use unicorn_engine::RegisterARM64 as R;
use zootr_hook::{Ask, Mailbox, OFFSETS, Outcome, Shared, Swap};

use super::*;

/// The fake game's memory, apart from everything else.
pub(super) const FAKE: u64 = 0x5000_0000;
const MANAGER: u64 = FAKE;
const ENTRIES: u64 = FAKE + 0x1000;
const LINKS: u64 = FAKE + 0x2000;
const CAMERA: u64 = FAKE + 0x3000;
const CAMERA_COMPONENT: u64 = FAKE + 0x3800;
/// The new (adult) Link, his components and his camera target part.
const ADULT: u64 = FAKE + 0x4000;
const PART: u64 = FAKE + 0x4800;
/// The old (child) Link, a player [`is_player`] accepts: the owner, its
/// movement component, the resource named `Player`.
const LINK: u64 = FAKE + 0x6000;
const OWNER: u64 = FAKE + 0x8000;
const MOVEMENT: u64 = OWNER + 0x1d50;
const RESOURCE: u64 = FAKE + 0xc000;
const GAMEDATA: u64 = FAKE + 0xd000;
const EVENTS: u64 = FAKE + 0xe000;
/// Cells `main`'s globals point at, holding GameData and the events.
const CELLS: u64 = FAKE + 0xf000;

/// The resident lists: common (the camera), child, adult.
const LISTS: [(u64, u64); 3] = [(0x28, CAMERA), (0x38, LINK), (0x48, ADULT)];
const POSITION: [f32; 3] = [1.5, -2.25, 40.0];
const ROTATION: [f32; 9] = [0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0];
const CHILD: u32 = 0xcea2_20d3;
/// A swap's stage gives up past this many frames.
const LIMIT: usize = 300;

/// A call into the game the hook made.
#[derive(Debug, PartialEq)]
enum Call {
    Pause(&'static str, String),
    SetBool(u32, bool),
    Create(u64, u64),
    /// The manager, where Link wakes, and the part the camera followed
    /// last when the wake came.
    Wake(u64, [f32; 3], [f32; 9], u64),
}

pub(super) struct Game {
    m: Machine,
    bools: BTreeMap<u32, bool>,
    calls: Vec<Call>,
    /// What create answers, call by call (1 once it runs out).
    creates: Vec<u64>,
}

pub(super) fn u64s(words: &[u64]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn f32s(words: &[f32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

impl Game {
    /// The machine with a table (dev on) and the fake game: a child Link
    /// in play, his adult self not made yet.
    pub(super) fn new() -> Option<Self> {
        let mut m = Machine::new()?;
        let table = patches(&progressive::LINES, &[], 0, &[], &[], None).unwrap();
        for p in table.iter().filter(|p| (TABLE..TABLE_END).contains(&p.at)) {
            m.uc.mem_write(at(p.at), &p.bytes).unwrap();
        }
        m.uc.mem_map(FAKE, 0x1_0000, Prot::READ | Prot::WRITE)
            .unwrap();
        let mut g = Self {
            m,
            bools: BTreeMap::from([(CHILD, true)]),
            calls: vec![],
            creates: vec![],
        };
        let o = &OFFSETS;
        // Each list: one entry, its link holding its actor while the ids
        // match. The adult's doesn't yet.
        for (i, &(list, actor)) in LISTS.iter().enumerate() {
            let (entry, link) = (ENTRIES + 0x100 * i as u64, LINKS + 0x100 * i as u64);
            g.write(MANAGER + list, &u64s(&[1, entry]));
            g.write(entry + 8, &u64s(&[link, 7]));
            g.write(
                link + 0x40,
                &u64s(&[actor, if actor == ADULT { 0 } else { 7 }]),
            );
        }
        // The camera's component, still on child Link's part.
        g.write(CAMERA + 0x270, &u64s(&[CAMERA + 0x400]));
        g.write(CAMERA + 0x400 + 0x360, &u64s(&[CAMERA_COMPONENT]));
        g.write(CAMERA_COMPONENT + 0xfe0, &u64s(&[LINK + 0x900]));
        // The adult, asleep once made, and his part.
        g.write(ADULT + 0x2c, &3u32.to_le_bytes());
        g.write(ADULT + 0x270, &u64s(&[ADULT + 0x400]));
        g.write(ADULT + 0x400 + 0x368, &u64s(&[PART]));
        // Child Link where he stands, and the player checks.
        g.write(LINK + 0x31c, &f32s(&POSITION));
        g.write(LINK + 0x328, &f32s(&ROTATION));
        g.write(LINK, &u64s(&[at(o.player_vtable)]));
        g.write(LINK + 0x270, &u64s(&[LINK + 0x800]));
        g.write(LINK + 0x800 + 0x320, &u64s(&[MOVEMENT]));
        g.write(MOVEMENT + 0x18, &u64s(&[LINK]));
        g.write(OWNER, &u64s(&[at(o.move_vtable)]));
        g.write(OWNER + 0x10, &u64s(&[RESOURCE]));
        g.write(RESOURCE + 0x58, b"Player\0");
        // GameData and the events (none playing) through main's globals.
        g.write(CELLS, &u64s(&[GAMEDATA, EVENTS]));
        g.write(at(o.gamedata), &u64s(&[CELLS]));
        g.write(at(o.events), &u64s(&[CELLS + 8]));
        Some(g)
    }

    pub(super) fn write(&mut self, to: u64, bytes: &[u8]) {
        self.m.uc.mem_write(to, bytes).unwrap();
    }

    fn read<const N: usize>(&self, from: u64) -> [u8; N] {
        let mut b = [0; N];
        self.m.uc.mem_read(from, &mut b).unwrap();
        b
    }

    pub(super) fn u64(&self, from: u64) -> u64 {
        u64::from_le_bytes(self.read(from))
    }

    fn f32s<const N: usize>(&self, from: u64) -> [f32; N] {
        let b: Vec<u8> = (0..N as u64 * 4)
            .map(|i| self.read::<1>(from + i)[0])
            .collect();
        std::array::from_fn(|i| f32::from_le_bytes(b[i * 4..][..4].try_into().unwrap()))
    }

    fn mailbox(&self, field: usize) -> u32 {
        let at = at(LIST) + (offset_of!(Shared, dev) + field) as u64;
        u32::from_le_bytes(self.read(at))
    }

    /// The swap the hook keeps: a lock word, then the swap.
    fn swap(&self) -> Swap {
        let b: [u8; size_of::<Swap>()] = self.read(at(LIST) + offset_of!(Shared, swap) as u64 + 8);
        // SAFETY: Swap is plain data, repr(C), read as the hook wrote it.
        unsafe { std::mem::transmute(b) }
    }

    /// The app asks for request `n`, `ask`.
    fn ask(&mut self, n: u32, ask: Ask) {
        let dev = at(LIST) + offset_of!(Shared, dev) as u64;
        self.write(
            dev + offset_of!(Mailbox, ask) as u64,
            &(ask as u32).to_le_bytes(),
        );
        self.write(dev + offset_of!(Mailbox, asked) as u64, &n.to_le_bytes());
    }

    /// One run of the function hook at `hooked` with `x0`, the game's
    /// routines it reaches stubbed, to its return.
    pub(super) fn call(&mut self, hooked: u32, x0: u64) {
        self.m.uc.reg_write(R::X0, x0).unwrap();
        self.m.uc.reg_write(R::SP, STACK + STACK_SIZE / 2).unwrap();
        self.m.uc.reg_write(R::X30, SENTINEL).unwrap();
        let mut pc = at(hooked);
        loop {
            *self.m.uc.get_data_mut() = Run {
                start: pc,
                until: SENTINEL,
                left: None,
            };
            let ran = self.m.uc.emu_start(pc, SENTINEL, 0, BUDGET);
            let now = self.m.uc.reg_read(R::PC).unwrap();
            ran.unwrap_or_else(|e| panic!("{e:?} at {now:#x}"));
            let Some(left) = self.m.uc.get_data().left else {
                assert_eq!(now, SENTINEL, "stopped at {now:#x} (budget?)");
                return;
            };
            let x0 = self.stub(hooked, (left - BASE) as u32);
            pc = self.m.uc.reg_read(R::X30).unwrap();
            self.m.uc.reg_write(R::X0, x0).unwrap();
        }
    }

    /// What the game's routine at `offset` does here, as `x0`.
    fn stub(&mut self, hooked: u32, offset: u32) -> u64 {
        let o = &OFFSETS;
        let x = |r| self.m.uc.reg_read(r).unwrap();
        let (x0, x1, x2) = (x(R::X0), x(R::X1), x(R::X2));
        match offset {
            _ if offset == hooked + 4 => 0,
            _ if offset == o.get_bool => {
                let value = self.bools.get(&(x2 as u32));
                self.write(x1, &[value.copied().unwrap_or(false) as u8]);
                1
            }
            _ if offset == o.set_bool => {
                self.bools.insert(x2 as u32, x1 & 1 == 1);
                self.calls.push(Call::SetBool(x2 as u32, x1 & 1 == 1));
                0
            }
            _ if offset == o.resident_create => {
                let made = if self.creates.is_empty() {
                    1
                } else {
                    self.creates.remove(0)
                };
                self.calls.push(Call::Create(x0, x1));
                made
            }
            _ if offset == o.cancel_sleep => {
                let followed = self.u64(CAMERA_COMPONENT + 0xfe0);
                self.calls
                    .push(Call::Wake(x0, self.f32s(x1), self.f32s(x2), followed));
                1
            }
            _ if offset == o.request_pause || offset == o.release_pause => {
                let name = self.u64(self.u64(x0 + 0x20));
                let text: Vec<u8> = (0..)
                    .map(|i| self.read::<1>(name + i)[0])
                    .take_while(|&b| b != 0)
                    .collect();
                let what = if offset == o.request_pause {
                    "request"
                } else {
                    "release"
                };
                self.calls
                    .push(Call::Pause(what, String::from_utf8(text).unwrap()));
                0
            }
            _ => panic!("the hook ran the game's code at {offset:#x}"),
        }
    }

    /// The game makes the residents: the hook keeps the manager.
    fn area_loaded(&mut self) {
        self.call(OFFSETS.resident_create, MANAGER);
        self.calls.clear();
    }

    /// Link's update (the mailbox), as long as he's there.
    fn link_update(&mut self) {
        self.call(OFFSETS.link_update, LINK);
    }

    /// The UI's update `n` times: the swap's frames.
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.call(OFFSETS.ui_update, 0);
        }
    }

    /// The new Link's entry holds him from now on.
    fn adult_made(&mut self) {
        self.write(LINKS + 0x200 + 0x48, &7u64.to_le_bytes());
    }

    fn answered(&self) -> (u32, u32) {
        let answered = self.mailbox(offset_of!(Mailbox, answered));
        (answered, self.mailbox(offset_of!(Mailbox, outcome)))
    }
}

fn pause(what: &'static str) -> Call {
    Call::Pause(what, "SceneTransition".into())
}

#[test]
fn a_swap_makes_the_other_age_and_wakes_him_where_link_stood_camera_kept() {
    let Some(mut g) = Game::new() else {
        return;
    };
    g.area_loaded();
    g.creates = vec![0, 1];
    g.ask(1, Ask::AgeHere);
    g.link_update();
    assert_eq!(g.calls, [pause("request"), Call::SetBool(CHILD, false)]);
    assert_eq!(g.answered().0, 0, "answered before the swap is done");

    // Create, until it says it made him; then he isn't there for a while.
    g.frames(5);
    assert_eq!(
        g.calls[2..],
        [Call::Create(MANAGER, 0), Call::Create(MANAGER, 0)]
    );
    assert_eq!(g.answered().0, 0);
    g.adult_made();
    g.frames(2);
    let woken = Call::Wake(MANAGER, POSITION, ROTATION, PART);
    assert_eq!(g.calls[4..], [woken, pause("release")]);
    assert_eq!(g.answered(), (1, Outcome::Done as u32));
    assert_eq!(g.swap(), Swap::default());
    // Nothing more once it's done.
    g.frames(3);
    assert_eq!(g.calls.len(), 6);
}

#[test]
fn a_swap_whose_new_link_never_comes_goes_back_and_says_stuck() {
    let Some(mut g) = Game::new() else {
        return;
    };
    g.area_loaded();
    g.ask(1, Ask::AgeHere);
    g.link_update();
    g.frames(1 + LIMIT + 1);
    // Given up: the age flipped back and the child made again.
    assert_eq!(
        g.calls[2..],
        [Call::Create(MANAGER, 0), Call::SetBool(CHILD, true)]
    );
    assert!(g.bools[&CHILD]);
    g.frames(1);
    assert_eq!(g.calls[4..], [Call::Create(MANAGER, 1)]);
    assert_eq!(g.answered().0, 0);
    // The child's entry holds him, but awake: still never asleep.
    g.frames(LIMIT + 1);
    assert_eq!(g.calls[5..], [pause("release")]);
    assert_eq!(g.answered(), (1, Outcome::Stuck as u32));
}

#[test]
fn a_swap_is_refused_before_an_area_load_and_while_one_is_under_way() {
    let Some(mut g) = Game::new() else {
        return;
    };
    g.ask(1, Ask::AgeHere);
    g.link_update();
    assert_eq!(g.answered(), (1, Outcome::NoResident as u32));
    assert_eq!(g.calls, []);

    g.area_loaded();
    g.ask(2, Ask::AgeHere);
    g.link_update();
    g.ask(3, Ask::AgeHere);
    g.link_update();
    assert_eq!(g.answered(), (3, Outcome::Soon as u32));
    assert_eq!(g.calls, [pause("request"), Call::SetBool(CHILD, false)]);
}
