//! Builds `zootr-hook` for the game's CPU and keeps only its code, which a
//! seed's cheat file writes into `main`: the cave's, `hook.bin`, and the
//! far code, `far.bin`. The code has to run where it lands, so the build
//! fails on anything that would need relocating.

use std::path::{Path, PathBuf};
use std::process::Command;

const TARGET: &str = "aarch64-unknown-none";
/// The PIE boilerplate the linker always emits, none of it used by the code.
const ALLOWED: [&str; 7] = [
    ".text",
    ".far",
    ".dynsym",
    ".gnu.hash",
    ".hash",
    ".dynstr",
    ".dynamic",
];
const SHT_RELA: u32 = 4;
const SHT_REL: u32 = 9;
const SHF_ALLOC: u64 = 2;
/// The `b` instructions the patches branch to, one per hook, right before
/// the branch slots.
const ENTRIES: u64 = zootr_hook::SLOTS as u64 / 4;

fn main() {
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("../zootr-hook");
    let (src, script) = (hook.join("src/lib.rs"), hook.join("hook.ld"));
    // A directory is watched through everything under it, and the hook has
    // more than one file.
    for p in [&hook.join("src"), &script] {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let elf = out.join("hook.elf");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let status = Command::new(rustc)
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "bin",
            "--crate-name",
            "zootr_hook",
        ])
        .args(["--target", TARGET, "-C", "opt-level=z", "-C", "panic=abort"])
        .args(["-C", "relocation-model=pic", "-C", "link-arg=-pie"])
        // Apple's ABI reserves x18 and macOS may clear it, so the PC build
        // could lose a value the hook keeps there. The flag is nightly
        // only, so this one call is let through on the pinned stable.
        // Prebuilt `core` wasn't built with it, hence the mismatch allow.
        .env("RUSTC_BOOTSTRAP", "1")
        .args([
            "-Z",
            "fixed-x18",
            "-C",
            "unsafe-allow-abi-mismatch=fixed-x18",
        ])
        // A build that translates the game's code ahead of time follows
        // direct branches only, so a `match` mustn't become a jump table.
        // It also translates each function on its own, keeping only what
        // the calling convention keeps, so no shared tail may be outlined
        // into a function that hands back flags or takes stray registers.
        .args(["-C", "jump-tables=no"])
        .args(["-C", "llvm-args=-enable-machine-outliner=never"])
        .arg(format!(
            "-Clink-arg=--defsym=__cave={:#x}",
            zootr_hook::CAVE
        ))
        .arg(format!("-Clink-arg=--defsym=__far={:#x}", zootr_hook::FAR))
        .arg(format!("-Clink-arg=-T{}", script.display()))
        .arg("-o")
        .arg(&elf)
        .arg(&src)
        .status()
        .expect("running rustc for the hook");
    assert!(
        status.success(),
        "the hook didn't build. Is the {TARGET} target installed?"
    );
    let data = std::fs::read(&elf).unwrap();
    let (code, far) = code(&data);
    std::fs::write(out.join("hook.bin"), code).unwrap();
    std::fs::write(out.join("far.bin"), far).unwrap();
}

/// `.text` and `.far`, after checking the hook needs nothing outside them.
fn code(d: &[u8]) -> (&[u8], &[u8]) {
    let u16_at = |at: usize| u16::from_le_bytes(d[at..at + 2].try_into().unwrap()) as usize;
    let u32_at = |at: usize| u32::from_le_bytes(d[at..at + 4].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(d[at..at + 8].try_into().unwrap());
    let (shoff, size, count) = (u64_at(0x28) as usize, u16_at(0x3a), u16_at(0x3c));
    let header = |i: usize| shoff + i * size;
    let names = u64_at(header(u16_at(0x3e)) + 0x18) as usize;
    let name = |h: usize| {
        let at = names + u32_at(h) as usize;
        let end = at + d[at..].iter().position(|&b| b == 0).unwrap();
        std::str::from_utf8(&d[at..end]).unwrap()
    };
    let (mut text, mut far, mut symtab) = (None, None, None);
    for h in (0..count).map(header) {
        let (kind, flags) = (u32_at(h + 4), u64_at(h + 8));
        let (n, addr, off, len) = (
            name(h),
            u64_at(h + 0x10),
            u64_at(h + 0x18),
            u64_at(h + 0x20),
        );
        assert!(
            kind != SHT_RELA && kind != SHT_REL,
            "the hook needs relocating ({n})"
        );
        assert!(
            flags & SHF_ALLOC == 0 || ALLOWED.contains(&n),
            "the hook has {n}"
        );
        match n {
            ".text" => text = Some((addr, off as usize, len as usize)),
            ".far" => far = Some((addr, off as usize, len as usize)),
            ".symtab" => symtab = Some((h, off as usize, len as usize)),
            _ => {}
        }
    }
    let (addr, off, len) = text.expect("the hook has no .text");
    assert_eq!(
        addr,
        u64::from(zootr_hook::CAVE),
        "the hook isn't linked at the cave"
    );
    let far = far.map_or(&[][..], |(at, off, len)| {
        assert_eq!(
            at,
            u64::from(zootr_hook::FAR),
            "the far code isn't linked at FAR"
        );
        &d[off..off + len]
    });
    let (h, sym_off, sym_len) = symtab.expect("the hook has no symbols");
    let strings = u64_at(header(u32_at(h + 0x28) as usize) + 0x18) as usize;
    let symbol = |name: &str| {
        let s = (sym_off..sym_off + sym_len).step_by(24).find(|&s| {
            let at = strings + u32_at(s) as usize;
            d[at..].starts_with(name.as_bytes()) && d[at + name.len()] == 0
        });
        u64_at(s.unwrap_or_else(|| panic!("the hook has no {name}")) + 8)
    };
    let outlined = (sym_off..sym_off + sym_len).step_by(24).any(|s| {
        let at = strings + u32_at(s) as usize;
        d[at..].starts_with(b"OUTLINED_FUNCTION")
    });
    assert!(!outlined, "the hook has an outlined function");
    let (entry, end) = (symbol("entries"), symbol("entries_end"));
    assert_eq!(
        entry, addr,
        "the entries aren't the first thing in the hook"
    );
    // Code before the strings, for the test that it never uses x18.
    println!("cargo:rustc-env=HOOK_CODE={}", symbol("__code_end") - addr);
    let text = &d[off..off + len];
    let branches = text.chunks(4).take_while(|w| w[3] & 0xfc == 0x14).count();
    assert!(
        end - entry == ENTRIES * 4 && branches as u64 >= ENTRIES,
        "the hook doesn't start with {ENTRIES} entry branches"
    );
    (text, far)
}
