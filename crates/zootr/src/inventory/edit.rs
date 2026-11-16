//! Changing what Link has from the grid: a click on a cell, as the save
//! edits it makes. A tier held keeps the ones below it, as the game does
//! (the Fairy and Time Ocarinas both stay in the pouch), except along a
//! trade sequence, where Link holds one item at a time. A bag also names
//! itself as its group's cap, which is where the game reads the cap from.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use ounce_formats::{BymlDoc, Romfs, Save, murmur3};
use serde::{Deserialize, Serialize};

use super::{GRID, Inventory, Slot};
use crate::known::{Name, field};
use crate::rsdb::ROW_ID;
use crate::savefile::Edit;
use crate::standins::POUCH_ACTOR_INFO;

/// What a click on a cell asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Intent {
    /// The next tier, wrapping to none after the last.
    Up,
    /// The tier before, wrapping from none to the last.
    Down,
    /// This many of a counted item, kept within its cap.
    Count(i32),
}

/// Where a counted item's cap comes from.
#[derive(Clone, Debug, PartialEq)]
enum Cap {
    Fixed(i32),
    /// The bag the save names for this group.
    Bag(String),
}

/// What the pouch table says about counts: each bag's group and the cap it
/// gives, and where each counted item's cap comes from.
#[derive(Clone, Debug, Default)]
pub struct Pouch {
    bags: HashMap<String, (String, i32)>,
    caps: HashMap<String, Cap>,
}

impl Pouch {
    pub fn load(romfs: &Romfs) -> Result<Self> {
        let doc = BymlDoc::parse(&romfs.read(POUCH_ACTOR_INFO)?)?;
        let mut pouch = Self::default();
        for row in doc.root.as_array()? {
            let row = row.as_map()?;
            let text = |key: &str| {
                row.get(key)
                    .and_then(|v| v.as_string().ok())
                    .map(|s| s.to_string())
            };
            let int = |key: &str| {
                row.get(key)
                    .and_then(|v| v.as_int::<i64>().ok())
                    .map(|n| n as i32)
            };
            let Some(id) = text(ROW_ID) else { continue };
            let group = text(field::MAX_NUM_GAME_DATA.get());
            match (
                group,
                int(field::AFTER_EXPANSION_MAX_NUM.get()),
                int(field::FIXED_MAX_NUM.get()),
            ) {
                (Some(group), Some(cap), _) => {
                    pouch.bags.insert(id, (group, cap));
                }
                (_, _, Some(cap)) => {
                    pouch.caps.insert(id, Cap::Fixed(cap));
                }
                (Some(group), None, None) => {
                    pouch.caps.insert(id, Cap::Bag(group));
                }
                _ => {}
            }
        }
        Ok(pouch)
    }

    /// The save's String64 naming the bag a group's cap comes from.
    fn expansion(group: &str) -> u32 {
        murmur3(format!("CurrentExpansionActor_{group}").as_bytes())
    }

    /// The bag the save names for `group`, empty for none.
    fn bag_in<'a>(save: &'a Save, group: &str) -> Option<&'a str> {
        let e = save.get(Self::expansion(group))?;
        let text = save.element(e, 0)?;
        let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
        std::str::from_utf8(&text[..end]).ok()
    }

    /// How many of `actor` the save lets Link hold, for a counted item.
    pub fn cap(&self, actor: &str, save: &Save) -> Option<i32> {
        match self.caps.get(actor)? {
            Cap::Fixed(n) => Some(*n),
            Cap::Bag(group) => Some(self.group_cap(group, save)),
        }
    }

    /// The cap the bag `save` names for `group` gives: 0 with none.
    fn group_cap(&self, group: &str, save: &Save) -> i32 {
        Self::bag_in(save, group)
            .and_then(|bag| self.bags.get(bag))
            .map_or(0, |b| b.1)
    }

    /// The cap of the group `bag` belongs to, as `save` stands: what the
    /// wallet allows, given any wallet.
    pub fn cap_of_bag(&self, bag: &str, save: &Save) -> Option<i32> {
        let (group, _) = self.bags.get(bag)?;
        Some(self.group_cap(group, save))
    }
}

