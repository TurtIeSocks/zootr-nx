//! Our code in the game: the hook from `zootr-hook` and its table, written
//! into `main` by a cheat file. Offsets are game 1.1.0's, see
//! `docs/research/binary.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use zootr_hook::{BUNNY_HOOD, CAVE, Credit, LIST, Offsets, PANIC, RESUMES, Shared, TABLE, encode};

use crate::known::Name;
use crate::progressive;

/// The hook's code, built by `build.rs`.
const HOOK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/hook.bin"));
pub const OFFSETS: Offsets = Offsets {
    setter: 0xfe_3fe0,
    reader: 0xbd_1c00,
    count: 0x59_63e0,
    manager: 0x4e7_8e28,
    hash: 0x17_39e0,
    delete: 0xe9_1290,
    create: 0xc1_7400,
    slot_vtable: 0x4b4_69a0,
    map_loaded: 0x108_3e20,
    chest_made: 0xd2_72f0,
    granted: 0x110_d570,
    link_update: 0x2c2_15e0,
    malloc: 0x14c_6c90,
    make_arg: 0xd7_9f10,
    request: 0xd7_a820,
    set_link: 0x2b_c440,
    release_link: 0x20_b6f0,
    link_empty: 0x497_4b20,
    get_bool: 0x20_d9c0,
    set_bool: 0x68_2b20,
    events: 0x4c7_e090,
    gamedata: 0x4c7_e180,
    // The slot the game's own set-time node reads. The one at 0x4c7_e878
    // also has a clock-like object at +0x510, and nothing reads that one.
    world: 0x4c7_e898,
    set_time: 0x345_22d0,
    player_vtable: 0x4b2_5e18,
    move_vtable: 0x4b4_c460,
    move_to: 0x6b_2530,
    controller_vtable: 0x4c3_4f10,
    run_setter: 0x93_52f0,
};
/// Where the recorder, and the free space after rodata and bss, end.
const CAVE_END: u32 = 0x17f_58e0;
const TABLE_END: u32 = 0x484_d000;
const LIST_END: u32 = 0x4e9_b000;
const _: () = assert!(LIST + size_of::<Shared>() as u32 <= PANIC && LIST.is_multiple_of(8));
const _: () = assert!(PANIC + 16 == LIST_END);
/// The physics step's `cbz` over the call into Havok's trace recorder, and
/// where it skips to. Made a `b`, the recorder never runs and holds `CAVE`.
const GATE: (u32, u32) = (0x17d_59bc, 0x17d_59c4);
/// Each call site and the hook entry its `bl` goes to, as an offset into
/// the hook. The event request's string argument calls, where an opening
/// chest sets its `ContentName`, go to `content_name`. The drop reader's
/// calls, where a loading room reads a chest's placement, go to
/// `drop_actor`.
const CALLS: [(u32, u32); 4] = [
    (0x2ae_0828, 0),
    (0x2ae_08cc, 0),
    (0x2ef_6560, 4),
    (0x2ef_4888, 4),
];
/// The run node's call to the speed setter and the hook entry it goes to,
/// `run_speed`, which scales the speed in the Bunny Hood. Rewritten only
/// in a seed with the setting.
const RUN_CALL: (u32, u32) = (0x93_4990, 24);
/// Each hooked function and the entry the `b` over its `pacibsp` goes to.
/// Link's update only runs the dev panel's requests and switches, so it's
/// hooked only with the dev panel on.
const HOOKS: &[(u32, u32)] = &[
    (OFFSETS.map_loaded, 8),
    (OFFSETS.chest_made, 12),
    (OFFSETS.granted, 16),
    (OFFSETS.link_update, 20),
];
/// `main`'s build ID, as its NSO header's 32 byte field holds it. Its first
/// 8 bytes name the cheat file and the rest, trailing zeros left out, the
/// exefs patch's `@nsobid`, so a build these offsets weren't found in never
/// loads either.
pub const BUILD_ID: [u8; 32] = [
    0xe0, 0xf3, 0xcc, 0xfa, 0xf5, 0x00, 0x65, 0xae, 0x4e, 0xa0, 0x1e, 0xb5, 0xae, 0xb1, 0x3b, 0x1a,
    0x45, 0xda, 0x33, 0x48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
/// The engine's limit on one cheat's opcode words.
const WORDS: usize = 0x100;

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
    let calls = CALLS.map(|(at, entry)| (at, entry, true));
    let hooks = HOOKS.iter().map(|&(at, entry)| (at, entry, false));
    let run = (RUN_CALL.0, RUN_CALL.1, true);
    calls
        .into_iter()
        .chain(hooks)
        .chain([run])
        .map(|(at, entry, link)| Patch {
            at,
            bytes: branch(at, CAVE + entry, link).to_vec(),
        })
}

