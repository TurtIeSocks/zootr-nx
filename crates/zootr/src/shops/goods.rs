//! What a shop slot brings to the pool once it's a check: its own goods,
//! as the pool holds them. Ammo comes at the same count where a chest form
//! has it, else the largest form under it (the smallest when none is). A
//! bottled good is a refill, which Ship keeps in shops: it brings a Red
//! Rupee.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{BymlDoc, Romfs};

use crate::known::{Name, actor, field, item};
use crate::standins::{POUCH_ACTOR_INFO, row_id};

/// What a bottled good brings.
pub const REFILL: &Name = actor::T_BOX_RUPEE_RED;

/// Goods that aren't a bundle of ammo, with what they bring.
const SINGLE: [(&Name, &Name); 7] = [
    (actor::SHOP_WEAPON_SHIELD_DEKU, actor::T_BOX_SHIELD_DEKU),
    (actor::SHOP_WEAPON_SHIELD_HYLIAN, actor::T_BOX_SHIELD_HYLIAN),
    (actor::SHOP_ARMOR_GORON, actor::OBJ_ARMOR_GORON),
    (actor::SHOP_ARMOR_ZORA, actor::OBJ_ARMOR_ZORA),
    (actor::SHOP_HEART, actor::OBJ_HEART),
    (actor::SHOP_MAGIC_JAR_SMALL, actor::OBJ_MAGIC_JAR_SMALL),
    (actor::SHOP_DEKU_STICK, actor::OBJ_DEKU_STICK),
];

/// Each ammo a good bundles (its `BundleActor`), with the pool's forms of
/// it by count, smallest first.
const AMMO: [(&Name, &[(i32, &Name)]); 6] = [
    (
        item::ARROW_NORMAL,
        &[
            (3, actor::T_BOX_ARROW_BUNDLE_SMALL),
            (5, actor::T_BOX_ARROW_BUNDLE_MEDIUM),
            (10, actor::T_BOX_ARROW_BUNDLE_LARGE),
        ],
    ),
    (
        item::BOMB,
        &[(1, actor::OBJ_BOMB), (5, item::BOMB_BUNDLE_SMALL)],
    ),
    (
        item::BOMBCHU,
        &[
            (5, item::BOMBCHU_BUNDLE_SMALL),
            (10, item::BOMBCHU_BUNDLE_MEDIUM),
            (20, item::BOMBCHU_BUNDLE_LARGE),
        ],
    ),
    (
        item::DEKU_NUT,
        &[(1, actor::OBJ_DEKU_NUT), (3, actor::T_BOX_DEKU_NUT_BUNDLE)],
    ),
    (
        item::DEKU_SEED,
        &[
            (3, actor::OBJ_DEKU_SEED),
            (5, actor::T_BOX_DEKU_SEED_BUNDLE),
        ],
    ),
    (
        item::DEKU_STICK,
        &[
            (1, actor::OBJ_DEKU_STICK),
            (3, item::DEKU_STICK_BUNDLE_SMALL),
        ],
    ),
];

/// How a param file names an actor.
const PARAM: (&str, &str) = ("Work/Actor/", ".engine__actor__ActorParam.gyml");

/// Each good's row in `PouchActorInfo`, as the pool reads it.
enum Row {
    /// A bundle: the actor and how many.
    Bundle(String, i32),
    /// A bottle's contents, sold to fill one (`BottleItem` set).
    Refill,
}

/// The goods' rows.
pub struct Bundles(HashMap<String, Row>);

impl Bundles {
    pub fn load(romfs: &Romfs) -> Result<Self> {
        let doc = BymlDoc::parse(&romfs.read(POUCH_ACTOR_INFO)?)?;
        let mut out = HashMap::new();
        for row in doc.root.as_array()? {
            let Ok(map) = row.as_map() else { continue };
            let actor = (map.get(field::BUNDLE_ACTOR.get()))
                .and_then(|a| a.as_string().ok())
                .and_then(|a| a.strip_prefix(PARAM.0)?.strip_suffix(PARAM.1));
            let num = (map.get(field::BUNDLE_ACTOR_NUM.get())).and_then(|n| n.as_int().ok());
            let refill = map.contains_key(field::BOTTLE_ITEM.get());
            let row_of = match (actor, num) {
                _ if refill => Row::Refill,
                (Some(actor), Some(num)) => Row::Bundle(actor.to_owned(), num),
                _ => continue,
            };
            out.insert(row_id(row).to_owned(), row_of);
        }
        Ok(Self(out))
    }

    /// What `goods` bring to the pool, and whether the count was rounded.
    pub fn pool_item(&self, goods: &str) -> Result<(&'static str, bool)> {
        if let Some((_, brings)) = SINGLE.iter().find(|(g, _)| *g == goods) {
            return Ok((brings.get(), false));
        }
        let (actor, num) = match self.0.get(goods) {
            Some(Row::Refill) => return Ok((REFILL.get(), false)),
            Some(Row::Bundle(actor, num)) => (actor, num),
            None => anyhow::bail!("{goods} is neither a bundle, a refill nor a single good"),
        };
        let (_, forms) = (AMMO.iter())
            .find(|(a, _)| *a == actor.as_str())
            .with_context(|| format!("{goods} bundles {actor}, which no pool item holds"))?;
        let under = forms.iter().rev().find(|(n, _)| n <= num);
        let &(n, form) = under.unwrap_or(&forms[0]);
        Ok((form.get(), n != *num))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shops::slots;

    /// Every slot's goods bring a pool item, and these had to round down
    /// (or up to the smallest form): no pool form holds their count.
    #[test]
    fn every_slot_s_goods_bring_a_pool_item() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let bundles = Bundles::load(&romfs).unwrap();
        let mut rounded = std::collections::BTreeSet::new();
        for slot in slots(&romfs).unwrap() {
            let (brings, round) = bundles.pool_item(&slot.goods).unwrap();
            if round {
                rounded.insert((slot.goods, brings));
            }
        }
        // Two arrow, three bomb, five bombchu, two nut and two seed packs,
        // and the medium stick pack (listed in the PR).
        assert_eq!(rounded.len(), 15, "{rounded:#?}");
    }

    #[test]
    fn goods_bring_their_own_items() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let b = Bundles::load(&romfs).unwrap();
        let brings = |g: &Name| b.pool_item(g).unwrap().0;
        assert_eq!(
            brings(actor::SHOP_ARMOR_GORON),
            actor::OBJ_ARMOR_GORON.get()
        );
        assert_eq!(
            brings(actor::SHOP_WEAPON_SHIELD_HYLIAN),
            actor::T_BOX_SHIELD_HYLIAN.get()
        );
        assert_eq!(brings(actor::SHOP_HEART), actor::OBJ_HEART.get());
        // Ten bombchus have a form of their own; one has none under it.
        assert_eq!(
            b.pool_item(actor::SHOP_BOMBCHU_BUNDLE_01).unwrap(),
            (item::BOMBCHU_BUNDLE_MEDIUM.get(), false)
        );
        assert_eq!(
            b.pool_item(actor::SHOP_BOMBCHU_BUNDLE_03).unwrap(),
            (item::BOMBCHU_BUNDLE_SMALL.get(), true)
        );
        let refills = (slots(&romfs).unwrap().into_iter())
            .filter(|s| b.pool_item(&s.goods).unwrap().0 == REFILL.get())
            .count();
        assert_eq!(
            refills, 22,
            "the potions, fairies, bugs, fish, Poes, Blue Fire and half milk"
        );
    }
}
