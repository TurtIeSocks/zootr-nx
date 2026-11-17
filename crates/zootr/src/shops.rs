//! Shopsanity: shelf slots as checks. Each check slot gets an actor of its
//! own, built from its vanilla goods' pack and the seed's item, that sells
//! the item once at a price rolled from the seed.

mod floors;
mod planks;

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs};

use crate::checks::{Drop, GYAML, ItemCheck, TRANSLATE, actors, bancs, hash_of, str_field, vec3};
use crate::gifts;
use crate::known::{Name, actor, field, flag, item, scene};
use crate::names::{ITEMS, name_label};
use crate::rng::Rng;
use crate::rules::Need;

use crate::settings::NewGame;
use crate::standins::{self, StandIn, Tables, item_pack};
use crate::text::{Copies, CopyLine};
pub(crate) use floors::{floor, stands};

/// A shop check's `file`, since its slot spans several map files.
pub const FILE: &str = "Shop";
const GOODS: &str = "Shop_";
/// Not goods: the mask shop lends, so it stays as it is, and boards are
/// the signs on the shelves.
const NOT_GOODS: [&str; 2] = ["Shop_Mask_", "Shop_Board_"];
/// Shopkeepers' actors start so. Those near a shop's slots with a shop
/// param are its keepers.
const KEEPER: &str = "Npc_";
/// Each check slot's actor is this plus the slot's number.
const NEW: &str = "Shop_Rando_";
/// Copies of one shelf spot sit this close.
const SAME_SLOT: f32 = 0.25;
/// A shop's slots sit this close to another of its slots.
const SAME_SHOP: f32 = 5.0;
/// What a check slot brings to the pool.
const RED: &Name = actor::T_BOX_RUPEE_RED;
/// Goods that are a vanilla logic source, with the item the source gives.
/// Their slots become checks last.
pub const SOURCES: [(&Name, &Name); 1] =
    [(actor::SHOP_WEAPON_SHIELD_DEKU, actor::T_BOX_SHIELD_DEKU)];
/// Every shop, in spoiler order.
pub const SHOPS: [&str; 8] = [
    "Kokiri Shop",
    "Market Bazaar",
    "Market Potion Shop",
    "Bombchu Shop",
    "Kakariko Bazaar",
    "Kakariko Potion Shop",
    "Goron Shop",
    "Zora Shop",
];
const TOWNS: [(&Name, &str); 5] = [
    (flag::KOKIRI_FOREST, "Kokiri"),
    (flag::MARKET, "Market"),
    (flag::KAKARIKO_VILLAGE, "Kakariko"),
    (flag::GORON_CITY, "Goron"),
    (flag::ZORA_DOMAIN, "Zora"),
];
pub const MEDIUM_WALLET: &Name = item::WALLET_MEDIUM;
pub const LARGE_WALLET: &Name = item::WALLET_LARGE;

pub struct Slot {
    pub area: String,
    pub shop: &'static str,
    /// From 1, in the order slots become checks.
    pub number: usize,
    /// The vanilla goods' actor.
    pub goods: String,
    /// Every placement, child maps first. The first is the check's id.
    pub placements: Vec<u64>,
    pos: [f32; 3],
    /// From 0, across every shop, for the new actor's name.
    index: usize,
    /// The slot's entry in its keepers' goods list.
    goods_index: i32,
    /// The actors near the slot's shop that may keep it, each age's.
    keepers: Vec<String>,
}

impl Slot {
    pub fn hash(&self) -> u64 {
        self.placements[0]
    }

    fn holder(&self) -> String {
        format!("{} {}", self.shop, self.number)
    }

    /// What the check adds to the pool.
    fn vanilla(&self) -> &'static str {
        source_of(&self.goods).unwrap_or(RED)
    }

    /// The new actor selling the seed's item here.
    fn actor(&self) -> String {
        format!("{NEW}{:02}", self.index)
    }
}

