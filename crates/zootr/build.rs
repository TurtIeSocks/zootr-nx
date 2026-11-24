//! Builds `zootr-hook` for the game's CPU and keeps only its code, which a
//! seed's cheat file writes into `main`: the cave's, `hook.bin`, and the
//! far code, `far.bin`. The code has to run where it lands, so the build
//! fails on anything that would need relocating. It builds once for each
//! build of the game, each against its own offsets, into a folder of its
//! own.

use std::path::{Path, PathBuf};
use std::process::Command;

use zootr_hook::{Build, V65536, V131072};

const TARGET: &str = "aarch64-unknown-none";
/// Each build the hook is built for: its folder under `OUT_DIR`, the
/// `zootr_build` it's compiled with (none for 1.1.1, whose offsets the
/// hook takes by default), and its offsets.
const BUILDS: [(&str, Option<&str>, Build); 2] = [
    ("v131072", None, V131072),
    ("v65536", Some("v65536"), V65536),
];
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

/// This crate's folder, read when the script runs. `env!` would bake in the
/// checkout that compiled the script, and a target folder shared between
/// checkouts (the dump runners' cache) reuses the script after that
/// checkout is gone.
fn manifest_dir() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .expect("cargo sets CARGO_MANIFEST_DIR for build scripts")
        .into()
}

fn out_dir() -> PathBuf {
    std::env::var_os("OUT_DIR")
        .expect("cargo sets OUT_DIR for build scripts")
        .into()
}

fn main() {
    room_files();
    let hook = manifest_dir().join("../zootr-hook");
    let (src, script) = (hook.join("src/lib.rs"), hook.join("hook.ld"));
    // A directory is watched through everything under it, and the hook has
    // more than one file.
    for p in [&hook.join("src"), &script] {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    for (name, cfg, build) in BUILDS {
        hook_for(name, cfg, &build, &src, &script);
    }
}

/// Builds the hook against `build`'s offsets into `OUT_DIR/name`.
fn hook_for(name: &str, cfg: Option<&str>, build: &Build, src: &Path, script: &Path) {
    let out = out_dir().join(name);
    std::fs::create_dir_all(&out).unwrap();
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
        // One unit, linked with `core` as one module, so code can be inlined
        // and merged across all of it: about 2.5 KB less in the cave.
        .args(["-C", "codegen-units=1", "-C", "lto=fat"])
        .args(["-C", "relocation-model=pic", "-C", "link-arg=-pie"])
        // Apple's ABI reserves x18 and macOS may clear it, so the game
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
        .args(cfg.map(|c| format!("--cfg=zootr_build=\"{c}\"")))
        .arg(format!(
            "-Clink-arg=--defsym=__cave={:#x}",
            build.regions.cave
        ))
        .arg(format!(
            "-Clink-arg=--defsym=__far={:#x}",
            build.regions.far
        ))
        .arg(format!("-Clink-arg=-T{}", script.display()))
        .arg("-o")
        .arg(&elf)
        .arg(src)
        .status()
        .expect("running rustc for the hook");
    assert!(
        status.success(),
        "the hook didn't build. Is the {TARGET} target installed?"
    );
    let data = std::fs::read(&elf).unwrap();
    let (code, far) = code(&data, name, build);
    std::fs::write(out.join("hook.bin"), code).unwrap();
    std::fs::write(out.join("far.bin"), far).unwrap();
}

/// `.text` and `.far`, after checking the hook needs nothing outside them
/// and is linked at `build`'s cave and far code.
fn code<'a>(d: &'a [u8], folder: &str, build: &Build) -> (&'a [u8], &'a [u8]) {
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
        u64::from(build.regions.cave),
        "the hook isn't linked at the cave"
    );
    let far = far.map_or(&[][..], |(at, off, len)| {
        assert_eq!(
            at,
            u64::from(build.regions.far),
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
    println!(
        "cargo:rustc-env=HOOK_CODE_{}={}",
        folder.to_uppercase(),
        symbol("__code_end") - addr
    );
    let text = &d[off..off + len];
    let branches = text.chunks(4).take_while(|w| w[3] & 0xfc == 0x14).count();
    assert!(
        end - entry == ENTRIES * 4 && branches as u64 >= ENTRIES,
        "the hook doesn't start with {ENTRIES} entry branches"
    );
    (text, far)
}

/// Lists every room file under `logic/baseline` and `logic/confirmed` with
/// its text, for `rules::rooms`. Derive writes them, so no hand-kept list.
fn room_files() {
    let logic = manifest_dir().join("logic");
    let mut files = Vec::new();
    for tier in ["baseline", "confirmed"] {
        let dir = logic.join(tier);
        println!("cargo:rerun-if-changed={}", dir.display());
        collect(&dir, &mut files);
    }
    files.sort();
    let mut out = String::from("&[\n");
    for f in &files {
        let rel = f
            .strip_prefix(&logic)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        out += &format!("    ({rel:?}, include_str!({:?})),\n", f.to_str().unwrap());
    }
    out += "]\n";
    let dest = out_dir().join("room_files.rs");
    std::fs::write(dest, out).unwrap();
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
            .path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|e| e == "toml") {
            files.push(path);
        }
    }
}
