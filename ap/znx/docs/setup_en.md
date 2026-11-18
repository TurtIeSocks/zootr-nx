# Ocarina of Time (znx) Setup Guide

## Required software

- The zootr-nx app, from its GitHub releases
- Ocarina of Time for the Switch 2, dumped from your own console
- Archipelago 0.6.8 or newer, with `znx.apworld` from the same zootr-nx release

## Generating a seed

1. Put `znx.apworld` in Archipelago's `custom_worlds` folder.
2. Make a player options file for `Ocarina of Time (znx)` from the Archipelago launcher's Options Creator.
3. Generate as usual. Each znx player gets a `.apznx` file.

## Playing

1. In the zootr-nx app, open the Play page and press Open AP seed. Pick your `.apznx`. It installs your slot with the settings from your options file.
2. Press Play and start a new file.
3. In the Archipelago panel under the seed, enter the room's address (like `archipelago.gg:38281`) and its password if it has one, then press Connect.

While connected, the app sends your checks to the room, gives Link the items other players find for him, and reports your goal. Its log lists each item that arrives. The server and password are kept for each seed, so reconnecting later is one press.

- Items from other worlds arrive whenever Link is in play, never during a text box, event or load.
- Each one shows at the top of the screen, like "Hookshot from Saria", with the item's icon over Link for ammo and other pouch items. A big release shows 3 one by one, then "+74 more from Saria".
- With DeathLink on in your options, Link's deaths go to the room and the room's deaths reach Link.
- Items other players found while you were disconnected arrive on your next connect.