/// The item `goods` is the vanilla source of, if any.
fn source_of(goods: &str) -> Option<&'static str> {
    SOURCES
        .iter()
        .find(|(g, _)| *g == goods)
        .map(|(_, item)| item.get())
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d: f32 = (0..3).map(|i| (a[i] - b[i]).powi(2)).sum();
    d.sqrt()
}

/// The shop selling `goods` in `area`.
fn shop_name(area: &str, goods: &[&str]) -> Result<&'static str> {
    let (_, town) = TOWNS
        .iter()
        .find(|(a, _)| *a == area)
        .with_context(|| format!("a shop in {area}, which has none"))?;
    let all = |prefix: &str| goods.iter().all(|g| g.starts_with(prefix));
    let name = match () {
        _ if all(actor::SHOP_BOMBCHU_BUNDLE_01.stem()) => "Bombchu Shop".to_owned(),
        _ if all("Shop_Bottle_") => format!("{town} Potion Shop"),
        _ if goods.iter().any(|g| *g == actor::SHOP_WEAPON_SHIELD_HYLIAN) => {
            format!("{town} Bazaar")
        }
        _ => format!("{town} Shop"),
    };
    SHOPS
        .into_iter()
        .find(|s| *s == name)
        .with_context(|| format!("{name} isn't a shop"))
}

