//! Checks an NPC hands over in an event flow rather than a chest.

mod cucco;
mod lines;
mod potion;
mod rolling;

pub use lines::lines;

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{EventKind, Flowchart, Link, Params, Romfs, Value};

use crate::checks::{Drop, ItemCheck};
use crate::cutscenes::{PLAYED, transition};
use crate::events::Flows;
use crate::flows::{
    self, ACTOR, FITS_POUCH, GAME_DATA, GRANT, IN_POUCH, IS_BOOL, Rewrites, SET_BOOL, is_local,
    only, param, params_mut, running, string,
};
use crate::known::{Name, actor, field, flag, flow, item, scene};
use crate::settings::{DUNGEON_REWARDS, MERCHANTS, NPC_GIFTS, OPEN_DOOR, SONGS};
use crate::{cows, dampe, grant, pickups, scrubs, shops, starting};

pub struct Gift {
    pub flow: &'static Name,
    /// Banc folder, for the spoiler.
    pub area: &'static Name,
    pub giver: &'static str,
    pub item: &'static Name,
    pub how: How,
    /// The setting that deals it, or none when it's always a check.
    pub setting: Option<&'static str>,
}

/// An edit to one flow.
type Edit = fn(&mut Flowchart) -> Result<()>;

pub enum How {
    /// Rename the item in the grant and the "You got" window.
    InPlace,
    /// Replace the cutscene with a grant, then `then` if any, then its
    /// save changes.
    Cut { then: Option<&'static Name> },
    /// Rename the item in each call to `call` that names it, as
    /// `Flowchart::Entry`, or `::Entry` for one in the same flow, with `|`
    /// between calls when one gift goes through several. `found` is the
    /// Bool set once it's given, if the game sets one.
    Calls {
        call: &'static [Call],
        found: Option<&'static Name>,
    },
    /// For a cutscene that can't hold the grant: each call to `call` naming
    /// the item is emptied, and `caller`, the flow that starts the cutscene,
    /// grants the new item with its window right after the transition. A
    /// cutscene skip that swapped the transition for the scene's played flag
    /// gets the grant right after that instead, the vanilla item included,
    /// since the scene no longer plays.
    Later {
        call: &'static [Call],
        caller: &'static Name,
        found: &'static Name,
    },
    /// A sale of one item. The call to `call` at `price` that sells `sold`
    /// sells the dealt item instead, and `gate` and `shop` edit the giver's
    /// flow and the shop flow around it, whichever the sale needs.
    Sale {
        call: &'static [Call],
        sold: &'static Name,
        price: i32,
        found: Option<&'static Name>,
        gate: Option<Edit>,
        shop: Edit,
    },
}

/// Scenes with a grant that a cutscene skip can cut, and the flow that starts
/// each: the skip swaps the transition for the scene's played flag, and the
/// grant is made right after it. Impa's lullaby is cut by `cutscenes::IMPA`,
/// the rest by the rule in `cut`, which keeps Sheik's Bolero, Minuet and
/// Serenade: the sweep still starts those through their callers.
const SKIPPED_IN: [(&Name, &Name); 17] = [
    (scene::MELODY_GET_ZELDA, flow::DOOR_OF_TIME),
    (scene::GORON_0030, flow::GET_STONE_OF_FIRE),
    (scene::DEATH_MT_TRAIL_0025, ESCAPE),
    (scene::ZORA_FOUNTAIN_0030, ESCAPE),
    (scene::AWAKE_FOREST_GET, ESCAPE),
    (scene::AWAKE_FIRE_GET, ESCAPE),
    (scene::AWAKE_WATER_GET, ESCAPE),
    (scene::AWAKE_SHADOW_GET, ESCAPE),
    (scene::AWAKE_SPIRIT_GET, ESCAPE),
    (scene::SAGE_ROOM_0005, flow::DOOR_OF_TIME),
    (scene::MELODY_GET_FOREST, flow::SAGE_OF_FOREST),
    (scene::MELODY_GET_FIRE, flow::SAGE_OF_FIRE),
    (scene::MELODY_GET_WATER, flow::SAGE_OF_WATER),
    (scene::MELODY_GET_SHADOW, scene::SY_BANC_CHANGE_ARRIVE),
    (scene::MELODY_GET_SHINE, flow::ALL_SAGE),
    (scene::MELODY_GET_STORM, actor::NPC_HYLIA_009_ADULT),
    (scene::TIME_TEMPLE_0130, flow::ALL_SAGE),
];
/// The flow every dungeon's boss warp leaves through.
const ESCAPE: &Name = scene::SY_BANC_CHANGE_ESCAPE_DUNGEON;

/// A call into an event flow, `Flowchart::Entry`, or `::Entry` for one in the
/// same flow.
#[derive(Clone, Copy)]
pub struct Call(Option<&'static Name>, &'static Name);

impl fmt::Display for Call {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Some(flow) => write!(f, "{flow}::{}", self.1),
            None => write!(f, "::{}", self.1),
        }
    }
}

/// `calls` joined by `|`, as a message or a hash says them.
fn said(calls: &[Call]) -> String {
    let said: Vec<_> = calls.iter().map(Call::to_string).collect();
    said.join("|")
}

const fn call(flow: &'static Name, entry: &'static Name) -> Call {
    Call(Some(flow), entry)
}

const fn local(entry: &'static Name) -> Call {
    Call(None, entry)
}

/// A call into the getting flow.
const fn get(entry: &'static Name) -> Call {
    call(scene::SY_GET_IN_EVENT, entry)
}

/// The getting sub-flows every NPC gift goes through.
const GET: Call = get(flow::GET_IN_EVENT);
const GET_QUIETLY: Call = get(flow::GET_IN_EVENT_NOT_TURN_NOT_NPC);
const HAND_OVER: Call = call(
    flow::NPC_COMMON,
    flow::NPC_COMMON_HAND_OVER_NPC_TO_PLAYER_NO_DISPLAY,
);
const HEART: &Name = actor::OBJ_HEART_PIECE;
/// The fishing pond's two prize paths, a plain win and a new record.
const FISHING: [Call; 2] = [HAND_OVER, get(flow::GET_IN_EVENT_NOT_TURN)];
/// How three Great Fairies hand over their gift.
const FAIRY: Call = get(flow::GET_IN_EVENT_DEMO_NOT_LOOKAT_OBJECT_ENTRY);
/// Biggoron, who hands over the sword three days after the claim check.
const BIGGORON: &Name = actor::NPC_GORON_BIGGORON_ADULT;
/// Medigoron, who sells the Giant's Knife once.
const MEDIGORON: &Name = actor::NPC_GORON_MEDIGORON_ADULT;
/// The call that starts his sale.
const MEDIGORON_SALE: Call = call(
    flow::SIMPLE_SHOP,
    flow::SIMPLE_SHOP_ROOT_NPC_GORON_MEDIGORON_ADULT,
);
/// The bean seller, whose first sale is a check.
const BEAN_SELLER: &Name = actor::NPC_HYLIA_047_CHILD;
/// The carpet merchant in the Haunted Wasteland, whose first sale is a
/// check.
const CARPET_MERCHANT: &Name = actor::NPC_HYLIA_044_ADULT;
/// The plain sale only he calls.
const CARPET_SALE: Call = call(flow::SIMPLE_SHOP, flow::SIMPLE_SHOP_ROOT);
/// What he sells: ten Bombchus, shown by the shop's stand-in.
const CARPET_BOMBCHUS: &Name = actor::SHOP_BOMBCHU_BUNDLE_01;
/// The Bool every plain sale sets. Only he calls it, so it is his own.
const CARPET_SOLD: &Name = flag::NPC_HYLIA_044_BUY;
/// The flow every simple sale goes through.
const SIMPLE_SHOP: &Name = flow::SIMPLE_SHOP;
/// The Bool Medigoron's sale sets, so that a sale of anything else still
/// happens once.
const MEDIGORON_SOLD: &str = "Randomizer_Medigoron";
/// The query for how long since a wait began. Case 0 is "long enough".
const ELAPSED: &Name = flow::EVENT_QUERY_CHECK_ELAPSED_TIME;
/// The flow every Great Fairy fountain's ocarina melody runs through.
const FOUNTAIN: &Name = flow::TRI_FORCE_POINT;
const FOUNTAIN_ENTRY: &Name = flow::SUB_PLAY_OCARINA_SEQUENCE_FAIRY_FOUNTAIN;
/// The Bool the game checks before any fountain but Din's opens.
const DIN_FIRE: &Name = flag::IS_GET_ITEM_DIN_FIRE;
/// Stands for the played flag of the gift's own scene, `HavePlayedEvent.`
/// then its flow, which the game's name tables don't list.
const SCENE_PLAYED: &Name = flag::HAVE_PLAYED_EVENT;
/// The plain grant with a get window, by item name.
const POUCH_GRANT: Call = call(
    flow::GET_POUCH_CONTENT,
    flow::GET_POUCH_CONTENT_BY_ACTOR_NAME,
);

const fn npc(
    flow: &'static Name,
    area: &'static Name,
    giver: &'static str,
    item: &'static Name,
    call: &'static [Call],
    found: &'static Name,
) -> Gift {
    Gift {
        flow,
        area,
        giver,
        item,
        how: How::Calls {
            call,
            found: Some(found),
        },
        setting: Some(NPC_GIFTS),
    }
}

/// A song, taught in a scene the gift cuts down to its grant, so there's
/// no lesson.
const fn song(
    flow: &'static Name,
    area: &'static Name,
    giver: &'static str,
    item: &'static Name,
) -> Gift {
    Gift {
        flow,
        area,
        giver,
        item,
        how: How::Cut { then: None },
        setting: Some(SONGS),
    }
}

/// A boss's reward, granted in the scene after its blue warp. `found` is
/// that scene's played flag.
const fn reward(
    flow: &'static Name,
    area: &'static Name,
    giver: &'static str,
    item: &'static Name,
    found: &'static Name,
) -> Gift {
    Gift {
        flow,
        area,
        giver,
        item,
        how: How::Calls {
            call: &[POUCH_GRANT],
            found: Some(found),
        },
        setting: Some(DUNGEON_REWARDS),
    }
}

