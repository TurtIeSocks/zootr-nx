# Changelog

What changed in each release of zootr-nx, newest first. Releases before
0.5.0 have their notes on the GitHub Releases page.

## [0.10.0] - 2026-11-23

### Added

- Cut the graph at the meshes level pieces collide with
- Measure gold skulltulas from floor Link stands on
- Derive the Deku Tree as a child, with crawls and shutter sides
- Stage 2 check kinds join the Archipelago world
- Log through the log facade, the CLI to stderr
- The app's own log, in app-<time>.log and live to the page
- An App > Logs page with the app's own log
- Quest steps, what moves each one and what waits on it
- Read or write one placement's own flag
- A built stand-in's take sets its own Bool, without the hook
- Every pickup stand-in plays its get on each take
- A twin breaks along with the holder whose drop was taken
- A stand-in's take ticks its holder without the hook
- Zootr flagmap quests, the quest catalogue
- `zootr logic explain` shows what derive sees round a spot
- Mark a dealt pot in gold until Link picks up its item
- Gold braces on the big crate, gold leaves on Hyrule Field's shrubs
- Mark every breakable kind, through one look seam
- Measure how far from a holder its button prompt shows
- Sheik stays in the crater until Link has her gift
- The weeding Kokiri offers his weeding until he pays
- Zootr logic quest-gates, the quest gate audit
- A graph's read gates only the node it feeds
- Skip the checks the maintainer keeps out of the pool
- Beating Phantom Ganon promises the forest sage quest

### Fixed

- Read the installed seed's flag for each check
- Name each gift location by its giver, not "<giver>'s Gift"
- Read the crate's folder when the build script runs
- Zelda's moat checks and the Closed door wait for her scene
- A skip seed's Door of Time opens to the Song of Time
- Name the door quest through get() in the catalogue test
- Read the AP data folder from the run-time CARGO_MANIFEST_DIR
- Paint the pot's band on its shoulder and its Triforces on its belly
- Skip the holder's new look field in the spots match

## [0.9.0] - 2026-11-22

### Added

- The child Bow fires arrows, magic arrows too
- An empty bow says the ammo its own projectile uses
- The child Bow's button guide says Shoot Arrow
- Offer Met Zelda in the AP world
- Deku Tree room B's scrub, settled by playtest
- Din's Fire also gets room B's scrub out of the ground
- A bomb also gets room B's scrub out of the ground
- An age switch lands at the arrival point nearest Link
- Offer Gerudo Fortress Freed in the AP world
- The adult Boomerang's throw and catch sounds
- Knowledge entries can name a check
- Each weapon's range, Din's Fire's radius among them
- Gold skulltulas get room checks from their height
- A live room's gold skulltula takes its room rule alone
- Child Link swings the two-handed swords, with his own yells
- Read the Havok meshes level pieces collide with
- The valley bridge and its neighbours follow the carpenters
- The running man's race needs the carpenters freed
- Boot an adult start and --spawn straight into their map
- Each Start item says where it came from
- Each Link draws and puts away the other's items with their sounds
- Watch ranges for one-frame states, and find without a region list
- The adult Slingshot, aimed with its own clips
- Songs and NPC Gifts join the Archipelago world through when rows

### Fixed

- The child aims the Bow with the Bow's clips
- The Deku Seed hash test needs the dump's names
- A check the options shut off gets filler, not a free spot
- Add the reach module the last commit named
- The child's two-handed rolls move as his own
- Lend each Link the other's Cba archive, so a two-handed tap swings
- Each Link keeps his own eyelids in the clips he borrows
- A climb down into water reaches the water's shore
- Copy another slot's Magic Meter or boss key
- Read mesh sections stored as plain floats
- An adult start holds the Master Sword only from its start inventory
- Size each carpenter's rescue schedule like his Step01 one
- Number assets over fixed-size call chunks
- The Songs description names all twelve songs
- Copy String64 sets and copies into a cut scene's caller
- An adult boot marks the Temple of Time as entered
- Leave a nested module's holders out of copy wiring
- Filler name without options, dump job runs only the export's slow test
- The client sends the checks a seed hands over at the start

## [0.8.0] - 2026-11-22

