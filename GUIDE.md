# zootr-nx guide

The full reference: features, settings, targets and the CLI. The
[README](README.md) has the short version.

## What you need

- Your own dump of the game with the 1.1.1 update, as a `.nup` file or a
  folder of its extracted romfs. zootr extracts the files it reads into a
  cache on first use.
- Somewhere to play it that loads a mod's `romfs` folder over the game's.
  zootr calls this a target. You add targets in the app's settings: the app
  to launch, the mod folder to install to, and optionally the save folder
  and settings lines.

On Linux, the `.deb` and `.rpm` add zootr-nx to your apps with its icon.
The AppImage runs from anywhere, but shows its icon only once an AppImage
manager such as Gear Lever adds it to your apps.

The builds aren't signed yet, so macOS and Windows warn the first time:

- macOS says it can't check the app for malicious software. Try to open it
  once, then choose Open Anyway in System Settings, under Privacy &
  Security. The CLI from the `.tar.gz` needs
  `xattr -d com.apple.quarantine zootr` instead.
- Windows SmartScreen says it protected your PC. Choose More info, then
  Run anyway.

## What it does

| Step | What happens |
|---|---|
| Shuffle | Moves items to new places, from chests out to whatever the Shuffles and Dungeons settings add, with glitchless logic so the seed can be beaten |
| Generate | Writes a mod folder with only the game files it changed, a code patch for progressive items, and a spoiler log |
| Install | Writes the mod into each target's mod folder, plus any settings lines the target needs to load it |
| Track | Watches the running game and ticks off checks and items as Link finds them. On macOS, or any system when the target has a `bridge` |

## Features

### Play

- The Play page files seeds in three tabs: Standard, Archipelago and
  History.
- Generate a seed from your settings, from a seed number, or from a friend's
  share code, which carries the seed and its settings together. Generate &
  install (⌘↵) installs it, Generate only (⇧⌘↵) just adds it to History,
  and the button shows each step while it runs, with Stop on hover.
- Each seed shows as a row of icons instead of a number.
- Every seed stays in the history with its spoiler, a name and a note, and
  can be installed again. History filters by Standard or Archipelago.
- A bar flags settings you've changed since the installed seed (or from the
  defaults, before the first seed, or the last standard seed while an
  Archipelago slot is installed), lists them, and makes a new seed from
  them.
- Play launches the game through whichever target you pick.

### Archipelago

- Joining a friend's game needs only what the host sends: your `.apznx`
  and the room's server. No Archipelago install.
- Hosting needs Archipelago and `znx.apworld` from the same release. The
  Archipelago tab's Save apworld saves the copy built with your app. The
  [setup guide](ap/znx/docs/setup_en.md) has the steps for both.
- The Play page's Archipelago tab installs your slot from its `.apznx`
  (drop it on the window, or Browse…), with the settings the host
  generated it with.
- Its Room section connects the running game to a room. It sends your
  checks, gives Link what other players find for him, and reports your
  goal. The server and password are kept for each seed, and the room's
  status shows under the installed seed from every tab.
- Its Client log lists what was sent and received, filterable by items or
  room notes.
- An item from another world shows at the top of the screen with its
  sender once Link is in play: "Hookshot from Saria". A big release shows
  3, then a count of the rest.
- A check holding another game's item shows a model coloured by its
  Archipelago class, and its get names the player it goes to.
- DeathLink works when your options turn it on.

### Checks

- Every check in the installed seed, by area. It reads the running game
  and ticks each one off as Link gets it, by itself.
- What a check holds shows as hidden until Link gets it. A check marked
  untracked has no flag zootr knows, so it never ticks.
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

### Save Editor

Changes a save you already have, without starting a new game. It reads the
saves in the target's save folder.

- Slots: each one shows when the game wrote it, Link's hearts, songs, Gold
  Skulltulas and rupees, and his stones and medallions.
- Items: Link's status, then his items laid out like the tracker. A click
  steps an item up a tier, a right click steps it down.
- Flags: every value in the save, grouped by name. Search by name, hash or
  label, filter by type, or show only what differs from a new game
  (Changed) or what you're about to write (Staged).
- Presets: the presets a save can take. Turning one on stages its changes,
  which then show in Flags. Presets that give items or move a quest on
  only work on a new game, so they aren't here.
- Nothing is written until you save. The bar counts your changes and
  Discard drops them.
- Close the game first. zootr won't write while a target is running.
- The old save stays beside the new one as a backup.
- If the game writes the slot after you opened it, zootr says so. Reload
  puts your changes on the new file.

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
- The installed seed's stays covered, so a glance doesn't spoil the run.
  Reveal it if you're stuck.

### For testing

- Test setup picks where a new game starts and what Link holds. Seeds built
  with one are marked in the history, and setups save by name.
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

Each page under Randomizer in the app's sidebar, with what every setting does.

### General

| Setting | What it does |
|---|---|
| Menus open | Menu pages skip their guide pop-up the first time they fill, and the challenge tapestry is open from the start |
| Clock runs | Time passes from the start, as it does once Link first walks into Hyrule Field |
| No tutorials | Every guide screen counts as already shown, except the Great Fairy's |
| Chest size | All small makes every chest a small one, which opens without the big chest's long animation. What a chest holds doesn't change |
| Progressive items | Upgrade chests and gifts give the next level Link doesn't have: Hookshot then Longshot, the Goron Bracelet then Silver and Gold Gauntlets, Silver then Gold Scale, Fairy Ocarina then Time, and each bag, quiver and wallet up a size. Business scrubs, cows and the three merchants give the level too, and a scrub's offer names it. Minigame prize chests give the level too, as any chest does, and a Red Rupee in place of arrows, seeds or bombs until Link has what fires them. A shop shelf shows the level Link would get, moves up once he buys one elsewhere, and keeps the level it sold. The spoiler calls each strength level Strength |
| Bombchus without a bomb bag | Bombchus hold up to 50 on their own, so they open the same checks bombs do |
| Quiet Navi | Navi never stops Link to talk on her own. Hints you ask her for still work |
| Gossip stone hints | Each gossip stone names where an item Link needs is hidden, without the Mask of Truth or any small talk |