pub const GIFTS: [Gift; 73] = [
    Gift {
        flow: scene::KOKIRI_0155,
        area: flag::KOKIRI_FOREST,
        giver: "Saria",
        item: item::OCARINA_FAIRY,
        how: How::InPlace,
        setting: None,
    },
    Gift {
        flow: scene::MELODY_GET_TIME,
        area: flag::HYRULE_FIELD,
        giver: "Zelda",
        item: item::OCARINA_TIME,
        how: How::Cut {
            then: Some(item::MELODY_TIME_SONG),
        },
        setting: None,
    },
    npc(
        actor::NPC_SKULL_KID_CHILD,
        flag::LOST_WOOD,
        "Skull Kid",
        HEART,
        &[GET_QUIETLY],
        flag::NPC_SKULL_KID_CHILD_IS_GET_HEART_PIECE,
    ),
    npc(
        actor::NPC_SKULL_KID_CHILD_02,
        flag::LOST_WOOD,
        "Skull Kid's Memory Game",
        HEART,
        &[GET_QUIETLY],
        flag::MINIGAME_SKULL_KID_SESSION_IS_GET_HEART_PIECE,
    ),
    // The target in the Lost Woods and the Deku Theater hand over a bag by
    // the one Link holds, so a moved gift goes straight to its Medium call.
    npc(
        scene::LOST_WOOD_0040,
        flag::LOST_WOOD,
        "Lost Woods target",
        item::BULLET_BAG_MEDIUM,
        &[get(flow::GET_IN_EVENT_NOT_LOOKAT_OBJECT_TURN_OFF)],
        flag::LOST_WOOD_HIT_TARGET_OF_DEKU_SCRUB_FIRST_CLEAR,
    ),
    npc(
        flow::LOST_WOOD_SCRUB_FOREST_STAGE_ELDER_SKULL_MASK_FIRST,
        flag::LOST_WOOD,
        "Deku Theater: Skull Mask",
        item::STICK_BAG_MEDIUM,
        &[get(flow::GET_IN_EVENT_NOT_TURN)],
        flag::LOST_WOOD_SCRUB_FOREST_STAGE_SKULL_MASK_EVENT_LAUNCHED,
    ),
    npc(
        flow::LOST_WOOD_SCRUB_FOREST_STAGE_ELDER_TRUE_MASK_FIRST,
        flag::LOST_WOOD,
        "Deku Theater: Mask of Truth",
        item::NUT_BAG_MEDIUM,
        &[get(flow::GET_IN_EVENT_NOT_TURN)],
        flag::LOST_WOOD_SCRUB_FOREST_STAGE_TRUE_MASK_EVENT_LAUNCHED,
    ),
    npc(
        flow::MINI_GAME_FROG_TENORS,
        flag::ZORA_RIVER,
        "Frogs' Songs",
        HEART,
        &[GET_QUIETLY],
        flag::MINI_GAME_FROG_TENORS_SONG_CLEAR,
    ),
    npc(
        flow::MINI_GAME_FROG_TENORS,
        flag::ZORA_RIVER,
        "Frogs' Ocarina Game",
        HEART,
        &[local(flow::MUSHI_TORI_GAME)],
        flag::MINI_GAME_FROG_TENORS_INSECT_REPELLENT,
    ),
    npc(
        flow::SUB_EVENT_FIND_RICHARD,
        flag::MARKET,
        "Richard's Owner",
        HEART,
        &[HAND_OVER],
        flag::SUB_EVENT_FIND_RICHARD_GET_HEART_PIECE,
    ),
    Gift {
        flow: actor::NPC_HYLIA_048_BOTTLE_SHOP,
        area: flag::MARKET,
        giver: "Poe collector",
        item: item::BOTTLE,
        how: How::Calls {
            call: &[get(flow::GET_IN_EVENT_POESHOP)],
            found: None,
        },
        setting: Some(NPC_GIFTS),
    },
    // The Gold Rupee is paid by its own call, so each gift renames only the
    // call naming its item. Every sale adds 100 points and the game places
    // exactly 10 Big Poes, each with its own cleared flag, so 1000 is reached
    // once and this call runs once.
    Gift {
        flow: actor::NPC_HYLIA_048_BOTTLE_SHOP,
        area: flag::MARKET,
        giver: "Poe collector: 1000 points",
        item: actor::OBJ_RUPEE_GOLD,
        how: How::Calls {
            call: &[get(flow::GET_IN_EVENT_POESHOP)],
            found: None,
        },
        setting: Some(NPC_GIFTS),
    },
    npc(
        potion::FLOW,
        flag::MARKET,
        "Potion shop bottle",
        item::BOTTLE,
        potion::CALL,
        potion::CLEARED,
    ),
    npc(
        actor::NPC_HYLIA_001_ADULT,
        flag::KAKARIKO_VILLAGE,
        "Man on the Roof",
        HEART,
        &[GET],
        flag::NPC_HYLIA_001_IS_GET_HEART_PIECE,
    ),
    npc(
        actor::NPC_HYLIA_045_CHILD,
        flag::HYRULE_FIELD,
        "Lake Scientist",
        HEART,
        &[HAND_OVER],
        flag::NPC_HYLIA_045_HAS_DIVED,
    ),
    npc(
        flow::MINIGAME_CATCH_CUCCO,
        flag::KAKARIKO_VILLAGE,
        "Anju",
        item::BOTTLE,
        &[HAND_OVER],
        flag::MINI_GAME_CATCH_CUCCO_IS_COMPLETED,
    ),
    // Talon's game asks for an empty bottle to fill with the milk, so a moved
    // gift first opens the game to Link without one.
    npc(
        cucco::FLOW,
        flag::HYRULE_FIELD,
        "Talon's cuccos",
        cucco::MILK,
        cucco::CALL,
        flag::IS_CLEAR_MINIGAME_FIND_OUT_CUCCO,
    ),
    npc(
        scene::FAIRY_FOUNTAIN_POWER,
        flag::HYRULE_CASTLE,
        "Great Fairy of Power",
        item::FAIRY_BLESS_MP,
        &[FAIRY],
        flag::HAVE_PLAYED_EVENT_DM_FAIRY_FOUNTAIN_POWER,
    ),
    npc(
        scene::FAIRY_FOUNTAIN_DIN,
        flag::DEATH_MOUNTAIN_TRAIL,
        "Great Fairy of Din",
        item::DIN_FIRE,
        &[get(flow::GET_IN_EVENT_GREAT_FAIRY_DIN)],
        flag::HAVE_PLAYED_EVENT_DM_FAIRY_FOUNTAIN_DIN,
    ),
    npc(
        scene::FAIRY_FOUNTAIN_FRORE,
        flag::ZORA_FOUNTAIN,
        "Great Fairy of Farore",
        item::FARORE_WIND,
        &[FAIRY],
        flag::HAVE_PLAYED_EVENT_DM_FAIRY_FOUNTAIN_FRORE,
    ),
    npc(
        scene::FAIRY_FOUNTAIN_WISDOM,
        flag::DEATH_MOUNTAIN_CRATER,
        "Great Fairy of Wisdom",
        item::FAIRY_BLESS_MP_DOUBLE,
        &[get(flow::GET_IN_EVENT_GREAT_FAIRY_WISDOM)],
        flag::HAVE_PLAYED_EVENT_DM_FAIRY_FOUNTAIN_WISDOM,
    ),
    npc(
        scene::FAIRY_FOUNTAIN_NAYRU,
        flag::DESERT_COLOSSUS,
        "Great Fairy of Nayru",
        item::NAYRU_LOVE,
        &[FAIRY],
        flag::HAVE_PLAYED_EVENT_DM_FAIRY_FOUNTAIN_NAYRU,
    ),
    npc(
        scene::FAIRY_FOUNTAIN_COURAGE,
        flag::GANON_CASTLE,
        "Great Fairy of Courage",
        item::FAIRY_BLESS_DEFENSE_DOUBLE,
        &[get(flow::GET_IN_EVENT_GREAT_FAIRY_COURGE)],
        flag::HAVE_PLAYED_EVENT_DM_FAIRY_FOUNTAIN_COURAGE,
    ),
    npc(
        scene::GORON_0030,
        flag::GORON_CITY,
        "Darunia",
        item::GAUNTLET_GORON,
        &[get(flow::GET_IN_EVENT_GORON)],
        flag::HAVE_PLAYED_EVENT_DM_GORON_0030,
    ),
    // Link the Goron, stopped as an adult. His first talk after that is
    // the only way to the grant, and it sets his quest step's Bool.
    npc(
        flow::SAGE_OF_FIRE,
        flag::GORON_CITY,
        "Link the Goron",
        actor::OBJ_ARMOR_GORON,
        &[get(flow::GET_IN_EVENT_NOT_TURN)],
        flag::SAGE_OF_FIRE_STEP02_DARUNIA_JR_FIRST_TALKED,
    ),
    // King Zora, thawed. The grant is in the melt, which happens once.
    Gift {
        flow: flow::SAGE_OF_WATER,
        area: flag::ZORA_DOMAIN,
        giver: "King Zora",
        item: actor::OBJ_ARMOR_ZORA,
        how: How::Calls {
            call: &[get(flow::GET_IN_EVENT_NOT_TURN_POS_CAMERA)],
            found: None,
        },
        setting: Some(NPC_GIFTS),
    },
    // The rolling Goron as a child. He picks the bag by the one Link
    // holds, so a moved gift first goes straight to its call.
    npc(
        rolling::FLOW,
        flag::GORON_CITY,
        "Rolling Goron",
        rolling::BAG,
        rolling::CALL,
        flag::GORON_CITY_GET_BOMB_BAG_FROM_GORON006,
    ),
    reward(
        scene::KOKIRI_0130,
        flag::INSIDE_THE_DEKU_TREE,
        "Queen Gohma",
        item::STONE_FOREST,
        SCENE_PLAYED,
    ),
    reward(
        scene::DEATH_MT_TRAIL_0025,
        flag::DODONGO_CAVERN,
        "King Dodongo",
        item::STONE_FIRE,
        SCENE_PLAYED,
    ),
    reward(
        scene::ZORA_FOUNTAIN_0030,
        flag::INSIDE_JABU_JABU_BELLY,
        "Barinade",
        item::STONE_WATER,
        SCENE_PLAYED,
    ),
    reward(
        scene::AWAKE_FOREST_GET,
        flag::FOREST_TEMPLE,
        "Phantom Ganon",
        item::MEDAL_FOREST,
        SCENE_PLAYED,
    ),
    reward(
        scene::AWAKE_FIRE_GET,
        flag::FIRE_TEMPLE,
        "Volvagia",
        item::MEDAL_FIRE,
        SCENE_PLAYED,
    ),
    reward(
        scene::AWAKE_WATER_GET,
        flag::WATER_TEMPLE,
        "Morpha",
        item::MEDAL_WATER,
        SCENE_PLAYED,
    ),
    reward(
        scene::AWAKE_SHADOW_GET,
        flag::SHADOW_TEMPLE,
        "Bongo Bongo",
        item::MEDAL_SHADOW,
        SCENE_PLAYED,
    ),
    reward(
        scene::AWAKE_SPIRIT_GET,
        flag::SPIRIT_TEMPLE,
        "Twinrova",
        item::MEDAL_SPIRIT,
        SCENE_PLAYED,
    ),
    reward(
        RAURU,
        flag::MARKET,
        "Rauru",
        item::MEDAL_LIGHT,
        SCENE_PLAYED,
    ),
    npc(
        flow::GATHERING_TOKEN_GOLD,
        flag::KAKARIKO_VILLAGE,
        "House of Skulltula: 10",
        item::WALLET_MEDIUM,
        &[local(flow::TAKEN_REWARD_COMMON_EVENT)],
        flag::KAKARIKO_VILLAGE_GATHERING_TOKEN_GOLD_GET_REWARD10,
    ),
    npc(
        flow::GATHERING_TOKEN_GOLD,
        flag::KAKARIKO_VILLAGE,
        "House of Skulltula: 20",
        item::AGONY_STONE,
        &[local(flow::TAKEN_REWARD_COMMON_EVENT)],
        flag::KAKARIKO_VILLAGE_GATHERING_TOKEN_GOLD_GET_REWARD20,
    ),
    npc(
        flow::GATHERING_TOKEN_GOLD,
        flag::KAKARIKO_VILLAGE,
        "House of Skulltula: 30",
        item::WALLET_LARGE,
        &[local(flow::TAKEN_REWARD_COMMON_EVENT)],
        flag::KAKARIKO_VILLAGE_GATHERING_TOKEN_GOLD_GET_REWARD30,
    ),
    npc(
        flow::GATHERING_TOKEN_GOLD,
        flag::KAKARIKO_VILLAGE,
        "House of Skulltula: 40",
        item::BOMBCHU_BUNDLE_LARGE,
        &[local(flow::TAKEN_REWARD_COMMON_EVENT)],
        flag::KAKARIKO_VILLAGE_GATHERING_TOKEN_GOLD_GET_REWARD40,
    ),
    npc(
        flow::GATHERING_TOKEN_GOLD,
        flag::KAKARIKO_VILLAGE,
        "House of Skulltula: 50",
        HEART,
        &[local(flow::TAKEN_REWARD_COMMON_EVENT)],
        flag::KAKARIKO_VILLAGE_GATHERING_TOKEN_GOLD_GET_REWARD50,
    ),
    npc(
        flow::GATHERING_TOKEN_GOLD,
        flag::KAKARIKO_VILLAGE,
        "House of Skulltula: 100",
        actor::OBJ_RUPEE_GOLD,
        &[local(flow::TAKEN_REWARD_COMMON_EVENT_200_RUPEE)],
        flag::GATHERING_TOKEN_GOLD_FATHER_BACKTO_HUMAN,
    ),
    // The letter is shown in the last cue of her courtyard scene, which a
    // swapped get sub-flow never finishes inside, so the grant moves out to
    // the flow that starts the scene.
    Gift {
        flow: scene::HRL_CASTLE_0050,
        area: flag::HYRULE_CASTLE,
        giver: "Zelda's letter",
        item: item::ZELDA_LETTER,
        how: How::Later {
            call: &[get(flow::GET_IN_EVENT_ZELDALETTER)],
            caller: flow::GOTO_HYRULE_CASTLE,
            found: SCENE_PLAYED,
        },
        setting: Some(NPC_GIFTS),
    },
    Gift {
        flow: flow::GOTO_HYRULE_CASTLE,
        area: flag::HYRULE_CASTLE,
        giver: "Weird Egg",
        item: item::WEIRD_EGG,
        how: How::Calls {
            call: &[get(flow::GET_IN_EVENT_NOT_TURN)],
            found: None,
        },
        setting: Some(NPC_GIFTS),
    },
    npc(
        scene::TIME_TEMPLE_0130,
        flag::MARKET,
        "Light Arrows",
        item::ARROW_LIGHT,
        &[POUCH_GRANT],
        SCENE_PLAYED,
    ),
    npc(
        scene::GERUDO_0025,
        flag::GERUDO_FORTRESS,
        "Gerudo membership card",
        item::TOKEN_GERUDO,
        &[get(flow::GET_IN_EVENT_NOT_LOOKAT_OBJECT_TURN_OFF)],
        SCENE_PLAYED,
    ),
    // The dug-up piece is only for show: the flow deletes it and grants
    // by this call.
    npc(
        actor::NPC_HYLIA_DAMPE_001,
        flag::GRAVEYARD,
        "Dampe's dig",
        HEART,
        &[get(flow::GET_IN_EVENT_NOT_TURN_NOT_NPC_DAMPE)],
        flag::GRAVE_YARD_DIGGING_HOLE_TOUR_DROP_HEART_PIECE,
    ),
    npc(
        flow::MINI_GAME_FISHING,
        flag::HYRULE_FIELD,
        "Fishing as a child",
        HEART,
        &FISHING,
        flag::MINI_GAME_FISHING_GET_HEART_PIECE,
    ),
    npc(
        flow::MINI_GAME_FISHING,
        flag::HYRULE_FIELD,
        "Fishing as an adult",
        item::SCALE_GOLD,
        &FISHING,
        flag::MINI_GAME_FISHING_GET_GOLD_SCALE,
    ),
    npc(
        flow::MINIGAME_HORSEBACK_ARCHERY,
        flag::GERUDO_FORTRESS,
        "Horseback archery",
        HEART,
        &[get(flow::GET_IN_EVENT_NOT_TURN_POS_CAMERA)],
        flag::MINI_GAME_HORSEBACK_ARCHERY_GOTA_HEART,
    ),
    npc(
        flow::MINIGAME_HORSEBACK_ARCHERY,
        flag::GERUDO_FORTRESS,
        "Horseback archery: quiver",
        item::QUIVER_MEDIUM,
        &[get(flow::GET_IN_EVENT_NOT_TURN_POS_CAMERA)],
        flag::MINI_GAME_HORSE_BACK_ARCHERY_GET_QUIVER,
    ),
    npc(
        flow::MINI_GAME_DIVING,
        flag::ZORA_DOMAIN,
        "Diving game",
        item::SCALE_SILVER,
        &[GET_QUIETLY],
        flag::MINI_GAME_DIVING_CLEARED,
    ),
    npc(
        flow::MINI_GAME_BOMBCHU_BOWLING,
        flag::MARKET,
        "Bombchu bowling: first prize",
        item::BOMBCHU_BUNDLE_MEDIUM,
        &[HAND_OVER],
        flag::MINI_GAME_BOMBCHU_BOWLING_CLEAR_FIRST_TIME,
    ),
    npc(
        flow::MINI_GAME_BOMBCHU_BOWLING,
        flag::MARKET,
        "Bombchu bowling: second prize",
        item::BOMB_BAG_MEDIUM,
        &[HAND_OVER],
        flag::MINI_GAME_BOMBCHU_BOWLING_CLEAR_SECOND_TIME,
    ),
    // The trade chain stays as it is. Only the sword at its end moves.
    Gift {
        flow: BIGGORON,
        area: flag::DEATH_MOUNTAIN_TRAIL,
        giver: "Biggoron",
        item: item::SWORD_BIGGORON,
        how: How::Calls {
            call: &[get(flow::GET_IN_EVENT_ITEM_SWORD_BIGGORON)],
            found: None,
        },
        setting: Some(NPC_GIFTS),
    },
    Gift {
        flow: MEDIGORON,
        area: flag::GORON_CITY,
        giver: "Medigoron",
        item: item::SWORD_GIANT,
        how: How::Sale {
            call: &[MEDIGORON_SALE],
            sold: actor::SHOP_SWORD_GIANT,
            price: 200,
            found: None,
            gate: Some(ask_medigorons_bool),
            shop: set_medigorons_bool,
        },
        setting: Some(MERCHANTS),
    },
    Gift {
        flow: BEAN_SELLER,
        area: flag::ZORA_RIVER,
        giver: "Bean seller",
        item: item::MAGIC_BEAN,
        how: How::Sale {
            call: &[call(flow::SIMPLE_SHOP, flow::SIMPLE_SHOP_ROOT_3_CHOICES)],
            sold: item::MAGIC_BEAN,
            price: 10,
            found: Some(flag::GET_MAGIC_BEAN_AFTER),
            gate: None,
            shop: hand_over_the_sold,
        },
        setting: Some(MERCHANTS),
    },
    Gift {
        flow: CARPET_MERCHANT,
        area: flag::HOUNTED_WASTELAND,
        giver: "Carpet merchant",
        item: item::BOMBCHU_BUNDLE_MEDIUM,
        how: How::Sale {
            call: &[CARPET_SALE],
            sold: CARPET_BOMBCHUS,
            price: 90,
            found: Some(CARPET_SOLD),
            gate: Some(sell_bombchus_after),
            shop: buys_while_unsold,
        },
        setting: Some(MERCHANTS),
    },
    npc(
        flow::MINI_GAME_BOMBCHU_BOWLING,
        flag::MARKET,
        "Bombchu bowling: third prize",
        HEART,
        &[HAND_OVER],
        flag::MINI_GAME_BOMBCHU_BOWLING_CLEAR_THIRD_TIME,
    ),
    npc(
        flow::MINI_GAME_HIT_TARGET,
        flag::MARKET,
        "Market shooting gallery",
        item::BULLET_BAG_MEDIUM,
        &[HAND_OVER],
        flag::MINI_GAME_HIT_TARGET_HRL_CITY_GET_BULLET_BAG,
    ),
    npc(
        flow::MINI_GAME_HIT_TARGET,
        flag::KAKARIKO_VILLAGE,
        "Kakariko shooting gallery",
        item::QUIVER_MEDIUM,
        &[HAND_OVER],
        flag::MINI_GAME_HIT_TARGET_KAKARIKO_GET_QUIVER,
    ),
    song(
        scene::MELODY_GET_ZELDA,
        flag::HYRULE_CASTLE,
        "Impa",
        item::MELODY_ZELDA_LULLABY,
    ),
    song(
        scene::MELODY_GET_EPONA,
        flag::HYRULE_FIELD,
        "Malon",
        item::MELODY_EPONA_SONG,
    ),
    song(
        scene::MELODY_GET_SARIA,
        flow::SACRED_FOREST_MEADOW,
        "Saria in the Meadow",
        item::MELODY_SARIA_SONG,
    ),
    song(
        scene::MELODY_GET_STORM,
        flag::KAKARIKO_VILLAGE,
        "Windmill Man",
        item::MELODY_STORM_SONG,
    ),
    song(
        scene::MELODY_GET_FOREST,
        flow::SACRED_FOREST_MEADOW,
        "Sheik in the Meadow",
        item::MELODY_FOREST_MINUET,
    ),
    song(
        scene::MELODY_GET_FIRE,
        flag::DEATH_MOUNTAIN_CRATER,
        "Sheik in the Crater",
        item::MELODY_FIRE_BOLERO,
    ),
    song(
        scene::MELODY_GET_WATER,
        flag::ICE_CAVERN,
        "Sheik in the Ice Cavern",
        item::MELODY_WATER_SERENADE,
    ),
    song(
        scene::MELODY_GET_SPILIT,
        flag::DESERT_COLOSSUS,
        "Sheik at the Colossus",
        item::MELODY_SPIRIT_REQUIEM,
    ),
    song(
        scene::MELODY_GET_SHADOW,
        flag::KAKARIKO_VILLAGE,
        "Sheik in Kakariko",
        item::MELODY_SHADOW_NOCTURNE,
    ),
    song(
        scene::MELODY_GET_SHINE,
        flag::MARKET,
        "Sheik at the Temple of Time",
        item::MELODY_LIGHT_PRELUDE,
    ),
    // The tomb's lesson teaches it by melody type, then this scene grants
    // it again by name. `TAUGHT` empties the lesson.
    Gift {
        flow: scene::MELODY_GET_SUN_03,
        area: flag::GRAVEYARD,
        giver: "Royal Family's Tomb",
        item: item::MELODY_SUN_SONG,
        how: How::Calls {
            call: &[POUCH_GRANT],
            found: Some(flag::ROYAL_FAMILY_TOMB_MELODY_GET_CLEAR),
        },
        setting: Some(SONGS),
    },
    // Zelda's cut copies this grant after her gift, so renaming it there
    // makes the song its own check.
    Gift {
        flow: scene::MELODY_GET_TIME,
        area: flag::HYRULE_FIELD,
        giver: "Zelda's song",
        item: item::MELODY_TIME_SONG,
        how: How::Calls {
            call: &[POUCH_GRANT],
            found: Some(SCENE_PLAYED),
        },
        setting: Some(SONGS),
    },
];

