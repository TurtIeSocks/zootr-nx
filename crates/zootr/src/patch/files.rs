//! The patch's output files: the cheat, the exefs patch and the IPS, where a
//! mod folder keeps them, and how each is installed, split and removed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use zootr_hook::look::LookSpec;
use zootr_hook::{Credit, Spot};

use super::{Hook, Patch, table_writes};
use crate::game::Release;
use crate::progressive;

/// The engine's limit on one cheat's opcode words.
const WORDS: usize = 0x100;

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
/// engine's word limit. Each patch is whole words.
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

/// The NSO header before `main`'s image, which exefs patch offsets count.
const NSO_HEADER: u32 = 0x100;
/// The most bytes one IPS record holds. Its size field is 2 bytes, and a
/// size of 0 means a run, so no record is empty.
const IPS_RECORD: usize = 0xffff;
const IPS_END: &[u8; 4] = b"EEOF";
// A record at the offset `EEOF` would read as the end, in any build.
const _: () = {
    let end = u32::from_be_bytes(*IPS_END);
    assert!(NSO_HEADER + zootr_hook::V65536.regions.list_end < end);
    assert!(NSO_HEADER + zootr_hook::V131072.regions.list_end < end);
};
/// How an exefs patch's first line names the build it fits.
const NSOBID: &str = "@nsobid-";

impl Hook {
    /// `main`'s build ID in hex, trailing zeros left out: how an exefs
    /// patch names the build it fits.
    fn build_id(&self) -> String {
        nup_format::nso::build_id_hex(&self.id)
    }

    /// IPSwitch text writing `patches`, for loaders that patch `main` as
    /// it loads (targets that take no code cheats). Offsets count from
    /// `main`'s image, past its NSO header.
    pub fn pchtxt(&self, patches: &[Patch]) -> String {
        let id = self.build_id();
        let mut out = format!("{NSOBID}{id}\n@flag offset_shift {NSO_HEADER:#x}\n\n@enabled\n");
        for p in patches {
            for (i, chunk) in p.bytes.chunks(64).enumerate() {
                let hex: String = chunk.iter().map(|b| format!("{b:02X}")).collect();
                out += &format!("{:08X} {hex}\n", p.at as usize + 64 * i);
            }
        }
        out + "@stop\n"
    }

    /// Where a mod folder's cheat goes, named for the build it fits.
    pub fn path(&self, mod_dir: &Path) -> PathBuf {
        let id: String = self.id[..8].iter().map(|b| format!("{b:02X}")).collect();
        mod_dir.join("cheats").join(format!("{id}.txt"))
    }

    /// Where a mod folder's IPS goes: the path Atmosphère reads under
    /// `atmosphere/`, named for the build. No loader we run reads it, so
    /// nothing applies the patch twice and no built game sees the table.
    pub fn ips_path(&self, mod_dir: &Path) -> PathBuf {
        (mod_dir.join("exefs_patches/zootr")).join(format!("{}.ips", self.build_id()))
    }

    /// The hook whose build the exefs patch `pchtxt` names.
    fn named_in(pchtxt: &str) -> Result<&'static Self> {
        let id = (pchtxt.lines().next())
            .and_then(|l| l.strip_prefix(NSOBID))
            .context("an exefs patch that names no build")?;
        (Self::all().iter())
            .find(|h| h.build_id().eq_ignore_ascii_case(id))
            .with_context(|| format!("an exefs patch for build {id}, which we have no hook for"))
    }
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

/// Where a mod folder's exefs patch goes.
pub fn exefs(mod_dir: &Path) -> PathBuf {
    mod_dir.join("exefs").join(EXEFS)
}

/// The exefs patch's file name.
pub const EXEFS: &str = "zootr.pchtxt";

/// Our files in a mod folder for build `h`, and what each holds of the
/// patches: the cheat, and the same bytes as an exefs patch and an IPS.
fn outputs(h: &Hook, mod_dir: &Path, patches: &[Patch]) -> [(PathBuf, Vec<u8>); 3] {
    [
        (h.path(mod_dir), cheat(patches).into_bytes()),
        (exefs(mod_dir), h.pchtxt(patches).into_bytes()),
        (h.ips_path(mod_dir), ips(patches)),
    ]
}

