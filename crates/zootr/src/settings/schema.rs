//! Every setting's definition, as the app and the CLI list them.

use super::*;

/// Every setting, each panel's sections in the order they show.
pub fn schema() -> Vec<SettingDef> {
    let presets = PRESETS
        .iter()
        .map(|p| toggle(p.name, p.place, p.label, p.what));
    let mut all: Vec<SettingDef> = presets.collect();
    all.extend(logic_defs());
    all.extend(check_defs());
    all.extend(breakable_defs());
    all.extend(pool_defs());
    all.extend(dungeon_defs());
    all.extend(enhancement_defs());
    all.push(start_def());
    all.extend(cutscene_defs());
    for d in &mut all {
        d.gives = gives(d);
        d.archipelago = crate::ap::archipelago(d).unwrap_or(Archipelago::Offered);
        if let Kind::Choice { options, .. } = &mut d.kind {
            for o in options {
                o.archipelago = crate::ap::archipelago_option(&d.id, &o.id);
            }
        }
    }
    // Stable, so each section keeps the order the panels above give it,
    // bar those `ORDER` lists.
    all.sort_by_key(|d| {
        let section = place::ALL
            .iter()
            .position(|p| p.group == d.group && p.section == d.section);
        (section, ORDER.iter().position(|id| *id == d.id))
    });
    all
}

/// The Logic panel: how items are placed, the win condition, what stands
/// open and where doors lead.
fn logic_defs() -> Vec<SettingDef> {
    vec![
        choice(
            LOGIC,
            place::LOGIC,
            "Logic",
            "Glitchless places each item where Link can reach it without that item, so a seed can always be beaten. No logic places items anywhere, so a seed may not be",
            Logic::label,
        ),
        forcing(
            choice(
                STARTING_AGE,
                place::LOGIC,
                "Starting Age",
                "How old Link is on a new file. Adult wakes in the Temple of Time, and opens the Door of Time and the forest, so he can go back to child and walk out. He holds the Master Sword only when the start inventory does: otherwise the pedestal gives its check on his first pull back to adult. Random rolls one from the seed, and opens the door and the forest whichever it rolls",
                StartingAge::label,
            ),
            StartingAge::forces,
        ),
        choice(
            GERUDO_FORTRESS,
            place::LOGIC,
            "Gerudo Fortress",
            "How the four carpenters in the Gerudo Fortress start. With Normal, Link frees all four, as in the game. With Fast, freeing any one frees all four and earns the membership card, and the other thieves still guard their cells. With Freed, all four are free from the start, with the membership card in hand, the guards standing down and the valley bridge up. Their thieves still stand, and beating one opens her cell without freeing anyone twice",
            GerudoFortress::label,
        ),
        toggle(
            MET_ZELDA,
            place::LOGIC,
            "Met Zelda",
            "Link starts having met Zelda, with her letter in hand, and Talon and Malon back at Lon Lon Ranch. Her letter's check holds the letter. Impa's check comes at the start (Zelda's Lullaby, unless Songs are shuffled), and Malon's does too when NPC gifts are shuffled. Zelda's Letter and the Weird Egg have been removed from the pool",
        ),
        choice(
            BRIDGE,
            place::WIN_CONDITION,
            "Rainbow Bridge",
            "What raises the bridge to Ganon's Castle. With Vanilla, Link walks onto the ledge once Zelda has given him the Light Arrows, as in the game. With Six Medallions, it rises once he holds all six, without the Light Arrows first. With Open, the bridge stands from the start",
            Bridge::label,
        ),
        choice(
            TRIALS,
            place::WIN_CONDITION,
            "Ganon's Trials",
            "With All Six, the barrier to Ganon's tower comes down once Link clears the castle's six trials, as in the game. With Done, all six barriers are down from the start",
            Trials::label,
        ),
        choice(
            ADULT_JABU,
            place::AREA_ACCESS,
            "Adult Jabu-Jabu",
            "Jabu-Jabu waits in the frozen fountain for adult Link too, from the start or once the Water sage is done. Adult Link walks in when his mouth is open (the Jabu-Jabu open preset), but only child Link's Boomerang beats the tentacles and Barinade. With dungeon entrances shuffled, his door leads wherever it leads for child Link",
            AdultJabu::label,
        ),
        choice(
            DOOR_OF_TIME,
            place::AREA_ACCESS,
            "Door of Time",
            "With Open, the Door of Time stands open from the start, so Link can grow up without the stones or the Song of Time. Zelda and Impa stay until he's met them. He skips Rauru, whose gift comes at the start, and can put the sword back any time. With Closed, it opens as in the game: once Link has met Zelda, brought the three stones to her scene at the moat, and plays the Song of Time on the Ocarina of Time",
            DoorOfTime::label,
        ),
        forcing(
            choice(
                DUNGEON_ENTRANCES,
                place::ENTRANCES,
                "Dungeon Entrances",
                "Each dungeon's front door leads into another dungeon, and its way out leads back to that door. On swaps the eleven dungeons. With Ganon's Castle, its door joins them, and Ganondorf waits wherever it leads. A door keeps what it needs to open, whatever is behind it. The Deku Tree lets adult Link in, and blue warps lead where they always do at Link's own age",
                DungeonEntrances::label,
            ),
            DungeonEntrances::forces,
        ),
    ]
}

