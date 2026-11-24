//! The hook's bytes run in unicorn over `main` as the Mac's game runs it,
//! with the patch written in: no game, no turn. The memory beyond `main`
//! is fake, so these check what the hook does with the game's registers
//! and its own code, not the game's structures. Spec: hook unicorn tests
//! (private notes). Run with `cargo test -p zootr --features emu --lib --
//! patch::emu`.

use std::ops::Range;

use unicorn_engine::{Arch, Mode, Prot, RegisterARM64 as R, Unicorn};

use super::tests::pc_main;
use super::*;

/// Where `main` is mapped.
const BASE: u64 = 0x1000_0000;
/// The stack, its top, and a page of zeros each argument points into.
const STACK: u64 = 0x2000_0000;
const STACK_SIZE: u64 = 0x1_0000;
const SCRATCH: u64 = 0x3000_0000;
/// The return address a run ends on, a page of `brk`s.
const SENTINEL: u64 = 0x4000_0000;
/// What a stubbed game function returns in `x0`, distinct at every width.
const MARK: u64 = 0x0123_4567_89ab_cdef;
/// Instructions a run may take before it counts as a loop.
const BUDGET: usize = 1_000_000;

/// The argument registers: `x0` to `x8` (`x8` the indirect result) and
/// `d0` to `d7`. A callee reads the first few of each.
const INTS: [R; 9] = [
    R::X0,
    R::X1,
    R::X2,
    R::X3,
    R::X4,
    R::X5,
    R::X6,
    R::X7,
    R::X8,
];
const FLOATS: [R; 8] = [R::D0, R::D1, R::D2, R::D3, R::D4, R::D5, R::D6, R::D7];
/// What a call must leave as it found: `x18` the platform's, `x19` to
/// `x29`, `d8` to `d15` and `sp`.
const KEPT: [R; 21] = [
    R::X18,
    R::X19,
    R::X20,
    R::X21,
    R::X22,
    R::X23,
    R::X24,
    R::X25,
    R::X26,
    R::X27,
    R::X28,
    R::X29,
    R::D8,
    R::D9,
    R::D10,
    R::D11,
    R::D12,
    R::D13,
    R::D14,
    R::D15,
    R::SP,
];

/// How many integer and float arguments a game function reads, and the
/// bits of `x0` its result fills (0 for none).
#[derive(Clone, Copy)]
struct Reads(usize, usize, u32);

/// What each original a call site calls reads, from Ghidra's decompiler
/// (2026-11-23), or what the hook forwards where that is more: build
/// `b`'s functions.
fn reads(b: &Build, callee: u32) -> Reads {
    let (f, i) = (&b.funcs, &b.imports);
    match callee {
        // The event request's string setter: (request, key, value).
        c if c == f.setter => Reads(3, 0, 0),
        // The drop reader: Ghidra sees two and no result, the hook
        // forwards four and a word.
        c if c == f.reader => Reads(4, 0, 64),
        // The child test: (link) -> a ushort.
        c if c == f.childhood => Reads(1, 0, 16),
        // The run node's speed setter: (state, dir) and the speed in s0.
        c if c == f.run_setter => Reads(2, 1, 0),
        // nn::hid::GetNpadStates' stubs: (states, count, id) -> an int.
        c if [i.npad_full, i.npad_handheld, i.npad_dual].contains(&c) => Reads(3, 0, 32),
        _ => panic!("what does {callee:#x} read?"),
    }
}

/// The machine, the hook it runs, and the dump's own word at each address
/// the patch changes.
struct Machine {
    uc: Unicorn<'static, Run>,
    h: &'static Hook,
    original: Vec<u8>,
}

/// Where the last run stopped outside the hook, if it did.
#[derive(Default)]
struct Run {
    start: u64,
    until: u64,
    left: Option<u64>,
}

fn at(offset: u32) -> u64 {
    BASE + u64::from(offset)
}

impl Machine {
    /// `h`'s build of `main` as the Mac game runs it, with its code
    /// patches and no table, or `None` without that build's conversion
    /// (as on CI).
    fn new(h: &'static Hook) -> Option<Self> {
        let original = pc_main(h.release)?;
        let r = &h.build.regions;
        let mut uc =
            Unicorn::new_with_data(Arch::ARM64, Mode::LITTLE_ENDIAN, Run::default()).unwrap();
        let size = (original.len() as u64)
            .max(u64::from(r.list_end))
            .next_multiple_of(0x1000);
        uc.mem_map(BASE, size, Prot::ALL).unwrap();
        uc.mem_write(BASE, &original).unwrap();
        for p in h.code_patches().unwrap() {
            uc.mem_write(at(p.at), &p.bytes).unwrap();
        }
        uc.mem_map(STACK, STACK_SIZE, Prot::READ | Prot::WRITE)
            .unwrap();
        uc.mem_map(SCRATCH, 0x1_0000, Prot::READ | Prot::WRITE)
            .unwrap();
        uc.mem_map(SENTINEL, 0x1000, Prot::ALL).unwrap();
        uc.mem_write(SENTINEL, &[0x00, 0x00, 0x20, 0xd4].repeat(0x400))
            .unwrap();
        // Every instruction outside the hook's pieces is the game's: the
        // first one past the start stops the run, unless it's where the run
        // ends.
        let ours: [Range<u32>; 2] = [r.cave..r.cave_end, r.far..r.far_end];
        uc.add_code_hook(BASE, at(r.text_end), move |uc, pc, _| {
            let ours = ours.iter().any(|r| r.contains(&((pc - BASE) as u32)));
            let run = uc.get_data_mut();
            if !ours && pc != run.start && pc != run.until && run.left.is_none() {
                run.left = Some(pc);
                uc.emu_stop().unwrap();
            }
        })
        .unwrap();
        Some(Self { uc, h, original })
    }