impl Inventory {
    /// Each cell's cap in [`Inventory::cells`]'s order: how many a counted
    /// cell can hold, `None` for the rest.
    pub fn caps(&self, pouch: &Pouch, save: &Save) -> Vec<Option<i32>> {
        let grid = GRID
            .iter()
            .map(|s| s.count.and_then(|a| pouch.cap(a.get(), save)));
        grid.chain(self.dungeons.iter().map(|_| None)).collect()
    }

    /// The values each cell reads, in [`Inventory::cells`]'s order.
    pub fn cell_hashes(&self) -> Vec<Vec<u32>> {
        let grid = GRID.iter().map(|s| {
            s.tiers
                .iter()
                .chain(&s.count)
                .filter_map(|a| self.keys.get(a).copied())
                .collect()
        });
        grid.chain(self.dungeons.iter().map(|k| k.key.into_iter().collect()))
            .collect()
    }

    /// The edits a click on cell `at`, in [`Inventory::cells`]'s order,
    /// makes to `save`, which the grid was last loaded from.
    pub fn edits(
        &self,
        pouch: &Pouch,
        save: &Save,
        at: usize,
        intent: Intent,
    ) -> Result<Vec<Edit>> {
        if let Some(slot) = GRID.get(at) {
            return self.slot_edits(pouch, save, slot, intent);
        }
        let kept = (self.dungeons.get(at - GRID.len())).context("no such cell")?;
        let key = kept.key.context("this dungeon has none")?;
        let now = self.values.get(&key).copied().unwrap_or(0);
        let next = match intent {
            Intent::Count(n) if kept.small => n.max(0),
            Intent::Count(_) => bail!("{} isn't counted", kept.label),
            _ => i32::from(now == 0),
        };
        Ok(vec![Edit::new(key, next.to_string())])
    }

    /// The tier `slot` is at: 0 for none.
    fn tier(&self, slot: &Slot) -> usize {
        (slot.tiers.iter())
            .rposition(|a| self.value(a) > 0)
            .map_or(0, |i| i + 1)
    }

    fn slot_edits(
        &self,
        pouch: &Pouch,
        save: &Save,
        slot: &Slot,
        intent: Intent,
    ) -> Result<Vec<Edit>> {
        let key = |a: &Name| self.keys[a];
        let n = slot.tiers.len();
        let next = match intent {
            Intent::Count(c) => {
                let actor = slot
                    .count
                    .with_context(|| format!("{} isn't counted", slot.label))?;
                let cap = pouch.cap(actor.get(), save).unwrap_or(i32::MAX);
                return Ok(vec![Edit::new(key(actor), c.clamp(0, cap).to_string())]);
            }
            Intent::Up => (self.tier(slot) + 1) % (n + 1),
            Intent::Down => (self.tier(slot) + n) % (n + 1),
        };
        let held = |i: usize| match slot.exclusive {
            true => i + 1 == next,
            false => i < next,
        };
        let mut out: Vec<Edit> = (slot.tiers.iter().enumerate())
            .filter(|&(i, a)| held(i) != (self.value(a) > 0))
            .map(|(i, a)| Edit::new(key(a), if held(i) { "1" } else { "0" }))
            .collect();

        // The highest bag held names the group's cap, and a count past the
        // new cap comes down to it.
        let bags = slot
            .tiers
            .iter()
            .enumerate()
            .filter_map(|(i, a)| Some((i, a, pouch.bags.get(a.get())?)));
        let mut groups: HashMap<&str, (&str, i32)> = HashMap::new();
        for (i, a, (group, cap)) in bags {
            let entry = groups.entry(group).or_insert(("", 0));
            if held(i) {
                *entry = (a.get(), *cap);
            }
        }
        for (&group, &(bag, cap)) in &groups {
            if Pouch::bag_in(save, group) != Some(bag) {
                out.push(Edit::new(Pouch::expansion(group), bag));
            }
            let capped = |a: &&Name| pouch.caps.get(a.get()) == Some(&Cap::Bag(group.into()));
            if let Some(actor) = slot.count.filter(capped)
                && self.value(actor) > cap
            {
                out.push(Edit::new(key(actor), cap.to_string()));
            }
        }

        // Taking ammo from none fills it, as finding it does. A count that
        // is the item itself, like a Gold Skulltula's, isn't ammo.
        if self.tier(slot) == 0
            && next > 0
            && let Some(actor) = slot.count
            && !slot.tiers.contains(&actor)
            && self.value(actor) == 0
        {
            out.extend(self.fill(pouch, save, actor, &groups));
        }
        Ok(out)
    }