/// The checks: which places and people give one.
fn check_defs() -> Vec<SettingDef> {
    vec![
        choice(
            ICE_TRAPS,
            place::CHECKS,
            "Ice Traps",
            "Swaps junk (rupees, ammo, recovery hearts) for ice traps. Each looks like an item logic needs until Link gets it, then says FOOL! and freezes him",
            IceTraps::label,
        ),
        toggle(
            SHUFFLE_ITEMS,
            place::CHECKS,
            "Shuffle Items",
            "Every check gives a shuffled item. Off, each gives what it always does, though a starting item's spot still holds a Blue Rupee",
        ),
        toggle(
            CHESTS,
            place::CHECKS,
            "Chests",
            "Each chest is a check. Off, a chest holds what it always does and the other checks shuffle among themselves, though a starting item's chest holds a Blue Rupee",
        ),
        toggle(
            SCRUBS,
            place::CHECKS,
            "Business Scrubs",
            "Every Business Scrub outside the Dream World sells a check once. Scrubs that sold ammo on repeat sell their one item and leave",
        ),
        requires(
            SCRUBS,
            choice(
                SCRUB_PRICE,
                place::CHECKS,
                "Scrub Price",
                "What every Business Scrub asks for its check, with Business scrubs on",
                ScrubPrice::label,
            ),
        ),
        toggle(
            NPC_GIFTS,
            place::CHECKS,
            "NPC Gifts",
            "What people hand over becomes a check: their gifts, minigame prizes, the House of Skulltula's rewards and the Great Fairies' upgrades",
        ),
        requires(
            NPC_GIFTS,
            toggle(
                HUNDRED_TOKENS,
                place::CHECKS,
                "100-Token Reward",
                "With NPC gifts on, the House of Skulltula's reward for all 100 tokens is a check too. Off, it pays its Gold Rupee as always, and logic never sends Link for all 100",
            ),
        ),
        toggle(
            SONGS,
            place::CHECKS,
            "Songs",
            "Twelve songs become checks, given without the lesson: Zelda's Lullaby, Epona's Song, Saria's Song, the Sun's Song, the Song of Time, the Song of Storms and Sheik's six. The Scarecrow's Song stays where it is",
        ),
        toggle(
            SHOPS,
            place::CHECKS,
            "Shopsanity",
            "Shelf slots in the eight shops sell checks, each once, at a random price. Prices over 99 rupees need a bigger wallet in logic",
        ),
        requires(
            SHOPS,
            choice(
                SHOP_SLOTS,
                place::CHECKS,
                "Shop Slots",
                "How many slots of each shop sell a check with Shopsanity on. The rest sell what they always did",
                ShopSlots::label,
            ),
        ),
        toggle(
            FREESTANDING,
            place::CHECKS,
            "Freestanding Heart Pieces",
            "The fourteen heart pieces lying out in the world are checks. One that holds something else shows it, spinning where the piece was",
        ),
        toggle(
            LOOSE_ITEMS,
            place::CHECKS,
            "Loose Items",
            "Every rupee and Recovery Heart lying loose in the world is a check, and so are the Deku Nut, Seed and Stick on Lake Hylia's bed. Each is taken once, as in the game. One that holds something else shows it where the rupee or heart was",
        ),
        toggle(
            COWS,
            place::CHECKS,
            "Cows",
            "Each of the eight cows gives a check once, for Epona's Song. After that it gives milk, which still needs a bottle",
        ),
        toggle(
            MERCHANTS,
            place::CHECKS,
            "Merchants",
            "People who sell one thing sell a check once, for the price of the thing: Medigoron's Giant's Knife, the carpet merchant's Bombchus, and the bean seller's first bean. Bombchus and beans stay on sale after that",
        ),
        toggle(
            TRADE_CHAIN,
            place::CHECKS,
            "Adult Trade Quest",
            "Each trade on the way to Biggoron's sword becomes a check, from Anju's Pocket Egg to Biggoron's Claim Check, and the ten trade items join the pool. Showing an item to the one who wants it gives that check, whatever order Link finds them in. Grog or Fado waits at the stump by which item Link holds. The Pocket Cucco still hatches from the egg",
        ),
        toggle(
            MASTER_SWORD,
            place::CHECKS,
            "Master Sword",
            "The pedestal holds an item from the pool, and the Master Sword can turn up anywhere. Pulling or returning the sword still travels through time without it. Adult Link without it has no sword",
        ),
        toggle(
            RUNNING_RACE,
            place::CHECKS,
            "Running Man's Race",
            "The running man's prize for Link's first win becomes a check. Later wins still pay a Red Rupee. Logic expects it only with Gerudo Fortress Freed, since he offers the race once the valley bridge is fixed",
        ),
        toggle(
            SKULLTULAS,
            place::CHECKS,
            "Gold Skulltulas",
            "Each of the 100 Gold Skulltulas drops a check when killed. A token for each goes in the pool, so the House of Skulltula still counts them. Logic wants the Boomerang or Hookshot for every one",
        ),
    ]
}

