# Contributing

Read the rules first. A pull request that breaks one can't be merged.

## Rules

- No game data in commits. That means no game files, dumps, extracted romfs,
  decompiled or disassembled code, text, art, audio, or screenshots of
  assets pulled from them. zootr reads the player's own dump at runtime.
  Tests that need the game read it the same way.
- `dump/` and `re/` are ignored by git and stay local. Don't force-add them.
- No emulator names. Don't name any emulator or runner in code, comments,
  docs, tests, commit messages or pull requests. Where the code needs one,
  say "the PC build" or "the emulator target". A target is configured by the
  user and nothing in the repo knows which one it is.
- Game names go through `known::*` consts (`n!(0x…)`) or the logic
  vocabulary words, never as text. That covers actor, item, flag, map and
  scene names, in code, comments and tests. Find a hash with
  `zootr probe hash`. A test that needs a made-up name uses one like
  `Item_A`. The guard test in `known/guard.rs` enforces this.
- No help obtaining emulators, firmware, keys or dumps, in issues or
  anywhere else.
- Tests skip without a dump. A test that needs the game gets it from
  `crate::game::dev_dump()` and returns early when that's `None`:

  ```rust
  let Some(romfs) = crate::game::dev_dump() else {
      return;
  };
  ```

  CI has no dump, so a test that panics without one fails the build.

## The gate

Run all of it before every commit. CI runs the same commands.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace --lib --bins --tests
```

For changes under `app/`, also:

```sh
cd app
pnpm install
pnpm typecheck && pnpm lint && pnpm test
```

- Use `--lib --bins --tests`. It runs the unit tests and the integration
  tests in `crates/*/tests`. A bare `cargo test --workspace` also runs the
  doctests, which are very slow.
- `app/src/bindings.ts` is generated from the Tauri commands. When it's
  stale, `bindings_are_current` fails and rewrites it. Commit the new file.

## What CI runs

| Job | Runner | Commands |
|---|---|---|
| Rust | Linux, macOS, Windows | fmt, clippy, tests |
| App | Linux | `pnpm install --frozen-lockfile`, typecheck, lint, tests |

macOS builds the live memory code for real. Linux and Windows build its
stub, which reports that live reads need macOS.

## Commits

- Conventional commits: `feat(scope): ...`, `fix(scope): ...`,
  `docs: ...`, `ci: ...`, `test(scope): ...`.
- The body says what changed and why.
- One change per commit.

## Design

Bigger features start as a short design, agreed in an issue before the
code. Ask the game's data with `cargo run -p zootr -- probe` before
writing a scratch test.