/// The hook, each function hook's resume slot branching past the
/// `pacibsp` its function gave up.
fn hook_code() -> Vec<u8> {
    let mut code = HOOK.to_vec();
    for (i, &(at, _)) in HOOKS.iter().enumerate() {
        let slot = RESUMES + 4 * i as u32;
        code[slot as usize..][..4].copy_from_slice(&branch(CAVE + slot, at + 4, false));
    }
    code
}

/// What the patch writes over a hooked function's `pacibsp`, if `at` is one.
pub fn site(at: u32) -> Option<Vec<u8>> {
    sites().find(|p| p.at == at).map(|p| p.bytes)
}

/// The gate, the hook, its table for `lines`, `flags` and `credits`, then the calls
/// and function hooks that reach it. Link's update is among them only with
/// `dev`, and the run's call only with [`BUNNY_HOOD`] in `flags`.
pub fn patches<A: AsRef<str>>(
    lines: &[&[(A, A)]],
    dev: bool,
    flags: u32,
    credits: &[(&str, Credit)],
) -> Result<Vec<Patch>> {
    let owned: Vec<Vec<(&str, &str)>> = (lines.iter())
        .map(|l| l.iter().map(|(a, b)| (a.as_ref(), b.as_ref())).collect())
        .collect();
    let lines: Vec<&[(&str, &str)]> = owned.iter().map(Vec::as_slice).collect();
    let fit = |at: u32, mut bytes: Vec<u8>, end: u32, what: &str| {
        // Writes go a word at a time.
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        let free = (end - at) as usize;
        ensure!(
            bytes.len() <= free,
            "the {what} needs {} bytes, {free} are free",
            bytes.len()
        );
        Ok(Patch { at, bytes })
    };
    let gate = Patch {
        at: GATE.0,
        bytes: branch(GATE.0, GATE.1, false).to_vec(),
    };
    let code = fit(CAVE, hook_code(), CAVE_END, "hook")?;
    let table = fit(
        TABLE,
        encode(OFFSETS, flags, &lines, credits),
        TABLE_END,
        "table",
    )?;
    let bunny = flags & BUNNY_HOOD != 0;
    let sites = sites().filter(|p| match p.at {
        at if at == OFFSETS.link_update => dev,
        at if at == RUN_CALL.0 => bunny,
        _ => true,
    });
    Ok([gate, code, table].into_iter().chain(sites).collect())
}

/// One type 0 write into `main`: 8 bytes where aligned, else 4.
fn writes(p: &Patch) -> impl Iterator<Item = (Vec<String>, usize)> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        let rest = p.bytes.get(at..).filter(|r| !r.is_empty())?;
        let addr = p.at + at as u32;
        let width = match addr.is_multiple_of(8) && rest.len() >= 8 {
            true => 8,
            false => 4,
        };
        let value = |b: &[u8]| format!("{:08X}", u32::from_le_bytes(b.try_into().unwrap()));
        let mut words = vec![format!("0{width}000000"), format!("{addr:08X}")];
        // Atmosphère takes an 8 byte value high word first.
        words.extend(rest[..width].chunks(4).rev().map(value));
        at += width;
        Some((words, width / 4 + 2))
    })
}

/// Atmosphère master codes writing `patches` in order, each cheat under the
/// engine's word limit.
pub fn cheat(patches: &[Patch]) -> String {
    let mut cheats: Vec<(usize, Vec<String>)> = vec![];
    for (words, n) in patches.iter().flat_map(writes) {
        match cheats.last_mut() {
            Some((used, lines)) if *used + n <= WORDS => {
                *used += n;
                lines.push(words.join(" "));
            }
            _ => cheats.push((n, vec![words.join(" ")])),
        }
    }
    let total = cheats.len();
    let mut out = String::new();
    for (i, (_, lines)) in cheats.iter().enumerate() {
        out += &format!("{{zootr {}/{total}}}\n{}\n\n", i + 1, lines.join("\n"));
    }
    out
}