/// Rauru's scene, which the open Door of Time skips.
const RAURU: &Name = scene::SAGE_ROOM_0005;

/// The flow chests and NPCs hand items out through.
use crate::grant::POUCH;
/// The "You got" window.
const WINDOW: &Name = flow::DISPLAY_GET_BY_ACTOR_NAME;
/// An NPC's reward: the grant and its window.
const REWARD: &Name = flow::GET_POUCH_CONTENT_BY_ACTOR_NAME;
pub(super) const NAME: &Name = flag::ACTOR_NAME;
/// What a Bool about an item's name starts with.
const IS_GET: &str = "IsGet.";
/// The song lesson, which needs an ocarina.
const LESSON: &Name = scene::SY_OCARINA;
const ENTRY: &Name = flag::ENTRY_POINT0;

impl Gift {
    /// A stable id that reads like a Banc hash, for plando and history.
    pub fn hash(&self) -> u64 {
        match self.how {
            How::Calls { call, .. } | How::Later { call, .. } | How::Sale { call, .. } => {
                crate::known::hash(&format!("{}{}{}", self.flow, said(call), self.item))
            }
            _ => crate::known::hash(self.flow),
        }
    }

    /// The GameData Bool set once it's given: a cutscene's played flag, or
    /// an NPC's own. Biggoron sets none.
    pub fn found(&self) -> Option<String> {
        let flag = |found: &Name| match found == SCENE_PLAYED {
            true => format!("{PLAYED}.{}", self.flow),
            false => found.to_string(),
        };
        match self.how {
            How::Calls { found, .. } | How::Sale { found, .. } => found.map(flag),
            How::Later { found, .. } => Some(flag(found)),
            _ => Some(flag(SCENE_PLAYED)),
        }
    }

    /// Whether a new game with `on` deals it: its setting is on, and it's
    /// not Rauru's with the Door of Time open.
    pub fn dealt(&self, on: &BTreeSet<String>) -> bool {
        self.setting().is_none_or(|s| on.contains(s))
            && !(self.flow == RAURU && on.contains(OPEN_DOOR))
    }

    /// The flow that starts its scene, when the grant can move there: a
    /// `Later` gift's caller, or the one a skip swaps a song's scene in.
    pub(crate) fn caller(&self) -> Option<&'static str> {
        match self.how {
            How::Later { caller, .. } => Some(caller.get()),
            _ => SKIPPED_IN
                .iter()
                .find(|s| s.0 == self.flow)
                .map(|s| s.1.get()),
        }
    }

    /// Whether a cutscene skip already swapped its scene's transition in
    /// `rewrites`, so the scene never plays and its own grant is lost.
    fn skipped(&self, rewrites: &Rewrites) -> Result<bool> {
        let Some(caller) = self.caller() else {
            return Ok(false);
        };
        let Some(data) = rewrites.get(&flows::rel(caller)) else {
            return Ok(false);
        };
        let flow = Flowchart::parse(data)?.with_context(|| format!("{caller} is a timeline"))?;
        Ok(scene_marks(&flow, self.flow).0.is_empty())
    }

    /// The items its scene grants: the gift's own, and the one a cut
    /// `then` hands over after it.
    pub(crate) fn items(&self) -> Vec<&'static str> {
        let then = match self.how {
            How::Cut { then } => then,
            _ => None,
        };
        std::iter::once(self.item)
            .chain(then)
            .map(|n| n.get())
            .collect()
    }

    /// The setting that deals it, if only one does.
    pub fn setting(&self) -> Option<&'static str> {
        self.setting
    }

    /// Whether it's a sale, which keeps selling what it sold.
    pub fn is_sale(&self) -> bool {
        matches!(self.how, How::Sale { .. })
    }
}

/// The gifts a scene hands over.
pub(crate) fn of_scene(scene: &str) -> Vec<&'static Gift> {
    GIFTS.iter().filter(|g| g.flow == scene).collect()
}

/// A check an event hands out, rather than a placed actor.
pub(crate) fn check(file: String, area: &str, hash: u64, holder: String, item: &str) -> ItemCheck {
    ItemCheck {
        file,
        area: area.into(),
        hash,
        holder,
        item: item.to_string(),
        get_event: None,
        substitute: None,
        presence: None,
    }
}

/// The item the seed puts at `hash`, else `vanilla`.
pub(crate) fn item_at<'a>(drops: &'a HashMap<u64, Drop>, hash: u64, vanilla: &'a str) -> &'a str {
    drops.get(&hash).map_or(vanilla, |d| d.item.as_str())
}

/// One check per gift.
pub fn checks() -> Vec<ItemCheck> {
    GIFTS
        .iter()
        .map(|g| check(flows::rel(g.flow), g.area, g.hash(), g.giver.into(), g.item))
        .collect()
}

pub fn of(check: &ItemCheck) -> Option<&'static Gift> {
    GIFTS.iter().find(|g| g.hash() == check.hash)
}

/// The minigame chests that are checks, by placement hash: the Treasure
/// Chest Game's heart piece, then Dampe's two. Every other chest whose event
/// starts `MiniGame_` stays vanilla. Gifts in all but where they're placed,
/// since a chest of the game can't turn into a substitute one.
pub(crate) const PRIZE_CHESTS: [u64; 3] = [
    16881481538337649070,
    dampe::HOOKSHOT_CHEST,
    dampe::HEART_CHEST,
];

pub(crate) fn is_prize(check: &ItemCheck) -> bool {
    PRIZE_CHESTS.contains(&check.hash)
}

/// Saria's and Zelda's, every business scrub's, shop slot's, cow's and
/// minigame prize chest's.
pub fn is_gift(check: &ItemCheck) -> bool {
    of(check).is_some()
        || is_prize(check)
        || scrubs::of(check).is_some()
        || shops::is_shop(check)
        || pickups::is_pickup(check)
        || cows::is_cow(check)
}

/// Whether `holder` is a gift's: a giver's name, a scrub's, a shop slot's,
/// a cow's or a thief's.
pub fn is_giver(holder: &str) -> bool {
    GIFTS.iter().any(|g| g.giver == holder)
        || holder.starts_with(scrubs::HOLDER)
        || shops::is_holder(holder)
        || cows::is_holder(holder)
        || holder == pickups::THIEF
}

/// Windows in another flow that show a gift's item before it's given, as
/// the gift's flow, then the window's flow and call. Ruto shows the Zora
/// Sapphire in Jabu-Jabu, and the fountain scene grants it.
const FORETOLD: [(&Name, &Name, &[Call]); 1] = [(
    scene::ZORA_FOUNTAIN_0030,
    scene::INSIDE_JABU_JABU_0030,
    &[call(
        flow::GET_POUCH_CONTENT,
        flow::DISPLAY_RUTO_GET_BY_ACTOR_NAME,
    )],
)];

/// Lessons in another flow that teach a gift's song again, as the gift's
/// flow, then the lesson's. The lesson flow is emptied when the item changes.
const TAUGHT: [(&Name, &Name); 1] = [(scene::MELODY_GET_SUN_03, scene::MELODY_GET_SUN)];

/// Answer every `ELAPSED` query in `flow` as over, so nothing waits.
fn waited(flow: &mut Flowchart) -> Result<()> {
    let over = |flow: &Flowchart, i: u16| match &flow.events[i as usize].kind {
        EventKind::Switch {
            actor,
            query,
            cases,
            ..
        } if flow.actor_of(*actor).queries[*query as usize].contains(&**ELAPSED) => {
            cases.iter().find(|c| c.0 == 0).map(|c| c.1)
        }
        _ => None,
    };
    let waits: Vec<u16> = (0..flow.events.len() as u16)
        .filter(|&i| over(flow, i).is_some())
        .collect();
    ensure!(!waits.is_empty(), "{} never waits", flow.name);
    for &wait in &waits {
        let mut to = over(flow, wait);
        while let Some(next) = to.and_then(|t| over(flow, t)) {
            to = Some(next);
        }
        for link in flow.links_to(wait) {
            flow.link(link, to)?;
        }
    }
    Ok(())
}

/// The switches the fountain melody runs that read Din's Fire.
fn din_gates(flow: &Flowchart) -> Result<Vec<u16>> {
    Ok(flows::reachable(flow, FOUNTAIN_ENTRY)?
        .into_iter()
        .filter(|&e| {
            matches!(&flow.events[e as usize].kind,
                EventKind::Switch { params, .. } if param(params, GAME_DATA) == Some(DIN_FIRE))
        })
        .collect())
}

/// Every fountain opens for its melody, whether Link holds Din's Fire or
/// not. The game sends the other five to a Navi line until he does.
fn ungated(flow: &mut Flowchart) -> Result<()> {
    let &[gate] = &din_gates(flow)?[..] else {
        bail!("{} gates on Din's Fire not exactly once", flow.name);
    };
    let EventKind::Switch { cases, .. } = &flow.events[gate as usize].kind else {
        unreachable!("din_gates found a switch");
    };
    let open = cases.iter().find(|c| c.0 == 0).map(|c| c.1);
    let open = open.context("the Din's Fire gate has no case 0")?;
    for link in flow.links_to(gate) {
        flow.link(link, Some(open))?;
    }
    Ok(())
}

/// Every entry point of `flow` ends at once.
fn empty(flow: &mut Flowchart) -> Result<()> {
    (0..flow.entry_points.len()).try_for_each(|i| flow.link(Link::Entry(i), None))
}

/// The Bools the gifts' edits set, which a new game adds to its save data.
pub fn flags(on: &BTreeSet<String>) -> Vec<String> {
    match on.contains(MERCHANTS) {
        true => vec![MEDIGORON_SOLD.into()],
        false => vec![],
    }
}

/// Flows a dealt gift would leave stuck, each with the edit that frees it.
const UNSTUCK: [(&Name, Edit); 2] = [
    (cucco::FLOW, cucco::open),
    (potion::FLOW, potion::cleared_with_the_gift),
];

/// Write each gift whose item changed, and Zelda's always, so her lesson
/// never plays without an ocarina. Biggoron never makes Link wait, sword
/// moved or not. Every Great Fairy opens in any order.
pub fn give_all(
    romfs: &Romfs,
    flows: &Flows,
    rewrites: &mut Rewrites,
    drops: &HashMap<u64, Drop>,
) -> Result<()> {
    for gift in &GIFTS {
        let item = item_at(drops, gift.hash(), gift.item);
        if let Some(line) = picked(gift.item).filter(|_| item != gift.item) {
            flows::edit(romfs, rewrites, gift.flow, |f| one_of(f, line, gift.item))?;
        }
        for (_, free) in UNSTUCK
            .iter()
            .filter(|u| u.0 == gift.flow && item != gift.item)
        {
            flows::edit(romfs, rewrites, gift.flow, free)?;
        }
        if item != gift.item || matches!(gift.how, How::Cut { .. }) || gift.skipped(rewrites)? {
            give(romfs, flows, rewrites, gift, item)
                .with_context(|| format!("{} giving {item}", gift.giver))?;
        }
        if item == gift.item {
            continue;
        }
        for (_, flow, call) in FORETOLD.iter().filter(|f| f.0 == gift.flow) {
            flows::edit(romfs, rewrites, flow, |f| {
                rename_calls(f, call, gift.item, item)
            })?;
        }
        for (_, lesson) in TAUGHT.iter().filter(|t| t.0 == gift.flow) {
            flows::edit(romfs, rewrites, lesson, empty)?;
        }
    }
    lines::retalk(romfs, rewrites, drops)?;
    flows::edit(romfs, rewrites, BIGGORON, waited)?;
    flows::edit(romfs, rewrites, FOUNTAIN, ungated)
}

fn give(
    romfs: &Romfs,
    flows: &Flows,
    rewrites: &mut Rewrites,
    gift: &Gift,
    item: &str,
) -> Result<()> {
    let mut back = None;
    let lost = gift.skipped(rewrites)?;
    // A scene that never plays has nothing to cut or silence.
    if !lost {
        flows::edit(romfs, rewrites, gift.flow, |flow| match gift.how {
            How::InPlace => in_place(flow, item),
            How::Cut { then } => cut(flow, flows, gift.item, item, then).map(|b| back = b),
            How::Calls { call, .. } => rename_calls(flow, call, gift.item, item),
            How::Later { call, .. } => silence_calls(flow, call, gift.item),
            How::Sale {
                call,
                sold,
                price,
                gate,
                ..
            } => {
                sell(flow, call, sold, price, item).and_then(|()| gate.map_or(Ok(()), |g| g(flow)))
            }
        })?;
        if let How::Sale { shop, .. } = gift.how {
            flows::edit(romfs, rewrites, SIMPLE_SHOP, shop)?;
        }
    }
    if let Some(caller) = gift
        .caller()
        .filter(|_| lost || matches!(gift.how, How::Later { .. }))
    {
        flows::edit(romfs, rewrites, caller, |f| {
            grant_after(f, flows, gift.flow, item)
        })?;
    }
    match back {
        Some((callee, entry)) => flows::edit(romfs, rewrites, &callee, |f| stay(f, &entry)),
        None => Ok(()),
    }
}

