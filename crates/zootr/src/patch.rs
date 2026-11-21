//! Our code in the game: the hook from `zootr-hook` and its table, written
//! into `main` by a cheat file, an exefs patch, and an IPS for a Switch.
//! Offsets are game 1.1.1's, see `docs/research/binary.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
pub use zootr_hook::OFFSETS;
use zootr_hook::look::LookSpec;
use zootr_hook::{
    CAVE, Credit, DEV, FAR, LIST, PANIC, SLOTS, Shared, Spot, TABLE, TRADE_CYCLE, encode,
};

use crate::{progressive, trade_slot};

/// The hook's code, built by `build.rs`.
const HOOK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/hook.bin"));
/// The hook's far code, written at `FAR`.
const FAR_CODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/far.bin"));
/// Where the recorder, and the free space after rodata and bss, end.
const CAVE_END: u32 = 0x17f_58e0;
/// Where the padding after text, which holds the far code, ends: rodata.
const FAR_END: u32 = 0x3c4_7000;
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
/// `drop_actor`.
const CALLS: [(u32, u32); 4] = [
    (0x2ae_0828, 0),
    (0x2ae_08cc, 0),
    (0x2ef_6560, 4),
    (0x2ef_4888, 4),
];
/// The run node's call to the speed setter and the hook entry it goes to,
/// `run_speed`, which scales the speed in the Bunny Hood in a seed with
/// the setting.
const RUN_CALL: (u32, u32) = (0x93_4990, 24);
/// The controller device's three `GetNpadStates` calls (full key,
/// handheld, joy dual) and their hook entries, which run the trade slot's
/// cycle with the three Items page hooks, in a seed with [`TRADE_CYCLE`].
const NPAD_CALLS: [(u32, u32); 3] = [(0x346_966c, 32), (0x346_9698, 36), (0x346_94f8, 40)];
/// The game's stubs for `nn::hid::GetNpadStates` those calls go to, in
/// the same order. The hook reaches them through its slots.
const NPAD_STUBS: [u32; 3] = [0x3c4_2800, 0x3c4_2830, 0x3c4_27f0];
/// Each hooked function and the entry the `b` over its `pacibsp` goes to.
/// Link's update runs the mailbox's requests and the dev switches in a
/// seed with [`DEV`], and springs ice traps. The nock sets Link's
/// projectile to the drawn bow's, so a child Bow fires arrows, and the out
/// of ammo handler's message follows that projectile, not Link's age.
const HOOKS: &[(u32, u32)] = &[
    (OFFSETS.map_loaded, 8),
    (OFFSETS.chest_made, 12),
    (OFFSETS.granted, 16),
    (OFFSETS.link_update, 20),
    (OFFSETS.picked, 28),
    (OFFSETS.items_built, 44),
    (OFFSETS.items_left, 48),
    (OFFSETS.ui_update, 52),
    (OFFSETS.taken, 56),
    (OFFSETS.nock, 60),
    (OFFSETS.emptied, 64),
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
/// halfway never parses.
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
    let actors = trade_slot::actors();
    let slot = (flags & TRADE_CYCLE != 0).then(|| trade_slot::spec(&actors));
    let table = encode(flags, &lines, &keyed, credits, slot.as_ref(), spots, look);
    let table = fit(TABLE, table, TABLE_END, "table")?;
    Ok((code()?.into_iter())
        .chain(table_writes(table))
        .chain(sites())
        .collect())
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

/// `main`'s build ID in hex, trailing zeros left out: how an exefs patch
/// names the build it fits.
fn build_id() -> String {
    let used = BUILD_ID.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    BUILD_ID[..used]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

/// The NSO header before `main`'s image, which exefs patch offsets count.
const NSO_HEADER: u32 = 0x100;
/// The most bytes one IPS record holds. Its size field is 2 bytes, and a
/// size of 0 means a run, so no record is empty.
const IPS_RECORD: usize = 0xffff;
const IPS_END: &[u8; 4] = b"EEOF";
// A record at the offset `EEOF` would read as the end.
const _: () = assert!(NSO_HEADER + LIST_END < u32::from_be_bytes(*IPS_END));

/// IPSwitch text writing `patches`, for loaders that patch `main` as it
/// loads (targets that take no code cheats). Offsets
/// count from `main`'s image, past its NSO header.
pub fn pchtxt(patches: &[Patch]) -> String {
    let id = build_id();
    let mut out = format!("@nsobid-{id}\n@flag offset_shift {NSO_HEADER:#x}\n\n@enabled\n");
    for p in patches {
        for (i, chunk) in p.bytes.chunks(64).enumerate() {
            let hex: String = chunk.iter().map(|b| format!("{b:02X}")).collect();
            out += &format!("{:08X} {hex}\n", p.at as usize + 64 * i);
        }
    }
    out + "@stop\n"
}

/// IPS32 writing `patches`, the same bytes as [`pchtxt`], for a Switch
/// running Atmosphère, whose loader applies it as `main` loads: code and
/// table both, since its cheats hold 0x400 words in all. Offsets count
/// the NSO header. See `docs/research/console.md`.
pub fn ips(patches: &[Patch]) -> Vec<u8> {
    let mut out = b"IPS32".to_vec();
    for p in patches {
        for (i, chunk) in p.bytes.chunks(IPS_RECORD).enumerate() {
            let at = NSO_HEADER + p.at + (IPS_RECORD * i) as u32;
            out.extend(at.to_be_bytes());
            out.extend((chunk.len() as u16).to_be_bytes());
            out.extend(chunk);
        }
    }
    out.extend(IPS_END);
    out
}

/// Where a mod folder's cheat goes, named for the build it fits.
pub fn path(mod_dir: &Path) -> PathBuf {
    let id: String = BUILD_ID[..8].iter().map(|b| format!("{b:02X}")).collect();
    mod_dir.join("cheats").join(format!("{id}.txt"))
}

/// Where a mod folder's exefs patch goes.
pub fn exefs(mod_dir: &Path) -> PathBuf {
    mod_dir.join("exefs").join(EXEFS)
}

/// The exefs patch's file name.
pub const EXEFS: &str = "zootr.pchtxt";

/// Where a mod folder's IPS goes: the path Atmosphère reads under
/// `atmosphere/`, named for the build. No loader we run reads it, so
/// nothing applies the patch twice and no built game sees the table.
pub fn ips_path(mod_dir: &Path) -> PathBuf {
    (mod_dir.join("exefs_patches/zootr")).join(format!("{}.ips", build_id()))
}

/// Writes patches as one file's bytes.
type Writer = fn(&[Patch]) -> Vec<u8>;

/// Our files in a mod folder and how each writes the patches: the cheat,
/// and the same bytes as an exefs patch and an IPS.
fn outputs(mod_dir: &Path) -> [(PathBuf, Writer); 3] {
    [
        (path(mod_dir), |p| cheat(p).into_bytes()),
        (exefs(mod_dir), |p| pchtxt(p).into_bytes()),
        (ips_path(mod_dir), ips),
    ]
}

/// Write the cheat, exefs patch and IPS. Without `progressive` the table has
/// no lines but the `keyed` ones (a prize chest's ammo). `credits` are
/// what items set as they're found, by name, `spots` the twinned holders
/// whose drops set a flag as they're taken (GameData holds them, the
/// table their count).
pub fn install(
    mod_dir: &Path,
    progressive: progressive::Lines,
    keyed: &[Vec<(String, String)>],
    flags: u32,
    credits: &[(String, Credit)],
    spots: &[Spot],
    look: Option<&LookSpec>,
) -> Result<()> {
    let lines: Vec<_> = match progressive.progressive {
        true => progressive.all().collect(),
        false => Vec::new(),
    };
    let credits: Vec<_> = credits.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    let patches = patches(&lines, keyed, flags, &credits, spots, look)?;
    for (path, write) in outputs(mod_dir) {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, write(&patches))
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Rewrite `mod_dir`'s patches for a game that builds the exefs patch in:
/// the exefs patch keeps [`code_patches`], the same for every seed, and
/// the cheat writes the table alone, as data the game reads as it runs. A
/// folder with no patch of ours stays as it is.
pub fn split(mod_dir: &Path) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(exefs(mod_dir)) else {
        return Ok(());
    };
    let table = table_of(&text)?;
    std::fs::write(exefs(mod_dir), pchtxt(&code_patches()?))?;
    std::fs::write(path(mod_dir), cheat(&table_writes(table)))?;
    Ok(())
}

/// The table an exefs patch writes, from its lines between `TABLE` and
/// `TABLE_END`.
fn table_of(pchtxt: &str) -> Result<Patch> {
    let mut lines = Vec::new();
    for (at, hex) in pchtxt.lines().filter_map(|l| l.split_once(' ')) {
        let Ok(at) = u32::from_str_radix(at, 16) else {
            continue;
        };
        if (TABLE..TABLE_END).contains(&at) {
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
                .collect::<Option<Vec<u8>>>()
                .context("a table line that isn't hex")?;
            lines.push((at, bytes));
        }
    }
    lines.sort_by_key(|l| l.0);
    let mut bytes = Vec::new();
    for (at, line) in lines {
        ensure!(
            at == TABLE + bytes.len() as u32,
            "a gap in the table's lines"
        );
        bytes.extend(line);
    }
    ensure!(!bytes.is_empty(), "an exefs patch with no table");
    Ok(Patch { at: TABLE, bytes })
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
    use std::collections::BTreeMap;
    use std::ops::Range;

    use zootr_hook::{BUNNY_HOOD, FREE_BOMBCHUS, HEADER, Header, ICE_TRAPS, MAGIC, Table};

    use super::*;

    /// Every flag that adds a site.
    const ALL: u32 = BUNNY_HOOD | TRADE_CYCLE;
    /// A holder that stands until picked, which adds the take's site.
    const SPOT: Spot = Spot {
        flag: 1,
        name: 2,
        at: [3, 4, 5],
    };

    #[test]
    fn the_hooks_seed_hash_is_the_deku_seeds_name() {
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
    const TABLE_AT: usize = SKIPS.len() + 2;
    /// Where the sites start in `patches`, after the table.
    const SITES_AT: usize = TABLE_AT + 2;

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
        let text = cheat(&patches(&progressive::LINES, &[], ALL, &[], &[SPOT], None).unwrap());
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

    /// An IPS32 file's records, as `(offset, bytes)`.
    fn ips_records(ips: &[u8]) -> Vec<(u32, Vec<u8>)> {
        assert_eq!(ips[..5], *b"IPS32");
        let (mut at, mut out) = (5, vec![]);
        while ips[at..at + 4] != *b"EEOF" {
            let offset = u32::from_be_bytes(ips[at..at + 4].try_into().unwrap());
            let size = u16::from_be_bytes(ips[at + 4..at + 6].try_into().unwrap()) as usize;
            out.push((offset, ips[at + 6..at + 6 + size].to_vec()));
            at += 6 + size;
        }
        assert_eq!(at + 4, ips.len(), "nothing after EEOF");
        out
    }

    /// Every byte `writes` puts in `main`'s image, by its offset.
    fn bytes_at(writes: impl IntoIterator<Item = (u32, Vec<u8>)>) -> BTreeMap<u32, u8> {
        let mut out = BTreeMap::new();
        for (at, bytes) in writes {
            out.extend((at..).zip(bytes));
        }
        out
    }

    #[test]
    fn the_ips_counts_the_nso_header_big_endian() {
        let p = [Patch {
            at: 0x17d_59bc,
            bytes: vec![1, 2, 3, 4],
        }];
        let mut want = b"IPS32".to_vec();
        want.extend([0x01, 0x7d, 0x5a, 0xbc, 0x00, 0x04, 1, 2, 3, 4]);
        want.extend(b"EEOF");
        assert_eq!(ips(&p), want);
    }

    #[test]
    fn a_patch_past_a_record_splits_and_no_record_is_empty() {
        let p = [
            Patch {
                at: 0x1000,
                bytes: (0..0x2_0002).map(|i| i as u8).collect(),
            },
            Patch {
                at: 0x9_0000,
                bytes: vec![],
            },
        ];
        let records = ips_records(&ips(&p));
        let sizes: Vec<_> = records.iter().map(|(_, b)| b.len()).collect();
        assert_eq!(sizes, [0xffff, 0xffff, 4]);
        let offsets: Vec<_> = records.iter().map(|(at, _)| *at).collect();
        assert_eq!(offsets, [0x1100, 0x1100 + 0xffff, 0x1100 + 0x1_fffe]);
        let back: Vec<u8> = records.into_iter().flat_map(|(_, b)| b).collect();
        assert_eq!(back, p[0].bytes);
    }

    #[test]
    fn the_ips_writes_what_the_exefs_patch_writes() {
        let p = patches(&progressive::LINES, &[], ALL, &[], &[SPOT], None).unwrap();
        let ips = ips_records(&ips(&p)).into_iter();
        let ips = bytes_at(ips.map(|(at, b)| (at - 0x100, b)));
        let text = pchtxt(&p);
        let pchtxt = bytes_at(text.lines().filter_map(|l| {
            let (at, hex) = l.split_once(' ')?;
            let at = u32::from_str_radix(at, 16).ok()?;
            let bytes = (0..hex.len()).step_by(2);
            Some((
                at,
                bytes
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect(),
            ))
        }));
        assert!(
            ips.keys().any(|at| (TABLE..TABLE_END).contains(at)),
            "the table"
        );
        assert_eq!(ips, pchtxt);
    }

    #[test]
    fn the_ips_is_named_as_atmosphere_reads_it() {
        let at = ips_path(Path::new("m"));
        assert!(at.starts_with("m/exefs_patches/zootr"), "{}", at.display());
        let name = at.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348.ips");
        // Atmosphère's rule: hex, an even length, at most a whole ID's.
        let id = name.strip_suffix(".ips").unwrap();
        assert!(
            id.len().is_multiple_of(2)
                && id.len() <= 64
                && id.chars().all(|c| c.is_ascii_hexdigit())
        );
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
    fn install_writes_and_remove_takes_away_only_ours() {
        let dir = std::env::temp_dir().join(format!("zootr-patch-{}", std::process::id()));
        let other = dir.join("cheats/OTHER.txt");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "").unwrap();
        install(
            &dir,
            crate::progressive::Lines::climbing(),
            &[],
            0,
            &[],
            &[],
            None,
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(path(&dir))
                .unwrap()
                .starts_with("{zootr 1/")
        );
        install(&dir, Default::default(), &[], 0, &[], &[], None).unwrap();
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
        install(&dir, Default::default(), &[], BUNNY_HOOD, &[], &[], None).unwrap();
        let hood = std::fs::read_to_string(path(&dir)).unwrap();
        assert!(hood.contains(&format!("{:08X}", RUN_CALL.0)), "{hood}");
        assert!(std::fs::read(ips_path(&dir)).unwrap().starts_with(b"IPS32"));
        remove(&dir).unwrap();
        assert!(!path(&dir).exists() && !exefs.exists() && !ips_path(&dir).exists());
        assert!(other.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_split_leaves_the_code_to_the_exefs_patch_and_the_table_to_the_cheat() {
        let dir = std::env::temp_dir().join(format!("zootr-split-{}", std::process::id()));
        let lines = progressive::Lines::climbing();
        install(&dir, lines, &[], ALL, &[], &[SPOT], None).unwrap();
        let lines: Vec<_> = lines.all().collect();
        let whole = patches(&lines, &[], ALL, &[], &[SPOT], None).unwrap();
        split(&dir).unwrap();
        assert_eq!(
            std::fs::read(ips_path(&dir)).unwrap(),
            ips(&whole),
            "the IPS keeps it all"
        );
        let exefs = std::fs::read_to_string(exefs(&dir)).unwrap();
        assert_eq!(exefs, pchtxt(&code_patches().unwrap()));
        let cheat = std::fs::read_to_string(path(&dir)).unwrap();
        assert_eq!(cheat, self::cheat(&whole[TABLE_AT..SITES_AT]));
        let addrs: Vec<u32> = (cheat.lines())
            .filter(|l| l.starts_with('0'))
            .filter_map(|l| l.split(' ').nth(1))
            .map(|a| u32::from_str_radix(a, 16).unwrap())
            .collect();
        assert!(addrs.iter().all(|a| (TABLE..TABLE_END).contains(a)));
        assert_eq!(addrs.last(), Some(&TABLE), "the magic goes last");
        std::fs::remove_dir_all(&dir).unwrap();
        split(&dir).unwrap();
        assert!(!dir.exists(), "no patch, nothing written");
    }
}