/// IPSwitch text writing `patches`, for loaders that patch `main` as it
/// loads (emulators, and recompilers that take no code cheats). Offsets
/// count from `main`'s image, past its NSO header.
pub fn pchtxt(patches: &[Patch]) -> String {
    let used = BUILD_ID.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    let id: String = BUILD_ID[..used]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    let mut out = format!("@nsobid-{id}\n@flag offset_shift 0x100\n\n@enabled\n");
    for p in patches {
        for (i, chunk) in p.bytes.chunks(64).enumerate() {
            let hex: String = chunk.iter().map(|b| format!("{b:02X}")).collect();
            out += &format!("{:08X} {hex}\n", p.at as usize + 64 * i);
        }
    }
    out + "@stop\n"
}

/// Where a mod folder's cheat goes, named for the build it fits.
pub fn path(mod_dir: &Path) -> PathBuf {
    let id: String = BUILD_ID[..8].iter().map(|b| format!("{b:02X}")).collect();
    mod_dir.join("cheats").join(format!("{id}.txt"))
}

/// Writes patches as one file's text.
type Writer = fn(&[Patch]) -> String;

/// Our files in a mod folder and how each writes the patches: the cheat,
/// and the same bytes as an exefs patch.
fn outputs(mod_dir: &Path) -> [(PathBuf, Writer); 2] {
    [
        (path(mod_dir), cheat),
        (mod_dir.join("exefs/zootr.pchtxt"), pchtxt),
    ]
}

/// Whether a seed with these settings runs the hook.
pub fn hooked(progressive: bool, dev: bool, flags: u32) -> bool {
    progressive || dev || flags & BUNNY_HOOD != 0
}