### Enhancements

Changes to how Link plays. All start off.

| Setting | What it does |
|---|---|
| Bunny Hood speed | Link runs half again as fast while he wears the Bunny Hood |
| Swords and shields for both ages | Child and adult Link can each wield every sword and shield, and logic expects them to: a child with the Mirror Shield counts as able to reflect light. Tunics and boots still need adult Link |

### Logic & access

| Setting | What it does |
|---|---|
| Open forest | Mido and the forest exit stop blocking Link, as child or adult |
| Open Kakariko gate | The Death Mountain trail is open from the start |
| Logic | Glitchless places each item where Link can reach it without that item, so a seed can always be beaten. No logic places items anywhere |
| Open Door of Time | The Door of Time stands open from the start, so Link can grow up without the stones or the Song of Time |
| Bridge at six medallions | The rainbow bridge rises once Link holds all six medallions, without the Light Arrows first |

### Inventory

| Setting | What it does |
|---|---|
| Warp kit | A new game starts with the Fairy Ocarina and the six warp songs |
| Starting items | A new game starts with any of the Kokiri Sword, Deku Shield, Slingshot, Bomb Bag and Fairy Ocarina. Wherever one is found holds a Blue Rupee instead. Also the Deku Nut and Stick bags |
| Quest progress | Start with a quest step done: Zelda met, the Door of Time open, the carpenters freed, the rainbow bridge up, the trials done, the Claim Check, or the magic meter |

### Shuffles

| Setting | What it does |
|---|---|
| Chests | On by default. Off, each chest holds what it always does and the other checks shuffle among themselves. A gift that can't give its item by what Link has, like a pot holding a bag upgrade, keeps it too |
| Business scrubs | Every Business Scrub outside the Dream World sells a check once, at a price you pick from 10 to 90 rupees |
| NPC gifts | Heart pieces and items people hand over become checks, from the Skull Kid to Darunia's bracelet. A gift dealt arrows, seeds or bombs gives a Red Rupee until Link has what fires them, as a chest does |
| Songs | Ten songs become checks, given without the lesson. The Sun's Song, the Song of Time and the Scarecrow's Song stay where they are |
| Shopsanity | 1 to 8 slots in each of the eight shops sell a check once, at a random price. A slot dealt arrows, seeds or bombs shows a Red Rupee until Link has what fires them |
| Freestanding heart pieces | The fourteen heart pieces lying out in the world are checks |
| Cows | Each of the eight cows gives a check once, for Epona's Song. A cow dealt arrows, seeds or bombs gives a Red Rupee until Link has what fires them |
| Merchants | Medigoron's Giant's Knife, the carpet merchant's Bombchus and the bean seller's first bean are checks. A merchant dealt arrows, seeds or bombs sells a Red Rupee until Link has what fires them |
| Gold Skulltulas | Each of the 100 Gold Skulltulas drops a check, and a token for each goes in the pool |
| Pots | Each pot placed with a drop is a check. A broken pot may come back when the area reloads, and its item with it |
| Crates | Each wooden crate placed with a drop is a check. Link breaks one by rolling into it |
| Rocks | Each small rock placed with a drop is a check. Link lifts and throws one to break it |
| Beehives | Each of Kakariko's beehives is a check. Anything thrown or shot but a slingshot seed knocks one down |

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

## Targets

A target is where a seed installs and where the game runs from. Each one
has these fields:

| Field | What it is |
|---|---|
| `name` | Shown in the Play picker |
| `app` | What Play opens. Without a `bridge`, its process also tells zootr the game is running |
| `mod_dir` | The mod folder. Install writes `romfs/` and the code patch, `exefs/zootr.pchtxt`, here. The same patch goes in `cheats/` too when a settings line names `{cheats}` |
| `args` | Optional launch arguments. `{game}` becomes your game dump |
| `save_dir` | Optional. The folder holding `slot_00`, `slot_01` and so on |
| `settings_file` | Optional. A `file` and the `lines` to set in it on install, for apps that read the mod path from a settings file |
| `control` | Optional. A file the game reads input lines from, for the play harness. A settings line names it with `{control}` |
| `bridge` | Optional. A control socket the app opens for zootr: a socket path on macOS and Linux, a pipe name on Windows. The tracker, Live log and dev panel read the game through it, and the game counts as running while it answers |
| `env` | Optional. `NAME=VALUE` lines set when Play starts the app. `{bridge}` becomes the socket |

Some apps apply the code patch only when they build the game, not each
time it starts. With one of those, rebuild the game in that app after
installing a seed from a new version of zootr. Seeds from the same version
share one patch, so a new seed on its own needs no rebuild.

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
Import on the app's Targets page and paste it or open its file, or run
`zootr target import FILE` (`-` reads stdin). The app adds it as a new
target for you to check with the Browse buttons, then save. The CLI saves it
straight away. A name already taken gets ` 2`, ` 3` and so on, and a key
zootr doesn't know is an error that names it. A leading `~` is your home
folder. To share your own, use Copy or Download on a target, or
`zootr target export NAME`.
Both write your home folder back as `~`.

## The CLI

The app is also the CLI: run it with arguments and it skips the window. From
a source checkout, run the `zootr` binary:

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