### Added

- Master Sword shuffle
- A file written twice builds on its first write
- Child Link uses the Hammer and hooks, adult Link Deku Sticks and the Boomerang
- A choice of three or more options is a select
- Every setting sits in a named section that folds
- No tutorials on by default, the running race and 100-token reward off
- A setting that needs another greys out until it's on
- A button to check for updates now
- A changelog of every release, loaded as you read
- Every section's header gives its description and count
- Logic & access leads the pages, Inventory closes them
- Each release in the changelog is a fold, only the newest open
- A new game starts with the Master Sword by default
- Zootr flagmap, a graph of the game's state (phase 1: event flows)
- All and None in every section with toggles
- A page's reset goes to the defaults or the last seed
- Zootr flagmap live, the running game's value of each state
- A search at the top of every page, docs-site style
- The search scopes to a group, and a lone group shows it all
- A setting that takes the magic meter off the magic items
- With magic apart, no meter check thinks a magic item brings the meter
- A Magic Meter item, and magic apart from the magic items by default
- The tracker and starting items get the magic line and both fairy powers
- The Archipelago world gets the Magic Meter
- Every destructive button asks for a second click
- Scene logic, phase 3: graphs, placements and actors
- Actor AI and the asks table, phase 4
- Pouch tables as states, phase 5
- Only what the dump holds, no hand tables
- The spinning pot's heart piece is a check (EIE)
- Child Link uses the Megaton Hammer
- The child's Hammer yell and slam clang
- A Win Condition section with Rainbow Bridge and Ganon's Trials
- An Open rainbow bridge that sets only the walk-in played flag
- Ganon's Trials Done sets only the barrier state
- The rainbow bridge by its setting, the tower by Ganon's Trials
- Read the old bridge and trials names as the Win Condition
- Ganon's Trials All Six asks for every trial's tools
- Rainbow Bridge is tested
- Trace, the arsenal probe and a full-gear start for measuring room logic
- Save give puts --start words onto a save
- Equip puts an item on ZR, or gear on or off
- Link's reach comes from logic/limits.toml, measured or read from his parameters
- Room files, confirmed over baseline, and flags the solver gains
- Zootr logic derive --kinds catalogs every dungeon object kind
- Dev face turns Link to a placement, then ZL, for aiming
- Zootr logic derive DUNGEON writes its room files and links
- Room checks and flags wait for their dungeon to go live
- Room files get their checks, and shutters their flags
- Zootr logic audit lists the room files' playtest queue
- Room files hold their exits and arrival spots
- A live dungeon's rooms are places of their own
- Say where each jump link is narrowest
- Face turns Link toward a spot as well as an actor
- A cheat table search for the values that follow a state
- A Door of Time choice, Closed or Open
- A Gerudo Fortress choice, Normal, Fast or Freed
- Drop the Claim Check preset
- A Met Zelda toggle, with the story checks given at start
- Mark the checks the opening gives as given at start
- Reshuffle Logic & Access and fold Dungeons into Shuffles
- Lock the starting inventory tiles a setting gives
- Freed leaves the thieves standing, so Fortress Keys goes with it
- Met Zelda takes Zelda's Lullaby out of the pool
- Setting captions link to the settings they name
- Leap runs a jump across a gap and times X by where Link is
- Leap reports run speed and air, from a 3 m run-up
- A measured child level jump settles the Deku Tree's gaps
- A placement plays the event its params name, so derive finds room clears

### Fixed

