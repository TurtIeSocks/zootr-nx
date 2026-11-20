# Releasing

A `v*` tag runs `.github/workflows/release.yml`. It makes a draft GitHub
release and attaches the app and the CLI for each platform. Nothing is
public until you publish the draft.

## What a release contains

| Platform | Runner | App bundles | CLI |
|---|---|---|---|
| macOS, Apple silicon | `macos-latest` | `zootr-nx-macos-arm64.dmg`, `.app.tar.gz` (the updater's) | `zootr-macos-arm64.tar.gz` |
| Windows, x86_64 | `windows-latest` | `zootr-nx-windows-x86_64-setup.exe` (NSIS), `.msi` | `zootr-windows-x86_64.zip` |
| Linux, x86_64 | `ubuntu-24.04` | `zootr-nx-linux-x86_64.AppImage`, `.deb`, `.rpm` | `zootr-linux-x86_64.tar.gz` |

The app is built by `tauri-apps/tauri-action`, with every bundle Tauri makes
for the platform (`bundle.targets` is `"all"`). File names leave out the
version, so `releases/latest/download/NAME` always finds the newest (the
README links them). The draft gets `latest.json`, the feed installed apps
read, which carries each update's signature (see "Updater key"). The
CLI is the `zootr` binary from `crates/zootr`, built after the app with
`cargo build --release -p zootr -p zootr-app --bin zootr` so it reuses the
app's compiled crates. On macOS
it gets an ad-hoc signature with `crates/zootr/live.entitlements`, like
`make zootr` does, so `zootr live` can read the running game.

## Cutting a release

Commit messages pick the version, and `CHANGELOG.md` writes itself.
`.github/workflows/release-pr.yml` does both, with `cliff.toml`.

1. Merge PRs into `main` as usual. Their commits' conventional types
   decide the next version: a `feat` bumps the minor, a `fix` or `perf` the
   patch (`features_always_bump_minor` in `cliff.toml`, while the version
   is 0.x).
2. After each merge the workflow opens or updates one PR, `chore(release):
   vX.Y.Z`, on the `release` branch. It bumps `version` in the root
   `Cargo.toml` (every crate inherits it, and the app's bundles take it
   from the app crate) and `world_version` in `ap/znx/archipelago.json`,
   and prepends the version's section to `CHANGELOG.md`. CI runs on it
   through `workflow_dispatch`.
3. Merge the release PR when it's time for a release. The workflow tags
   `vX.Y.Z` and calls `release.yml` with the tag, which builds every
   platform and makes the draft with the changelog section as its notes.
4. Each platform builds on its own, and a failed one doesn't stop the
   others.
5. Open the draft on the Releases page. Check every asset in the table above
   is there, then publish.

Publishing is what tells running copies of the app. Each one asks GitHub
for the latest release at launch and every 6 hours, and shows its notes
in the sidebar. A draft or a release marked as a prerelease never shows.

Its popover also offers "Install & restart" once the release's
`latest.json` lists the player's platform. The app downloads the update,
checks its signature, installs it and comes back up on the new version.
Releases before 0.5.0 have no `latest.json`, so copies older than that
update by hand once.

### What the changelog keeps

`Added` for `feat`, `Fixed` for `fix`, `Faster` for `perf`. Everything else
stays out: merge commits, `chore`, `refactor`, `test`, `docs`, `ci`, and the
dev-only scopes `sweep`, `probe` and `play`. Players read it in the app, so
a new dev-only scope belongs in `cliff.toml`'s skip list.

To see what the next release would say, with git-cliff installed:

```sh
git cliff --bumped-version
git cliff --unreleased --tag "$(git cliff --bumped-version)"
```

## When something goes wrong

- A failed build: fix it on `main`, then rerun the failed job from the
  Actions page. The rerun finds the draft the first run made and replaces
  the assets it uploads.
- A bad tag: delete the draft, then the tag. The next push to `main` tags
  the version again, since it has no tag.

  ```sh
  git push origin :refs/tags/v0.2.0
  git tag -d v0.2.0
  ```

- The workflow is down: bump `version` in the root `Cargo.toml`, run
  `cargo update --workspace`, prepend the section with
  `git cliff --unreleased --tag vX.Y.Z --prepend CHANGELOG.md`, commit as
  `chore(release): vX.Y.Z`, then tag and push the tag by hand. A pushed tag
  runs `release.yml` on its own.

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

### Updater key

The updater trusts only bundles signed with the key whose public half is
`plugins.updater.pubkey` in `app/src-tauri/tauri.conf.json`. Tagged builds
sign with two secrets:

| Secret | What it is |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | The private key from `pnpm tauri signer generate` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Its password |

- The maintainer keeps a copy of both in a password manager. A lost key means
  installed copies can't update again until players reinstall by hand.
- A new key pair means a new `pubkey`, and copies built with the old one
  can't install a release signed with the new one.
- Untagged builds have no key and pass `--config
  src-tauri/no-updater.conf.json` to turn the updater files off. Do the same
  for a local bundle: `pnpm tauri build --config src-tauri/no-updater.conf.json`.
- A debug build reads `ZOOTR_UPDATER_ENDPOINT` and `ZOOTR_UPDATER_PUBKEY` in
  place of the configured feed and key, to try an install against a local
  `latest.json`. A release build ignores both.

### Platform signing

There are no platform signing secrets yet. Without them:

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

## The build cache

A tag can only restore caches saved on its own ref or on `main`, so the
Release workflow also runs on `main` every Monday, and by hand from the
Actions tab. Those runs build everything, release nothing, and save the
cache the next tag starts from. Tag runs don't save one.