    /// `actor` filled to its cap, with `held` the bag this click leaves
    /// each group at. With no bag, Link gets the group's smallest, as the
    /// Slingshot brings a Bullet Bag.
    fn fill(
        &self,
        pouch: &Pouch,
        save: &Save,
        actor: &Name,
        held: &HashMap<&str, (&str, i32)>,
    ) -> Vec<Edit> {
        let count = |n: i32| Edit::new(self.keys[actor], n.to_string());
        let group = match pouch.caps.get(actor.get()) {
            Some(Cap::Fixed(n)) => return vec![count(*n)],
            Some(Cap::Bag(group)) => group.as_str(),
            None => return Vec::new(),
        };
        let now = (held.get(group).map(|b| b.1)).unwrap_or_else(|| pouch.group_cap(group, save));
        if now > 0 {
            return vec![count(now)];
        }
        let smallest = (pouch.bags.iter())
            .filter(|(_, (g, _))| g == group)
            .min_by_key(|(_, (_, cap))| *cap);
        let Some((bag, &(_, cap))) = smallest else {
            return Vec::new();
        };
        let has = (self.keys.iter()).find(|(a, _)| a.get() == bag.as_str());
        let mut out: Vec<Edit> = has.map(|(_, &k)| Edit::new(k, "1")).into_iter().collect();
        out.push(Edit::new(Pouch::expansion(group), bag.as_str()));
        out.push(count(cap));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cell_at, inventory};
    use super::*;
    use crate::known::item;

    /// A save holding `scalars` and, as a String64, `bomb_bag` as the bomb
    /// group's bag.
    fn save_of(scalars: &[(&Name, u32)], bomb_bag: &str) -> Save {
        let mut pairs = vec![(0, 2)];
        pairs.extend(scalars.iter().map(|&(a, v)| (murmur3(a.as_bytes()), v)));
        pairs.push((0, 16));
        pairs.push((Pouch::expansion("FakeBomb"), 0));
        let mut b = Save::fixture(&pairs);
        let at = b.len() as u32;
        let pair = b.len() - 4;
        b[pair..].copy_from_slice(&at.to_le_bytes());
        let mut text = bomb_bag.as_bytes().to_vec();
        text.resize(64, 0);
        b.extend(text);
        Save::parse(b).unwrap()
    }

    fn pouch() -> Pouch {
        let bag = |a: &Name, cap| (a.to_string(), ("FakeBomb".to_string(), cap));
        let seed_bag = |a: &Name, cap| (a.to_string(), ("FakeSeed".to_string(), cap));
        Pouch {
            bags: [
                bag(item::BOMB_BAG_SMALL, 20),
                bag(item::BOMB_BAG_MEDIUM, 30),
                bag(item::BOMB_BAG_LARGE, 40),
                seed_bag(item::BULLET_BAG_SMALL, 30),
                seed_bag(item::BULLET_BAG_MEDIUM, 40),
            ]
            .into(),
            caps: [
                (item::BOMB.to_string(), Cap::Bag("FakeBomb".into())),
                (item::DEKU_SEED.to_string(), Cap::Bag("FakeSeed".into())),
                (item::TOKEN_GOLD.to_string(), Cap::Fixed(100)),
            ]
            .into(),
        }
    }