/// The things Link breaks or cuts that give a check once.
fn breakable_defs() -> Vec<SettingDef> {
    vec![
        toggle(
            POTS,
            place::BREAKABLES,
            "Pots",
            "Each pot is a check, flying pots included. One placed with no drop of its own brings a Recovery Heart, a Deku Stick and the like to the pool. Its item stays until Link picks it up. After that the pot drops what it always did",
        ),
        toggle(
            CRATES,
            place::BREAKABLES,
            "Crates",
            "Each wooden crate is a check. Link breaks a big one by rolling into it",
        ),
        toggle(
            ROCKS,
            place::BREAKABLES,
            "Rocks",
            "Each small rock is a check. Link lifts and throws one to break it",
        ),
        toggle(
            GRASS,
            place::BREAKABLES,
            "Grass",
            "Each grass shrub Link can cut or lift is a check, like those outside Saria's house",
        ),
        toggle(
            HIVES,
            place::BREAKABLES,
            "Beehives",
            "Each of Kakariko's beehives is a check. Anything thrown or shot but a slingshot seed knocks one down",
        ),
    ]
}

/// The items that join the pool or change how it gives them.
fn pool_defs() -> Vec<SettingDef> {
    vec![
        toggle(
            PROGRESSIVE,
            place::ITEM_POOL,
            "Progressive Items",
            "Upgrade chests and gifts give the next level Link doesn't have: Hookshot then Longshot, the Goron Bracelet then Silver and Gold Gauntlets, Silver then Gold Scale, Fairy Ocarina then Time, and each bag, quiver and wallet up a size",
        ),
        choice(
            BOMBCHUS,
            place::ITEM_POOL,
            "Bombchus",
            "What caps the Bombchus Link holds. The bomb bag's, as in the game. Their own cap of 40, so they open the same checks bombs do without waiting on the bag. Or three Bombchu Bags in the pool, holding 20, 30 and 40, and none until the first",
            Bombchus::label,
        ),
        toggle(
            SHUFFLE_NUT_BAG,
            place::ITEM_POOL,
            "Shuffle the Nut Bag",
            "The Small Nut Bag is an item in the pool, and Link holds no Deku Nuts until he finds it. Off, the first nut he picks up brings it, as in the game",
        ),
        toggle(
            SHUFFLE_STICK_BAG,
            place::ITEM_POOL,
            "Shuffle the Stick Bag",
            "The Small Stick Bag is an item in the pool, and Link holds no Deku Sticks until he finds it. Off, the first stick he picks up brings it, as in the game",
        ),
        toggle(
            SHUFFLE_MASKS,
            place::ITEM_POOL,
            "Shuffle Masks",
            "The eight masks are items in the pool, less any Link starts with, and the Happy Mask Shop lends none. Off, the shop lends them in order, as in the game",
        ),
        toggle(
            MAGIC_WITH_FIRST_ITEM,
            place::ITEM_POOL,
            "Magic Comes with the First Magic Item",
            "The first spell, magic arrow, Lens of Truth or Improved Spin Link finds turns the magic meter on, as in the game. Off, only a Magic Meter or Doubled Magic does, and a Magic Meter joins the pool",
        ),
    ]
}

