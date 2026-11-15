# nup-format

A Rust reader for NUP containers: unencrypted NX game dumps with the base game, its updates and its add-ons in one `.nup` file.

Written from [SPEC.md](https://github.com/a55uka/nup/blob/main/SPEC.md) v1 of the [nup](https://github.com/a55uka/nup) project. All credit for the format goes there.

```rust
let nup = nup_format::Nup::open("game.nup")?;
let update = nup.packages().iter().find(|p| p.kind == nup_format::Kind::Update).unwrap();
nup.extract(update.romfs.unwrap(), "out/romfs".as_ref())?;
```

## What it checks

- The TOC's SHA-256 before anything in it is used
- Every record, child range and extent is in bounds
- Every file's SHA-256 after decoding
- Names that could leave the extract folder (`..`, `/`, `\`, `:`, NUL) are refused

## What it skips

- Writing containers
- The per-package meta TLV block. The reference tool's TLV ids have been off by one since its typing refactor (upstream issue #35), so containers exist with both numberings. Reading meta needs the legacy-id detection proposed there first.

## Tests

The `fixture` feature adds a small container builder for tests, here and in crates that depend on this one.