/// Moves Link to where a cutscene left him.
const WARP: (&Name, &str) = (actor::PLAYER_EVENT, "EventPlayerWarpTo");

/// Start `entry` past a warp it opens with, since a cut cutscene leaves
/// Link where he was. Zelda's would put him back on the drawbridge.
fn stay(flow: &mut Flowchart, entry: &str) -> Result<()> {
    let i = flow.entry_points.iter().position(|e| e.name == entry);
    let i = i.with_context(|| format!("no entry point {entry}"))?;
    let Some(first) = flow.entry_points[i].main else {
        return Ok(());
    };
    let EventKind::Action {
        actor,
        action,
        next,
        ..
    } = flow.events[first as usize].kind
    else {
        return Ok(());
    };
    let a = flow.actor_of(actor);
    match a.name == WARP.0 && a.actions[action as usize].starts_with(WARP.1) {
        true => flow.link(Link::Entry(i), next),
        false => Ok(()),
    }
}

/// The pouch call an event makes, and the item it names.
fn pouch_call(e: &EventKind) -> Option<(&str, Option<&str>)> {
    match e {
        EventKind::SubFlow {
            flowchart,
            entry_point,
            params,
            ..
        } if flowchart == POUCH => Some((entry_point, param(params, NAME))),
        _ => None,
    }
}

/// `flow`'s one call to `GetPouchContent::<entry>`, naming `name` if given.
fn only_call(flow: &Flowchart, entry: &str, name: Option<&str>) -> Result<u16> {
    let calls: Vec<u16> = (0u16..)
        .zip(&flow.events)
        .filter(|(_, e)| {
            pouch_call(&e.kind)
                .is_some_and(|(at, n)| at == entry && name.is_none_or(|w| n == Some(w)))
        })
        .map(|(i, _)| i)
        .collect();
    match calls[..] {
        [at] => Ok(at),
        _ => bail!(
            "{} calls {POUCH}::{entry} {} times, not once",
            flow.name,
            calls.len()
        ),
    }
}

fn in_place(flow: &mut Flowchart, item: &str) -> Result<()> {
    let grant = only(flow, GRANT)?;
    flows::set(
        params_mut(&mut flow.events[grant as usize].kind),
        ACTOR,
        string(item),
    )?;
    let window = only_call(flow, WINDOW, None)?;
    flows::set(
        params_mut(&mut flow.events[window as usize].kind),
        NAME,
        string(item),
    )
}

/// The params a getting sub-flow takes its item in.
const ITEM_PARAMS: [&Name; 3] = [NAME, flow::REVEIVE_ITEM, flow::TAKEN_REWARD_ACTOR];

/// The getting sub-flow that only shows the item.
const PLAIN: &Name = flow::GET_IN_EVENT_DEMO_NOT_LOOKAT_OBJECT_ENTRY;
/// Getting sub-flows dressed for one item, with a magic meter animation or
/// the bracelet tutorial around the same grant. A swapped item gets `PLAIN`.
const DRESSED: [&Name; 8] = [
    flow::GET_IN_EVENT_GREAT_FAIRY_DIN,
    flow::GET_IN_EVENT_GREAT_FAIRY_WISDOM,
    flow::GET_IN_EVENT_GREAT_FAIRY_COURGE,
    flow::GET_IN_EVENT_GORON,
    flow::GET_IN_EVENT_ITEM_SWORD_BIGGORON,
    flow::GET_IN_EVENT_POESHOP,
    flow::GET_IN_EVENT_NOT_TURN_FIXED_CAMERA_WARP_LINK,
    flow::GET_IN_EVENT_ITEM_SWORD_MEDIGORON,
];

/// The events that make any of `calls`.
fn calls_to(flow: &Flowchart, calls: &[Call]) -> Result<Vec<usize>> {
    Ok((0..flow.events.len())
        .filter(|&i| match &flow.events[i].kind {
            EventKind::SubFlow {
                flowchart,
                entry_point,
                ..
            } => calls.iter().any(|&Call(chart, entry)| {
                entry_point == entry
                    && match chart {
                        None => is_local(flow, flowchart),
                        Some(chart) => flowchart == chart,
                    }
            }),
            _ => false,
        })
        .collect())
}

/// Rename `from` to `to` in every call to `call` naming it, at least one.
/// A dressed call becomes `PLAIN`.
fn rename_calls(flow: &mut Flowchart, call: &[Call], from: &str, to: &str) -> Result<()> {
    let calls = calls_to(flow, call)?;
    let mut renamed = 0;
    for i in calls {
        let kind = &mut flow.events[i].kind;
        let params = params_mut(kind);
        let before = renamed;
        for key in ITEM_PARAMS {
            if param(params, key) == Some(from) {
                flows::set(params, key, string(to))?;
                renamed += 1;
            }
        }
        if let EventKind::SubFlow { entry_point, .. } = kind
            && renamed > before
            && DRESSED.iter().any(|d| **d == **entry_point)
        {
            *entry_point = PLAIN.to_string();
        }
    }
    ensure!(
        renamed > 0,
        "{} never calls {} with {from}",
        flow.name,
        said(call)
    );
    Ok(())
}

/// The bag and quiver lines, each as the start of its levels' names, whose
/// Medium level a gift gives when the game picks it by what Link holds.
const PICKED: [&str; 5] = [
    "Item_BombBag_",
    "Item_Quiver_",
    "Item_BulletBag_",
    "Item_StickBag_",
    "Item_NutBag_",
];

/// The line of `item` when it's the Medium level of a picked one.
fn picked(item: &str) -> Option<&'static str> {
    PICKED
        .into_iter()
        .find(|line| item.strip_prefix(line) == Some("Medium"))
}

/// The event a link leaves from, none for an entry point.
fn leaves(link: &Link) -> Option<u16> {
    match *link {
        Link::Entry(_) => None,
        Link::Next(e) | Link::Branch(e, _) | Link::Join(e) => Some(e),
    }
}

/// The event of `flow` giving `keep`, the one sub-flow call naming it.
fn giving(flow: &Flowchart, keep: &str) -> Result<u16> {
    let calls: Vec<u16> = (0u16..)
        .zip(&flow.events)
        .filter(|(_, e)| match &e.kind {
            EventKind::SubFlow { params, .. } => {
                ITEM_PARAMS.iter().any(|k| param(params, k) == Some(keep))
            }
            _ => false,
        })
        .map(|(i, _)| i)
        .collect();
    match calls[..] {
        [call] => Ok(call),
        _ => bail!("{} gives {keep} {} times, not once", flow.name, calls.len()),
    }
}

/// The events of `flow` asking whether Link holds a level of the `ask`
/// line, in his pouch or by its Bool, and leading to `call`.
fn asked(flow: &Flowchart, ask: &str, call: u16) -> Vec<u16> {
    (0u16..)
        .zip(&flow.events)
        .filter(|(i, e)| match &e.kind {
            EventKind::Switch { params, .. } => {
                ["Actor", field::GAME_DATA_NAME]
                    .iter()
                    .filter_map(|k| param(params, k))
                    .any(|n| n.strip_prefix(IS_GET).unwrap_or(n).starts_with(ask))
                    && flows::reachable_from(flow, Some(*i)).contains(&call)
            }
            _ => false,
        })
        .map(|(i, _)| i)
        .collect()
}

/// The game gives the Medium or the Large level of a bag by what Link
/// holds, but a gift has one call. Send whatever leads to the first
/// question about the `ask` line before the call giving `keep` straight to
/// that call, or to the event that runs it, so it's reached whatever Link
/// holds.
fn one_of(flow: &mut Flowchart, ask: &str, keep: &str) -> Result<()> {
    let call = giving(flow, keep)?;
    let asks = asked(flow, ask, call);
    // The first question is the one no other question leads to.
    let first = asks.iter().copied().find(|&q| {
        flow.links_to(q)
            .iter()
            .all(|l| !leaves(l).is_some_and(|e| asks.contains(&e)))
    });
    let first = first.with_context(|| format!("{} never asks for {ask}", flow.name))?;
    let runs = (0u16..).zip(&flow.events).find_map(|(i, e)| match &e.kind {
        EventKind::Action { next: Some(n), .. } if *n == call => Some(i),
        EventKind::Fork { branches, .. } if branches.contains(&call) => Some(i),
        _ => None,
    });
    let links = flow.links_to(first);
    ensure!(
        !links.is_empty(),
        "nothing leads to {}'s {ask} questions",
        flow.name
    );
    let to = runs.unwrap_or(call);
    links.into_iter().try_for_each(|l| flow.link(l, Some(to)))
}

/// Make the call to `call` at `price` that sells `sold` sell `item`, named
/// by its `ActorName` and shown by its `DisplayItemActor` if it has one.
fn sell(flow: &mut Flowchart, call: &[Call], sold: &str, price: i32, item: &str) -> Result<()> {
    let mut sells = 0;
    for i in calls_to(flow, call)? {
        let params = params_mut(&mut flow.events[i].kind);
        let asks = params
            .iter()
            .any(|(k, v)| k == "Price" && *v == Value::Int(vec![price]));
        if !asks || param(params, NAME) != Some(sold) {
            continue;
        }
        flows::set(params, NAME, string(item))?;
        for (key, value) in params.iter_mut() {
            if let Value::ActorId { name, .. } = value
                && key == DISPLAY
            {
                *name = item.into();
            }
        }
        sells += 1;
    }
    ensure!(
        sells > 0,
        "{} never sells {sold} at {price} by {}",
        flow.name,
        said(call)
    );
    Ok(())
}

/// The param a sale shows its item by.
const DISPLAY: &Name = flow::DISPLAY_ITEM_ACTOR;

/// Medigoron asks whether he has sold, by his own Bool, not whether Link
/// holds the knife, since what he sells may be anything.
fn ask_medigorons_bool(flow: &mut Flowchart) -> Result<()> {
    let at = (flows::events(flow, IN_POUCH).into_iter())
        .find(|(_, p)| param(p, ACTOR) == Some(actor::WEAPON_SWORD_GIANT))
        .with_context(|| format!("{} never asks for the knife", flow.name))?
        .0;
    let asks =
        flows::switch(flow, IS_BOOL).with_context(|| format!("{} asks no Bool", flow.name))?;
    let asks = flows::model(flow, IS_BOOL, asks)?;
    let mut params = asks.params;
    flows::set(&mut params, GAME_DATA, string(MEDIGORON_SOLD))?;
    let (actor, query) = flow.query(&asks.actor, asks.name);
    let EventKind::Switch { cases, .. } = &flow.events[at as usize].kind else {
        unreachable!("the gate was found as a switch");
    };
    // Case 0 is the Bool set, as it was the knife held.
    let cases = cases.clone();
    flow.events[at as usize].kind = EventKind::Switch {
        actor,
        query,
        params,
        cases,
    };
    Ok(())
}

/// The sale Medigoron's call leads to hands over `ActorName` plainly and sets
/// his Bool right after.
fn set_medigorons_bool(flow: &mut Flowchart) -> Result<()> {
    let sets = flows::running(flow, SET_BOOL);
    let sets = sets
        .first()
        .with_context(|| format!("{} sets no Bool", flow.name))?;
    let sets = flows::model(flow, SET_BOOL, *sets)?;
    let calls = calls_to(flow, &[get(flow::GET_IN_EVENT_ITEM_SWORD_MEDIGORON)])?;
    ensure!(!calls.is_empty(), "{} never gives the knife", flow.name);
    for i in calls {
        let EventKind::SubFlow {
            entry_point, next, ..
        } = &mut flow.events[i].kind
        else {
            unreachable!("calls_to finds sub-flows");
        };
        if DRESSED.iter().any(|d| **d == **entry_point) {
            *entry_point = PLAIN.to_string();
        }
        let next = *next;
        let mut params = sets.params.clone();
        flows::set(&mut params, GAME_DATA, string(MEDIGORON_SOLD))?;
        flows::set(&mut params, "Value", Value::Bool(vec![true]))?;
        let (actor, action) = flow.action(&sets.actor, sets.name);
        let mark = flow.push(EventKind::Action {
            next,
            actor,
            action,
            params,
        });
        flow.link(Link::Next(i as u16), Some(mark))?;
    }
    buys_into_a_full_pouch(flow)
}

/// Medigoron's sale asks whether the item fits the pouch, and his entry has
/// no message for a full one. A dealt rupee at his price always fills the
/// Adult's Wallet, so the talk ended with nothing said. Every answer buys.
fn buys_into_a_full_pouch(flow: &mut Flowchart) -> Result<()> {
    let fits = pouch_question(flow, MEDIGORON_SALE)?;
    let buy = answer(flow, fits, 0)?;
    cases_mut(flow, fits).iter_mut().for_each(|c| c.1 = buy);
    Ok(())
}

/// The one question whether the item fits the pouch that the sale `call`
/// starts asks. Answer 0 buys, 2 is a full pouch.
fn pouch_question(flow: &Flowchart, Call(_, entry): Call) -> Result<u16> {
    let reach = flows::reachable(flow, entry)?;
    let fits = running(flow, FITS_POUCH);
    let fits: Vec<u16> = fits.into_iter().filter(|i| reach.contains(i)).collect();
    match fits[..] {
        [fits] => Ok(fits),
        _ => bail!("{entry} asks whether the item fits {} times", fits.len()),
    }
}

/// Where the switch at `at` goes on `case`.
fn answer(flow: &mut Flowchart, at: u16, case: u32) -> Result<u16> {
    let found = cases_mut(flow, at)
        .iter()
        .find(|c| c.0 == case)
        .map(|c| c.1);
    found.with_context(|| format!("{} event {at} has no case {case}", flow.name))
}

/// The cases of the switch at `at`.
fn cases_mut(flow: &mut Flowchart, at: u16) -> &mut Vec<(u32, u16)> {
    let EventKind::Switch { cases, .. } = &mut flow.events[at as usize].kind else {
        unreachable!("the question was found as a switch");
    };
    cases
}

/// The bean seller's one hand-over gives what the sale it's in names, which
/// is a bean in every call but the first sale's.
fn hand_over_the_sold(flow: &mut Flowchart) -> Result<()> {
    let beans: Vec<usize> = calls_to(flow, &[HAND_OVER])?
        .into_iter()
        .filter(|&i| {
            let params = params_mut(&mut flow.events[i].kind);
            param(params, GIVEN) == Some(item::MAGIC_BEAN)
        })
        .collect();
    ensure!(
        beans.len() == 1,
        "{} hands over beans {} times, not once",
        flow.name,
        beans.len()
    );
    let params = params_mut(&mut flow.events[beans[0]].kind);
    flows::set(params, GIVEN, Value::Argument(NAME.to_string()))
}

/// The param a hand-over names its item by.
const GIVEN: &Name = flow::REVEIVE_ITEM;

/// A query on `flow`'s Bool `name`, modelled on the flow's first Bool
/// query: to `set` when it holds, else to `unset`.
fn ask_bool(flow: &mut Flowchart, name: &str, set: u16, unset: u16) -> Result<u16> {
    let asks =
        flows::switch(flow, IS_BOOL).with_context(|| format!("{} asks no Bool", flow.name))?;
    let asks = flows::model(flow, IS_BOOL, asks)?;
    let mut params = asks.params;
    flows::set(&mut params, GAME_DATA, string(name))?;
    let (actor, query) = flow.query(&asks.actor, asks.name);
    // Case 0 is true.
    Ok(flow.push(EventKind::Switch {
        actor,
        query,
        params,
        cases: vec![(0, set), (1, unset)],
    }))
}