    /// Each edit as `(hash, value)`.
    fn pairs(edits: Vec<Edit>) -> Vec<(u32, String)> {
        edits.into_iter().map(|e| (e.hash, e.value)).collect()
    }

    fn h(a: &Name) -> u32 {
        murmur3(a.as_bytes())
    }

    #[test]
    fn up_holds_the_next_tier_and_keeps_the_ones_below() {
        needs_names!();
        let save = save_of(&[(item::HOOKSHOT, 1)], "");
        let mut inv = inventory();
        inv.load(crate::inventory::in_save(&save));
        let hookshot = cell_at(&inv.cells(), "Hookshot");
        let up = inv.edits(&pouch(), &save, hookshot, Intent::Up).unwrap();
        assert_eq!(pairs(up), [(h(item::LONGSHOT), "1".into())]);
        let down = inv.edits(&pouch(), &save, hookshot, Intent::Down).unwrap();
        assert_eq!(pairs(down), [(h(item::HOOKSHOT), "0".into())]);
    }

    #[test]
    fn up_from_the_last_tier_wraps_to_none() {
        needs_names!();
        let save = save_of(&[(item::HOOKSHOT, 1), (item::LONGSHOT, 1)], "");
        let mut inv = inventory();
        inv.load(crate::inventory::in_save(&save));
        let up = inv.edits(
            &pouch(),
            &save,
            cell_at(&inv.cells(), "Hookshot"),
            Intent::Up,
        );
        assert_eq!(
            pairs(up.unwrap()),
            [
                (h(item::HOOKSHOT), "0".into()),
                (h(item::LONGSHOT), "0".into())
            ]
        );
    }

    #[test]
    fn a_trade_item_is_held_alone() {
        needs_names!();
        let save = save_of(&[(item::WEIRD_EGG, 1)], "");
        let mut inv = inventory();
        inv.load(crate::inventory::in_save(&save));
        let up = inv.edits(
            &pouch(),
            &save,
            cell_at(&inv.cells(), "Child Trade"),
            Intent::Up,
        );
        assert_eq!(
            pairs(up.unwrap()),
            [
                (h(item::WEIRD_EGG), "0".into()),
                (h(item::ALARM_CUCCO), "1".into())
            ]
        );
    }

    #[test]
    fn a_smaller_bag_names_itself_and_brings_the_count_down() {
        needs_names!();
        let held = [
            (item::BOMB_BAG_SMALL, 1),
            (item::BOMB_BAG_MEDIUM, 1),
            (item::BOMB, 30),
        ];
        let save = save_of(&held, item::BOMB_BAG_MEDIUM.get());
        let mut inv = inventory();
        inv.load(crate::inventory::in_save(&save));
        let bombs = cell_at(&inv.cells(), "Bombs");
        assert_eq!(inv.caps(&pouch(), &save)[bombs], Some(30));
        let down = inv.edits(&pouch(), &save, bombs, Intent::Down).unwrap();
        assert_eq!(
            pairs(down),
            [
                (h(item::BOMB_BAG_MEDIUM), "0".into()),
                (
                    Pouch::expansion("FakeBomb"),
                    item::BOMB_BAG_SMALL.to_string()
                ),
                (h(item::BOMB), "20".into()),
            ]
        );
        let none = inv.edits(&pouch(), &save, bombs, Intent::Up).unwrap();
        assert!(pairs(none).contains(&(
            Pouch::expansion("FakeBomb"),
            item::BOMB_BAG_LARGE.to_string()
        )));
    }