/// Write the cheat, exefs patch and IPS for `release`'s `main`, and take
/// off any other build's. Without `progressive` the table has no lines
/// but the `keyed` ones (a prize chest's ammo). `credits` are what items
/// set as they're found, by name, `spots` the twinned holders whose drops
/// set a flag as they're taken (GameData holds them, the table their
/// count).
#[allow(clippy::too_many_arguments)]
pub fn install(
    mod_dir: &Path,
    release: Release,
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
    let h = Hook::of(release);
    let patches = h.patches(&lines, keyed, flags, &credits, spots, look)?;
    remove(mod_dir)?;
    for (path, bytes) in outputs(h, mod_dir, &patches) {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Rewrite `mod_dir`'s patches for a game that builds the exefs patch in:
/// the exefs patch keeps its build's [`Hook::code_patches`], the same for
/// every seed, and the cheat writes the table alone, as data the game
/// reads as it runs. A folder with no patch of ours stays as it is.
pub fn split(mod_dir: &Path) -> Result<()> {
    let text = match std::fs::read_to_string(exefs(mod_dir)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        read => read.with_context(|| format!("reading {}", exefs(mod_dir).display()))?,
    };
    let h = Hook::named_in(&text)?;
    let table = table_of(h, &text)?;
    std::fs::write(exefs(mod_dir), h.pchtxt(&h.code_patches()?))?;
    std::fs::write(h.path(mod_dir), cheat(&table_writes(table)))?;
    Ok(())
}

/// The table an exefs patch for `h`'s build writes, from its lines in the
/// table's room.
fn table_of(h: &Hook, pchtxt: &str) -> Result<Patch> {
    let (table, table_end) = (h.build.regions.table, h.build.regions.table_end);
    let mut lines = Vec::new();
    for (at, hex) in pchtxt.lines().filter_map(|l| l.split_once(' ')) {
        let Ok(at) = u32::from_str_radix(at, 16) else {
            continue;
        };
        if (table..table_end).contains(&at) {
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
            at == table + bytes.len() as u32,
            "a gap in the table's lines"
        );
        bytes.extend(line);
    }
    ensure!(!bytes.is_empty(), "an exefs patch with no table");
    ensure!(bytes.len() % 4 == 0, "a table that isn't whole words");
    Ok(Patch { at: table, bytes })
}

/// Take our cheat, exefs patch and IPS off `mod_dir`, every build's,
/// leaving any others.
pub fn remove(mod_dir: &Path) -> Result<()> {
    take(&exefs(mod_dir))?;
    (Hook::all().iter()).try_for_each(|h| {
        take(&h.path(mod_dir))?;
        take(&h.ips_path(mod_dir))
    })
}

/// Take the cheat alone off `mod_dir`, every build's, for a game that
/// reads the exefs patch. A game that reads both runs the hook twice, and
/// reapplies the cheat many times a second.
pub fn remove_cheat(mod_dir: &Path) -> Result<()> {
    (Hook::all().iter()).try_for_each(|h| take(&h.path(mod_dir)))
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

    use zootr_hook::BUNNY_HOOD;

    use super::super::run_call;
    use super::super::tests::{ALL, SITES_AT, SPOT, TABLE_AT, v111};
    use super::*;

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
        let h = v111();
        let text = cheat(&(h.patches(&progressive::LINES, &[], ALL, &[], &[SPOT], None)).unwrap());
        let cheats: Vec<_> = text.split("\n\n").filter(|c| !c.is_empty()).collect();
        let total = cheats.len();
        for (i, c) in cheats.iter().enumerate() {
            assert!(c.starts_with(&format!("{{zootr {}/{total}}}\n", i + 1)));
            let words = c.lines().skip(1).flat_map(|l| l.split(' ')).count();
            assert!(words <= WORDS, "{words}");
        }
        // The sites may straddle two cheats, so look at every write.
        let last: Vec<_> = text.lines().rev().filter(|l| l.starts_with('0')).collect();
        let sites: Vec<_> = h.sites().map(|p| format!(" {:08X} ", p.at)).collect();
        assert!(sites.iter().rev().zip(last).all(|(at, l)| l.contains(at)));
    }

    #[test]
    fn the_exefs_patch_writes_the_same_bytes_in_64_byte_lines() {
        let p = [Patch {
            at: 0x1_0000,
            bytes: (0..66).collect(),
        }];
        let text = v111().pchtxt(&p);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[0], "@nsobid-E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348");
        assert_eq!(lines[1..4], ["@flag offset_shift 0x100", "", "@enabled"]);
        assert!(lines[4].starts_with("00010000 00010203") && lines[4].len() == 9 + 128);
        assert_eq!(lines[5..], ["00010040 4041", "@stop"]);
        let text = Hook::of(Release::V100).pchtxt(&p);
        let first = text.lines().next().unwrap();
        assert_eq!(first, "@nsobid-33FA6C2C23848282A77E8B88A0A93F4887C55920");
        for h in Hook::all() {
            assert_eq!(Hook::named_in(&h.pchtxt(&p)).unwrap().release, h.release);
        }
        assert!(Hook::named_in("@nsobid-0123\n").is_err());
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
            at: 0x1_0000,
            bytes: vec![1, 2, 3, 4],
        }];
        let mut want = b"IPS32".to_vec();
        want.extend([0x00, 0x01, 0x01, 0x00, 0x00, 0x04, 1, 2, 3, 4]);
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
        let h = v111();
        let r = &h.build.regions;
        let p = h.patches(&progressive::LINES, &[], ALL, &[], &[SPOT], None);
        let p = p.unwrap();
        let ips = ips_records(&ips(&p)).into_iter();
        let ips = bytes_at(ips.map(|(at, b)| (at - 0x100, b)));
        let text = h.pchtxt(&p);
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
            ips.keys().any(|at| (r.table..r.table_end).contains(at)),
            "the table"
        );
        assert_eq!(ips, pchtxt);
    }

    #[test]
    fn the_ips_is_named_as_atmosphere_reads_it() {
        let names = [
            (
                Release::V100,
                "33FA6C2C23848282A77E8B88A0A93F4887C55920.ips",
            ),
            (
                Release::V111,
                "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348.ips",
            ),
        ];
        for (release, want) in names {
            let at = Hook::of(release).ips_path(Path::new("m"));
            assert!(at.starts_with("m/exefs_patches/zootr"), "{}", at.display());
            let name = at.file_name().unwrap().to_str().unwrap();
            assert_eq!(name, want);
            // Atmosphère's rule: hex, an even length, at most a whole ID's.
            let id = name.strip_suffix(".ips").unwrap();
            assert!(
                id.len().is_multiple_of(2)
                    && id.len() <= 64
                    && id.chars().all(|c| c.is_ascii_hexdigit())
            );
        }
    }

    #[test]
    fn the_build_id_is_the_dumps() {
        let cheats = [
            (Release::V100, "cheats/33FA6C2C23848282.txt"),
            (Release::V111, "cheats/E0F3CCFAF50065AE.txt"),
        ];
        for (release, cheat) in cheats {
            assert!(Hook::of(release).path(Path::new("m")).ends_with(cheat));
        }
        let dump = Path::new("../../dump/oot.nup");
        if !dump.exists() {
            return;
        }
        for h in Hook::all() {
            let header = crate::game::main_nso(dump, h.release).unwrap();
            assert_eq!(header[0x40..0x60], h.id, "{}", h.release.name());
        }
    }

    #[test]
    fn install_writes_and_remove_takes_away_only_ours() {
        let dir = std::env::temp_dir().join(format!("zootr-patch-{}", std::process::id()));
        let other = dir.join("cheats/OTHER.txt");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "").unwrap();
        let (h, release) = (v111(), Release::V111);
        install(
            &dir,
            release,
            crate::progressive::Lines::climbing(),
            &[],
            0,
            &[],
            &[],
            None,
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(h.path(&dir))
                .unwrap()
                .starts_with("{zootr 1/")
        );
        install(&dir, release, Default::default(), &[], 0, &[], &[], None).unwrap();
        let dev = std::fs::read_to_string(h.path(&dir)).unwrap();
        let update = h.build.entries.link_update;
        assert!(dev.contains(&format!("{update:08X}")), "{dev}");
        let exefs = dir.join("exefs/zootr.pchtxt");
        assert!(
            std::fs::read_to_string(&exefs)
                .unwrap()
                .contains(&format!("{update:08X} "))
        );
        install(
            &dir,
            release,
            Default::default(),
            &[],
            BUNNY_HOOD,
            &[],
            &[],
            None,
        )
        .unwrap();
        let hood = std::fs::read_to_string(h.path(&dir)).unwrap();
        assert!(
            hood.contains(&format!("{:08X}", run_call(&h.build).0)),
            "{hood}"
        );
        assert!(
            std::fs::read(h.ips_path(&dir))
                .unwrap()
                .starts_with(b"IPS32")
        );
        // Another build's install takes this one's files off.
        let v100 = Hook::of(Release::V100);
        install(
            &dir,
            v100.release,
            Default::default(),
            &[],
            0,
            &[],
            &[],
            None,
        )
        .unwrap();
        assert!(!h.path(&dir).exists() && !h.ips_path(&dir).exists());
        assert!(v100.path(&dir).exists() && v100.ips_path(&dir).exists());
        remove(&dir).unwrap();
        for h in Hook::all() {
            assert!(!h.path(&dir).exists() && !exefs.exists() && !h.ips_path(&dir).exists());
        }
        assert!(other.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_split_leaves_the_code_to_the_exefs_patch_and_the_table_to_the_cheat() {
        let dir = std::env::temp_dir().join(format!("zootr-split-{}", std::process::id()));
        for h in Hook::all() {
            let r = &h.build.regions;
            let lines = progressive::Lines::climbing();
            install(&dir, h.release, lines, &[], ALL, &[], &[SPOT], None).unwrap();
            let lines: Vec<_> = lines.all().collect();
            let whole = h.patches(&lines, &[], ALL, &[], &[SPOT], None).unwrap();
            split(&dir).unwrap();
            assert_eq!(
                std::fs::read(h.ips_path(&dir)).unwrap(),
                ips(&whole),
                "the IPS keeps it all"
            );
            let exefs = std::fs::read_to_string(exefs(&dir)).unwrap();
            assert_eq!(exefs, h.pchtxt(&h.code_patches().unwrap()));
            let cheat = std::fs::read_to_string(h.path(&dir)).unwrap();
            assert_eq!(cheat, self::cheat(&whole[TABLE_AT..SITES_AT]));
            let addrs: Vec<u32> = (cheat.lines())
                .filter(|l| l.starts_with('0'))
                .filter_map(|l| l.split(' ').nth(1))
                .map(|a| u32::from_str_radix(a, 16).unwrap())
                .collect();
            assert!(addrs.iter().all(|a| (r.table..r.table_end).contains(a)));
            assert_eq!(addrs.last(), Some(&r.table), "the magic goes last");
        }
        std::fs::remove_dir_all(&dir).unwrap();
        split(&dir).unwrap();
        assert!(!dir.exists(), "no patch, nothing written");
    }

    #[test]
    fn a_patch_the_split_cant_read_is_an_error() {
        let dir = std::env::temp_dir().join(format!("zootr-unread-{}", std::process::id()));
        std::fs::create_dir_all(exefs(&dir).parent().unwrap()).unwrap();
        std::fs::write(exefs(&dir), [0xFF, 0xFE]).unwrap();
        assert!(split(&dir).is_err(), "not text isn't no patch");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_table_that_isnt_whole_words_is_an_error() {
        let h = v111();
        let line = |hex: &str| format!("{:08X} {hex}\n", h.build.regions.table);
        assert!(table_of(h, &line("0102")).is_err());
        assert!(table_of(h, &line("0102030405")).is_err());
        assert_eq!(table_of(h, &line("01020304")).unwrap().bytes, [1, 2, 3, 4]);
    }
}