/// The carpet merchant's one sale, now of the dealt item, runs only until
/// he has sold. A copy of it sells his Bombchus after that.
fn sell_bombchus_after(flow: &mut Flowchart) -> Result<()> {
    let [sale] = calls_to(flow, &[CARPET_SALE])?[..] else {
        bail!("{} has no one sale", flow.name);
    };
    let sale = sale as u16;
    let mut bombchus = flow.events[sale as usize].kind.clone();
    flows::set(params_mut(&mut bombchus), NAME, string(CARPET_BOMBCHUS))?;
    let links = flow.links_to(sale);
    ensure!(!links.is_empty(), "nothing runs {}'s sale", flow.name);
    let bombchus = flow.push(bombchus);
    let ask = ask_bool(flow, CARPET_SOLD, bombchus, sale)?;
    links.into_iter().try_for_each(|l| flow.link(l, Some(ask)))
}

/// The plain sale's full-pouch answer buys while the carpet merchant hasn't
/// sold, so a dealt item that fills its pouch still sells once.
fn buys_while_unsold(flow: &mut Flowchart) -> Result<()> {
    let fits = pouch_question(flow, CARPET_SALE)?;
    let (buy, full) = (answer(flow, fits, 0)?, answer(flow, fits, 2)?);
    let ask = ask_bool(flow, CARPET_SOLD, full, buy)?;
    (cases_mut(flow, fits).iter_mut())
        .filter(|c| c.0 == 2)
        .for_each(|c| c.1 = ask);
    Ok(())
}

/// An event that does nothing and ends a branch, which the game's cues
/// already run beside their real work.
const NOTHING: (&Name, &Name) = (
    field::DEVELOP_EVENT_SYSTEM_ACTOR,
    flow::EVENT_DEMO_PRODUCT_SUPPORT_COMMENT,
);

/// Empty every call to `call` naming `item`, at least one, so the scene
/// holding them grants nothing. Each becomes the flow's `NOTHING`, since a
/// fork branch can't be unlinked.
fn silence_calls(flow: &mut Flowchart, call: &[Call], item: &str) -> Result<()> {
    let named = |kind: &EventKind| match kind {
        EventKind::SubFlow { params, .. } => {
            ITEM_PARAMS.iter().any(|k| param(params, k) == Some(item))
        }
        _ => false,
    };
    let silenced: Vec<usize> = calls_to(flow, call)?
        .into_iter()
        .filter(|&i| named(&flow.events[i].kind))
        .collect();
    ensure!(
        !silenced.is_empty(),
        "{} never calls {} with {item}",
        flow.name,
        said(call)
    );
    let nothing = (0u16..).zip(&flow.events).find(|(_, e)| match &e.kind {
        EventKind::Action {
            actor,
            action,
            next: None,
            ..
        } => {
            let a = flow.actor_of(*actor);
            a.name.as_str() == NOTHING.0 && a.actions[*action as usize].as_str() == NOTHING.1
        }
        _ => false,
    });
    let nothing =
        nothing.with_context(|| format!("{} has no {} to stand in", flow.name, NOTHING.1))?;
    let nothing = nothing.1.kind.clone();
    for i in silenced {
        flow.events[i].kind = nothing.clone();
    }
    Ok(())
}

/// The events in `flow` where the cutscene `scene` plays: its transitions,
/// and the sets of its played flag a cutscene skip swapped them for.
pub(crate) fn scene_marks(flow: &Flowchart, scene: &str) -> (Vec<u16>, Vec<u16>) {
    let flag = format!("{PLAYED}.{scene}");
    let plays = (0u16..)
        .zip(&flow.events)
        .filter(|(_, e)| transition(flow, &e.kind) == Some(scene))
        .map(|(i, _)| i)
        .collect();
    let swapped = running(flow, SET_BOOL)
        .into_iter()
        .filter(|&i| {
            matches!(&flow.events[i as usize].kind,
                EventKind::Action { params, .. } if param(params, GAME_DATA) == Some(&flag))
        })
        .collect();
    (plays, swapped)
}

/// Grant `item` right after each of `flow`'s transitions to the cutscene
/// `scene`, or the sets of its played flag a skip swapped them for, before
/// whatever follows. The grant is inline: a caller's event never binds
/// `GetPouchContent`. A set gets a fade-in first: the scenes faded back in
/// as they ended, and the caller may have faded out before them.
fn grant_after(flow: &mut Flowchart, flows: &Flows, scene: &str, item: &str) -> Result<()> {
    let (plays, swapped) = scene_marks(flow, scene);
    ensure!(
        !plays.is_empty() || !swapped.is_empty(),
        "{} never marks {scene}",
        flow.name
    );
    let chart = |name: &str| flows.0.get(name).with_context(|| format!("no {name} flow"));
    let inline = grant::Models::of(chart(starting::TEMPLATE)?, chart(grant::POUCH)?);
    for (at, fade) in plays
        .into_iter()
        .map(|at| (at, false))
        .chain(swapped.into_iter().map(|at| (at, true)))
    {
        let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
            unreachable!("transition found an action");
        };
        let mut first = inline.push(flow, string(item), next)?;
        if fade {
            first = flows::fade_in(flow, Some(first));
        }
        flow.link(Link::Next(at), Some(first))?;
    }
    Ok(())
}

/// Open `flow` straight onto its grant, then its "You got" window, past
/// the cutscene around them. Both stay single, so `in_place` still swaps
/// the item.
pub fn straight(flow: &mut Flowchart) -> Result<()> {
    let grant = only(flow, GRANT)?;
    let window = only_call(flow, WINDOW, None)?;
    flow.link(Link::Next(grant), Some(window))?;
    flow.link(Link::Next(window), None)?;
    let entry = flow.entry_points.iter().position(|e| e.name == ENTRY);
    flow.link(Link::Entry(entry.context("no entry point")?), Some(grant))
}

/// The events of `flow`'s cutscene to keep when it's cut: whatever changes
/// the save, in flow order, past the grants of `items`, any call that names
/// one, and the song lesson. Bolero's scene also sets its song's `IsGet`
/// flag by hand.
pub(crate) fn kept(flow: &Flowchart, flows: &Flows, items: &[&str]) -> Result<Vec<u16>> {
    let got: Vec<String> = items.iter().map(|i| format!("IsGet.{i}")).collect();
    let gives = |e: &EventKind| match e {
        EventKind::Action { params, .. } => {
            param(params, ACTOR).is_some_and(|a| items.contains(&a))
        }
        EventKind::SubFlow { params, .. } => names_item(params, items),
        _ => false,
    };
    let sets_got = |e: &EventKind| match e {
        EventKind::Action { params, .. } => {
            param(params, GAME_DATA).is_some_and(|n| got.iter().any(|g| g == n))
        }
        _ => false,
    };
    let grants = running(flow, GRANT);
    let mut kept = Vec::new();
    for e in flows::reachable(flow, ENTRY)? {
        let kind = &flow.events[e as usize].kind;
        let skip = match kind {
            EventKind::Action { .. } => (grants.contains(&e) && gives(kind)) || sets_got(kind),
            EventKind::SubFlow { flowchart, .. } | EventKind::SubFlowSwitch { flowchart, .. } => {
                is_local(flow, flowchart) || flowchart == LESSON || gives(kind)
            }
            _ => true,
        };
        if !skip && flows.changes_save(flow, e) {
            ensure!(
                matches!(kind, EventKind::Action { .. } | EventKind::SubFlow { .. }),
                "kept event {e} branches"
            );
            kept.push(e);
        }
    }
    Ok(kept)
}

/// Whether a call's params hand it one of `items`, by whichever param its
/// getting sub-flow reads the item from.
fn names_item(params: &Params, items: &[&str]) -> bool {
    ITEM_PARAMS
        .iter()
        .any(|k| param(params, k).is_some_and(|n| items.contains(&n)))
}

