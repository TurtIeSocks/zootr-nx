//! Business scrubs as checks: each sells the seed's item, once.
//!
//! The spots and what they sell live in `spots`, the placement fields and
//! seller lines a seed writes in `placements`, and the edits to the shared
//! flow in `events` and `sales`.
use std::sync::LazyLock;

use crate::known::{Name, actor, field, flag, flow, scene};

pub const FLOW: &Name = flow::ENEMY_SCRUB_BUSINESS_COMMON;
pub const HOLDER: &str = "Business Scrub";
/// The placed actor every Business Scrub check is.
pub const ACTOR: &Name = actor::ENEMY_SCRUB_BUSINESS;
/// Copies the placement's items into `TEMP`, then dispatches on it.
const ROOT: &Name = flow::SIMPLE_SHOP_ROOT_ENEMY_SCRUB_BUSINESS_POUCH;
const SECRET: &str = "Secret";
/// Takes the rupees, grants `ActorName` and sends the scrub off.
const ONE_TIME: &Name = flow::SIMPLE_SHOP_SUB_SCRUB_BUSINESS_ONE_TIME;
/// `ONE_TIME` with the get event swapped for the plain grant.
const PLAIN_ONE_TIME: &str = "Rando_ScrubPlainOneTime";
/// The vanilla get event, which shows the item in Link's hands.
const SHOW: &Name = flow::GET_IN_EVENT_NOT_TURN_2_D_SHOP;
const SHOW_FLOW: &Name = scene::SY_GET_IN_EVENT;
/// The plain grant with a get window, by item name.
const GRANT: (&Name, &Name) = (
    flow::GET_POUCH_CONTENT,
    flow::GET_POUCH_CONTENT_BY_ACTOR_NAME,
);
const PAY: (&Name, &Name) = (
    field::POUCH_SYSTEM_ACTOR,
    flow::EVENT_TRIGGER_ADD_GAME_DATA_INT_TO_RUPEE,
);
const WAIT_PAID: (&Name, &Name) = (field::UI_SYSTEM_ACTOR, flow::EVENT_WAIT_STABLE_NUM_DISPLAY);
const PRICE: &Name = flag::SIMPLE_SHOP_PRICE;
/// The sale call's price, which `PRICE` takes and the seller line shows.
const PRICE_PARAM: &str = "Price";
/// Set by a sale Link paid for, and asked after the pouch root returns.
const BOUGHT: &Name = flag::SCRUB_BUSINESS_BUY_SUCCESS;
/// A one-time talk: the pouch root, then whether Link paid, then its sold
/// flag. Each spot's entry copies the last two.
const SOLD_LIKE: &Name = flag::ENEMY_SCRUB_BUSINESS_DEKU_STICK_POUCH_01;
/// What the scrub's talk starts with, to find the talk's `TEMP` write.
const WRITE_ITEM: (&Name, &Name) = (
    ACTOR,
    flow::EVENT_SCRUB_BUSINESS_WRITE_GAME_DATA_BANC_ITEM_NAME,
);
const COMPARE: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_QUERY_IS_GAME_DATA_STRING64,
);
const SET_STRING: (&Name, &Name) = (
    field::GAME_DATA_ACTOR,
    flow::EVENT_TRIGGER_SET_GAME_DATA_STRING64,
);
const TEMP: &Name = flag::SCRUB_BUSINESS_TEMP_ITEM_NAME_00;
const SELECTED: &Name = flag::SHOP_SELECT_ITEM_NAME;
const HEART: &Name = actor::OBJ_HEART_PIECE;
/// The vanilla chain's first compare.
const HEAD: &Name = HEART;
const ITEM: &Name = field::SCRUB_BUSINESS_SHOP_ITEM_ACTOR_NAME0;
const TALK: &Name = field::TALK_EVENT_NAME;
const NAME: &Name = flag::ACTOR_NAME;
const ATTACKED: &Name = field::ATTACKED_EVENT_NAME;
const SELLER: &Name = flow::SELLER_MESSAGE_ID;
/// The message file holding the seller lines.
static TEXT: LazyLock<String> =
    LazyLock::new(|| format!("{}/{}", field::EVENT_FLOW_MSG, field::ENEMY_EVENT));
/// The vanilla line naming the heart piece and its price, which each new
/// line copies.
const PRICED: &Name = actor::ENEMY_SCRUB_BUSINESS_HEART_01;
/// A seller line naming no item, copied where the priced line can't be.
const NEUTRAL: &Name = actor::ENEMY_SCRUB_BUSINESS_GREETING_06;
/// A pouch scrub's talk, whose event rows each new talk copies: it calls
/// every flow a sale does.
const LIKE: &Name = actor::ENEMY_SCRUB_BUSINESS_BOMB_01;

mod events;
mod placements;
mod sales;
mod spots;
#[cfg(test)]
mod tests;

pub use placements::{events, fields, flags, gone, lines};
pub(crate) use sales::{Level, write};
pub use spots::{SCRUBS, checks, of};
