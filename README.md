# zootr-nx

A randomizer for The Legend of Zelda: Ocarina of Time on Nintendo Switch 2.
It reads your own copy of the game, moves items to new places, and writes
the result as a mod.

## Read this first

- No piracy. zootr-nx contains no game files, code, text, art or audio. You
  need your own legally obtained copy of the game, dumped from your own
  console. zootr reads your dump at runtime and never distributes anything
  from it.
- No emulation help. The project doesn't provide, link to or support
  obtaining emulators, firmware, keys or game dumps, and won't answer
  questions about them. Issues and discussions asking for them get closed.
- Not affiliated with, endorsed by or associated with Nintendo, the Ocarina
  of Time Randomizer project, or the Ship of Harkinian (Shipwright) project.
  Nintendo, Nintendo Switch 2, The Legend of Zelda and Ocarina of Time are
  trademarks of Nintendo. All trademarks belong to their owners.

## Status

Early. Expect seeds that break, settings that change, and features that
only work on one platform.

- Settings marked Untested in the app haven't been played through yet.
- zootr reads the 1.1.0 update of the game only.
- macOS is the main platform. Windows and Linux builds exist but are barely
  tested.
- Live tracking, `give` and `set` read the running game's memory, which
  works on macOS only. `give` also needs a seed installed with the dev panel
  on.

## What it does

| Step | What happens |
|---|---|
| Shuffle | Moves items to new places, from chests out to whatever the Shuffles and Dungeons settings add, with glitchless logic so the seed can be beaten |
| Generate | Writes a mod folder with only the game files it changed, a code patch for progressive items, and a spoiler log |
| Install | Writes the mod into each target's mod folder, plus any settings lines the target needs to load it |
| Track | Watches the running game and ticks off checks and items as Link finds them. macOS only |

## Features

### Play

- Generate a seed from your settings, from a seed number, or from a friend's
  share code, which carries the seed and its settings together.
- Each seed shows as a row of icons instead of a number.
- Every seed stays in the history with its spoiler, a name and a note, and
  can be installed again.
- A bar flags settings you've changed since the installed seed, lists them,
  and makes a new seed from them.
- Play launches the game through whichever target you pick.

### Checks

- Every check in the installed seed, by area, ticked off as Link opens it.
- Filter to every check, those not done, or those you've marked.
- Rename a check, add a note to it, or set it aside as skipped.

### Items

- An item tracker that fills in as Link picks things up, with counts and
  upgrade levels.
- Draws zootr-nx's own icons. zootr-nx ships none of the game's art, so the
  game's icons are an opt-in that builds them from your own dump.
- Each section can move, hide and change width, and the tracker takes any
  background colour.
- Pop it out into its own window, sized to the grid, for OBS to capture.

### Live

- A log of the game's values as they change, for working out what a flag
  means. Name a value once and it keeps that name, or mute it for the session.

### Spoiler

- Rounds: what Link can reach at the start, then what each round of finds
  opens up, column by column.
- Way of the Hero: the checks Link can't reach Ganondorf without, starred
  everywhere else too.
- All checks: every check and what it holds, searchable.
- Pick a check to see why it's in its round. What it needs glows gold,
  dashed when it's one option of several, and what it opens up glows blue.
- Covered while the game runs, so a glance doesn't spoil the run. Reveal it
  if you're stuck.

### For testing

- Test setup picks where a new game starts and what Link holds. Seeds built
  with one are marked in the history, and setups save by name.
- Save slot applies presets to an existing save without a new game.
- The dev panel gives items, sets values and teleports Link in the running
  game.

### Everything else

- Light and dark themes, or follow the system.
- Any part of the window that breaks shows what went wrong, with a button to
  copy it for a bug report, and the rest keeps working.
- The sidebar says when a newer release is out, with its notes and a link
  to it. It asks GitHub at launch and every 6 hours. App → Settings turns
  it off.

## Settings

Each panel in the app's sidebar, with what every setting does.

### General

