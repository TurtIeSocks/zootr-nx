use std::fs;
use std::path::PathBuf;

use nup_format::fixture::{self, Package};
use nup_format::{Error, Kind, Node, Nup};

const TITLE: u64 = 0x0100_0000_0000_1000;
/// Long enough that zstd shrinks it.
const BIG: &[u8] = &[b'z'; 4096];

fn base(files: &[(&str, &[u8])]) -> Vec<u8> {
    fixture::build(&[Package {
        kind: 0,
        version: 0,
        title_id: TITLE,
        files,
    }])
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nup-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn open(name: &str, bytes: &[u8]) -> nup_format::Result<Nup> {
    let path = temp(name).join("t.nup");
    fs::write(&path, bytes).unwrap();
    Nup::open(&path)
}

fn file(nup: &Nup, path: &str) -> nup_format::File {
    let romfs = nup.packages()[0].romfs.unwrap();
    match nup.find(romfs, path) {
        Some(Node::File(f)) => f,
        other => panic!("{path}: {other:?}"),
    }
}

#[test]
fn reads_raw_and_zstd() {
    let bytes = base(&[
        ("romfs/a.txt", b"hi"),
        ("romfs/Pack/big.bin", BIG),
        ("romfs/empty", b""),
    ]);
    let nup = open("read", &bytes).unwrap();
    let pkg = &nup.packages()[0];
    assert_eq!(
        (pkg.title_id, pkg.kind, pkg.exefs),
        (TITLE, Kind::Base, None)
    );
    assert_eq!(nup.read(file(&nup, "a.txt")).unwrap(), b"hi");
    assert_eq!(nup.read(file(&nup, "Pack/big.bin")).unwrap(), BIG);
    assert_eq!(nup.read(file(&nup, "empty")).unwrap(), b"");
    assert!(nup.find(pkg.romfs.unwrap(), "Pack/missing").is_none());
    assert!(nup.find(pkg.romfs.unwrap(), "a.txt/x").is_none());
}

#[test]
fn extracts_a_tree() {
    let bytes = base(&[
        ("romfs/a.txt", b"hi"),
        ("romfs/Pack/big.bin", BIG),
        ("exefs/main", b"x"),
    ]);
    let nup = open("extract", &bytes).unwrap();
    let out = temp("extract-out");
    assert_eq!(
        nup.extract(nup.packages()[0].romfs.unwrap(), &out).unwrap(),
        2
    );
    assert_eq!(fs::read(out.join("a.txt")).unwrap(), b"hi");
    assert_eq!(fs::read(out.join("Pack/big.bin")).unwrap(), BIG);
    assert!(!out.join("main").exists());
}

#[test]
fn rejects_a_bad_toc_hash() {
    let mut bytes = base(&[("romfs/a.txt", b"hi")]);
    bytes[0x20] ^= 1;
    assert!(matches!(open("toc-hash", &bytes), Err(Error::Corrupt(_))));
}

#[test]
fn rejects_an_extent_out_of_bounds() {
    let mut bytes = base(&[("romfs/a.txt", b"hi")]);
    let at = fixture::file_record(&bytes, 0) + 0x10;
    bytes[at..at + 8].copy_from_slice(&(1u64 << 40).to_le_bytes());
    fixture::rehash(&mut bytes);
    assert!(matches!(open("extent", &bytes), Err(Error::Corrupt(_))));
}

#[test]
fn rejects_an_unknown_codec() {
    let mut bytes = base(&[("romfs/a.txt", b"hi")]);
    let at = fixture::file_record(&bytes, 0) + 0x40;
    bytes[at] = 7;
    fixture::rehash(&mut bytes);
    assert!(matches!(open("codec", &bytes), Err(Error::Unsupported(_))));
}

#[test]
fn rejects_reserved_kinds() {
    for kind in [3, 4, 9] {
        let bytes = fixture::build(&[Package {
            kind,
            version: 0,
            title_id: TITLE,
            files: &[],
        }]);
        assert!(
            matches!(open("kind", &bytes), Err(Error::Unsupported(_))),
            "kind {kind}"
        );
    }
}

#[test]
fn rejects_a_bad_file_hash() {
    let mut bytes = base(&[("romfs/a.txt", b"hi")]);
    let at = fixture::file_record(&bytes, 0) + 0x20;
    bytes[at] ^= 1;
    fixture::rehash(&mut bytes);
    let nup = open("file-hash", &bytes).unwrap();
    assert!(matches!(nup.read(file(&nup, "a.txt")), Err(Error::Hash(_))));
}

#[test]
fn rejects_unsafe_names() {
    for name in ["", ".", "..", "a\\b", "a:b", "a\0b"] {
        let path = format!("romfs/{name}/x");
        let bytes = base(&[(&path, b"hi")]);
        assert!(
            matches!(open("name", &bytes), Err(Error::UnsafeName(_))),
            "{name:?}"
        );
    }
    // A slash can't be built from a path, so patch one into the pool.
    let mut bytes = base(&[("romfs/a_b", b"hi")]);
    let at = bytes.windows(3).rposition(|w| w == b"a_b").unwrap();
    bytes[at + 1] = b'/';
    fixture::rehash(&mut bytes);
    assert!(matches!(open("slash", &bytes), Err(Error::UnsafeName(_))));
}

#[test]
fn reads_several_packages() {
    let bytes = fixture::build(&[
        Package {
            kind: 0,
            version: 0,
            title_id: TITLE,
            files: &[("romfs/a", b"base")],
        },
        Package {
            kind: 1,
            version: 131072,
            title_id: TITLE | 0x800,
            files: &[("romfs/a", b"upd")],
        },
    ]);
    let nup = open("packages", &bytes).unwrap();
    let [b, u] = nup.packages() else { panic!() };
    assert_eq!(
        (b.kind, u.kind, u.version),
        (Kind::Base, Kind::Update, 131072)
    );
    let Some(Node::File(f)) = nup.find(u.romfs.unwrap(), "a") else {
        panic!()
    };
    assert_eq!(nup.read(f).unwrap(), b"upd");
}
