# Releasing

A `v*` tag runs `.github/workflows/release.yml`. It makes a draft GitHub
release and attaches the app and the CLI for each platform. Nothing is
public until you publish the draft.

## What a release contains

| Platform | Runner | App bundles | CLI |
|---|---|---|---|
| macOS, Apple silicon | `macos-latest` | `.app` (as `.app.tar.gz`), `.dmg` | `zootr-<tag>-macos-arm64.tar.gz` |
| Windows, x86_64 | `windows-latest` | NSIS `-setup.exe`, `.msi` | `zootr-<tag>-windows-x86_64.zip` |
| Linux, x86_64 | `ubuntu-24.04` | `.deb`, `.AppImage`, `.rpm` | `zootr-<tag>-linux-x86_64.tar.gz` |

The app is built by `tauri-apps/tauri-action`, with every bundle Tauri makes
for the platform (`bundle.targets` is `"all"`). The CLI is the `zootr` binary
from `crates/zootr`, built with `cargo build --release --bin zootr`. On macOS
it gets an ad-hoc signature with `crates/zootr/live.entitlements`, like
`make zootr` does, so `zootr live` can read the running game.

## Cutting a release

1. Pick the new version, for example `0.2.0`.
2. Bump it everywhere it's written:

   | File | Field |
   |---|---|
   | `crates/zootr/Cargo.toml` | `version` |
   | `crates/ounce-formats/Cargo.toml` | `version` |
   | `crates/nup-format/Cargo.toml` | `version` |
   | `crates/zootr-hook/Cargo.toml` | `version` |
   | `app/src-tauri/Cargo.toml` | `version` |
   | `app/package.json` | `version` |
   | `app/src-tauri/tauri.conf.json` | `version` |

   The crates aren't published, so only the app's version shows anywhere.
   Keeping all of them in step means nobody has to remember which ones count.
   `tauri.conf.json` decides the version in the bundle names and metadata.

3. Run the gate. It also refreshes `Cargo.lock`:

   ```sh
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test --workspace --lib --bins --tests
   cd app && pnpm typecheck && pnpm lint && pnpm test
   ```

4. Commit the bump as `chore(release): v0.2.0`.
5. Tag and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

6. Wait for the Release workflow. Each platform builds on its own, and a
   failed one doesn't stop the others.
7. Open the draft on the Releases page. Check every asset in the table above
   is there, edit the generated notes, then publish.

Publishing is what tells running copies of the app. Each one asks GitHub
for the latest release at launch and every 6 hours, and shows its notes
in the sidebar. A draft or a release marked as a prerelease never shows.

## When something goes wrong

- A failed build: fix it on `main`, then rerun the failed job from the
  Actions page. The rerun finds the draft the first run made and replaces
  the assets it uploads.
- A bad tag: delete the draft, then the tag, and tag again.

  ```sh
  git push origin :refs/tags/v0.2.0
  git tag -d v0.2.0
  ```

## App identifier

`identifier` in `app/src-tauri/tauri.conf.json` is
`io.github.turtiesocks.zootr-nx`. It's the macOS bundle ID, and Tauri keeps
the webview's data in a folder named after it, which holds the theme and the
Live tab's column widths. Changing it resets those for installed apps.

zootr's own files don't depend on it. Config, seed history, labels and
session logs live in a `zootr` folder in the user's config folder, and
extracted dumps in a `zootr` folder in the cache folder:

| Platform | Config | Cache |
|---|---|---|
| macOS | `~/Library/Application Support/zootr` | `~/Library/Caches/zootr` |
| Windows | `%APPDATA%\zootr` | `%LOCALAPPDATA%\zootr` |
| Linux | `$XDG_CONFIG_HOME/zootr`, or `~/.config/zootr` | `$XDG_CACHE_HOME/zootr`, or `~/.cache/zootr` |

`ZOOTR_CONFIG` overrides the path of `config.json`, and the other files sit
beside it. `ZOOTR_CACHE` overrides the cache folder.

## Signing

There are no signing secrets yet. Without them:

- The macOS app keeps the ad-hoc signature `tauri.conf.json` asks for
  (`signingIdentity: "-"`) and isn't notarized. Players have to approve it in
  System Settings, Privacy and Security, the first time it opens.
- Windows shows a SmartScreen warning for the installers.
- Linux bundles aren't signed.

### macOS secrets

All optional. Add them under Settings, Secrets and variables, Actions. The
workflow passes on only the ones that are set.

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | A Developer ID Application certificate with its private key, exported from Keychain Access as `.p12`, then base64 encoded (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password chosen when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | The certificate's name in the keychain, `Developer ID Application: <name> (<team id>)`. It replaces the ad-hoc `-` from `tauri.conf.json` |
| `APPLE_ID` | The Apple Account that notarizes the app |
| `APPLE_PASSWORD` | An app-specific password for that account, made at account.apple.com. Not the account password |
| `APPLE_TEAM_ID` | The 10 character team ID from the Apple Developer account |

- Signing needs the first three.
- Notarizing needs all six. Tauri notarizes when `APPLE_ID`, `APPLE_PASSWORD`
  and `APPLE_TEAM_ID` are set.
- The app is signed with the debugger entitlement from
  `crates/zootr/live.entitlements`, which the live features need. Check that
  notarization accepts it before relying on it.
- The CLI keeps its ad-hoc signature either way.

Windows and Linux have no secret slots yet. Tauri's guides cover adding
Windows code signing when it's wanted.