| Setting | What it does |
|---|---|
| Menus open | Untested. Menu pages skip their guide pop-up the first time they fill, and the challenge tapestry is open from the start |
| Clock runs | Time passes from the start, as it does once Link first walks into Hyrule Field |
| No tutorials | Untested. Every guide screen counts as already shown, except the Great Fairy's |
| Chest size | All small makes every chest a small one, which opens without the big chest's long animation. What a chest holds doesn't change |
| Progressive items | Upgrade chests give the next level Link doesn't have: Hookshot then Longshot, Silver then Gold Gauntlets and Scale, Fairy Ocarina then Time, and each bag, quiver and wallet up a size |
| Bombchus without a bomb bag | Bombchus hold up to 50 on their own, so they open the same checks bombs do |
| Quiet Navi | Navi never stops Link to talk on her own. Hints you ask her for still work |
| Gossip stone hints | Each gossip stone names where an item Link needs is hidden, without the Mask of Truth or any small talk |

### Logic & access

| Setting | What it does |
|---|---|
| Open forest | Mido and the forest exit stop blocking Link, as child or adult |
| Open Kakariko gate | The Death Mountain trail is open from the start |
| Logic | Glitchless places each item where Link can reach it without that item, so a seed can always be beaten. No logic places items anywhere |
| Open Door of Time | The Door of Time stands open from the start, so Link can grow up without the stones or the Song of Time |
| Bridge at six medallions | Untested. The rainbow bridge rises once Link holds all six medallions, without the Light Arrows first |

### Inventory

| Setting | What it does |
|---|---|
| Warp kit | A new game starts with the Fairy Ocarina and the six warp songs |
| Starting items | A new game starts with any of the Kokiri Sword, Deku Shield, Slingshot, Bomb Bag and Fairy Ocarina. Wherever one is found holds a Blue Rupee instead. Also the Deku Nut and Stick bags (untested) |
| Quest progress | Untested. Start with a quest step done: Zelda met, the Door of Time open, the carpenters freed, the rainbow bridge up, the trials done, the Claim Check, or the magic meter |

### Shuffles

| Setting | What it does |
|---|---|
| Vanilla chests | Nothing is shuffled: chests, Saria and Zelda give what they always do |
| Business scrubs | Every Business Scrub outside the Dream World sells a check once, at a price you pick from 10 to 90 rupees |
| NPC gifts | Heart pieces and items people hand over become checks, from the Skull Kid to Darunia's bracelet |
| Songs | Ten songs become checks, given without the lesson. The Sun's Song, the Song of Time and the Scarecrow's Song stay where they are |
| Shopsanity | 1 to 8 slots in each of the eight shops sell a check once, at a random price |
| Freestanding heart pieces | The fourteen heart pieces lying out in the world are checks |
| Cows | Each of the eight cows gives a check once, for Epona's Song |
| Merchants | Medigoron's Giant's Knife, the carpet merchant's Bombchus and the bean seller's first bean are checks |
| Gold Skulltulas | Each of the 100 Gold Skulltulas drops a check, and a token for each goes in the pool |
| Pots, crates, rocks and hives | Untested. Each one placed with a drop is a check |

### Dungeons

| Setting | What it does |
|---|---|
| Dungeon rewards | The Spiritual Stones and five medallions become checks. Light stays with Rauru |
| Maps and compasses | Each dungeon's map and compass turn up in a random chest of that dungeon |
| Keys | Start with every dungeon's small keys and boss key, and their 43 chests become checks |
| Fortress keys | Start with the four Gerudo Fortress keys, and the keys the thieves carry become checks |
| Boss heart containers | The heart container each of the eight bosses leaves behind is a check |

### Cutscenes

| Setting | What it does |
|---|---|
| Skip all cutscenes | Turns on every skip below. Cutscenes that give an item, set a flag or move Link still play |
| Skip the intro | A new game starts with Link in control |
| Skip Saria's greeting | Link leaves his house without Saria running up |
| Skip Kaepora Gaebora | The owl never stops Link to talk |
| Skip meeting the Great Deku Tree | His mouth is open from the start |
| Skip area intros | Reaching an area for the first time skips its camera pan |
| By area | For each of 27 areas, dungeons and events, skip every cutscene that only plays, changing nothing |

## What you need

- Your own dump of the game with the 1.1.0 update, as a `.nup` file or a
  folder of its extracted romfs. zootr extracts the files it reads into a
  cache on first use.
- Somewhere to play it that loads a mod's `romfs` folder over the game's.
  zootr calls this a target. You add targets in the app's settings: the app
  to launch, the mod folder to install to, and optionally the save folder
  and settings lines.

There are no prebuilt downloads yet. Build from source below.

## Targets

A target is where a seed installs and where the game runs from. Each one
has these fields:

