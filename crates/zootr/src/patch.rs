//! Our code in the game: the hook from `zootr-hook` and its table, written
//! into `main` by a cheat file, an exefs patch, and an IPS for a Switch.
//! Offsets are game 1.1.1's, see `docs/research/binary.md`. The sites and
//! the table are built here; `files` writes them out and manages the files.

use anyhow::{Result, ensure};
pub use zootr_hook::OFFSETS;
use zootr_hook::entry;
use zootr_hook::look::LookSpec;
use zootr_hook::{
    CAVE, Credit, DEV, FAR, LIST, MENU_CYCLE, PANIC, SLOTS, Shared, Spot, TABLE, encode,
};

use crate::menu_slots;

mod files;
pub use files::{EXEFS, exefs, install, pchtxt, remove, remove_cheat, split};
#[cfg(test)]
pub use files::{ips_path, path};

/// The hook's code, built by `build.rs`.
const HOOK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/hook.bin"));
/// The hook's far code, written at `FAR`.
const FAR_CODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/far.bin"));
/// Where the recorder, and the free space after rodata and bss, end.
const CAVE_END: u32 = 0x17f_58e0;
/// Where the far code's room ends: the first word past `FAR` the Mac's
/// game rewrites, which a cheat can't write over.
const FAR_END: u32 = 0x17f_8a1c;
const TABLE_END: u32 = 0x484_d000;
const LIST_END: u32 = 0x4e9_b000;
const _: () = assert!(LIST + size_of::<Shared>() as u32 <= PANIC && LIST.is_multiple_of(8));
const _: () = assert!(PANIC + 16 == LIST_END);
/// The physics step's `cbz` over the call into Havok's trace recorder, and
/// where it skips to. Made a `b`, the recorder never runs and holds `CAVE`.
const GATE: (u32, u32) = (0x17d_59bc, 0x17d_59c4);
/// Link's "can he catch the Boomerang" check's `tbz` into its child test
/// (the age override, then `PlayerGameParam.IsChild`), and where it skips
/// to: whether the Boomerang is back. Made a `b`, the adult catches it as
/// the child does, and every selector that asks gets the same answer.
const CATCH: (u32, u32) = (0x41_0250, 0x41_0258);
/// The `b`s written over the game's own code, each `(at, to)`.
const SKIPS: [(u32, u32); 2] = [GATE, CATCH];
/// Each call site and the hook entry its `bl` goes to, as an offset into
/// the hook. The event request's string argument calls, where an opening
/// chest sets its `ContentName`, go to `content_name`. The drop reader's
/// calls, where a loading room reads a chest's placement, go to
/// `drop_actor`. The bow draw's child test, which picks its guide, goes
/// to `seed_guide`, so a child Bow says "Shoot Arrow".
const CALLS: [(u32, u32); 5] = [
    (0x2ae_0828, entry::CONTENT_NAME),
    (0x2ae_08cc, entry::CONTENT_NAME),
    (0x2ef_6560, entry::DROP_ACTOR),
    (0x2ef_4888, entry::DROP_ACTOR),
    (0x4b_01a4, entry::SEED_GUIDE),
];
/// The run node's call to the speed setter and the hook entry it goes to,
/// `run_speed`, which scales the speed in the Bunny Hood in a seed with
/// the setting.
const RUN_CALL: (u32, u32) = (0x93_4990, entry::RUN_SPEED);
/// The controller device's three `GetNpadStates` calls (full key,
/// handheld, joy dual) and their hook entries, which run the trade slot's
/// cycle with the three Items page hooks, in a seed with [`MENU_CYCLE`].
const NPAD_CALLS: [(u32, u32); 3] = [
    (0x346_966c, entry::NPAD_FULL),
    (0x346_9698, entry::NPAD_HANDHELD),
    (0x346_94f8, entry::NPAD_DUAL),
];
/// The game's stubs for `nn::hid::GetNpadStates` those calls go to, in
/// the same order. The hook reaches them through its slots.
const NPAD_STUBS: [u32; 3] = [0x3c4_2800, 0x3c4_2830, 0x3c4_27f0];
/// Each hooked function and the entry the `b` over its `pacibsp` goes to.
/// Link's update runs the mailbox's requests and the dev switches in a
/// seed with [`DEV`], and springs ice traps. The nock sets Link's
/// projectile to the drawn bow's, so a child Bow fires arrows, and the out
/// of ammo handler's message follows that projectile, not Link's age.
const HOOKS: &[(u32, u32)] = &[
    (OFFSETS.map_loaded, entry::MAP_LOADED),
    (OFFSETS.chest_made, entry::CHEST_MADE),
    (OFFSETS.granted, entry::GRANTED),
    (OFFSETS.link_update, entry::LINK_UPDATE),
    (OFFSETS.picked, entry::PICKED),
    (OFFSETS.items_built, entry::ITEMS_BUILT),
    (OFFSETS.items_left, entry::ITEMS_LEFT),
    (OFFSETS.ui_update, entry::UI_UPDATE),
    (OFFSETS.taken, entry::TAKEN),
    (OFFSETS.nock, entry::NOCKED),
    (OFFSETS.emptied, entry::EMPTIED),
    (OFFSETS.equipment_built, entry::EQUIPMENT_BUILT),
    (OFFSETS.equipment_left, entry::EQUIPMENT_LEFT),
    (OFFSETS.resident_create, entry::RESIDENT_MADE),
];
/// `main`'s build ID, as its NSO header's 32 byte field holds it. Its first
/// 8 bytes name the cheat file and the rest, trailing zeros left out, the
/// exefs patch's `@nsobid`, so a build these offsets weren't found in never
/// loads either.
pub const BUILD_ID: [u8; 32] = [
    0xe0, 0xf3, 0xcc, 0xfa, 0xf5, 0x00, 0x65, 0xae, 0x4e, 0xa0, 0x1e, 0xb5, 0xae, 0xb1, 0x3b, 0x1a,
    0x45, 0xda, 0x33, 0x48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Bytes to write at an offset into `main`.
#[derive(Debug, PartialEq)]
pub struct Patch {
    pub at: u32,
    pub bytes: Vec<u8>,
}

/// `bl to`, or `b to` without `link`, placed at `from`.
fn branch(from: u32, to: u32, link: bool) -> [u8; 4] {
    let delta = i64::from(to) - i64::from(from);
    assert!(
        delta % 4 == 0 && (-(1 << 27)..1 << 27).contains(&delta),
        "branch out of range"
    );
    let op = if link { 0x9400_0000 } else { 0x1400_0000 };
    (op | ((delta >> 2) as u32 & 0x03ff_ffff)).to_le_bytes()
}

/// The calls and hooked functions, each branching to its hook entry.
fn sites() -> impl Iterator<Item = Patch> {
    let calls = CALLS.into_iter().chain(NPAD_CALLS);
    let calls = calls.map(|(at, entry)| (at, entry, true));
    let hooks = HOOKS.iter().map(|&(at, entry)| (at, entry, false));
    let run = (RUN_CALL.0, RUN_CALL.1, true);
    calls
        .chain(hooks)
        .chain([run])
        .map(|(at, entry, link)| Patch {
            at,
            bytes: branch(at, CAVE + entry, link).to_vec(),
        })
}

/// Where each of the hook's slots branches, in their order: past each
/// hooked function's `pacibsp`, then to each controller read's stub.
fn slot_targets() -> impl Iterator<Item = u32> {
    HOOKS.iter().map(|&(at, _)| at + 4).chain(NPAD_STUBS)
}

/// The hook, with each slot's `b` written in.
fn hook_code() -> Vec<u8> {
    let mut code = HOOK.to_vec();
    for (i, to) in slot_targets().enumerate() {
        let slot = SLOTS + 4 * i as u32;
        code[slot as usize..][..4].copy_from_slice(&branch(CAVE + slot, to, false));
    }
    code
}

/// What the patch writes over a hooked function's `pacibsp`, if `at` is one.
pub fn site(at: u32) -> Option<Vec<u8>> {
    sites().find(|p| p.at == at).map(|p| p.bytes)
}

/// `bytes` at `at`, padded to a word, or an error past `end`.
fn fit(at: u32, mut bytes: Vec<u8>, end: u32, what: &str) -> Result<Patch> {
    // Writes go a word at a time.
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    let free = (end - at) as usize;
    ensure!(
        bytes.len() <= free,
        "the {what} needs {} bytes, {free} are free",
        bytes.len()
    );
    Ok(Patch { at, bytes })
}

/// The skips, the hook and its far code.
fn code() -> Result<Vec<Patch>> {
    let skips = SKIPS.map(|(at, to)| Patch {
        at,
        bytes: branch(at, to, false).to_vec(),
    });
    let code = fit(CAVE, hook_code(), CAVE_END, "hook")?;
    let far = fit(FAR, FAR_CODE.to_vec(), FAR_END, "far code")?;
    Ok(skips.into_iter().chain([code, far]).collect())
}

/// The skips, the hook, its far code and every call site: the same for
/// every seed, since each hook entry checks its own setting in the table.
/// A game that builds the exefs patch in rebuilds only when this changes.
pub fn code_patches() -> Result<Vec<Patch>> {
    Ok(code()?.into_iter().chain(sites()).collect())
}

/// The table's writes: its body, then its magic, so a table caught
/// halfway never parses. The table is whole words.
fn table_writes(Patch { at, mut bytes }: Patch) -> [Patch; 2] {
    let body = bytes.split_off(4);
    [
        Patch {
            at: at + 4,
            bytes: body,
        },
        Patch { at, bytes },
    ]
}

/// The skips, the hook, its table for `lines`, the [`zootr_hook::KEYED`]
/// lines `keyed`, `flags` (with [`DEV`] always, so the mailbox answers in
/// every seed), `credits` and `spots`, then the calls and function hooks
/// that reach it, every one in every seed.
pub fn patches<A: AsRef<str>>(
    lines: &[&[(A, A)]],
    keyed: &[Vec<(String, String)>],
    flags: u32,
    credits: &[(&str, Credit)],
    spots: &[Spot],
    look: Option<&LookSpec>,
) -> Result<Vec<Patch>> {
    let owned: Vec<Vec<(&str, &str)>> = (lines.iter())
        .map(|l| l.iter().map(|(a, b)| (a.as_ref(), b.as_ref())).collect())
        .collect();
    let lines: Vec<&[(&str, &str)]> = owned.iter().map(Vec::as_slice).collect();
    let keyed: Vec<Vec<(&str, &str)>> = (keyed.iter())
        .map(|l| l.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect())
        .collect();
    let keyed: Vec<&[(&str, &str)]> = keyed.iter().map(Vec::as_slice).collect();
    let flags = flags | DEV;
    let cycled = menu_slots::ALL.iter().filter(|_| flags & MENU_CYCLE != 0);
    let actors: Vec<_> = cycled.clone().map(|s| s.actors()).collect();
    let slots: Vec<_> = cycled.zip(&actors).map(|(s, a)| s.spec(a)).collect();
    let table = encode(flags, &lines, &keyed, credits, &slots, spots, look);
    let table = fit(TABLE, table, TABLE_END, "table")?;
    Ok((code()?.into_iter())
        .chain(table_writes(table))
        .chain(sites())
        .collect())
}

#[cfg(all(test, feature = "emu"))]
mod emu;

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use crate::progressive;
    use zootr_hook::{BUNNY_HOOD, FREE_BOMBCHUS, HEADER, Header, ICE_TRAPS, MAGIC, Table, fnv};

    use super::*;

    /// Every flag that adds a site.
    pub(super) const ALL: u32 = BUNNY_HOOD | MENU_CYCLE;
    /// A holder that stands until picked, which adds the take's site.
    pub(super) const SPOT: Spot = Spot {
        flag: 1,
        name: 2,
        at: [3, 4, 5],
        placed: 6,
        twin: 7,
    };

    #[test]
    fn the_hooks_seed_hash_is_the_deku_seeds_name() {
        if crate::game::no_names() {
            return;
        }
        let name = crate::known::actor::SHOOTABLE_DEKU_SEED.get();
        assert_eq!(
            ounce_formats::murmur3(name.as_bytes()),
            zootr_hook::DEKU_SEED
        );
    }

    #[test]
    fn the_fullest_table_fits() {
        use crate::settings::DungeonKeys;
        // The longest ammo lines, one for each prize chest. Their names
        // come from the dump.
        let mut keyed: Vec<_> = match crate::game::dev_dump() {
            Some(_) => (crate::ammo::NEEDS.iter())
                .map(|(a, _)| crate::ammo::line(a).unwrap())
                .collect(),
            None => Vec::new(),
        };
        let size =
            |l: &Vec<(String, String)>| -> usize { l.iter().map(|(a, b)| a.len() + b.len()).sum() };
        keyed.sort_by_key(|l| std::cmp::Reverse(size(l)));
        keyed.truncate(crate::gifts::PRIZE_CHESTS.len());
        for keys in [
            DungeonKeys::Vanilla,
            DungeonKeys::Rings,
            DungeonKeys::Singles,
        ] {
            let credits = crate::key_tints::credits(true, keys, true);
            let credits: Vec<_> = credits.iter().map(|(n, c)| (n.as_str(), *c)).collect();
            let p = patches(&progressive::LINES, &keyed, ALL, &credits, &[SPOT], None);
            assert!(p.is_ok(), "{keys:?}: {}", p.unwrap_err());
        }
    }

    /// Where the table's two writes start in `patches`: after the skips,
    /// the hook and its far code.
    pub(super) const TABLE_AT: usize = SKIPS.len() + 2;
    /// Where the sites start in `patches`, after the table.
    pub(super) const SITES_AT: usize = TABLE_AT + 2;

    /// `patches` without its table's two writes.
    fn without_table(p: Vec<Patch>) -> Vec<Patch> {
        p.into_iter()
            .filter(|p| !(TABLE..TABLE_END).contains(&p.at))
            .collect()
    }

    #[test]
    fn every_seed_writes_the_same_code_whatever_its_settings() {
        let code = code_patches().unwrap();
        for (flags, spots) in [(0, &[][..]), (ALL | ICE_TRAPS | FREE_BOMBCHUS, &[SPOT])] {
            let p = patches(&progressive::LINES, &[], flags, &[], spots, None).unwrap();
            assert_eq!(without_table(p), code, "{flags}");
        }
    }

    #[test]
    fn every_seed_s_table_answers_the_mailbox() {
        let p = patches(&progressive::LINES, &[], 0, &[], &[], None).unwrap();
        let magic = p.iter().find(|p| p.at == TABLE).unwrap();
        let body = p.iter().find(|p| p.at == TABLE + 4).unwrap();
        let header = Header::read(&[&magic.bytes[..], &body.bytes].concat());
        assert_ne!(header.flags & DEV, 0);
    }

    #[test]
    fn the_code_and_table_go_in_their_places_and_read_back() {
        let p = patches(&progressive::LINES, &[], 0, &[], &[], None).unwrap();
        let [code, far, body, magic, ..] = &p[SKIPS.len()..] else {
            panic!("patches")
        };
        assert_eq!((code.at, code.bytes.len()), (CAVE, HOOK.len()));
        assert_eq!((far.at, &far.bytes[..FAR_CODE.len()]), (FAR, FAR_CODE));
        assert_eq!((magic.at, &magic.bytes[..]), (TABLE, &MAGIC[..]));
        assert_eq!(magic.at % 8, 0, "the header is read in place");
        assert_eq!(body.at, TABLE + 4);
        let bytes = [&magic.bytes[..], &body.bytes].concat();
        let header = Header::read(&bytes);
        let lines = &bytes[HEADER..][..header.size as usize];
        let table = Table::parse(&header, lines).unwrap();
        for line in progressive::LINES {
            let (_, read, _) = table.find(line[1].0.as_bytes()).unwrap();
            let read: Vec<_> = read.collect();
            let names: Vec<_> = read.iter().map(|l| (l.drop, l.pouch)).collect();
            let want: Vec<_> = line
                .iter()
                .map(|(d, p)| (d.as_bytes(), p.as_bytes()))
                .collect();
            assert_eq!(names, want);
        }
    }

    #[test]
    fn each_slot_branches_past_its_pacibsp_or_to_its_stub() {
        let word =
            |b: &[u8], at: u32| u32::from_le_bytes(b[at as usize..][..4].try_into().unwrap());
        let code = hook_code();
        let resumes = HOOKS.iter().map(|&(at, _)| at + 4);
        let targets: Vec<_> = resumes.chain(NPAD_STUBS).collect();
        assert_eq!(targets.len(), HOOKS.len() + NPAD_CALLS.len());
        for (i, &to) in targets.iter().enumerate() {
            let slot = SLOTS + 4 * i as u32;
            // brk #0 until the patch fills it.
            assert_eq!(word(HOOK, slot), 0xd420_0000, "slot {i}");
            assert_eq!(code[slot as usize..][..4], branch(CAVE + slot, to, false));
        }
        assert_eq!(code[..SLOTS as usize], HOOK[..SLOTS as usize]);
        let end = SLOTS as usize + 4 * targets.len();
        assert_eq!(code[end..], HOOK[end..]);
        // Past the last slot is code, not another slot left unfilled.
        assert_ne!(word(HOOK, end as u32), 0xd420_0000);
    }

    #[test]
    fn the_gate_skips_the_recorder() {
        let gate = &patches(&progressive::LINES, &[], 0, &[], &[], None).unwrap()[0];
        assert_eq!(gate.at, 0x17d_59bc);
        // b 0x17d59c4
        assert_eq!(gate.bytes, 0x1400_0002u32.to_le_bytes());
    }

    #[test]
    fn the_catch_skips_the_child_test() {
        let catch = &patches(&progressive::LINES, &[], 0, &[], &[], None).unwrap()[1];
        assert_eq!(catch.at, 0x41_0250);
        // b 0x410258, over tbz w9, #8, 0x4102f0
        assert_eq!(catch.bytes, 0x1400_0002u32.to_le_bytes());
    }

    #[test]
    fn the_hook_never_uses_x18() {
        use yaxpeax_arch::{Decoder, U8Reader};
        use yaxpeax_arm::armv8::a64::InstDecoder;
        let x18 = |word: &[u8]| {
            let inst = InstDecoder::default()
                .decode(&mut U8Reader::new(word))
                .unwrap();
            let text = inst.to_string();
            let hit = text
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w == "x18" || w == "w18");
            hit.then_some(text)
        };
        // mov x18, x0 and ldrb w18, [x9, #0x12]
        assert!(x18(&0xaa00_03f2u32.to_le_bytes()).is_some());
        assert!(x18(&0x3940_4932u32.to_le_bytes()).is_some());
        for (at, bytes, code) in pieces() {
            for (i, word) in bytes[..code].chunks(4).enumerate() {
                assert_eq!(x18(word), None, "at {:#x}", at as usize + i * 4);
            }
        }
    }

    /// Each piece of the hook: where it goes, its bytes, and how many of
    /// them are code. The cave's strings follow its code.
    fn pieces() -> [(u32, &'static [u8], usize); 2] {
        let code: usize = env!("HOOK_CODE").parse().unwrap();
        [(CAVE, HOOK, code), (FAR, FAR_CODE, FAR_CODE.len())]
    }

    #[test]
    fn page_relative_addresses_land_in_the_hook() {
        let hook = pieces().map(|(at, bytes, _)| i64::from(at)..i64::from(at) + bytes.len() as i64);
        for (base, bytes, code) in pieces() {
            page_relative_addresses_land_in(&hook, base, bytes, code);
        }
    }

    fn page_relative_addresses_land_in(hook: &[Range<i64>], base: u32, bytes: &[u8], code: usize) {
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        for i in 0..code / 4 {
            let adrp = word(i);
            if adrp & 0x9f00_0000 != 0x9000_0000 {
                continue;
            }
            let pages = ((adrp >> 29) & 3) | (((adrp >> 5) & 0x7ffff) << 2);
            let pages = i64::from(((pages << 11) as i32) >> 11);
            let at = i64::from(base) + i as i64 * 4;
            let page = (at & !0xfff) + (pages << 12);
            // The `add` that finishes the address, from the same register.
            let rd = adrp & 0x1f;
            let add = (i + 1..(i + 8).min(code / 4))
                .map(word)
                .find(|w| w & 0xffc0_0000 == 0x9100_0000 && (w >> 5) & 0x1f == rd);
            if let Some(add) = add {
                let target = page + i64::from((add >> 10) & 0xfff);
                assert!(
                    hook.iter().any(|h| h.contains(&target)),
                    "adrp at {at:#x} to {target:#x}"
                );
            }
        }
    }

    #[test]
    fn calls_and_hooks_branch_to_the_hook() {
        let p = patches(&progressive::LINES, &[], ALL, &[], &[SPOT], None).unwrap();
        assert_eq!(p[SITES_AT..], sites().collect::<Vec<_>>());
        let at = |i: usize| u32::from_le_bytes(p[SITES_AT + i].bytes[..].try_into().unwrap());
        assert_eq!(at(0) >> 26, 0b100101, "a call stays a bl");
        assert_eq!(
            at(CALLS.len() + NPAD_CALLS.len()) >> 26,
            0b000101,
            "a hook is a b"
        );
        let run = p.last().unwrap();
        assert_eq!(run.at, RUN_CALL.0);
        assert_eq!(run.bytes, branch(RUN_CALL.0, CAVE + RUN_CALL.1, true));
        assert_eq!(branch(0x100, 0x104, true), 0x9400_0001u32.to_le_bytes());
        assert_eq!(branch(0x104, 0x100, true), 0x97ff_ffffu32.to_le_bytes());
        assert_eq!(branch(0x100, 0x108, false), 0x1400_0002u32.to_le_bytes());
        assert!(std::panic::catch_unwind(|| branch(0, 1 << 27, true)).is_err());
    }

    #[test]
    fn a_table_too_big_for_the_space_is_refused() {
        let name = "Item_".repeat(40);
        let big = vec![(name.as_str(), name.as_str()); 20];
        assert!(patches(&[&big], &[], 0, &[], &[], None).is_err());
    }

    /// `main` as the Mac's game runs it: the dump's, but for the words it
    /// rewrote, which its cheats can't write over. A word that holds the
    /// dump's original here is one it left alone.
    pub(super) const PC_MAIN: &str =
        "../../dump/0400486024DEC000_build/patched_v131072/main.patched.bin";

    /// FNV-1a over 64 bits, enough to tell two patches apart.
    fn fingerprint(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Pins the 1.1.1 patch: one hash for each thing the patch writes, for
    /// a fixed seed (every setting that adds a site, a spot, credits and
    /// the progressive lines). Moving an offset, or the table the offsets
    /// come from, changes none of them. A change that is meant to change
    /// the patch updates these in the same commit and says why.
    #[test]
    fn the_1_1_1_patch_is_pinned() {
        let credits = [
            ("Key_01", Credit::Bool(0x1234_5678)),
            ("Key_02", Credit::Int(0xdead_beef, -3)),
            ("Key_03", Credit::Add(0xfeed_face, 1)),
        ];
        let all = ALL | ICE_TRAPS | FREE_BOMBCHUS;
        let seed = patches(&progressive::LINES, &[], all, &credits, &[SPOT], None).unwrap();
        let flat = |p: &[Patch]| -> Vec<u8> {
            (p.iter())
                .flat_map(|p| p.at.to_le_bytes().into_iter().chain(p.bytes.iter().copied()))
                .collect()
        };
        let sites: Vec<_> = sites().collect();
        let got = [
            ("hook", fingerprint(HOOK)),
            ("far", fingerprint(FAR_CODE)),
            ("code", fingerprint(&flat(&code_patches().unwrap()))),
            ("sites", fingerprint(&flat(&sites))),
            ("patches", fingerprint(&flat(&seed))),
            ("pchtxt", fingerprint(pchtxt(&seed).as_bytes())),
            ("ips", fingerprint(&files::ips(&seed))),
            ("cheat", fingerprint(files::cheat(&seed).as_bytes())),
        ];
        let want = [
            ("hook", 0x819e_56db_d5b1_8f40),
            ("far", 0x6244_fff5_6621_104a),
            ("code", 0x433f_a38d_1679_8e07),
            ("sites", 0x8316_48fb_631c_ff1c),
            ("patches", 0x315a_54b3_66ae_b5ef),
            ("pchtxt", 0x09ac_e57c_7b0a_411d),
            ("ips", 0x1083_af06_e0cb_712f),
            ("cheat", 0x451b_71a4_113e_7a7f),
        ];
        assert_eq!(got, want, "{got:#x?}");
    }

    #[test]
    fn every_word_the_patch_writes_over_holds_what_we_expect() {
        let Ok(main) = std::fs::read(PC_MAIN) else {
            return;
        };
        let word = |at: u32| u32::from_le_bytes(main[at as usize..][..4].try_into().unwrap());
        let room = |at: u32, end: u32| &main[at as usize..end as usize];
        assert_eq!(word(GATE.0), 0x3400_0048, "cbz w8, 0x17d59c4");
        assert_eq!(word(CATCH.0), 0x3640_0509, "tbz w9, #8, 0x4102f0");
        for (at, _) in CALLS.into_iter().chain([RUN_CALL]) {
            assert_eq!(word(at) >> 26, 0x25, "{at:#x} isn't a bl");
        }
        for (&(at, _), stub) in NPAD_CALLS.iter().zip(NPAD_STUBS) {
            assert_eq!(word(at).to_le_bytes(), branch(at, stub, true), "{at:#x}");
        }
        for &(at, _) in HOOKS {
            assert_eq!(word(at), 0xd503_237f, "{at:#x} isn't a pacibsp");
        }
        // Havok's trace recorder, and a function only its dispatcher
        // calls, as 1.1.1 has them, untouched by the Mac's game.
        assert_eq!(fnv(room(CAVE, CAVE_END)), 0xc734_cad6, "the cave");
        assert_eq!(fnv(room(FAR, FAR_END)), 0x7aea_f248, "the far code's room");
        assert!(
            room(TABLE, TABLE_END).iter().all(|&b| b == 0),
            "the table's room"
        );
        // Each patch starts at a place checked above.
        let sites = SKIPS.iter().chain(&CALLS).chain(&NPAD_CALLS).chain(HOOKS);
        let checked: Vec<u32> = (sites.map(|&(at, _)| at))
            .chain([RUN_CALL.0, CAVE, FAR, TABLE, TABLE + 4])
            .collect();
        for p in patches(&progressive::LINES, &[], 0, &[], &[], None).unwrap() {
            assert!(
                checked.contains(&p.at),
                "nothing checks what {:#x} holds",
                p.at
            );
        }
    }
}
