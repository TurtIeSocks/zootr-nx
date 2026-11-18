# Ocarina of Time (znx) Setup Guide

Archipelago shuffles items between several games at once. One person hosts: they generate the multiworld and run its room. Everyone else just joins with the file the host sends them.

## What you need

| | Joining | Hosting |
|---|---|---|
| The zootr-nx app, from its [GitHub releases](https://github.com/TurtIeSocks/zootr-nx/releases) | Yes | Only if you play too |
| Ocarina of Time for the Switch 2, dumped from your own console | Yes | Only if you play too |
| [Archipelago](https://github.com/ArchipelagoMW/Archipelago/releases) 0.6.8 or newer | No | Yes |
| `znx.apworld`, from the same zootr-nx release as the app | No | Yes |

## Joining a friend's game

1. Get two things from your host: your `.apznx` file and the room's server (like `archipelago.gg:38281`), plus its password if it has one.
2. In the zootr-nx app, open the Play page's Archipelago tab and drop the `.apznx` on the window, or press Browse… and pick it. It installs your slot with the settings the host generated it with.
3. Under Room in the same tab, enter the server and password, then press Connect.
4. Press Play and start a new file.

## Hosting

1. Download `znx.apworld` from the Assets of the zootr-nx release that matches everyone's app. The app's Archipelago tab links the right one.
2. Install it: open the Archipelago Launcher, choose Install APWorld and pick `znx.apworld`. Copying it into Archipelago's `custom_worlds` folder does the same.
3. Each player makes an options file (a `.yaml`) with the Launcher's Options Creator. For zootr-nx, pick the game `Ocarina of Time (znx)`. Players of other games make theirs for their own game.
4. Put every player's `.yaml` in Archipelago's `Players` folder and run Generate from the Launcher. The result is a `.zip` in the `output` folder.
5. Host the room: upload that `.zip` at [archipelago.gg/uploads](https://archipelago.gg/uploads) and use the server shown on its room page, or run Host from the Launcher on your own machine.
6. Inside the `.zip`, each zootr-nx player has a `.apznx`. Send each one their file and the server.

## While you play

The app sends your checks to the room, gives Link the items other players find for him, and reports your goal. The Client log lists each item sent and received. The server and password are kept for each seed, and the room's status line under the installed seed has Connect, so reconnecting later is one press.

- Items from other worlds arrive whenever Link is in play, never during a text box, event or load.
- Each one shows at the top of the screen, like "Hookshot from Saria", with the item's icon over Link for ammo and other pouch items. A big release shows 3 one by one, then "+74 more from Saria".
- With DeathLink on in your options, Link's deaths go to the room and the room's deaths reach Link.
- Items other players found while you were disconnected arrive on your next connect.