- Take the forced Master Sword from a swordless Link after a pull
- The wake-up scene hands a swordless adult no Master Sword
- A skipped first pull still runs the closed door's first-pull path
- The animation file gets the same RESTBL slack
- The Megaton Hammer waits for its own change
- Each Link borrows every one of the other's animation archives
- A lent archive hides every clip its Link already has
- A lent item clip moves as its Link's plain one
- Drop the help bubble beside the sidebar's status pills
- With no seed built, the page reset leads with the defaults
- A section without toggles counts what's "off default"
- The palette's arrows scroll the list and never lose a press
- Done says what is still missing
- Show "Loading…" while the spoiler loads
- Name the table's columns, and point the empty state to Play
- A toggle with an unmet requirement shows off
- The changelog can retry a failed fetch
- A choice with an unmet requirement shows no pick
- The Magic Meter's sigil reads green, not gold
- The magic setting sits in the Item pool section
- Name the roll button
- Give same-named buttons names of their own
- Lower an item's tier from the keyboard
- Let a keyboard reach a badge's tooltip
- Drift always measures from "the last seed"
- One name for the mod, values, spoiler, badges and tabs
- One Filter box, plain flag types, the platform copy key
- Generate only adds to History, Standard gets a sword
- Tooltips say what a button does, not its label
- Title Case for names, buttons and menu items
- The changelog and find's seed results leave the spoiler's icon
- Read links the game converts across types
- An age switch with no other-age map lands at the Market entrance
- The Kakariko gate preset moves the Fire quest past the letter
- Beating Ganondorf needs Ganon's Castle's boss key
- Save give names starting by its path, as main moved its import
- The Deku Tree's Lower Three Deku Sticks needs explosives or fire
- A cell door on a freed carpenter only opens
- The world holds the places any one option opens
- Met Zelda leaves Zelda's Lullaby to Impa's check
- Plainer wording for what Met Zelda takes out of the pool
- Drop the item import Met Zelda's lullaby edit used
- Leap steers by where Link goes, never ZL

## [0.7.0] - 2026-11-21

### Added

- Watch for foes that wake during a shot and slay them
- No-pickup-texts, so a first recovery heart or rupee doesn't stop Link
- No-pickup-texts on by default
- One Tip for every hover hint
- Help bubbles, from one catalog of topics
- Explain the app's pages with help bubbles and tips
- Tuck rare fields under Advanced, list the variables
- A developer tools switch that hidden tools obey
- Hide developer tools behind the new switch
- Warn that the Flags tab edits the save directly
- One code patch for every seed, the table as a cheat
- King Zora moved, Jabu-Jabu open and the Zora's River waterfall open
- A welcome screen, a Get started card, and stuck states that point the way
- Rebuild command and patch list are developer-only fields
- An IPS of the whole patch for a Switch running Atmosphère
- A bad game dump says what's wrong in one sentence, with the raw error behind Details
- Live, builds, Rebuild, the update and the Save Editor say what's wrong in player words
- Adult Deku Tree, the debris at its mouth gone
- Shuffle which dungeon each dungeon door leads to
- Follow shuffled dungeon doors
- List each shuffled dungeon door
- Blue warps keep Link's age while doors shuffle
- Each dungeon sets its own Bool as Link arrives
- Reveal each shuffled door as Link goes through it
- Shuffled doors clear the adult Deku Tree's debris
- Gerudo Fortress fast, one carpenter counts as four
- A starting nut or stick bag shows its slot from the start
- Controls that can't act while the game runs say why instead of failing
- Change dots mark drift from the installed seed, not changes from the defaults
- Enable all, Disable all and Reset to defaults on each settings group page
- One drift bar on every settings page, worded and reviewed like Play's pill
- Gohma counts for adult Link, and his blue warp keeps adult Link adult
- Link's Pocket, Rauru's check given in the opening with the Door of Time open
- The adult catches the Boomerang
- Starting age, child, adult or random
- Hide a live actor's model, and read each actor's placement
- `dev nearby` and `dev hide`
- `dev_nearby` and `dev_hide` commands
- Live actors section in the dev panel
- Every read hides again a listed actor found shown
- Adult Jabu-Jabu, off, always or after Morpha
- Jabu moves off the ice path, and what he'd swallow makes room
- Adult Jabu is sick while the fountain is frozen, happy once it thaws
- Dev panel redesign: fixed header, five tabs, activity line
- Starter bags, the nut, stick and Bombchu bags in the pool
- Bombchu Bags show their bomb bag in the Bombchu's blue
- Read a shape's attributes and bones, and replace a mesh part by part
- The Small Bombchu Bag holds Bombchus, not bombs
- The bags wear the Bombchu's navy and gold, not one blue
- The Small bag gets the red stripes the Medium and Large have

### Fixed