| Field | What it is |
|---|---|
| `name` | Shown in the Play picker |
| `app` | What Play opens. Its process also tells zootr the game is running |
| `mod_dir` | The mod folder. Install writes `romfs/` here |
| `args` | Optional launch arguments. `{game}` becomes your game dump |
| `save_dir` | Optional. The folder holding `slot_00`, `slot_01` and so on |
| `settings_file` | Optional. A `file` and the `lines` to set in it on install, for apps that read the mod path from a settings file |
| `control` | Optional. A file the game reads input lines from, for the play harness. A settings line names it with `{control}` |

In a settings line, `{romfs}` becomes the mod's romfs folder, `{cheats}` its
cheat folder and `{control}` the target's control file. Install replaces
lines with the same key and keeps the rest of the file.

A target is plain JSON, the same as in `config.json`:

```json
{
  "name": "My target",
  "app": "~/Games/target/app",
  "mod_dir": "~/Games/target/mods/zootr",
  "settings_file": { "file": "~/Games/target/env.txt", "lines": ["MODS={romfs}"] },
  "save_dir": "~/Games/target/save"
}
```

Presets are shared in the community, not in this repo. To take one in, use
Import in the app's Targets settings and paste it, or run
`zootr target import FILE` (`-` reads stdin). The app adds it as a new
target for you to check with the Browse buttons, then save. The CLI saves it
straight away. A name already taken gets ` 2`, ` 3` and so on, and a key
zootr doesn't know is an error that names it. A leading `~` is your home
folder. To share your own, use Copy on a target or `zootr target export NAME`.
Both write your home folder back as `~`.

## Building from source

Tools:

| Tool | Version | Why |
|---|---|---|
| Rust via rustup | Pinned in `rust-toolchain.toml`, installed on first use | Everything |
| `aarch64-unknown-none` target | Listed in `rust-toolchain.toml` | The code patch that runs inside the game, built by `crates/zootr/build.rs` |
| Node.js | A current LTS | The app's UI |
| pnpm | Pinned in `app/package.json` (`packageManager`) | The app's UI |
| Tauri's system libraries | See Tauri's prerequisites guide | The app. Linux needs webkit2gtk 4.1 and friends |

Build:

```sh
git clone <repo-url> zootr-nx
cd zootr-nx
cargo build                     # the CLI and the app's Rust side
cd app
pnpm install
pnpm tauri dev                  # run the app
```

On macOS, live reads need a binary signed with the debugger entitlement.
`pnpm tauri dev` signs the app for you. For the CLI, `make zootr` builds
and signs it, and `make live` runs `zootr live`.

### The CLI

The app is also the CLI: run it with arguments and it skips the window. In
development, run the `zootr` binary:

```sh
cargo run -p zootr -- --game path/to/oot.nup shuffle --help
```

`--game` defaults to the dump set in the app, then `dump/oot.nup`.

| Command | What it does |
|---|---|
| `shuffle` | Generate a seed into a mod folder |
| `extract` | Extract the files zootr reads into the cache and print where |
| `target` | Add a target from a shared preset, or print one to share |
| `save` | Read, compare or edit a save slot by flag name |
| `live` | Print game values as the running game changes them (macOS) |
| `give`, `set` | Give Link an item (with the game's own grant, needs the dev panel seed) or write one value in the running game (macOS) |
| `census`, `items`, `flags`, `flag`, `exits`, `spawns`, `cutscenes`, `chest-events`, `effects`, `flow`, `logic`, `probe` | Research: questions about the game's data |
| `selftest` | Round-trip the game's files through the format code |
| `set-drop` | Make one placed actor drop a different item |

`zootr help <command>` explains each one.

## Running the tests

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace --lib --bins --tests
cd app && pnpm typecheck && pnpm lint && pnpm test
```

Tests that need the game look for `dump/oot.nup` and skip themselves without
it, so the suite passes on a machine with no dump. Put your own dump there to
run them too. `dump/` is ignored by git.

See `CONTRIBUTING.md` before opening a pull request.

## Repository layout

| Path | What it is |
|---|---|
| `crates/zootr` | The randomizer library and the `zootr` CLI |
| `crates/zootr-hook` | Code that runs inside the game, for progressive items |
| `crates/ounce-formats` | Readers and writers for the game engine's file formats |
| `crates/nup-format` | Reader for `.nup` game dumps |
| `app` | The desktop app: Tauri 2, React and TypeScript |

## Licence

MIT. See `LICENSE`.