/// Every shelf slot outside the mask shop, by shop in `SHOPS` order, each
/// shop's in the order they become checks.
pub fn slots(romfs: &Romfs) -> Result<Vec<Slot>> {
    let mut found = Vec::new();
    let mut npcs = Vec::new();
    for (file, doc) in bancs(romfs)? {
        let doc = doc?;
        for actor in actors(&doc.root)? {
            let goods = str_field(actor, GYAML).unwrap_or_default();
            let area = file.split('/').nth(1).unwrap_or("?").to_owned();
            if goods.starts_with(KEEPER) {
                if let Some(pos) = vec3(actor, "Translate") {
                    npcs.push((area, goods.to_owned(), pos));
                }
                continue;
            }
            if !goods.starts_with(GOODS) || NOT_GOODS.iter().any(|n| goods.starts_with(n)) {
                continue;
            }
            let hash = hash_of(actor).context("a shelf actor without Hash")?;
            let pos = vec3(actor, "Translate").context("a shelf actor without Translate")?;
            let index = goods_index(actor);
            found.push((
                !file.contains("_Child_"),
                area,
                goods.to_owned(),
                hash,
                pos,
                index,
            ));
        }
    }
    found.sort_by_key(|f| f.0);
    let mut slots: Vec<Slot> = Vec::new();
    for (_, area, goods, hash, pos, goods_index) in found {
        let same =
            |s: &&mut Slot| s.area == area && s.goods == goods && dist(s.pos, pos) < SAME_SLOT;
        match slots.iter_mut().find(same) {
            Some(slot) => slot.placements.push(hash),
            None => slots.push(Slot {
                area,
                shop: "",
                number: 0,
                goods,
                placements: vec![hash],
                pos,
                index: 0,
                goods_index,
                keepers: Vec::new(),
            }),
        }
    }
    let mut groups: Vec<Vec<Slot>> = Vec::new();
    for slot in slots {
        let near = |g: &Vec<Slot>| {
            g.iter()
                .any(|s| s.area == slot.area && dist(s.pos, slot.pos) < SAME_SHOP)
        };
        let (mut joined, rest): (Vec<_>, Vec<_>) = groups.into_iter().partition(near);
        let mut group: Vec<Slot> = joined.drain(..).flatten().collect();
        group.push(slot);
        groups = rest;
        groups.push(group);
    }
    let mut out = Vec::new();
    for mut group in groups {
        let goods: Vec<&str> = group.iter().map(|s| s.goods.as_str()).collect();
        let shop = shop_name(&group[0].area, &goods)?;
        let mut keepers: Vec<String> = (npcs.iter())
            .filter(|(area, _, pos)| {
                (group.iter()).any(|s| s.area == *area && dist(s.pos, *pos) < SAME_SHOP)
            })
            .map(|(_, name, _)| name.clone())
            .collect();
        keepers.sort();
        keepers.dedup();
        group.sort_by(|a, b| {
            let key = |s: &Slot| (source_of(&s.goods).is_some(), s.pos);
            let (ka, kb) = (key(a), key(b));
            ka.0.cmp(&kb.0).then_with(|| {
                (0..3)
                    .map(|i| ka.1[i].total_cmp(&kb.1[i]))
                    .find(|o| o.is_ne())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        for (i, slot) in group.iter_mut().enumerate() {
            slot.shop = shop;
            slot.number = i + 1;
            slot.keepers.clone_from(&keepers);
        }
        out.extend(group);
    }
    out.sort_by_key(|s| (SHOPS.iter().position(|n| *n == s.shop), s.number));
    for (i, slot) in out.iter_mut().enumerate() {
        slot.index = i;
    }
    Ok(out)
}

/// A shelf placement's entry in its keeper's goods list. The first
/// entry's placements leave it out.
fn goods_index(actor: &Byml) -> i32 {
    let index = || {
        let dynamic = actor.as_map().ok()?.get(field::DYNAMIC.get())?;
        let dynamic = dynamic.as_map().ok()?;
        dynamic.get(field::SHOP_ITEM_INDEX.get())?.as_int().ok()
    };
    index().unwrap_or(0)
}

/// The first `count` slots of each shop, as checks.
pub fn checks(slots: &[Slot], count: usize) -> Vec<ItemCheck> {
    slots
        .iter()
        .filter(|s| s.number <= count)
        .map(|s| gifts::check(FILE.into(), &s.area, s.hash(), s.holder(), s.vanilla()))
        .collect()
}

pub fn is_shop(check: &ItemCheck) -> bool {
    check.file == FILE
}

/// Whether `holder` names a shop slot, like `Goron Shop 3`.
pub fn is_holder(holder: &str) -> bool {
    SHOPS.iter().any(|s| {
        holder
            .strip_prefix(s)
            .and_then(|n| n.strip_prefix(' '))
            .is_some_and(|n| n.parse::<usize>().is_ok())
    })
}

/// A slot's price: steps of 5 from 5 to 300, most of them cheap.
pub fn price(seed: u64, hash: u64) -> i32 {
    let x = Rng::new(seed ^ hash).next_u64();
    let u = (x >> 11) as f64 / (1u64 << 53) as f64;
    (5 * (1 + (60.0 * u * u * u) as i32)).min(300)
}

/// The rupees each Great Fairy asks before her gift, by her scene's flow:
/// its `EventQueryHasRuppe`.
const OFFERINGS: [(&Name, i32); 6] = [
    (scene::FAIRY_FOUNTAIN_POWER, 50),
    (scene::FAIRY_FOUNTAIN_DIN, 25),
    (scene::FAIRY_FOUNTAIN_FRORE, 100),
    (scene::FAIRY_FOUNTAIN_WISDOM, 200),
    (scene::FAIRY_FOUNTAIN_NAYRU, 300),
    (scene::FAIRY_FOUNTAIN_COURAGE, 500),
];

/// The offering the gift with check `hash` asks, if it's a Great Fairy's.
pub fn offering(hash: u64) -> Option<i32> {
    let gift = gifts::GIFTS.iter().find(|g| g.hash() == hash)?;
    let &(_, rupees) = OFFERINGS.iter().find(|(flow, _)| *flow == gift.flow)?;
    Some(rupees)
}

/// The wallet a price needs: the Kokiri Wallet holds 200, the Adult's
/// 500 and the Giant's 1,000 (their captions, and 300 rupees given from 0
/// leave 200 in the Kokiri Wallet).
pub fn wallet(price: i32, progressive: bool) -> Need {
    let items = |actors: &[&str], count| Need::Items {
        actors: actors.iter().map(|a| (*a).to_owned()).collect(),
        count,
    };
    match price {
        ..=200 => Need::Always,
        201..=500 => items(&[MEDIUM_WALLET, LARGE_WALLET], 1),
        _ if progressive => items(&[MEDIUM_WALLET, LARGE_WALLET], 2),
        _ => items(&[LARGE_WALLET], 1),
    }
}

/// Build each check slot's actor and its RSDB rows, and return the
/// placement fields, as `HASH.PATH=VALUE`, that shelve it.
/// The shop text under an item's name, which only the game's own goods
/// have: `<stem>_ShopCaption`, beside the `<stem>_Name` naming the item.
const CAPTION: &str = "_ShopCaption";
/// Where a stand-in's caption is copied from: its get window's, which says
/// what it does as the goods' do, else its menu's (maintainer, 2026-11-15).
/// Songs have only the first.
const CAPTION_SOURCES: [&str; 2] = ["_GetCaption", "_Caption"];

/// The items the first `count` slots of each shop sell, by slot.
fn sold<'a>(
    slots: &'a [Slot],
    count: usize,
    drops: &'a HashMap<u64, Drop>,
) -> impl Iterator<Item = (&'a Slot, &'a str)> {
    let open = slots.iter().filter(move |s| s.number <= count);
    open.map(|s| (s, gifts::item_at(drops, s.hash(), s.vanilla())))
}

/// A caption for each item the slots sell, copied from its own text.
pub fn captions(
    romfs: &Romfs,
    slots: &[Slot],
    count: usize,
    drops: &HashMap<u64, Drop>,
) -> Result<Copies<'static>> {
    let mut new = Vec::new();
    for (_, item) in sold(slots, count, drops) {
        let Some(label) = name_label(romfs, item)? else {
            continue;
        };
        let stem = label.strip_suffix("_Name").context("a name label")?;
        let caption = format!("{stem}{CAPTION}");
        if !new.iter().any(|l: &CopyLine| l.label == caption) {
            let sources = CAPTION_SOURCES.map(|s| format!("{stem}{s}")).to_vec();
            new.push(CopyLine::plain(caption, sources));
        }
    }
    Ok(Copies {
        file: ITEMS.strip_suffix(".msbt").context("a message file")?,
        new,
    })
}

pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    slots: &[Slot],
    drops: &HashMap<u64, Drop>,
    new_game: &NewGame,
) -> Result<Vec<String>> {
    let mut fields = Vec::new();
    let sold: Vec<_> = sold(slots, new_game.shop_slots, drops).collect();
    stock_once(romfs, builder, sold.iter().map(|(slot, _)| *slot))?;
    for (slot, item) in sold {
        let name = slot.actor();
        let sale = StandIn {
            name: &name,
            template: ActorPack::load(romfs, &slot.goods)?,
            item: item_pack(romfs, item, new_game)?,
            price: Some(price(new_game.seed, slot.hash())),
        };
        sale.build(builder, tables)
            .with_context(|| format!("{} selling {item}", slot.holder()))?;
        let plank = planks::plank(slot.shop, slot.number)
            .with_context(|| format!("no plank measured for {}", slot.holder()))?;
        let rise = standins::rise(tables, plank, item)?;
        for h in &slot.placements {
            fields.push(format!("{h}.{GYAML}={name}"));
            if rise != 0.0 {
                fields.push(format!("{h}.{TRANSLATE}.1+={rise}"));
            }
        }
    }
    Ok(fields)
}

/// Give each of `slots` a stock of one in its keepers' goods lists, so the
/// game sells it once, then shows it sold out. Every vanilla slot's stock
/// is endless.
fn stock_once<'a>(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    slots: impl Iterator<Item = &'a Slot>,
) -> Result<()> {
    let mut by_keeper: HashMap<&str, Vec<(i32, &str)>> = HashMap::new();
    for slot in slots {
        let index = slot.goods_index;
        for keeper in &slot.keepers {
            (by_keeper.entry(keeper))
                .or_default()
                .push((index, &slot.goods));
        }
    }
    let mut keepers: Vec<_> = by_keeper.into_iter().collect();
    keepers.sort();
    for (keeper, goods) in keepers {
        let pack = ActorPack::load(romfs, keeper)?;
        let suffix = format!(".game__component__{}.bgyml", field::SHOP_PARAM);
        for param in pack.files().into_iter().filter(|f| f.ends_with(&suffix)) {
            builder
                .edit_param(&[keeper], &param, |data| limited(data, &goods))
                .with_context(|| format!("{keeper}'s goods"))?;
        }
    }
    Ok(())
}