/// What bosses and dungeons give, where keys land, and the hints.
fn dungeon_defs() -> Vec<SettingDef> {
    vec![
        toggle(
            DUNGEON_REWARDS,
            place::DUNGEONS,
            "Dungeon Rewards",
            "The Spiritual Stones and five medallions become checks, given by the scene after each boss. Light stays with Rauru",
        ),
        toggle(
            MAPS_COMPASSES,
            place::DUNGEONS,
            "Maps and Compasses",
            "Each dungeon's map and compass turn up at random checks anywhere, and mark their own dungeon wherever Link finds them. Keys stay where they are",
        ),
        choice(
            KEYS,
            place::DUNGEONS,
            "Keys",
            "With Start, Link holds every dungeon's small keys and boss key from the start. With Key rings, each dungeon's small keys come as one ring that opens all its small-key doors. With Keysanity, each small key comes on its own. Rings, small keys and boss keys turn up in random chests and gifts, never in their own dungeon. The 43 chests that held a key become checks. With Start each brings a Red Rupee to the pool. With Key rings, one of a dungeon's rupees becomes its ring and another its boss key. The two keys lying in the open stay where they are",
            DungeonKeys::label,
        ),
        choice(
            FORTRESS_KEYS,
            place::DUNGEONS,
            "Fortress Keys",
            "With Start, Link holds the four Gerudo Fortress keys from the start, so every cell opens whatever the thieves drop. With Keysanity, each key turns up at a random check, anywhere. Either way, the four keys the thieves carry become checks",
            FortressKeys::label,
        ),
        toggle(
            BOSS_HEARTS,
            place::DUNGEONS,
            "Boss Heart Containers",
            "The heart container each of the eight bosses leaves behind is a check",
        ),
        toggle(
            HINTS,
            place::HINTS,
            "Gossip Stone Hints",
            "Each gossip stone names where an item Link needs is hidden, without the Mask of Truth or any small talk",
        ),
    ]
}

