//! The table of gifts: every check an NPC hands over in an event flow.

use super::sales::{
    ask_medigorons_bool, buys_while_unsold, hand_over_the_sold, sell_bombchus_after,
    set_medigorons_bool,
};
use super::*;

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

/// An NPC's gift the game marks with nothing of its own.
const fn unmarked(
    flow: &'static Name,
    area: &'static Name,
    giver: &'static str,
    item: &'static Name,
    call: &'static [Call],
) -> Gift {
    Gift {
        flow,
        area,
        giver,
        item,
        how: How::Calls { call, found: None },
        setting: Some(NPC_GIFTS),
    }
}

/// A step of the adult trade chain, given for the item shown.
const fn trade(
    flow: &'static Name,
    area: &'static Name,
    giver: &'static str,
    item: &'static Name,
    call: &'static [Call],
) -> Gift {
    Gift {
        flow,
        area,
        giver,
        item,
        how: How::Calls { call, found: None },
        setting: Some(TRADE_CHAIN),
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

pub const GIFTS: [Gift; 87] = [
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
    unmarked(
        actor::NPC_HYLIA_048_BOTTLE_SHOP,
        flag::MARKET,
        "Poe collector",
        item::BOTTLE,
        &[get(flow::GET_IN_EVENT_POESHOP)],
    ),
    // The Gold Rupee is paid by its own call, so each gift renames only the
    // call naming its item. Every sale adds 100 points and the game places
    // exactly 10 Big Poes, each with its own cleared flag, so 1000 is reached
    // once and this call runs once.
    unmarked(
        actor::NPC_HYLIA_048_BOTTLE_SHOP,
        flag::MARKET,
        "Poe collector: 1000 points",
        actor::OBJ_RUPEE_GOLD,
        &[get(flow::GET_IN_EVENT_POESHOP)],
    ),
    // A Kokiri kid pays a Blue Rupee once Link has broken the ring of
    // rocks around him, then sets his paid Bool.
    npc(
        actor::NPC_KOKIRI_008_CHILD,
        flag::KOKIRI_FOREST,
        "Kokiri rock clearing",
        actor::OBJ_RUPEE_BLUE,
        &[get(flow::GET_IN_EVENT_NOT_TURN)],
        flag::KOKIRI_FOREST_NPC_KOKIRI_008_CHILD_ALREADY_PAID,
    ),
    // Another pays a Blue Rupee for clearing the grass in his yard, on
    // either of his talks, each setting the same Bool.
    npc(
        actor::NPC_KOKIRI_005_CHILD,
        flag::KOKIRI_FOREST,
        "Kokiri yard weeding",
        actor::OBJ_RUPEE_BLUE,
        &[get(flow::GET_IN_EVENT_NOT_TURN)],
        flag::HAVE_TALKED_SPECIAL_NPC_KOKIRI_005_CHILD_TALK_EVENT35,
    ),
    // The running man pays a Purple Rupee for Link's first win, then sets
    // the clear Bool. Every later win pays a Red Rupee through the same
    // call, which names its own rupee, so it stays vanilla.
    Gift {
        flow: actor::NPC_HYLIA_RUNNING_MAN_ADULT,
        area: flag::GERUDO_VALLEY,
        giver: "Running man's race",
        item: actor::OBJ_RUPEE_PURPLE,
        how: How::Calls {
            call: &[get(flow::GET_IN_EVENT_NOT_TURN_FOR_POS_CAMERA)],
            found: Some(
                flag::HAVE_TALKED_SPECIAL_NPC_HYLIA_RUNNING_MAN_ADULT_RUNNING_RACE_CLEAR_EVENT32,
            ),
        },
        setting: Some(RUNNING_RACE),
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
    // Talon's first win hands over a new bottle of milk. His replays and his
    // milk sale fill an empty one, and stay milk.
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
    unmarked(
        flow::SAGE_OF_WATER,
        flag::ZORA_DOMAIN,
        "King Zora",
        actor::OBJ_ARMOR_ZORA,
        &[get(flow::GET_IN_EVENT_NOT_TURN_POS_CAMERA)],
    ),
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
    Gift {
        flow: super::pedestal::FLOW,
        area: flag::MARKET,
        giver: "Master Sword pedestal",
        item: actor::WEAPON_SWORD_MASTER,
        how: How::Pedestal,
        setting: Some(MASTER_SWORD),
    },
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
    unmarked(
        flow::GOTO_HYRULE_CASTLE,
        flag::HYRULE_CASTLE,
        "Weird Egg",
        item::WEIRD_EGG,
        &[get(flow::GET_IN_EVENT_NOT_TURN)],
    ),
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
    // The adult trade chain, each giver by what Link shows them.
    trade(
        actor::NPC_HYLIA_049_ADULT,
        flag::KAKARIKO_VILLAGE,
        "Anju as adult",
        item::POCKET_EGG,
        &[HAND_OVER],
    ),
    Gift {
        flow: actor::NPC_HYLIA_TALON_ADULT_KAKARIKO,
        area: flag::KAKARIKO_VILLAGE,
        giver: "Talon: Pocket Cucco",
        item: item::COJIRO_CUCCO,
        how: How::Added {
            also: &[actor::NPC_HYLIA_TALON_ADULT],
        },
        setting: Some(TRADE_CHAIN),
    },
    trade(
        actor::NPC_HYLIA_017_ADULT,
        flag::LOST_WOOD,
        "Grog: Cojiro",
        item::ODD_MUSHROOM,
        &[HAND_OVER],
    ),
    trade(
        actor::NPC_HYLIA_053_ADULT,
        flag::KAKARIKO_VILLAGE,
        "Granny: Odd Mushroom",
        item::ODD_POWDER,
        &[HAND_OVER],
    ),
    trade(
        actor::NPC_KOKIRI_FADO_ADULT,
        flag::LOST_WOOD,
        "Fado: Odd Powder",
        item::POACHER_SAW,
        &[get(flow::GET_IN_EVENT_NOT_TURN_FADO)],
    ),
    trade(
        actor::NPC_HYLIA_012_ADULT,
        flag::GERUDO_VALLEY,
        "Carpenter boss: Poacher's Saw",
        item::BROKEN_SWORD,
        &[HAND_OVER],
    ),
    trade(
        BIGGORON,
        flag::DEATH_MOUNTAIN_TRAIL,
        "Biggoron: Broken Sword",
        item::PRESCRIPTION,
        &[get(flow::GET_IN_EVENT_NOT_TURN_FOR_POS_CAMERA_BIG_GORON)],
    ),
    trade(
        actor::NPC_ZORA_KING_ZORA_ADULT,
        flag::ZORA_DOMAIN,
        "King Zora: Prescription",
        item::EYEBALL_FROG,
        &[get(flow::GET_IN_EVENT_NOT_TURN_POS_CAMERA)],
    ),
    trade(
        actor::NPC_HYLIA_045_ADULT,
        flag::HYRULE_FIELD,
        "Lake Scientist: Eyeball Frog",
        item::EYE_DROP,
        &[HAND_OVER],
    ),
    trade(
        scene::DEATH_MT_TRAIL_0050,
        flag::DEATH_MOUNTAIN_TRAIL,
        "Biggoron: Eye Drops",
        item::CLAIM_CHECK,
        &[get(flow::GET_IN_EVENT_NOT_LOOKAT_OBJECT_TURN_OFF)],
    ),
    unmarked(
        BIGGORON,
        flag::DEATH_MOUNTAIN_TRAIL,
        "Biggoron",
        item::SWORD_BIGGORON,
        &[get(flow::GET_IN_EVENT_ITEM_SWORD_BIGGORON)],
    ),
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