/// A shop param with each of `goods` (index, vanilla actor) stocked once.
/// An entry selling other goods at that index is left alone: that keeper
/// doesn't keep the slot.
fn limited(data: &[u8], goods: &[(i32, &str)]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(data)?;
    let root = doc.root.as_mut_map()?;
    let list = (root.get_mut(field::GOODS_LIST.get()))
        .context("a shop param without goods")?
        .as_mut_array()?;
    for &(index, actor) in goods {
        let Some(entry) = usize::try_from(index).ok().and_then(|i| list.get_mut(i)) else {
            continue;
        };
        let entry = entry.as_mut_map()?;
        let sells = (entry
            .get(field::ACTOR.get())
            .and_then(|a| a.as_string().ok()))
        .is_some_and(|a| a.ends_with(&format!("/{actor}.engine__actor__ActorParam.gyml")));
        if sells {
            entry.insert(field::STOCK_NUM.to_string().into(), Byml::I32(1));
        }
    }
    Ok(doc.to_binary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{actor, field, flag, item, scene};
    use crate::settings::DungeonKeys;

    #[test]
    fn a_shield_stands_up_on_a_shelf_and_the_rupee_lies_as_its_vertices_say() {
        needs_names!();
        let at = |item: &Name| floor(item).unwrap();
        // Posed in their own shop poses, they stand on their rims.
        assert!((at(actor::T_BOX_SHIELD_DEKU) - 0.022).abs() < 0.001);
        assert!((at(actor::T_BOX_SHIELD_HYLIAN) - 0.004).abs() < 0.001);
        // The token's pose leaves the rupee centred on its origin.
        assert!((at(actor::T_BOX_RUPEE_RED) + 0.191).abs() < 0.001);
        assert_eq!(floor("FakeItem_Nothing"), None);
    }
    use crate::standins::*;
    use ounce_formats::{Byml, BymlDoc, BymlMap, pack_path};
    use roead::sarc::Sarc;

    #[test]
    fn sixty_four_slots_in_eight_shops() {
        let Some(romfs) = dev_dump() else { return };
        let slots = slots(&romfs).unwrap();
        assert_eq!(slots.len(), 64);
        for shop in SHOPS {
            let numbers: Vec<_> = slots
                .iter()
                .filter(|s| s.shop == shop)
                .map(|s| s.number)
                .collect();
            assert_eq!(numbers, (1..=8).collect::<Vec<_>>(), "{shop}");
        }
        let shield = slots.iter().find(|s| s.goods == SOURCES[0].0).unwrap();
        assert_eq!((shield.shop, shield.number), ("Kokiri Shop", 8));
        assert!(slots.iter().all(|s| !s.goods.starts_with(NOT_GOODS[0])));
        let kokiri = slots.iter().filter(|s| s.area == flag::KOKIRI_FOREST);
        assert_eq!(kokiri.map(|s| s.placements.len()).sum::<usize>(), 24);
    }

    #[test]
    fn every_slot_is_kept_and_a_check_slot_sells_once() {
        let Some(romfs) = dev_dump() else { return };
        let suffix = format!(".game__component__{}.bgyml", field::SHOP_PARAM);
        let params = |keeper: &str| {
            let pack = ActorPack::load(&romfs, keeper).unwrap();
            (pack.files().into_iter())
                .filter(|f| f.ends_with(&suffix))
                .map(|f| pack.raw(&f).unwrap().to_vec())
                .collect::<Vec<_>>()
        };
        let stock = |data: &[u8], index: i32| {
            let doc = BymlDoc::parse(data).unwrap();
            let list = doc.root.as_map().unwrap()[field::GOODS_LIST.get()].clone();
            let entry = list.as_array().unwrap()[index as usize].clone();
            let entry = entry.as_map().unwrap().clone();
            let actor = entry[field::ACTOR.get()].as_string().unwrap().to_string();
            let stock = entry.get(field::STOCK_NUM.get());
            (actor, stock.map(|s| s.as_int::<i32>().unwrap()))
        };
        for slot in slots(&romfs).unwrap() {
            let index = slot.goods_index;
            let goods = [(index, slot.goods.as_str())];
            let mut kept = 0;
            for data in slot.keepers.iter().flat_map(|k| params(k)) {
                let (actor, before) = stock(&data, index);
                let (_, after) = stock(&limited(&data, &goods).unwrap(), index);
                if actor.ends_with(&format!("/{}.engine__actor__ActorParam.gyml", slot.goods)) {
                    kept += 1;
                    // Mostly endless. The keeper before the Deku Tree sells some
                    // goods once already, and leaves the shield's out.
                    assert!(matches!(before, None | Some(-1 | 1)), "{}", slot.holder());
                    assert_eq!(after, Some(1), "{}", slot.holder());
                } else {
                    assert_eq!(after, before, "{}", slot.holder());
                }
            }
            assert!(kept > 0, "{} has no keeper", slot.holder());
        }
    }

    #[test]
    fn the_setting_picks_how_many_per_shop() {
        let Some(romfs) = dev_dump() else { return };
        let slots = slots(&romfs).unwrap();
        assert_eq!(checks(&slots, 0).len(), 0);
        assert_eq!(checks(&slots, 4).len(), 32);
        let all = checks(&slots, 8);
        assert_eq!(all.len(), 64);
        assert!(all.iter().all(|c| is_shop(c) && is_holder(&c.holder)));
        assert_eq!(all.iter().filter(|c| c.item == SOURCES[0].1).count(), 1);
    }

    #[test]
    fn holders_name_a_shop_and_a_number() {
        assert!(is_holder("Goron Shop 3"));
        assert!(is_holder("Kakariko Potion Shop 8"));
        assert!(!is_holder("Goron Shop"));
        assert!(!is_holder("Business Scrub"));
    }

    #[test]
    fn prices_are_cheap_steps_of_five() {
        let prices: Vec<i32> = (0..2000).map(|h| price(7, h)).collect();
        assert!(prices.iter().all(|p| (5..=300).contains(p) && p % 5 == 0));
        let cheap = prices.iter().filter(|p| **p <= 99).count();
        assert!(cheap > prices.len() * 3 / 5, "{cheap}");
        assert!(prices.iter().any(|p| *p > 200));
        assert_eq!(price(7, 42), price(7, 42));
        assert_ne!(
            (0..20).map(|h| price(7, h)).collect::<Vec<_>>(),
            (0..20).map(|h| price(8, h)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn dear_prices_need_a_wallet() {
        assert_eq!(wallet(200, true), Need::Always);
        let need = |n: &Need| match n {
            Need::Items { actors, count } => (actors.len(), *count),
            _ => (0, 0),
        };
        assert_eq!(need(&wallet(201, true)), (2, 1));
        assert_eq!(need(&wallet(500, false)), (2, 1));
        assert_eq!(need(&wallet(501, true)), (2, 2));
        assert_eq!(need(&wallet(501, false)), (1, 1));
    }

    #[test]
    fn the_dearest_great_fairies_need_a_wallet() {
        let fairy = |flow| gifts::GIFTS.iter().find(|g| g.flow == flow).unwrap().hash();
        assert_eq!(offering(fairy(scene::FAIRY_FOUNTAIN_DIN)), Some(25));
        assert_eq!(offering(fairy(scene::FAIRY_FOUNTAIN_NAYRU)), Some(300));
        assert_eq!(offering(fairy(scene::KOKIRI_0155)), None);
    }

    #[test]
    fn each_offering_is_what_its_fairy_asks() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for (flow, rupees) in OFFERINGS {
            let f = crate::flows::load(&romfs, flow).unwrap().unwrap();
            let asks = format!("{}(Value={rupees})", field::EVENT_QUERY_HAS_RUPPE);
            let lines = crate::flows::render(&f);
            let all: Vec<_> = lines.lines().filter(|l| l.contains("HasRuppe")).collect();
            assert!(all.len() == 1 && all[0].contains(&asks), "{flow}: {all:?}");
        }
    }

    #[test]
    fn a_slot_captions_its_item_from_the_get_window_then_the_menu() {
        let Some(romfs) = dev_dump() else { return };
        let slots = slots(&romfs).unwrap();
        let goron = slots.iter().find(|s| s.shop == "Goron Shop").unwrap();
        let drops = HashMap::from([(goron.hash(), Drop::from(item::HOOKSHOT.to_string()))]);
        let copies = captions(&romfs, &slots, 1, &drops).unwrap();
        assert_eq!(copies.file, format!("{}/PouchContent", field::ACTOR_MSG));
        let line = (copies.new.iter())
            .find(|l| l.label == format!("{}_ShopCaption", item::HOOKSHOT))
            .unwrap();
        assert_eq!(
            line.sources,
            &[
                format!("{}_GetCaption", item::HOOKSHOT),
                format!("{}_Caption", item::HOOKSHOT)
            ]
        );
        let labels = copies.new.iter().map(|l| &l.label);
        assert!(labels.clone().all(|l| l.ends_with("_ShopCaption")));
        let unique: std::collections::HashSet<_> = labels.clone().collect();
        assert_eq!(unique.len(), labels.count());
    }

    #[test]
    fn a_slot_sells_the_seed_item_from_a_pack_of_its_own() {
        let Some(romfs) = dev_dump() else { return };
        let slots = slots(&romfs).unwrap();
        let goron = slots.iter().find(|s| s.shop == "Goron Shop").unwrap();
        let drops = HashMap::from([(goron.hash(), Drop::from(item::HOOKSHOT.to_string()))]);
        let dir = std::env::temp_dir().join(format!("zootr-shops-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let fields = write(
            &romfs,
            &mut builder,
            &mut tables,
            &slots,
            &drops,
            &NewGame {
                shop_slots: 1,
                seed: 7,
                keys: DungeonKeys::Rings,
                ..NewGame::default()
            },
        )
        .unwrap();
        tables.write(&mut builder).unwrap();
        builder.finish().unwrap();

        let name = goron.actor();
        let placed = |s: &Slot| (s.number == 1) as usize * s.placements.len();
        let placed: usize = slots.iter().map(placed).sum();
        assert!(fields.contains(&format!("{}.Gyaml={name}", goron.hash())));
        // The Hookshot reaches 0.15 below its origin, so it rises that much
        // past the slot's plank.
        let raised = format!("{}.Translate.1+=", goron.hash());
        let rise: f32 = (fields.iter())
            .find_map(|f| f.strip_prefix(&raised))
            .unwrap()
            .parse()
            .unwrap();
        let plank = planks::plank(goron.shop, goron.number).unwrap();
        assert!((plank + 0.1..plank + 0.2).contains(&rise), "{rise}");
        let rises = fields.iter().filter(|f| f.contains("+=")).count();
        assert_eq!(fields.len() - rises, placed);

        let rel = format!("Pack/Actor/{name}.pack.zs");
        let raw = std::fs::read(dir.join("romfs").join(&rel)).unwrap();
        let data = romfs.dicts().decompress(&rel, &raw).unwrap();
        let sarc = Sarc::new(data).unwrap();
        let param = |path: &str| -> BymlMap {
            let pack_file = pack_path(path);
            let data = sarc
                .get_data(&pack_file)
                .unwrap_or_else(|| panic!("no {pack_file}"));
            Byml::from_binary(data).unwrap().as_map().unwrap().clone()
        };
        let actor = param(&actor_path(&name));
        let refs = |key: &str| component(&actor, key).unwrap();
        assert_eq!(
            refs(MODEL),
            format!(
                "?Component/{}/{}.engine__component__ModelInfo.bgyml",
                field::MODEL_INFO,
                item::HOOKSHOT
            )
        );
        assert_eq!(refs(RESPAWN), NO_RESPAWN);
        param(&refs(MODEL));
        // It spins when selected, in the token's pose.
        let animations = param(&refs(ANIMATION));
        let archives: Vec<String> = animations[field::ANIMATION_RESOURCES.get()]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                a.as_map().unwrap()[field::MODEL_PROJECT_NAME.get()]
                    .as_string()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            archives,
            [item::COMMON_ANIMATION, actor::OBJ_TOKEN_GOLD_ANIMATION]
        );
        let pouch = param(&refs(POUCH));
        assert_eq!(
            pouch
                .get(POUCH_ACTOR.get())
                .unwrap()
                .as_string()
                .unwrap()
                .as_str(),
            format!(
                "Work/Actor/{}.engine__actor__ActorParam.gyml",
                item::HOOKSHOT
            )
        );
        let table = param(&refs(TABLE));
        let parent = table.get(PARENT).unwrap().as_string().unwrap();
        assert_eq!(
            parent.as_str(),
            format!(
                "Work/{}/{}/{}.engine__actor__GameParameterTable.gyml",
                field::GAME_PARAMETER,
                field::GAME_PARAMETER_TABLE,
                item::HOOKSHOT
            )
        );
        let parts: Vec<&str> = table[COMPONENTS]
            .as_map()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(parts.len(), 2, "flat: {parts:?}");
        assert!(
            parts
                .iter()
                .all(|p| [PRICE, PUT_IN_POUCH].iter().any(|n| p == n)),
            "{parts:?}"
        );
        param(&format!(
            "?{}",
            parent
                .strip_prefix("Work/")
                .unwrap()
                .replace(".gyml", ".bgyml")
        ));
        let price_param = param(&component(&table, PRICE).unwrap());
        assert_eq!(
            price_param.get(BUYING.get()),
            Some(&Byml::I32(price(7, goron.hash())))
        );
        assert!(component(&table, PUT_IN_POUCH).is_some());

        let restbl = ounce_formats::Restbl::parse(
            &romfs
                .dicts()
                .decompress(
                    &romfs.restbl_rel().unwrap(),
                    &std::fs::read(dir.join("romfs").join(romfs.restbl_rel().unwrap())).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        assert!(restbl.get(&rel).is_some());
        assert!(restbl.get(&pack_path(&refs(POUCH))).is_some());

        for table in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_ACTOR_INFO] {
            let raw = std::fs::read(dir.join("romfs").join(table)).unwrap();
            let doc = BymlDoc::parse(&romfs.dicts().decompress(table, &raw).unwrap()).unwrap();
            let ids: Vec<&str> = doc.root.as_array().unwrap().iter().map(row_id).collect();
            assert!(ids.windows(2).all(|w| w[0] < w[1]), "{table} out of order");
            assert_eq!(
                ids.iter().filter(|i| i.starts_with(NEW)).count(),
                8,
                "{table}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