- No-pickup-texts covers the fairy too
- Name no target in what the app shows
- Describe a target as where you play
- Drop tips that only repeat what's shown
- Let the dev panel's hook follow its own switch again
- A removed patch on a build of today's code reads as current
- A target runs only when its app does, not when a command names it
- A moved door takes its dungeon's first-arrival scene
- Read the game once and try up to 100 door layouts
- Items shows only a save played since the installed seed went in
- A moved door carries what its dungeon needs inside
- Two hundred door tries, so a seed almost never runs out
- Drift words live in the seeds store, so the review file exports components only
- An actor no placement made has no hash
- Jabu opens his mouth the first time Link swims up
- The place search no longer stretches down the Warp tab
- Whole seeds with the bags shuffled write again
- The Medium and Large bags carry the Small's rewritten param
- Size the Small bag's rewritten param like the Medium's
- Give the Bombchu slot a saved cap, so a bag get fills it
- Give each Bombchu Bag a pouch count, so the next chest gives the next bag

## [0.6.0] - 2026-11-20

### Added

- Stable file names, linked from the README
- Unlink an input back to a value, and repoint a child
- Gold Skulltulas by day
- Photograph every check untouched for the ledger
- Swing the camera off Link's back, and keep him alive
- Default the camera swing to 200 ms around, 300 ms up
- A survey mode that shoots every spot and keeps the best two
- Shoot close-ups first and keep the frame Hob scores best
- Wear the Iron Boots as the adult, so underwater checks sink
- Dive the child to underwater checks, and shoot Gold Skulltulas at noon on a day seed
- Stop a check's tries at the first frame Hob scores 4 or more
- Keep every try's frame in tries/
- --go to stand at a check, --here to shoot it by hand

### Fixed

- One name per OS for the app files and the CLI
- Raise each holder before its photo, and ask Hob by name
- Beat gate-only enemies before a shot, and stand back first
- Read Hob's survey score when it comes back as a number
- The child looks down into the water instead of diving
- Drop a landing in the air to the floor, and look up at a high holder
- --go keeps Link's gear and turns infinite health on
- Clear foes a death spawns and ones that rise as Link lands

## [0.5.0] - 2026-11-19

### Added