/// Point the entry point at the reward for `item`, then `then` if any,
/// then copies of the cutscene's save changes. Returns the flow and entry
/// point it hands back to, if it ends in a call to another flow.
fn cut(
    flow: &mut Flowchart,
    flows: &Flows,
    gift: &Name,
    item: &str,
    then: Option<&Name>,
) -> Result<Option<(String, String)>> {
    let items: Vec<&str> = [Some(gift), then]
        .into_iter()
        .flatten()
        .map(|n| &**n)
        .collect();
    let kept = kept(flow, flows, &items)?;
    let back = match kept.last().map(|&e| &flow.events[e as usize].kind) {
        Some(EventKind::SubFlow {
            flowchart,
            entry_point,
            ..
        }) if !is_local(flow, flowchart) => Some((flowchart.clone(), entry_point.clone())),
        _ => None,
    };
    let reward = only_call(flow, REWARD, Some(&**then.unwrap_or(gift)))?;
    let mut given = flow.events[reward as usize].kind.clone();
    flows::set(params_mut(&mut given), NAME, string(item))?;
    let then = then.map(|_| flow.events[reward as usize].kind.clone());
    let chain: Vec<EventKind> = [given]
        .into_iter()
        .chain(then)
        .chain(kept.iter().map(|&e| flow.events[e as usize].kind.clone()))
        .collect();
    let mut first = None;
    for mut kind in chain.into_iter().rev() {
        if let EventKind::Action { next, .. } | EventKind::SubFlow { next, .. } = &mut kind {
            *next = first;
        }
        first = Some(flow.push(kind));
    }
    let entry = flow.entry_points.iter().position(|e| e.name == ENTRY);
    flow.link(Link::Entry(entry.context("no entry point")?), first)?;
    Ok(back)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field, flag, flow, item, scene};

    /// Sheik's songs whose scene the rule keeps: each calls `AllSage`, which
    /// the event that plays it doesn't bind, so it plays with its own grant.
    const UNBOUND: [&Name; 3] = [
        scene::MELODY_GET_FOREST,
        scene::MELODY_GET_FIRE,
        scene::MELODY_GET_WATER,
    ];

    /// Each event the entry point runs, rendered, following `next`.
    fn chain(flow: &Flowchart) -> Vec<String> {
        let lines: Vec<String> = flows::render(flow).lines().map(str::to_owned).collect();
        let entry = flow.entry_points.iter().find(|e| e.name == ENTRY).unwrap();
        let mut out = Vec::new();
        let mut at = entry.main;
        while let Some(e) = at {
            let line = lines
                .iter()
                .find(|l| l.starts_with(&format!("#{e} ")))
                .unwrap();
            out.push(line.split_once(' ').unwrap().1.to_owned());
            at = match &flow.events[e as usize].kind {
                EventKind::Action { next, .. } | EventKind::SubFlow { next, .. } => *next,
                _ => None,
            };
        }
        out
    }

    fn strip(line: &str) -> &str {
        line.rsplit_once(" -> ").map_or(line, |(l, _)| l)
    }

    #[test]
    fn a_cut_gives_the_item_the_song_and_the_saves_in_order() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (all, zelda) = (Flows::load(&romfs).unwrap(), &GIFTS[1]);
        let How::Cut { then } = zelda.how else {
            panic!("Zelda's lesson is cut")
        };
        let mut flow = flows::load(&romfs, zelda.flow).unwrap().unwrap();
        let back = cut(&mut flow, &all, zelda.item, "Item_A", then).unwrap();
        assert_eq!(
            back,
            Some((
                flow::DOOR_OF_TIME.to_string(),
                scene::MELODY_GET_TIME_AFTER.to_string()
            ))
        );
        // The song's own gift renames the copied grant.
        let song = GIFTS
            .iter()
            .find(|g| g.flow == zelda.flow && g.item == then.unwrap());
        let How::Calls { call, .. } = song.unwrap().how else {
            panic!("the Song of Time is a call gift")
        };
        rename_calls(&mut flow, call, then.unwrap(), "Item_B").unwrap();
        let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        let ran = chain(&flow);
        let ran: Vec<_> = ran.iter().map(|l| strip(l)).collect();
        assert_eq!(
            ran,
            [
                &format!(
                    r#"call {}::{}({}="Item_A")"#,
                    flow::GET_POUCH_CONTENT,
                    flow::GET_POUCH_CONTENT_BY_ACTOR_NAME,
                    flag::ACTOR_NAME
                ),
                &format!(
                    r#"call {}::{}({}="Item_B")"#,
                    flow::GET_POUCH_CONTENT,
                    flow::GET_POUCH_CONTENT_BY_ACTOR_NAME,
                    flag::ACTOR_NAME
                ),
                &format!(
                    r#"{}.{}({}="{}" Index=-1 Value=true)"#,
                    field::GAME_DATA_ACTOR,
                    flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL,
                    field::GAME_DATA_NAME,
                    flag::MELODY_GET_TIME_END_CONVERSATION_WITH_ZELDA
                ),
                &format!(
                    "call {}::{}()",
                    flow::DOOR_OF_TIME,
                    scene::MELODY_GET_TIME_AFTER
                ),
            ]
        );
    }

    #[test]
    fn after_zeldas_cut_link_stays_in_the_moat() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, flow::DOOR_OF_TIME).unwrap().unwrap();
        let first = |f: &Flowchart| {
            let e = f
                .entry_points
                .iter()
                .find(|e| e.name == scene::MELODY_GET_TIME_AFTER);
            let at = e.unwrap().main.unwrap();
            let lines = flows::render(f);
            let line = lines.lines().find(|l| l.starts_with(&format!("#{at} ")));
            line.unwrap().to_owned()
        };
        assert!(first(&flow).contains(WARP.1));
        stay(&mut flow, scene::MELODY_GET_TIME_AFTER).unwrap();
        let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert!(!first(&flow).contains(WARP.1), "{}", first(&flow));
    }

    #[test]
    fn in_place_renames_only_the_grant_and_the_window() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let before = flows::load(&romfs, GIFTS[0].flow).unwrap().unwrap();
        let mut flow = flows::load(&romfs, GIFTS[0].flow).unwrap().unwrap();
        in_place(&mut flow, "Item_A").unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        let (old, new) = (flows::render(&before), flows::render(&after));
        let changed: Vec<_> = old
            .lines()
            .zip(new.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(changed.len(), 2, "{changed:#?}");
        for (was, now) in changed {
            assert!(was.contains(&**GIFTS[0].item), "{was}");
            assert_eq!(was.replace(&**GIFTS[0].item, "Item_A"), now);
        }
        assert!(new.contains(&**scene::ITEM_OCARINA_FAIRY_SARIA));
    }

    #[test]
    fn each_npc_gift_renames_its_calls_and_nothing_else() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for gift in &GIFTS {
            let How::Calls { call, .. } = gift.how else {
                continue;
            };
            let before = flows::load(&romfs, gift.flow).unwrap().unwrap();
            let mut flow = flows::load(&romfs, gift.flow).unwrap().unwrap();
            rename_calls(&mut flow, call, gift.item, "Item_A").unwrap();
            let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
            let (old, new) = (flows::render(&before), flows::render(&after));
            let changed: Vec<_> = old
                .lines()
                .zip(new.lines())
                .filter(|(a, b)| a != b)
                .collect();
            let named: Vec<String> = said(call).split('|').map(|c| format!("{c}(")).collect();
            let calls = old
                .lines()
                .filter(|l| named.iter().any(|n| l.contains(n)) && l.contains(&**gift.item))
                .count()
                .max(1);
            assert_eq!(changed.len(), calls, "{}: {changed:#?}", gift.giver);
            for (was, now) in changed {
                let mut was = was.replace(&**gift.item, "Item_A");
                if let Some(dressed) = DRESSED.iter().find(|d| was.contains(&format!("::{d}("))) {
                    was = was.replace(&***dressed, PLAIN);
                }
                assert_eq!(was, now, "{}", gift.giver);
            }
        }
    }

    /// Whether any entry point of `flow` reaches a call naming `item`.
    fn gives(flow: &Flowchart, item: &str) -> bool {
        let reached: BTreeSet<u16> = (flow.entry_points.iter())
            .flat_map(|e| flows::reachable_from(flow, e.main))
            .collect();
        (0u16..).zip(&flow.events).any(|(i, e)| match &e.kind {
            EventKind::SubFlow { params, .. } => {
                reached.contains(&i) && ITEM_PARAMS.iter().any(|k| param(params, k) == Some(item))
            }
            _ => false,
        })
    }

    fn picked_gifts() -> Vec<&'static Gift> {
        let gifts: Vec<_> = GIFTS.iter().filter(|g| picked(g.item).is_some()).collect();
        assert_eq!(gifts.len(), 8);
        gifts
    }

    #[test]
    fn a_moved_bag_or_quiver_never_asks_what_link_holds_and_gives_the_medium_call() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for gift in picked_gifts() {
            let line = picked(gift.item).unwrap();
            let mut flow = flows::load(&romfs, gift.flow).unwrap().unwrap();
            let asks = asked(&flow, line, giving(&flow, gift.item).unwrap());
            assert!(!asks.is_empty(), "{}", gift.giver);
            one_of(&mut flow, line, gift.item).unwrap();
            let reached: BTreeSet<u16> = (flow.entry_points.iter())
                .flat_map(|e| flows::reachable_from(&flow, e.main))
                .collect();
            let still: Vec<_> = asks.iter().filter(|q| reached.contains(q)).collect();
            assert!(still.is_empty(), "{}: {still:?} still asked", gift.giver);
            assert!(gives(&flow, gift.item), "{} never gives it", gift.giver);
            let large = format!("{line}Large");
            assert!(!gives(&flow, &large), "{} still gives {large}", gift.giver);
        }
    }

    #[test]
    fn a_dealt_bag_or_quiver_gift_gives_the_new_item_and_no_other_level() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dealt: HashMap<u64, Drop> = picked_gifts()
            .iter()
            .map(|g| (g.hash(), Drop::from(HEART.to_string())))
            .collect();
        let mut rewrites = Rewrites::default();
        give_all(&romfs, &Flows::load(&romfs).unwrap(), &mut rewrites, &dealt).unwrap();
        for gift in picked_gifts() {
            let flow = written(&rewrites, gift.flow);
            assert!(gives(&flow, HEART), "{} never gives the heart", gift.giver);
            assert!(!gives(&flow, gift.item), "{} still gives it", gift.giver);
            let large = format!("{}Large", picked(gift.item).unwrap());
            assert!(!gives(&flow, &large), "{}", gift.giver);
        }
    }

    #[test]
    fn biggoron_never_makes_link_wait() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, BIGGORON).unwrap().unwrap();
        let waits = |f: &Flowchart| -> Vec<u16> {
            (0..f.events.len() as u16)
                .filter(|&i| match &f.events[i as usize].kind {
                    EventKind::Switch { actor, query, .. } => {
                        f.actor_of(*actor).queries[*query as usize].contains(&**ELAPSED)
                    }
                    _ => false,
                })
                .collect()
        };
        let before = waits(&flow);
        assert!(!before.is_empty());
        waited(&mut flow).unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        for wait in before {
            assert_eq!(after.links_to(wait), [], "#{wait} still runs");
        }
        let claim = flows::reachable(&after, field::SUB_EVENT_GET_SWORD_BIG_GORON_STEP11).unwrap();
        let sword = |e: &u16| {
            matches!(&after.events[*e as usize].kind,
                EventKind::SubFlow { entry_point, .. } if entry_point == flow::GET_IN_EVENT_ITEM_SWORD_BIGGORON)
        };
        assert!(
            claim.iter().any(sword),
            "the claim check never gives the sword"
        );
    }

    #[test]
    fn every_fountain_opens_without_dins_fire() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, FOUNTAIN).unwrap().unwrap();
        let gates = din_gates(&flow).unwrap();
        assert_eq!(gates.len(), 1, "{gates:?}");
        let gate = gates[0];
        let EventKind::Switch { cases, .. } = &flow.events[gate as usize].kind else {
            panic!("#{gate} is no switch");
        };
        let open = cases.iter().find(|c| c.0 == 0).unwrap().1;
        assert!(!flow.links_to(gate).is_empty());
        let before = flows::reachable(&flow, FOUNTAIN_ENTRY).unwrap();
        assert!(
            running(&flow, (actor::NPC_NAVI, flow::EVENT_TALK))
                .iter()
                .any(|e| before.contains(e))
        );
        ungated(&mut flow).unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        assert_eq!(after.links_to(gate), [], "#{gate} still runs");
        assert!(din_gates(&after).unwrap().is_empty());
        let ran = flows::reachable(&after, FOUNTAIN_ENTRY).unwrap();
        assert!(ran.contains(&open), "the melody never reaches #{open}");
        // The Navi line behind the gate no longer runs.
        let navi = (actor::NPC_NAVI, flow::EVENT_TALK);
        assert!(running(&after, navi).iter().all(|e| !ran.contains(e)));
    }

    #[test]
    fn rauru_is_dealt_only_with_the_door_closed() {
        let rauru = GIFTS.iter().find(|g| g.flow == RAURU).unwrap();
        let on = |ids: &[&str]| ids.iter().map(|&i| i.to_owned()).collect();
        assert!(rauru.dealt(&on(&[DUNGEON_REWARDS])));
        assert!(!rauru.dealt(&on(&[DUNGEON_REWARDS, OPEN_DOOR])));
        assert!(!rauru.dealt(&on(&[])));
    }

    #[test]
    fn each_song_opens_on_its_new_item_with_no_lesson_or_song() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let cut_songs = GIFTS.iter().filter(|g| matches!(g.how, How::Cut { .. }));
        for gift in cut_songs.filter(|g| g.setting() == Some(SONGS)) {
            let mut flow = flows::load(&romfs, gift.flow).unwrap().unwrap();
            cut(&mut flow, &all, gift.item, "Item_A", None).unwrap();
            let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
            let ran = chain(&flow);
            assert!(
                ran[0].contains(&format!(r#"{}="Item_A""#, flag::ACTOR_NAME)),
                "{}: {ran:?}",
                gift.flow
            );
            for line in &ran {
                assert!(!line.contains(&**gift.item), "{}: {line}", gift.flow);
                assert!(!line.contains(&**LESSON), "{}: {line}", gift.flow);
            }
        }
    }

    #[test]
    fn a_taught_song_moved_leaves_its_lesson_empty() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (gift, lesson) in TAUGHT {
            assert!(GIFTS.iter().any(|g| g.flow == gift));
            let mut flow = flows::load(&romfs, lesson).unwrap().unwrap();
            assert!(flows::render(&flow).contains(&**LESSON), "{lesson}");
            empty(&mut flow).unwrap();
            let flow = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
            assert!(
                flow.entry_points.iter().all(|e| e.main.is_none()),
                "{lesson}"
            );
        }
    }

    #[test]
    fn rutos_window_names_the_sapphire_once() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (gift, flow, call) in FORETOLD {
            let gift = GIFTS.iter().find(|g| g.flow == gift).unwrap();
            let mut flow = flows::load(&romfs, flow).unwrap().unwrap();
            rename_calls(&mut flow, call, gift.item, "Item_A").unwrap();
            assert!(flows::render(&flow).contains(&format!(r#"{}="Item_A""#, flag::ACTOR_NAME)));
        }
    }

    #[test]
    fn no_placed_actor_shares_a_gift_hash() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let hashes: std::collections::BTreeSet<u64> = GIFTS.iter().map(Gift::hash).collect();
        assert_eq!(hashes.len(), GIFTS.len());
        let checks = crate::checks::scan(&romfs).unwrap();
        assert!(checks.iter().all(|c| !hashes.contains(&c.hash)));
    }

    #[test]
    fn no_cutscene_skip_steps_over_a_gift() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = crate::cutscenes::Catalogue::load(&romfs).unwrap();
        let every = crate::cutscenes::Skips {
            intro: true,
            saria: true,
            owl: true,
            great_deku: true,
            arrivals: true,
            areas: crate::cutscenes::AREAS.iter().map(|a| a.0).collect(),
        };
        let picked = catalogue.picked(&every);
        assert!(!picked.is_empty());
        // Zelda's letter and Impa's lullaby are swapped for their played
        // flags, and their grants move to where the flags are set.
        for gift in &GIFTS {
            let moved = gift.caller().is_some();
            assert!(
                moved || picked.iter().all(|c| c.name != gift.flow),
                "{}",
                gift.flow
            );
        }
    }

    #[test]
    fn saria_opens_straight_onto_her_gift() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut flow = flows::load(&romfs, scene::KOKIRI_0155).unwrap().unwrap();
        straight(&mut flow).unwrap();
        in_place(&mut flow, item::BOOMERANG).unwrap();
        let chain = chain(&flow);
        let chain: Vec<&str> = chain.iter().map(|l| strip(l)).collect();
        assert_eq!(chain.len(), 2, "{chain:?}");
        assert!(chain[0].starts_with(&format!(
            "{}.{}",
            field::POUCH_SYSTEM_ACTOR,
            flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT
        )));
        assert!(chain[0].contains(&format!(r#"Actor="{}""#, item::BOOMERANG)));
        assert_eq!(
            chain[1],
            format!(
                r#"call {}::{}({}="{}")"#,
                flow::GET_POUCH_CONTENT,
                flow::DISPLAY_GET_BY_ACTOR_NAME,
                flag::ACTOR_NAME,
                item::BOOMERANG
            )
        );
    }

    #[test]
    fn zeldas_letter_is_granted_after_her_scene_and_never_inside_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let zelda = GIFTS
            .iter()
            .find(|g| g.flow == scene::HRL_CASTLE_0050)
            .unwrap();
        let How::Later { caller, .. } = zelda.how else {
            panic!("Zelda's letter is granted by its caller")
        };
        let mut rewrites = Rewrites::new();
        give(&romfs, &all, &mut rewrites, zelda, item::MEDAL_FIRE).unwrap();
        let parse = |name: &str| {
            let bytes = &rewrites[&flows::rel(name)];
            Flowchart::parse(bytes).unwrap().unwrap()
        };
        let scene = flows::render(&parse(zelda.flow));
        // The letter prop still acts in the cue, but nothing grants.
        for gone in [
            "Zeldaletter",
            &format!("{}=\"{}\"", flag::ACTOR_NAME, item::ZELDA_LETTER),
            item::MEDAL_FIRE,
        ] {
            assert!(!scene.contains(gone), "the scene still has {gone}");
        }
        // The scene's other cue events are untouched but for that one.
        let before = flows::render(&flows::load(&romfs, zelda.flow).unwrap().unwrap());
        let events = |r: &str| -> Vec<String> {
            r.lines()
                .filter(|l| l.starts_with('#'))
                .map(str::to_owned)
                .collect()
        };
        let (before, after) = (events(&before), events(&scene));
        let changed = before.iter().zip(&after).filter(|(a, b)| a != b);
        assert_eq!(changed.count(), 1);

        let flow = parse(caller);
        let rendered = flows::render(&flow);
        let grant = &granting(item::MEDAL_FIRE);
        assert_eq!(rendered.matches(grant).count(), 1, "granted once");
        let lines: Vec<&str> = rendered.lines().collect();
        let at = lines.iter().position(|l| l.contains(grant)).unwrap();
        let before_it = lines.iter().find(|l| {
            l.contains(&format!(
                r#"{}="{}""#,
                flow::EVENT_NAME,
                scene::HRL_CASTLE_0050
            ))
        });
        let (hop, to) = lines[at].split_once(" -> ").unwrap();
        assert!(hop.contains(&**item::MEDAL_FIRE));
        let hop = before_it.unwrap().rsplit_once(" -> ").unwrap().1;
        assert!(
            lines[at].starts_with(&format!("{} ", hop)),
            "{hop}: {}",
            lines[at]
        );
        let window = flows::render(&flow);
        let next = window.lines().find(|l| l.starts_with(&format!("{to} ")));
        assert!(
            next.unwrap().contains(grant::GET_LINES[0].as_str()),
            "the get window follows: {to}"
        );
    }

    #[test]
    fn only_the_castle_flow_starts_zeldas_scene_and_no_generic_skip_takes_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = crate::cutscenes::Catalogue::load(&romfs).unwrap();
        let scene = catalogue
            .0
            .iter()
            .find(|c| c.name == scene::HRL_CASTLE_0050);
        let scene = scene.unwrap();
        assert_eq!(
            scene.flows,
            BTreeSet::from([flow::GOTO_HYRULE_CASTLE.to_string()])
        );
        assert!(!scene.tier.skippable());
    }

    /// Every event `entry` can reach, in the order it runs them: each
    /// event, then what follows it, a fork's branches before its join.
    fn reach(flow: &Flowchart, entry: &str) -> Vec<u16> {
        let entry = flow.entry_points.iter().find(|e| e.name == entry);
        let mut stack: Vec<u16> = entry.unwrap().main.into_iter().collect();
        let mut out = Vec::new();
        while let Some(i) = stack.pop() {
            if out.contains(&i) {
                continue;
            }
            out.push(i);
            let after: Vec<u16> = match &flow.events[i as usize].kind {
                EventKind::Action { next, .. }
                | EventKind::SubFlow { next, .. }
                | EventKind::Join { next } => next.iter().copied().collect(),
                EventKind::Switch { cases, .. } | EventKind::SubFlowSwitch { cases, .. } => {
                    cases.iter().map(|c| c.1).collect()
                }
                EventKind::Fork { join, branches } => {
                    branches.iter().chain([join]).copied().collect()
                }
                EventKind::Return { .. } => vec![],
            };
            stack.extend(after.into_iter().rev());
        }
        out
    }

    /// Each of `flow`'s events, rendered without its number or its next.
    fn shown(flow: &Flowchart) -> Vec<String> {
        flows::render(flow)
            .lines()
            .filter(|l| l.starts_with('#'))
            .map(|l| strip(l.split_once(' ').unwrap().1).to_owned())
            .collect()
    }

    /// `reach`, rendered, one line per event without its next.
    fn run(flow: &Flowchart, entry: &str) -> Vec<String> {
        let lines = shown(flow);
        reach(flow, entry)
            .into_iter()
            .map(|i| lines[i as usize].clone())
            .collect()
    }

    /// `flow`'s event `e`, rendered without its number or its next.
    fn event(flow: &Flowchart, e: u16) -> String {
        let line = flows::render(flow)
            .lines()
            .find(|l| l.starts_with(&format!("#{e} ")))
            .unwrap()
            .to_owned();
        strip(line.split_once(' ').unwrap().1).to_owned()
    }

    fn sets(scene: &str) -> String {
        format!(
            r#"{}="{PLAYED}.{scene}" Index=-1 Value=true"#,
            field::GAME_DATA_NAME
        )
    }

    fn is_set(line: &str, scene: &str) -> bool {
        line.starts_with(&format!(
            "{}.{}",
            field::GAME_DATA_ACTOR,
            flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL
        )) && line.contains(&sets(scene))
    }

    /// The fade-in a moved grant opens with, a copy of the castle flow's own.
    const FADE: &str =
        "UISystemActor.EventEventFadeIn(Length=1 IsWaitEnd=true IsNoPlayAnime=false Color=0)";

    /// The grant, then the get window's two lines.
    fn granted(item: &str) -> Vec<String> {
        let talk = |id: &str| {
            format!(
                r#"{}.{}({}=true {}=true {}=0 {}="" {}=false {}=false {}=0 {}=0 {}=0 {}=0 {}=0 {}=0 {}=0 {}=false {}=false {}=false {}=false {}=false {}=true {}=4 {}="{id}" {}=0)"#,
                field::SYSTEM_TEXT_NPC,
                flow::EVENT_TALK,
                field::IS_WAIT_END,
                field::IS_CLOSE_DIALOG,
                field::END_DIALOG_OPTION,
                field::AS_NAME,
                field::IS_ANIME_DRIVEN,
                field::IS_PLAY_AS_ALL_SLOT,
                field::CHOICE_NUMBER,
                field::CHOICE_LABEL1,
                field::CHOICE_LABEL2,
                field::CHOICE_LABEL3,
                field::CHOICE_LABEL4,
                field::CHOICE_DEFAULT_NO,
                field::CHOICE_CANCEL_NO,
                field::IS_NOT_BECOME_SPEAKER,
                field::CHANGE_MSTXT_FOR_ACTOR,
                field::IS_ANONYMOUS,
                field::NOT_PLAY_LIP_SYNC_ANIM,
                field::IS_NOT_OPEN_IF_SKIPED_MOST_RECENT,
                field::IS_OVERRIDE_STYLE,
                field::OVERRIDE_STYLE,
                field::MESSAGE_ID,
                field::MAINTAIN_PERFORMANCE_STATE
            )
        };
        [granting(item)]
            .into_iter()
            .chain(grant::GET_LINES.iter().map(|l| talk(l)))
            .collect()
    }

    const GRANT_PREFIX: &str = "PouchSystemActor.EventTriggerAddOrRemovePouchContent(";

    fn granting(item: &str) -> String {
        format!(
            r#"{}.{}(Num=1 {}=false {}=true Actor="{item}" {}=false)"#,
            field::POUCH_SYSTEM_ACTOR,
            flow::EVENT_TRIGGER_ADD_OR_REMOVE_POUCH_CONTENT,
            field::IS_EQUIP_D_IRECT,
            field::IS_SET_MESSAGE_TARGET_ACTOR_NAME,
            field::IS_GET_EVENT
        )
    }

    fn skip_all() -> crate::cutscenes::Skips {
        crate::cutscenes::Skips {
            intro: true,
            saria: true,
            owl: true,
            great_deku: true,
            arrivals: true,
            areas: crate::cutscenes::AREAS.iter().map(|a| a.0).collect(),
        }
    }

    /// What `skips` and the gifts write, with `dealt` for `item` placed at
    /// the gift whose scene is `flow`.
    fn seed(
        romfs: &Romfs,
        skips: &crate::cutscenes::Skips,
        dealt: Option<(&Name, &Name)>,
    ) -> (Rewrites, Vec<String>) {
        let (mut rewrites, flags) = crate::cutscenes::skip(romfs, skips).unwrap();
        let drops: HashMap<u64, Drop> = dealt
            .into_iter()
            .map(|(flow, item)| {
                let gift = GIFTS.iter().find(|g| g.flow == flow).unwrap();
                let drop = Drop {
                    item: item.to_string(),
                    swap: None,
                };
                (gift.hash(), drop)
            })
            .collect();
        let all = Flows::load(romfs).unwrap();
        give_all(romfs, &all, &mut rewrites, &drops).unwrap();
        (rewrites, flags)
    }

    fn written(rewrites: &Rewrites, flow: &str) -> Flowchart {
        let bytes = &rewrites[&flows::rel(flow)];
        Flowchart::parse(bytes).unwrap().unwrap()
    }

    const MEET: &Name = flag::GOTO_HYRULE_CASTLE_MEET_ZELDA;
    const LETTER: &Name = item::ZELDA_LETTER;

    fn zeldas_chain_is_flags_and_one_grant(item: &str, dealt: Option<(&Name, &Name)>) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, flags) = seed(&romfs, &skip_all(), dealt);
        let flow = written(&rewrites, flow::GOTO_HYRULE_CASTLE);
        let ran = run(&flow, MEET);
        for z in crate::cutscenes::ZELDA {
            assert!(
                !ran.iter()
                    .any(|l| l.contains(&format!(r#"{}="{z}""#, flow::EVENT_NAME)))
            );
            assert!(
                !flags.contains(&format!("{PLAYED}.{z}=true")),
                "{z} at new game"
            );
        }
        // The five flags, in the order the scenes played, with the fade-in and
        // then the grant right after the fourth, and nothing else granting.
        let at: Vec<usize> = crate::cutscenes::ZELDA
            .iter()
            .map(|z| ran.iter().position(|l| is_set(l, z)).unwrap())
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{at:?}");
        let grant = granting(item);
        assert_eq!(ran.iter().filter(|l| **l == grant).count(), 1, "{item}");
        assert_eq!(ran[at[3] + 1], FADE);
        assert_eq!(ran[at[3] + 2..at[3] + 5], granted(item));
        assert_eq!(flows::render(&flow).matches(&grant).count(), 1);
        // The rest of the scene stays: her talk flag, her warp, both quest
        // steps and the fades.
        let has = |needles: &[&str]| ran.iter().any(|l| needles.iter().all(|n| l.contains(n)));
        assert!(has(&[&format!(
            "{}.{}",
            actor::NPC_HYLIA_ZELDA_CHILD,
            field::EVENT_TRIGGER_SET_HAVE_TALKED
        )]));
        assert!(has(&[&format!(
            "{}.{}",
            actor::NPC_HYLIA_ZELDA_CHILD,
            field::EVENT_AI_SCHEDULE_WARP_TO_ANCHOR
        )]));
        assert!(has(&[
            &format!(
                r#"{}="{}""#,
                field::CHALLENGE_NAME,
                flow::GOTO_HYRULE_CASTLE
            ),
            &format!(r#"{}="Complete""#, field::STEP_NAME)
        ]));
        assert!(has(&[
            &format!(r#"{}="{}""#, field::CHALLENGE_NAME, flow::DOOR_OF_TIME),
            &format!(r#"{}="Step01""#, field::STEP_NAME)
        ]));
        assert!(has(&[flow::EVENT_EVENT_FADE_OUT]) && has(&[flow::EVENT_EVENT_FADE_IN]));
        // 0050's own scene is never reached, so it is left alone.
        assert!(!rewrites.contains_key(&flows::rel(scene::HRL_CASTLE_0050)));
    }

    #[test]
    fn skipped_zelda_is_five_flags_and_the_shuffled_letter_once() {
        zeldas_chain_is_flags_and_one_grant(
            item::MEDAL_FIRE,
            Some((scene::HRL_CASTLE_0050, item::MEDAL_FIRE)),
        );
    }

    #[test]
    fn skipped_zelda_without_npc_gifts_still_grants_the_letter_once() {
        zeldas_chain_is_flags_and_one_grant(LETTER, None);
    }

    #[test]
    fn without_the_castle_skip_zeldas_chain_is_vanilla_but_for_the_gift() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut skips = skip_all();
        skips.areas.remove("castle");
        let dealt = Some((scene::HRL_CASTLE_0050, item::MEDAL_FIRE));
        let (rewrites, flags) = seed(&romfs, &skips, dealt);
        let flow = written(&rewrites, flow::GOTO_HYRULE_CASTLE);
        let ran = run(&flow, MEET);
        let vanilla = flows::load(&romfs, flow::GOTO_HYRULE_CASTLE)
            .unwrap()
            .unwrap();
        let before = run(&vanilla, MEET);
        for z in crate::cutscenes::ZELDA {
            let scene = format!(r#"{}="{z}""#, flow::EVENT_NAME);
            assert!(ran.iter().any(|l| l.contains(&scene)), "{z}");
            assert!(!ran.iter().any(|l| is_set(l, z)), "{z}");
        }
        // Only the one grant and its window, after 0050, differ.
        let at = ran
            .iter()
            .position(|l| {
                l.contains(&format!(
                    r#"{}="{}""#,
                    flow::EVENT_NAME,
                    scene::HRL_CASTLE_0050
                ))
            })
            .unwrap();
        assert_eq!(ran[at + 1..at + 4], granted(item::MEDAL_FIRE));
        let without = [&ran[..at + 1], &ran[at + 4..]].concat();
        assert_eq!(without, before);
        // Nothing dealt and no skip at all: the flow isn't written.
        let (plain, _) = seed(&romfs, &Default::default(), None);
        assert!(!plain.contains_key(&flows::rel(flow::GOTO_HYRULE_CASTLE)));
        drop(flags);
    }

    const IMPA: &Name = field::STEP01_NPC_HYLIA_IMPA_CHILD_TALK;
    const SEND_OFF: &Name = field::DOOR_OF_TIME_SEND_OFF;
    const LULLABY: &Name = item::MELODY_ZELDA_LULLABY;

    fn impa_is_a_flag_a_grant_and_the_banc_change(item: &str, dealt: Option<(&Name, &Name)>) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, flags) = seed(&romfs, &skip_all(), dealt);
        let flow = written(&rewrites, flow::DOOR_OF_TIME);
        let vanilla = flows::load(&romfs, flow::DOOR_OF_TIME).unwrap().unwrap();
        let banc = run(
            &vanilla,
            scene::MELODY_GET_ZELDA_AFTER_NPC_HYLIA_IMPA_CHILD_TALK,
        )
        .into_iter()
        .find(|l| l.contains(&**flow::EVENT_RESERVE_BANC_CHANGE))
        .unwrap();
        let ran = run(&flow, IMPA);
        assert!(
            !ran.iter().any(|l| l.contains(&**flow::EVENT_TRANSITION)),
            "{ran:?}"
        );
        let grant = granting(item);
        let tail = &ran[ran.len() - 6..];
        assert!(is_set(&tail[0], scene::MELODY_GET_ZELDA), "{tail:?}");
        assert_eq!(tail[1], FADE);
        assert_eq!(tail[2..5], granted(item));
        assert_eq!(tail[5], banc);
        assert_eq!(
            flows::render(&flow).matches(&grant).count(),
            1,
            "granted once"
        );
        for scene in crate::cutscenes::IMPA {
            assert!(
                !flags.contains(&format!("{PLAYED}.{scene}=true")),
                "{scene}"
            );
        }
        // The escort's transition is its flag, and the steps after it run.
        let escort = run(&flow, SEND_OFF);
        assert!(
            !escort.iter().any(|l| l.contains(&**flow::EVENT_TRANSITION)),
            "{escort:?}"
        );
        let at = escort
            .iter()
            .position(|l| is_set(l, scene::HRL_FIELD_0020))
            .unwrap();
        assert!(
            escort[at..]
                .iter()
                .any(|l| l.contains(&format!(r#"{}="Step02""#, field::STEP_NAME)))
        );
        assert!(escort.iter().any(|l| l.contains(&format!(
            r#"{}="{}""#,
            field::CHALLENGE_NAME,
            flow::GET_STONE_OF_FIRE
        ))));
        // The scene itself never plays, so nothing in it grants.
        if let Some(scene) = rewrites.get(&flows::rel(scene::MELODY_GET_ZELDA)) {
            let scene = flows::render(&Flowchart::parse(scene).unwrap().unwrap());
            assert!(!scene.contains(&**flow::GET_POUCH_CONTENT_BY_ACTOR_NAME));
        }
    }

    #[test]
    fn skipped_impa_sets_her_flag_grants_the_shuffled_song_once_and_leaves() {
        impa_is_a_flag_a_grant_and_the_banc_change(
            item::MEDAL_WATER,
            Some((scene::MELODY_GET_ZELDA, item::MEDAL_WATER)),
        );
    }

    #[test]
    fn skipped_impa_without_shuffled_songs_grants_the_lullaby_once() {
        impa_is_a_flag_a_grant_and_the_banc_change(LULLABY, None);
    }

    #[test]
    fn without_the_songs_skip_impa_still_plays_her_scene() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut skips = skip_all();
        skips.areas.remove("songs");
        let (rewrites, _) = seed(&romfs, &skips, None);
        let flow = written(&rewrites, flow::DOOR_OF_TIME);
        let ran = run(&flow, IMPA);
        assert!(ran.iter().any(|l| l.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::MELODY_GET_ZELDA
        ))));
        assert!(!ran.iter().any(|l| is_set(l, scene::MELODY_GET_ZELDA)));
        // Its escort is the hyrule-field setting's, which still plays it.
        let escort = run(&flow, SEND_OFF);
        assert!(escort.iter().any(|l| l.contains(&format!(
            r#"{}="{}""#,
            flow::EVENT_NAME,
            scene::HRL_FIELD_0020
        ))));
        assert!(rewrites.contains_key(&flows::rel(scene::MELODY_GET_ZELDA)));
    }

    /// How many times `flow` grants `item` through the reward call.
    fn grants(flow: &Flowchart, item: &str) -> usize {
        flows::render(flow).matches(&granting(item)).count()
    }

    fn every_cut_scene_with_a_grant_grants_once_where_it_played(dealt: bool) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let swapped = SKIPPED_IN
            .iter()
            .filter(|(scene, _)| !UNBOUND.contains(scene));
        for &(scene, caller) in swapped {
            let gift = GIFTS.iter().find(|g| g.flow == scene).unwrap();
            let item = if dealt { item::BOTTLE } else { gift.item };
            let drop = dealt.then_some((scene, item));
            let (rewrites, _) = seed(&romfs, &skip_all(), drop);
            let vanilla = flows::load(&romfs, caller).unwrap().unwrap();
            let sites = scene_marks(&vanilla, scene).0.len();
            assert!(sites > 0, "{caller} never played {scene}");
            let flow = written(&rewrites, caller);
            // Once at each place the scene played, and not the vanilla item
            // as well when another was dealt.
            assert_eq!(grants(&flow, item), sites, "{scene} in {caller}");
            if dealt {
                assert_eq!(grants(&flow, gift.item), 0, "{scene} in {caller}");
            }
            let (plays, sets) = scene_marks(&flow, scene);
            assert!(plays.is_empty());
            assert_eq!(sets.len(), sites);
            // Each set is followed by a fade-in, its grant and the get
            // window, before anything the caller does next.
            let after = |at: u16| {
                let EventKind::Action { next, .. } = flow.events[at as usize].kind else {
                    unreachable!();
                };
                next.unwrap()
            };
            for at in sets {
                let mut shown = Vec::new();
                let mut e = at;
                for _ in 0..4 {
                    e = after(e);
                    shown.push(event(&flow, e));
                }
                assert_eq!(shown[0], FADE, "{scene} in {caller}");
                assert_eq!(shown[1..], granted(item), "{scene} in {caller}");
            }
            // The scene's own flow is never written to grant it again.
            if let Some(bytes) = rewrites.get(&flows::rel(scene)) {
                let own = Flowchart::parse(bytes).unwrap().unwrap();
                assert_eq!(grants(&own, item), 0, "{scene}");
            }
        }
    }

    /// A grant a skip moves into a caller opens its window on a screen the
    /// caller may have faded out, so each is run straight after a fade-in.
    #[test]
    fn every_moved_grant_in_a_skip_all_seed_follows_a_fade_in() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, _) = seed(&romfs, &skip_all(), None);
        let mut moved = 0;
        for rel in rewrites.keys() {
            let Some(flow) = Flowchart::parse(&rewrites[rel]).unwrap() else {
                continue;
            };
            let Some(vanilla) = flows::load(&romfs, &flow.name).unwrap() else {
                continue;
            };
            // A moved grant is one added to the flow that opens the get
            // window: a cut scene's copied grants and starting items don't.
            let shown = shown(&flow);
            let opens = |e: usize| match flow.events[e].kind {
                EventKind::Action { next: Some(n), .. } => {
                    shown[n as usize].contains(grant::GET_LINES[0].as_str())
                }
                _ => false,
            };
            for e in vanilla.events.len()..flow.events.len() {
                if !shown[e].starts_with(GRANT_PREFIX) || !opens(e) {
                    continue;
                }
                moved += 1;
                for link in flow.links_to(e as u16) {
                    let Link::Next(p) = link else {
                        panic!("{} grants at #{e} off a {link:?}", flow.name);
                    };
                    assert_eq!(shown[p as usize], FADE, "{} grants at #{e}", flow.name);
                }
            }
        }
        // Every swapped scene with a gift, Zelda's and Impa's included.
        assert!(moved >= SKIPPED_IN.len() - UNBOUND.len(), "{moved}");
    }

    #[test]
    fn every_cut_scene_with_a_grant_grants_the_vanilla_item_once_without_its_setting() {
        every_cut_scene_with_a_grant_grants_once_where_it_played(false);
    }

    #[test]
    fn every_cut_scene_with_a_grant_grants_the_dealt_item_once_with_its_setting() {
        every_cut_scene_with_a_grant_grants_once_where_it_played(true);
    }

    const STONE: &Name = flow::GET_STONE_OF_FIRE;
    const BRACELET: &Name = item::GAUNTLET_GORON;

    fn darunia_asks_for_the_tune_then_gives_the_bracelet_and_opens_dodongos(
        skips: &crate::cutscenes::Skips,
        item: &str,
        dealt: Option<(&Name, &Name)>,
    ) {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (rewrites, _) = seed(&romfs, skips, dealt);
        let flow = written(&rewrites, STONE);
        let ran = run(&flow, field::GET_STONE_OF_FIRE_PLAY_OCARINA);
        let at = |what: &dyn Fn(&str) -> bool| ran.iter().position(|l| what(l));
        // The prompt stays, and so does the wrong tune's scene.
        assert!(
            at(&|l| l.contains(&format!(
                "{}::{}",
                scene::SY_OCARINA,
                field::PLAY_OCARINA_FREE
            )))
            .is_some(),
            "{ran:?}"
        );
        assert!(
            at(&|l| l.contains(&format!(r#"{}="{}""#, flow::EVENT_NAME, scene::GORON_0028)))
                .is_some()
        );
        // The dance and his gift are flags, and no scene plays.
        for scene in [scene::GORON_0025, scene::GORON_0030] {
            assert!(
                !ran.iter()
                    .any(|l| l.contains(&format!(r#"{}="{scene}""#, flow::EVENT_NAME)))
            );
            assert!(at(&|l| is_set(l, scene)).is_some(), "{scene}");
        }
        // The bracelet, then the step, right after the gift scene's flag.
        let gift = at(&|l| is_set(l, scene::GORON_0030)).unwrap();
        assert_eq!(ran[gift + 1], FADE, "{ran:?}");
        assert_eq!(ran[gift + 2..gift + 5], granted(item), "{ran:?}");
        assert!(ran[gift + 5].contains(&format!(
            r#"{}="{}""#,
            field::CHALLENGE_NAME,
            flow::GET_STONE_OF_FIRE
        )));
        assert!(ran[gift + 5].contains(&format!(
            r#"{}="{}""#,
            field::STEP_NAME,
            flag::DODONGO_CAVERN_OPNE
        )));
        assert_eq!(grants(&flow, item), 1);
        if item != BRACELET {
            assert_eq!(grants(&flow, BRACELET), 0);
        }
        // The dance's flag comes first, after the tune.
        let dance = at(&|l| is_set(l, scene::GORON_0025)).unwrap();
        let tune = at(&|l| {
            l.contains(&format!(
                "{}::{}",
                scene::SY_OCARINA,
                field::PLAY_OCARINA_FREE
            ))
        })
        .unwrap();
        assert!(tune < dance && dance < gift);
    }

    #[test]
    fn darunias_dance_is_cut_with_every_area_skipped() {
        darunia_asks_for_the_tune_then_gives_the_bracelet_and_opens_dodongos(
            &skip_all(),
            BRACELET,
            None,
        );
    }

    #[test]
    fn darunias_dance_is_cut_with_only_goron_city_skipped_and_his_gift_dealt() {
        let mut skips = crate::cutscenes::Skips::default();
        skips.areas.insert("goron");
        darunia_asks_for_the_tune_then_gives_the_bracelet_and_opens_dodongos(
            &skips,
            item::BOTTLE,
            Some((scene::GORON_0030, item::BOTTLE)),
        );
    }

    #[test]
    fn without_the_goron_skip_darunias_dance_plays_and_the_gift_stays_inside_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut skips = skip_all();
        skips.areas.remove("goron");
        let (rewrites, _) = seed(&romfs, &skips, None);
        // Other areas' skips write the flow, and it still plays both scenes
        // and grants nothing itself.
        let flow = match rewrites.get(&flows::rel(STONE)) {
            Some(bytes) => Flowchart::parse(bytes).unwrap().unwrap(),
            None => flows::load(&romfs, STONE).unwrap().unwrap(),
        };
        for scene in [scene::GORON_0025, scene::GORON_0030] {
            assert_eq!(scene_marks(&flow, scene).0.len(), 1, "{scene}");
            assert!(scene_marks(&flow, scene).1.is_empty(), "{scene}");
        }
        assert_eq!(grants(&flow, BRACELET), 0);
        assert!(!rewrites.contains_key(&flows::rel(scene::GORON_0030)));
    }

    /// A flow before and after `f`, rendered, as the game would read it.
    fn edited(
        romfs: &Romfs,
        name: &str,
        f: impl FnOnce(&mut Flowchart) -> Result<()>,
    ) -> (String, String) {
        let mut flow = flows::load(romfs, name).unwrap().unwrap();
        let before = flows::render(&flow);
        f(&mut flow).unwrap();
        let after = Flowchart::parse(&flow.to_bytes()).unwrap().unwrap();
        (before, flows::render(&after))
    }

    /// The lines of `after` that differ from `before` at the same place.
    fn changed<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
        before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .collect()
    }

    fn sale(flow: &str) -> (&'static Gift, &'static [Call], &'static str, i32) {
        let gift = GIFTS.iter().find(|g| g.flow == flow).unwrap();
        let How::Sale {
            call, sold, price, ..
        } = gift.how
        else {
            panic!("{flow} isn't a sale");
        };
        (gift, call, sold, price)
    }

    #[test]
    fn each_poe_collector_gift_renames_only_the_call_naming_its_item() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gifts: Vec<_> = GIFTS
            .iter()
            .filter(|g| g.flow == actor::NPC_HYLIA_048_BOTTLE_SHOP)
            .collect();
        assert_eq!(gifts.len(), 2);
        assert_ne!(gifts[0].hash(), gifts[1].hash());
        for gift in gifts {
            let How::Calls { call, found } = gift.how else {
                panic!("{} is a call gift", gift.giver);
            };
            assert!(found.is_none());
            let (before, after) = edited(&romfs, gift.flow, |f| {
                rename_calls(f, call, gift.item, "Item_A")
            });
            let lines = changed(&before, &after);
            assert_eq!(lines.len(), 1, "{} {lines:#?}", gift.giver);
            let (was, now) = lines[0];
            assert!(was.contains(&format!("=\"{}\"", gift.item)), "{was}");
            assert!(now.contains("=\"Item_A\"") && !now.contains(&**gift.item));
            assert!(now.contains(&format!("::{PLAIN}(")), "{now}");
        }
    }

    #[test]
    fn the_sales_are_dealt_by_merchants_alone() {
        let sales: Vec<_> = GIFTS.iter().filter(|g| g.is_sale()).collect();
        assert_eq!(sales.len(), 3);
        assert!(sales.iter().all(|g| g.setting() == Some(MERCHANTS)));
        let on = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert!(sales.iter().all(|g| g.dealt(&on(&[MERCHANTS]))));
        assert!(sales.iter().all(|g| !g.dealt(&on(&[NPC_GIFTS]))));
        assert_eq!(flags(&on(&[MERCHANTS])), [MEDIGORON_SOLD]);
        assert!(flags(&on(&[NPC_GIFTS])).is_empty());
    }

    #[test]
    fn medigoron_sells_the_dealt_item_once_by_his_own_bool() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (gift, call, sold, price) = sale(MEDIGORON);
        let How::Sale { gate, shop, .. } = gift.how else {
            unreachable!()
        };
        let (before, after) = edited(&romfs, MEDIGORON, |f| {
            sell(f, call, sold, price, "Item_A")?;
            gate.unwrap()(f)
        });
        // The sale's call, and the gate in front of the talk.
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 2, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(was.starts_with("#2 ") && now.starts_with("#2 "), "{now}");
        assert!(was.contains("IsExistInPouch") && was.ends_with("? 0:#35 1:#3"));
        assert!(
            now.contains(&format!("{}(", flow::EVENT_QUERY_IS_GAME_DATA_BOOL))
                && now.contains(MEDIGORON_SOLD)
        );
        assert!(now.ends_with("? 0:#35 1:#3"), "{now}");
        let (_, now) = lines[1];
        assert!(
            now.starts_with("#6 ") && now.contains(&format!("{}=\"Item_A\"", flag::ACTOR_NAME))
        );
        assert!(
            now.contains(&format!("{}=<Item_A >", flow::DISPLAY_ITEM_ACTOR)) && !now.contains(sold)
        );
        // The shop buys whether or not the item fits the pouch, hands it
        // over plainly, then sets the Bool.
        let (before, after) = edited(&romfs, SIMPLE_SHOP, shop);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 3, "{lines:#?}");
        // Its full-pouch cancel is out of reach.
        let (was, now) = lines[0];
        assert!(was.contains(" #13") && !now.contains(" #13"), "{now}");
        let (was, now) = lines[1];
        assert!(
            was.contains(&format!("{}(", flow::EVENT_QUERY_CAN_ADD_TO_PORCH))
                && was.ends_with("? 0:#18 1:#16 2:#13")
        );
        assert!(now.ends_with("? 0:#18 1:#18 2:#18"), "{now}");
        let (was, now) = lines[2];
        assert!(
            was.contains(&format!("::{}(", flow::GET_IN_EVENT_ITEM_SWORD_MEDIGORON))
                && was.starts_with("#112 ")
        );
        assert!(now.contains(&format!("::{PLAIN}(")) && now.starts_with("#112 "));
        let set = after.lines().find(|l| l.contains(MEDIGORON_SOLD)).unwrap();
        let at = set.split(' ').next().unwrap();
        assert!(now.ends_with(&format!("-> {at}")), "{now}");
        assert!(
            set.contains(&format!("{}(", flow::EVENT_TRIGGER_SET_GAME_DATA_BOOL))
                && set.contains("Value=true")
        );
        assert!(set.ends_with(&was[was.rfind("-> ").unwrap()..]), "{set}");
    }

    #[test]
    fn the_carpet_merchant_sells_the_dealt_item_until_he_has_sold() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (gift, call, sold, price) = sale(CARPET_MERCHANT);
        let How::Sale { gate, shop, .. } = gift.how else {
            unreachable!()
        };
        let (before, after) = edited(&romfs, CARPET_MERCHANT, |f| {
            sell(f, call, sold, price, "Item_A")?;
            gate.unwrap()(f)
        });
        let line = |at: &str| {
            after
                .lines()
                .find(|l| l.starts_with(&format!("{at} ")))
                .unwrap()
        };
        let sold_bool = format!("{}=\"{CARPET_SOLD}\"", field::GAME_DATA_NAME);
        let ask = (after.lines().rev())
            .find(|l| l.contains(&sold_bool))
            .unwrap();
        let ask_at = ask.split(' ').next().unwrap();
        assert!(!before.contains(&format!("{ask_at} ")), "{ask}");
        // Both talks that led to the sale now ask first.
        for talk in ["#12", "#13"] {
            assert!(
                line(talk).ends_with(&format!("-> {ask_at}")),
                "{}",
                line(talk)
            );
        }
        // Sold goes to a Bombchu copy, unsold to the dealt sale.
        let (set, unset) = ask
            .rsplit_once("? 0:")
            .unwrap()
            .1
            .split_once(" 1:")
            .unwrap();
        assert_eq!(unset, "#0");
        assert!(
            line("#0").contains(&format!("{}=\"Item_A\"", flag::ACTOR_NAME))
                && line("#0").contains("Price=90 ")
        );
        assert!(
            line(set).contains(&format!("{}=\"{sold}\"", flag::ACTOR_NAME)),
            "{}",
            line(set)
        );
        assert!(
            line(set).contains("Price=90 ")
                && line(set).contains(&format!("::{}(", flow::SIMPLE_SHOP_ROOT))
        );
        // The plain sale's full pouch buys until he has sold.
        let (before, after) = edited(&romfs, SIMPLE_SHOP, shop);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(
            was.starts_with("#7 ") && was.ends_with("? 0:#25 1:#8 2:#5"),
            "{was}"
        );
        let ask_at = now.rsplit_once(" 2:").unwrap().1;
        let ask = after
            .lines()
            .find(|l| l.starts_with(&format!("{ask_at} ")))
            .unwrap();
        assert!(
            ask.contains(&sold_bool) && ask.ends_with("? 0:#5 1:#25"),
            "{ask}"
        );
    }

    #[test]
    fn the_bean_seller_gives_the_dealt_item_for_his_first_sale_only() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (gift, call, sold, price) = sale(BEAN_SELLER);
        let How::Sale { gate, shop, .. } = gift.how else {
            unreachable!()
        };
        assert!(gate.is_none());
        let (before, after) = edited(&romfs, BEAN_SELLER, |f| {
            sell(f, call, sold, price, "Item_A")
        });
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(was.starts_with("#0 ") && now.starts_with("#0 "));
        assert!(
            now.contains("Price=10 ") && now.contains(&format!("{}=\"Item_A\"", flag::ACTOR_NAME))
        );
        // The seven others, at 20 and up, still sell beans.
        let beans = |t: &str| {
            t.matches(&format!("{}=\"{}\"", flag::ACTOR_NAME, item::MAGIC_BEAN))
                .count()
        };
        assert_eq!(beans(&before), beans(&after) + 1);
        assert!(beans(&after) >= 5);
        // The one hand-over gives what its sale names.
        let (before, after) = edited(&romfs, SIMPLE_SHOP, shop);
        let lines = changed(&before, &after);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let (was, now) = lines[0];
        assert!(
            was.contains(&format!("{}=\"{}\"", flow::REVEIVE_ITEM, item::MAGIC_BEAN)),
            "{was}"
        );
        assert_eq!(
            was.replace(
                &format!("\"{}\"", item::MAGIC_BEAN),
                &format!("${}", flag::ACTOR_NAME)
            ),
            now
        );
    }
}