    /// The dump's `bl` target at a call site.
    fn callee(&self, site: u32) -> u32 {
        let word = u32::from_le_bytes(self.original[site as usize..][..4].try_into().unwrap());
        assert_eq!(word >> 26, 0x25, "{site:#x} isn't a bl");
        let delta = ((word & 0x03ff_ffff) << 6) as i32 >> 4;
        site.wrapping_add_signed(delta)
    }

    /// Distinct values in every argument and kept register, and `x30`
    /// the sentinel. Returns what each was set to.
    fn set_registers(&mut self) -> Vec<(R, u64)> {
        let ints = INTS
            .iter()
            .enumerate()
            .map(|(i, &r)| (r, SCRATCH + 0x100 * i as u64));
        // 1.5f and up in the low half, which an f32 argument reads.
        let floats = FLOATS
            .iter()
            .enumerate()
            .map(|(i, &r)| (r, 0x7777_0000_3fc0_0000 + i as u64));
        let kept = KEPT
            .iter()
            .enumerate()
            .map(|(i, &r)| (r, 0x1800 + i as u64));
        let mut set: Vec<(R, u64)> = ints.chain(floats).chain(kept).collect();
        // sp mid-stack, 16 aligned.
        set.last_mut().unwrap().1 = STACK + STACK_SIZE / 2;
        for &(r, v) in &set {
            self.uc.reg_write(r, v).unwrap();
        }
        self.uc.reg_write(R::X30, SENTINEL).unwrap();
        set
    }

    /// Each of `regs` that differs from what `set` gave it, with its value.
    /// A float argument is an f32, its register's low half: writing one
    /// clears the rest.
    fn changed(&self, set: &[(R, u64)], regs: &[R]) -> Vec<String> {
        let width = |r: R| {
            if FLOATS.contains(&r) {
                0xffff_ffff
            } else {
                u64::MAX
            }
        };
        (set.iter().filter(|(r, _)| regs.contains(r)))
            .filter_map(|&(r, was)| {
                let (was, now) = (was & width(r), self.uc.reg_read(r).unwrap() & width(r));
                (now != was).then(|| format!("{r:?} {was:#x} -> {now:#x}"))
            })
            .collect()
    }

    /// Runs from `start` until `until`, failing on game code before it, a
    /// fault, or a run past the budget.
    fn run(&mut self, start: u64, until: u64, what: &str) {
        *self.uc.get_data_mut() = Run {
            start,
            until,
            left: None,
        };
        let ran = self.uc.emu_start(start, until, 0, BUDGET);
        let pc = self.uc.reg_read(R::PC).unwrap();
        if let Some(left) = self.uc.get_data().left {
            panic!("{what}: ran the game's code at {:#x}", left - BASE);
        }
        ran.unwrap_or_else(|e| panic!("{what}: {e:?} at {pc:#x}"));
        assert_eq!(pc, until, "{what}: stopped at {pc:#x} (budget?)");
    }

    /// The game's function at `pc` returns `MARK` at once.
    fn stub_return(&mut self) {
        let lr = self.uc.reg_read(R::X30).unwrap();
        self.uc.reg_write(R::X0, MARK).unwrap();
        self.uc.reg_write(R::PC, lr).unwrap();
    }

    /// Runs `start` with the registers set, into `original` (checking
    /// the arguments it reads arrive as given), stubs it, and runs on to
    /// `end` (checking the original's result and every kept register).
    fn passes_through(&mut self, start: u32, original: u32, end: u64, reads: Reads, what: &str) {
        let set = self.set_registers();
        self.run(at(start), at(original), what);
        let args = [&INTS[..reads.0], &FLOATS[..reads.1]].concat();
        assert_eq!(
            self.changed(&set, &args),
            [""; 0],
            "{what}: the arguments the original reads"
        );
        let lr = self.uc.reg_read(R::X30).unwrap();
        self.stub_return();
        self.run(lr, end, what);
        let mask = u64::MAX.checked_shr(64 - reads.2).unwrap_or(0);
        let result = self.uc.reg_read(R::X0).unwrap() & mask;
        assert_eq!(result, MARK & mask, "{what}: the original's result");
        assert_eq!(
            self.changed(&set, &KEPT),
            [""; 0],
            "{what}: the registers a call keeps"
        );
    }
}

/// A machine for each build whose `main` is here.
fn machines() -> impl Iterator<Item = Machine> {
    Hook::all().iter().filter_map(Machine::new)
}

#[test]
fn without_a_table_each_call_site_calls_its_original_and_nothing_else() {
    for mut m in machines() {
        let b = &m.h.build;
        let sites = calls(b)
            .into_iter()
            .chain(npad_calls(b))
            .chain([run_call(b)]);
        for (site, _) in sites {
            let original = m.callee(site);
            let what = format!("{} call at {site:#x}", m.h.release.name());
            m.passes_through(site, original, at(site + 4), reads(b, original), &what);
        }
    }
}

#[test]
fn without_a_table_each_hooked_function_runs_its_own_body_and_nothing_else() {
    for mut m in machines() {
        for (hooked, _) in hooks(&m.h.build) {
            // The body is the whole function: every argument register.
            let all = Reads(INTS.len(), FLOATS.len(), 64);
            m.passes_through(
                hooked,
                hooked + 4,
                SENTINEL,
                all,
                &format!("{} hook at {hooked:#x}", m.h.release.name()),
            );
        }
    }
}

mod arrows;
mod swap;