/// Write the cheat and exefs patch when [`hooked`], else take ours away.
/// `dev` adds the dev panel's hook, the hood the run's call, and without
/// `progressive` the table has no lines. `credits` are what items set as
/// they're found, by name, and a seed with any runs the hook too.
pub fn install(
    mod_dir: &Path,
    progressive: bool,
    dev: bool,
    flags: u32,
    credits: &[(String, Credit)],
) -> Result<()> {
    if !hooked(progressive, dev, flags) && credits.is_empty() {
        return remove(mod_dir);
    }
    let lines: &[&[(&Name, &Name)]] = if progressive {
        &progressive::LINES
    } else {
        &[]
    };
    let credits: Vec<_> = credits.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    let patches = patches(lines, dev, flags, &credits)?;
    for (path, write) in outputs(mod_dir) {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, write(&patches))
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Take our cheat and exefs patch off `mod_dir`, leaving any others.
pub fn remove(mod_dir: &Path) -> Result<()> {
    outputs(mod_dir).iter().try_for_each(|(path, _)| take(path))
}

/// Take the cheat alone off `mod_dir`, for a game that reads the exefs
/// patch. A game that reads both runs the hook twice, and reapplies the
/// cheat many times a second.
pub fn remove_cheat(mod_dir: &Path) -> Result<()> {
    take(&path(mod_dir))
}

/// Delete `path` if it's there.
fn take(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("removing {}", path.display()))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use zootr_hook::{HEADER, Header, Table};

    use super::*;

    #[test]
    fn the_code_and_table_go_in_their_places_and_read_back() {
        let [_, code, table, ..] = &patches(&progressive::LINES, false, 0, &[]).unwrap()[..] else {
            panic!("patches")
        };
        assert_eq!((code.at, code.bytes.len()), (CAVE, HOOK.len()));
        assert_eq!(table.at % 8, 0, "the header is read in place");
        let header = Header::read(&table.bytes);
        let lines = &table.bytes[HEADER..][..header.size as usize];
        let table = Table::parse(&header, lines).unwrap();
        assert_eq!(*table.offsets, OFFSETS);
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
    fn each_function_hook_resumes_past_its_pacibsp() {
        let word =
            |b: &[u8], at: u32| u32::from_le_bytes(b[at as usize..][..4].try_into().unwrap());
        let code = hook_code();
        for (i, &(at, _)) in HOOKS.iter().enumerate() {
            let slot = RESUMES + 4 * i as u32;
            // brk #0 until the patch fills it.
            assert_eq!(word(HOOK, slot), 0xd420_0000, "slot {i}");
            assert_eq!(
                code[slot as usize..][..4],
                branch(CAVE + slot, at + 4, false)
            );
        }
        assert_eq!(code[..RESUMES as usize], HOOK[..RESUMES as usize]);
        let end = (RESUMES as usize) + 4 * HOOKS.len();
        assert_eq!(code[end..], HOOK[end..]);
    }

    #[test]
    fn the_gate_skips_the_recorder() {
        let gate = &patches(&progressive::LINES, false, 0, &[]).unwrap()[0];
        assert_eq!(gate.at, 0x17d_59bc);
        // b 0x17d59c4
        assert_eq!(gate.bytes, 0x1400_0002u32.to_le_bytes());
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
        let code: usize = env!("HOOK_CODE").parse().unwrap();
        for (i, word) in HOOK[..code].chunks(4).enumerate() {
            assert_eq!(x18(word), None, "at {:#x}", i * 4);
        }
    }

    #[test]
    fn page_relative_addresses_land_in_the_hook_at_the_cave() {
        let word = |i: usize| u32::from_le_bytes(HOOK[i * 4..i * 4 + 4].try_into().unwrap());
        let code: usize = env!("HOOK_CODE").parse().unwrap();
        for i in 0..code / 4 {
            let adrp = word(i);
            if adrp & 0x9f00_0000 != 0x9000_0000 {
                continue;
            }
            let pages = ((adrp >> 29) & 3) | (((adrp >> 5) & 0x7ffff) << 2);
            let pages = i64::from(((pages << 11) as i32) >> 11);
            let at = i64::from(CAVE) + i as i64 * 4;
            let page = (at & !0xfff) + (pages << 12);
            // The `add` that finishes the address, from the same register.
            let rd = adrp & 0x1f;
            let add = (i + 1..(i + 8).min(code / 4))
                .map(word)
                .find(|w| w & 0xffc0_0000 == 0x9100_0000 && (w >> 5) & 0x1f == rd);
            if let Some(add) = add {
                let target = page + i64::from((add >> 10) & 0xfff);
                let hook = i64::from(CAVE)..i64::from(CAVE) + HOOK.len() as i64;
                assert!(
                    hook.contains(&target),
                    "adrp at {:#x} to {target:#x}",
                    i * 4
                );
            }
        }
    }

    #[test]
    fn calls_and_hooks_branch_to_the_hook() {
        let p = patches(&progressive::LINES, true, BUNNY_HOOD, &[]).unwrap();
        assert_eq!(p[3..], sites().collect::<Vec<_>>());
        let at = |i: usize| u32::from_le_bytes(p[3 + i].bytes[..].try_into().unwrap());
        assert_eq!(at(0) >> 26, 0b100101, "a call stays a bl");
        assert_eq!(at(CALLS.len()) >> 26, 0b000101, "a hook is a b");
        let run = p.last().unwrap();
        assert_eq!(run.at, RUN_CALL.0);
        assert_eq!(run.bytes, branch(RUN_CALL.0, CAVE + RUN_CALL.1, true));
        assert_eq!(branch(0x100, 0x104, true), 0x9400_0001u32.to_le_bytes());
        assert_eq!(branch(0x104, 0x100, true), 0x97ff_ffffu32.to_le_bytes());
        assert_eq!(branch(0x100, 0x108, false), 0x1400_0002u32.to_le_bytes());
        assert!(std::panic::catch_unwind(|| branch(0, 1 << 27, true)).is_err());
    }

    #[test]
    fn link_s_update_needs_dev_and_the_run_call_the_hood() {
        let all = patches(&progressive::LINES, true, BUNNY_HOOD, &[]).unwrap();
        for (dev, flags, gone) in [
            (false, BUNNY_HOOD, vec![OFFSETS.link_update]),
            (true, 0, vec![RUN_CALL.0]),
            (false, 0, vec![OFFSETS.link_update, RUN_CALL.0]),
        ] {
            let some = patches(&progressive::LINES, dev, flags, &[]).unwrap();
            assert_eq!(some.len(), all.len() - gone.len(), "{dev} {flags}");
            assert!(some.iter().all(|p| !gone.contains(&p.at)), "{dev} {flags}");
        }
    }

    #[test]
    fn a_table_too_big_for_the_space_is_refused() {
        let name = "Item_".repeat(40);
        let big = vec![(name.as_str(), name.as_str()); 20];
        assert!(patches(&[&big], false, 0, &[]).is_err());
    }

    #[test]
    fn the_cheat_writes_in_order_under_the_word_limit() {
        let p = [
            Patch {
                at: 0x10,
                bytes: (0..16).collect(),
            },
            Patch {
                at: 0x2c,
                bytes: vec![1, 2, 3, 4],
            },
        ];
        assert_eq!(
            cheat(&p),
            "{zootr 1/1}\n\
             08000000 00000010 07060504 03020100\n\
             08000000 00000018 0F0E0D0C 0B0A0908\n\
             04000000 0000002C 04030201\n\n"
        );
        let text = cheat(&patches(&progressive::LINES, true, BUNNY_HOOD, &[]).unwrap());
        let cheats: Vec<_> = text.split("\n\n").filter(|c| !c.is_empty()).collect();
        let total = cheats.len();
        for (i, c) in cheats.iter().enumerate() {
            assert!(c.starts_with(&format!("{{zootr {}/{total}}}\n", i + 1)));
            let words = c.lines().skip(1).flat_map(|l| l.split(' ')).count();
            assert!(words <= WORDS, "{words}");
        }
        // The sites may straddle two cheats, so look at every write.
        let last: Vec<_> = text.lines().rev().filter(|l| l.starts_with('0')).collect();
        let sites: Vec<_> = sites().map(|p| format!(" {:08X} ", p.at)).collect();
        assert!(sites.iter().rev().zip(last).all(|(at, l)| l.contains(at)));
    }

    #[test]
    fn the_exefs_patch_writes_the_same_bytes_in_64_byte_lines() {
        let p = [Patch {
            at: 0x17d_59bc,
            bytes: (0..66).collect(),
        }];
        let text = pchtxt(&p);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[0], "@nsobid-E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348");
        assert_eq!(lines[1..4], ["@flag offset_shift 0x100", "", "@enabled"]);
        assert!(lines[4].starts_with("017D59BC 00010203") && lines[4].len() == 9 + 128);
        assert_eq!(lines[5..], ["017D59FC 4041", "@stop"]);
    }

    #[test]
    fn the_build_id_is_the_dumps() {
        let dump = Path::new("../../dump/oot.nup");
        if !dump.exists() {
            return;
        }
        let nup = nup_format::Nup::open(dump).unwrap();
        let title = crate::game::TITLE | 0x800;
        let update = nup.packages().iter().find(|p| p.title_id == title);
        let exefs = update.and_then(|p| p.exefs).unwrap();
        let Some(nup_format::Node::File(main)) = nup.find(exefs, "main") else {
            panic!("no exefs/main")
        };
        let header = nup.read(main).unwrap();
        assert_eq!(header[0x40..0x60], BUILD_ID);
        assert!(path(Path::new("m")).ends_with("cheats/E0F3CCFAF50065AE.txt"));
    }

    #[test]
    fn install_writes_or_takes_away_only_ours() {
        let dir = std::env::temp_dir().join(format!("zootr-patch-{}", std::process::id()));
        let other = dir.join("cheats/OTHER.txt");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "").unwrap();
        install(&dir, true, false, 0, &[]).unwrap();
        assert!(
            std::fs::read_to_string(path(&dir))
                .unwrap()
                .starts_with("{zootr 1/")
        );
        install(&dir, false, true, 0, &[]).unwrap();
        let dev = std::fs::read_to_string(path(&dir)).unwrap();
        assert!(
            dev.contains(&format!("{:08X}", OFFSETS.link_update)),
            "{dev}"
        );
        let exefs = dir.join("exefs/zootr.pchtxt");
        assert!(
            std::fs::read_to_string(&exefs)
                .unwrap()
                .contains(&format!("{:08X} ", OFFSETS.link_update))
        );
        install(&dir, false, false, BUNNY_HOOD, &[]).unwrap();
        let hood = std::fs::read_to_string(path(&dir)).unwrap();
        assert!(hood.contains(&format!("{:08X}", RUN_CALL.0)), "{hood}");
        install(&dir, false, false, 0, &[]).unwrap();
        assert!(!path(&dir).exists() && !exefs.exists());
        assert!(other.exists());
        remove(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