- Zootr mem lists, searches, reads and writes the game's memory
- The in-game UI's pure parts: input, shared slots, cycle mode
- Link picks what the trade slot shows on the pause menu
- The trade slot blooms while Link cycles it
- The trade slot's border turns blue while Link cycles it
- Leave a toggle open as a per-player setting
- Name the small Deku Stick bundle, four chest items and the Dream World
- Export the world graph, pool and options for Archipelago
- The znx Archipelago world
- Cut the trade chain's countdowns on every seed
- The app installs a new release and restarts on it
- Install & restart sits beside the release page and skip
- Links in the notes, skip and install in the footer
- Release links in the popover header, version in the sidebar
- Deal the adult trade chain as ten checks
- Rules for the ten trade checks
- Copies that stand in for another player's item
- The .apznx carries the start inventory
- Open a .apznx and install its slot's mod
- Open an AP seed from the Play page
- A gift may hold what a flow-mate gives (lift narrowing 7)
- A scrub gives an upgrade's level and ammo by what Link owns
- Tick a Business Scrub once Link buys from it
- A shared queue for every game, and a hold for every command
- Play takes this machine's game in the turn queue
- Give a progressive item at the level Link needs next
- An AP seed carries the give branch and a received count
- A dealt give swaps ammo for a red rupee, as a chest does
- The client core: send checks, give received items, report the goal
- Zootr ap connect runs the Archipelago client headless
- Connect the installed Archipelago seed to its room from Play
- Tick cows and the gifts whose Bool the seed adds
- A seed Bool for each gift the game marks with nothing
- Tick each Gerudo thief by its own key or a seed Bool
- `join --both` waits for every game at once
- Tick placements by their per-placement flags
- List the per-placement flags that are set
- Toggle for the House of Skulltula's 100-token reward
- Cows and sales give ammo and a line's level by what Link owns
- The running man's race as a check, with its own setting
- Minigame prize chests hold every upgrade level
- Generate a seed for History without installing it
- Twin each shuffled pot, box, rock and hive with its vanilla self
- Tick a twinned pot, box, rock or hive as Link takes its drop
- Tick Ruto's letter's spot when Link takes what lies there
- The spot list moves to the padding after text
- Gold Skulltulas stand until their drop is picked up
- A check slot sets a Bool when it sells, and the tracker ticks it
- A BoolExp program reads other BoolExps and ORs
- A shelf shows the upgrade level or ammo a slot gives
- Redraw BC1 textures, and probe icons says each format
- Another game's item shows a tinted harp by its AP class
- Minigame prize chests hold ammo
- Another game's item shows a Poe's spirit tinted by its class
- Probe ledger and the accounting test
- Read the spot list from GameData
- Show a check's name from the rule files
- `zootr logic set` edits one check in the rule files
- A one-shot kill bit, and `zootr dev kill`
- A DeathLink option, carried in the seed
- DeathLink in the client
- `zootr dev checked HASH` reads a check's flag live
- Every breakable is a check (EIE #12)
- A holder dropping bugs brings a Recovery Heart
- Routes climb ladders and vine walls
- Dampe's repeat prize chests are the maintainer's call (EIE #8)
- The Business Scrub Seed chests join the shuffle (EIE #9)
- Routes climb down ladders and vine walls
- Seeds carry every slot's name
- Received items show in game
- Swim across water: routes, swimming ashore, dashing
- A writer that can grow a graph
- Sink to a holder under water in the Iron Boots
- Under water, try each side of a holder in turn
- Dive onto a holder under water with the Gold Scale
- Find a graph's root and its first bool selector
- A stale step drop removes itself
- Show a line's level or ammo a step at a time
- The Play page's folder tabs
- One Generate & install split button
- A seed's summary says whether its slot plays DeathLink
- The Archipelago folder's Slot, Room and Client log
- The hero for an installed Archipelago slot
- History filters by kind, and drift skips an AP slot
- The Archipelago tab says who needs what, with the apworld
- Save znx.apworld, the copy built with this app
- The Slot section's help trimmed, the apworld in its header
- Steps at holders quiet logic groups watch, each copy gets its own group
- A second code cave for code that rarely runs
- Fortress Keysanity, the thieves' keys as copies at any check
- An Enhancements group, with Bunny Hood speed in it
- Equipment for both ages, unlocking the Equipment page
- Count on Equipment for both ages, and expose it to AP
- Quiet enemies for drill and sweep seeds
- The two loose keys are checks whenever keys move
- Swords and shields for both ages, tunics and boots stay adult
- Maps and compasses as their dungeon's copies, dealt anywhere
- Count small keys room by room, starting with the well
- Ganon's Castle counts its one key room by room
- Join --on GAME waits for that game alone
- Duplicate a node and grow or swap a multi-input's sources
- A loud-watched holder's copies get nodes of their own
- Read a share code's settings without generating
- The build overview on the Standard tab
- A found ice trap freezes Link once he's free to move
- `zootr dev freeze` springs an ice trap, and `mem` reads `main+`
- An ice-traps setting swaps junk for traps disguised as needed items
- The full-body ice on a freeze, fitted to child Link
- A trap's get window shows "FOOL!" alone
- A freeze lasts about 3 s, for the same damage
- Two Kokiri kids' rupee rewards become checks (EIE #13)
- Quiet mode keeps the enemies a check needs, by rule
- Walk the live actors from the game's actor table
- Slay an enemy for real, by placement or name
- The thieves' keys show a line's level a step at a time
- Storeys by room, and each face's dungeon room
- Each dungeon room's small key count
- Each spot's storey, and locks by the rooms they join

### Fixed

- A cycle starts from the choice and survives its own refresh
- The left stick cycles the trade slot
- A child's trade items aren't greyed in the trade slot
- A launch makes the bridge socket's folder
- One exit per pair of places, and no event that never happens
- Export only what Link reaches, one spot per place, full golden settings
- Give 14 never-reachable chests rules Link can meet
- Biggoron's sword wants the Claim Check, which the chain gives
- Turn updater files off with a config file, not TAURI_CONFIG
- A version the app can't read leaves the sidebar's blank
- Name copies through key_tints' made-item prefix
- A sold scrub takes its leaf with it
- A hold covers only this machine's game
- The riddle scrub shows what it sells, at both ages
- A session with a kept game waits for that game alone
- Refuse to keep a folder that holds no slots
- An Archipelago seed isn't badged as a test, and names its slot
- Receive items despite archipelago_rs's items flags, and test a room
- A give that fails no longer holds up sending checks
- Rule the Spirit Temple's thirteen unruled checks
- Link holds up an ocarina dealt to a chest
- The ocarinas calculate in turn, so Link holds them up
- The ocarinas turn over Link's head in a get
- Grow the save layout for every added Bool and Int
- Tick only by per-placement flags that never reset
- `ap connect` renews its turn while it runs
- Read the controller through direct branches
- Face a person again after closing in, before A
- A gift's line before its grant names the level it gives
- A restore drops the stale slots at the save folder's top
- Count the running man's Purple Rupee as a chest's
- Only playing the game renews a hold, reading it doesn't
- Tick a twinned holder on every take, window or not
- A --mod-dir that is a target's mod folder needs the turn
- Programs the AppImage starts leave its mount behind
- The rebuild tests start their programs through platform::command
- Set and its test read the game's own gamedata when no target is set up
- A client pass gives every item still owed, not one
- A Gold Skulltula keeping its own token stays vanilla
- Let foreign.rs spell out the harp's files
- Refuse a --setting value the setting doesn't take
- Restore other games' items shown by class, lost in a merge
- The spirit's glow takes its class's colour from its material
- Count flying pots as pots (EIE #12)
- `dev checked` says when the seed lacks a check's Bool
- Twin a check that keeps its own item
- Notices take over the amiibo banners
- A burst's first notice shows while the rest are given
- Gives wait only on a burst's first few notices
- Browse… sized for its label, and the seed row wraps
- Folder tabs switch without bouncing, inactive ones tucked behind
- The carpet merchant needs the Hover Boots
- The Graveyard south heart piece chest needs nothing
- King Dodongo's door needs a way past the bomb room's eyes
- A key copy placed by hand is one of the copies dealt
- Tunics and boots stay adult with Equipment for both ages
- Count the thieves' keys as moved, as every shuffle should
- Only adult Link reflects light with the Mirror Shield
- Keep the two-handed swords adult-only
- Name the setting for one-handed swords
- The ladder holds its turn and renews it, from any shell
- A rupee on a shelf never warns the bag is full
- No tutorials skips the shop roof girl's targeting stop
- Name at most 64 ranges a mem_read_many, as Hookshot takes
- Keep each mem_read_many under the game's 1 MB a request
- A check slot's sign says what it sells
- Starting inventory chips in the build overview
- One freeze per trap, and child Link's freeze shows its ice
- Declare the actor a thief's key call is renamed to
- A new game no longer starts the forest sage's quest
- The first swap to adult starts the forest sage's quest
- Declare every actor a rewrite names, and refuse a flow that doesn't
- Slay refuses anything that isn't an enemy
- Slay's lines end without a blank one
- A loose key dealt a line or ammo stands until picked
- A thief's stepped key waits for her to fall, as the vanilla key does
- A thief's stepped key keeps her vanilla gate
- Give each step of a thief's key its own instance name
- Lay a moved thief key where she falls, on the floor

### Faster

- Look a gift up by its hash through an index built once
- Build our crate at opt-level 1 in dev builds too
- Find each GameData value once, read check flags in one pass
- Give one item a pass so the room is answered between gives
- A lean busy look for the AP client and settle
- Look right away when items or a DeathLink arrive
- Keep the client log to its newest 200 lines
- A snapshot in three requests, so the recorder stops hogging the bridge
- Build it as one codegen unit with fat LTO, 2.5 KB smaller
- Keep the dev panel's attached game between commands
- Parse history.json again only once it changes
- Keep the creators and chest gates per dump
- Keep the placed checks and shop slots per dump
- Skip cutscenes with the seed's flows
- Keep the catalogue, arrivals and exits per dump
- Parse the mod's maps a batch at a time on every core
- Serialize the changed maps in batches on every core