    #[test]
    fn ammo_from_none_comes_full() {
        needs_names!();
        let save = save_of(&[], "");
        let mut inv = inventory();
        inv.load(crate::inventory::in_save(&save));
        let cells = inv.cells();
        let up = |label| {
            pairs(
                inv.edits(&pouch(), &save, cell_at(&cells, label), Intent::Up)
                    .unwrap(),
            )
        };
        let bombs = up("Bombs");
        assert!(bombs.contains(&(h(item::BOMB_BAG_SMALL), "1".into())));
        assert!(bombs.contains(&(h(item::BOMB), "20".into())));
        // No Bullet Bag yet: the Slingshot brings the smallest.
        let sling = up("Slingshot");
        assert!(sling.contains(&(h(item::BULLET_BAG_SMALL), "1".into())));
        assert!(sling.contains(&(
            Pouch::expansion("FakeSeed"),
            item::BULLET_BAG_SMALL.to_string()
        )));
        assert!(sling.contains(&(h(item::DEKU_SEED), "30".into())));
    }

    #[test]
    fn ammo_already_held_keeps_its_count() {
        needs_names!();
        let save = save_of(&[(item::DEKU_SEED, 7)], "");
        let mut inv = inventory();
        inv.load(crate::inventory::in_save(&save));
        let sling = cell_at(&inv.cells(), "Slingshot");
        let up = pairs(inv.edits(&pouch(), &save, sling, Intent::Up).unwrap());
        assert!(!up.iter().any(|(k, _)| *k == h(item::DEKU_SEED)), "{up:?}");
    }

    #[test]
    fn a_count_stays_within_its_cap() {
        needs_names!();
        let save = save_of(&[(item::BOMB_BAG_SMALL, 1)], item::BOMB_BAG_SMALL.get());
        let mut inv = inventory();
        inv.load(crate::inventory::in_save(&save));
        let cells = inv.cells();
        let at = |label| cell_at(&cells, label);
        let count = |at, n| pairs(inv.edits(&pouch(), &save, at, Intent::Count(n)).unwrap());
        assert_eq!(count(at("Bombs"), 99), [(h(item::BOMB), "20".into())]);
        assert_eq!(count(at("Bombs"), -3), [(h(item::BOMB), "0".into())]);
        assert_eq!(
            count(at("Gold Skulltulas"), 120),
            [(h(item::TOKEN_GOLD), "100".into())]
        );
        assert!(
            inv.edits(&pouch(), &save, at("Hookshot"), Intent::Count(1))
                .is_err()
        );
    }

    #[test]
    fn a_dungeon_cell_toggles_or_counts_its_keys() {
        needs_names!();
        let inv = crate::inventory::tests::inventory_of(&[(
            crate::known::flag::FOREST_TEMPLE,
            item::DUNGEON_MAP,
        )]);
        let save = save_of(&[], "");
        let cells = inv.cells();
        let map = cell_at(&cells, "Forest Temple Map");
        let keys = cell_at(&cells, "Forest Temple Small Keys");
        assert_eq!(
            pairs(inv.edits(&pouch(), &save, map, Intent::Up).unwrap())[0].1,
            "1"
        );
        assert!(inv.edits(&pouch(), &save, map, Intent::Count(2)).is_err());
        assert!(inv.edits(&pouch(), &save, keys, Intent::Up).is_err());
    }

    #[test]
    fn the_games_pouch_table_gives_bags_and_fixed_caps() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let pouch = Pouch::load(&romfs).unwrap();
        let bag = |a: &Name| pouch.bags.get(a.get()).map(|b| b.1);
        assert_eq!(
            [
                item::BOMB_BAG_SMALL,
                item::BOMB_BAG_MEDIUM,
                item::BOMB_BAG_LARGE
            ]
            .map(bag),
            [Some(20), Some(30), Some(40)]
        );
        assert_eq!(
            pouch.caps.get(item::TOKEN_GOLD.get()),
            Some(&Cap::Fixed(100))
        );
        let bomb_group = &pouch.bags[item::BOMB_BAG_SMALL.get()].0;
        assert_eq!(
            pouch.caps.get(item::BOMB.get()),
            Some(&Cap::Bag(bomb_group.clone()))
        );
        assert!(pouch.bags.contains_key(item::WALLET_LARGE.get()));
    }
}