/// The comfort and gameplay changes, and the items either Link can use.
fn enhancement_defs() -> Vec<SettingDef> {
    let both = (crate::ageless::toggles::TOGGLES.iter())
        .map(|t| toggle(t.id, place::BOTH_AGES, t.label, t.about));
    let mut defs = vec![
        choice(
            CHEST_SIZE,
            place::COMFORT,
            "Chest Size",
            "All small makes every big chest a small one, which opens without the big chest's long animation. Matches contents makes a chest big when it holds an item the seed needs, the boss key chest when it holds a boss key, and small otherwise. What a chest holds doesn't change",
            ChestSize::label,
        ),
        toggle(
            CHEST_LOOKS,
            place::COMFORT,
            "Chest Looks Match Contents",
            "A chest's metal turns gold when it holds an item the seed needs, or its dungeon's colour when it holds a key. A chest holding a Gold Skulltula token is stained dark with a skull, and one holding a heart is tinted rose with a heart. Anything else looks as it always does. Works with every Chest Size",
        ),
        toggle(
            QUIET_NAVI,
            place::COMFORT,
            "Quiet Navi",
            "Navi never stops Link to talk on her own: the Lost Woods warning, dungeon and quest tips, and reminders. Hints you ask her for still work",
        ),
        toggle(
            QUIET_ENEMIES,
            place::COMFORT,
            "Quiet Enemies",
            "Enemies never show, so nothing grabs Link or knocks him back. Gold Skulltulas, scrubs, Big Poes, the Skull Kid and enemies that are checks stay. For the play harness's seeds",
        ),
        toggle(
            ONE_TAP_SONGS,
            place::COMFORT,
            "One-Tap Songs",
            "With the ocarina out and the songbook open (hold ZL), A on a song plays it, as if Link had entered its notes, instead of a preview",
        ),
        toggle(
            BUNNY_HOOD,
            place::GAMEPLAY,
            "Bunny Hood Speed",
            "Link runs half again as fast while he wears the Bunny Hood",
        ),
        toggle(
            DAY_SKULLTULAS,
            place::GAMEPLAY,
            "Gold Skulltulas by Day",
            "Gold Skulltulas that only come out at night show at any hour",
        ),
    ];
    defs.extend(both);
    defs
}

/// The starting inventory.
fn start_def() -> SettingDef {
    SettingDef {
        kind: Kind::Inventory {
            default: starting::Start::of(&starting::DEFAULT),
        },
        ..toggle(
            starting::SETTING,
            place::START,
            "Starting Inventory",
            "What a new game starts with. Each item leaves the shuffle, and wherever it was found holds a Blue Rupee instead",
        )
    }
}

/// A cutscene skip, which Skip All turns on.
fn cutscene(
    id: String,
    place: Place,
    label: &'static str,
    description: &'static str,
) -> SettingDef {
    SettingDef {
        parent: Some(SKIP_ALL),
        ..toggle(id, place, label, description)
    }
}

/// Skip All, then each skip by name, then each area's.
fn cutscene_defs() -> Vec<SettingDef> {
    let mut defs = vec![
        toggle(
            SKIP_ALL,
            place::CUTSCENES,
            "Skip All Cutscenes",
            "Turns on every skip below. Cutscenes that give an item, set a flag or move Link still play",
        ),
        cutscene(
            SKIP_INTRO.into(),
            place::CUTSCENES,
            "Skip the Intro",
            "A new game starts with Link in control, without the opening movie and Navi's wake-up",
        ),
        cutscene(
            SKIP_SARIA.into(),
            place::CUTSCENES,
            "Skip Saria's Greeting",
            "Link leaves his house without the forest pan or Saria running up to greet him",
        ),
        cutscene(
            SKIP_OWL.into(),
            place::CUTSCENES,
            "Skip Kaepora Gaebora",
            "The owl never stops Link to talk. His rides go too",
        ),
        cutscene(
            SKIP_GREAT_DEKU.into(),
            place::CUTSCENES,
            "Skip Meeting the Great Deku Tree",
            "His mouth is open from the start, and the forest quest is where his talk leaves it",
        ),
        cutscene(
            SKIP_ARRIVALS.into(),
            place::CUTSCENES,
            "Skip Area Intros",
            "Reaching an area for the first time skips its camera pan. Any quest step the arrival sets still happens",
        ),
    ];
    let areas = AREAS.iter().map(|(id, label, _)| {
        cutscene(
            skip_id(id),
            place::CUTSCENE_AREAS,
            label,
            place::CUTSCENE_AREAS.about,
        )
    });
    defs.extend(areas);
    defs
}
