//! Each check's places in the world, and what it needs at each.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail, ensure};

use super::{Build, Location, Place, STUN_SCRUB, eras_of, places_of, start_of, switches};
use crate::checks::ItemCheck;
use crate::rules::{CheckEntry, Need};
use crate::tools::Tools;
use crate::world::node_of;
use crate::{gifts, keys, pickups, scrubs, shops};

impl Build<'_> {
    /// Every check at each of its places, with what it needs there. Fails
    /// on a check with no place, and on a rule naming a check none has.
    pub(super) fn locations(
        &self,
        checks: &[ItemCheck],
        tools: &mut Tools,
    ) -> Result<Vec<Location>> {
        let ctx = self.ctx;
        let pocket = gifts::pocket(&ctx.on);
        // Each obstacle's need, resolved once.
        let mut by_tool = HashMap::new();
        let mut locations = Vec::new();
        let mut seen = HashSet::new();
        for check in checks {
            if !seen.insert((check.hash, check.file.as_str())) {
                continue;
            }
            let rule = self.book.check(check.hash);
            let pocketed = pocket.contains(&check.hash);
            let presence = match &check.presence {
                Some(p) if !p.negated => Need::Flag(p.flag.clone()),
                _ => Need::Always,
            };
            // What Link pays there: a shop's price or a Great Fairy's offering.
            let rupees = match shops::is_shop(check) {
                true => Some(shops::price(ctx.seed, check.hash)),
                false => shops::offering(check.hash),
            };
            let cost = match rupees {
                Some(r) => ctx.progressive(|p| Ok(shops::wallet(r, p)))?,
                None => Need::Always,
            };
            for place in self.places_of_check(check, rule, pocketed)? {
                let need = self.need_at(check, rule, &place, tools, &mut by_tool)?;
                // The opening gives a pocketed check, whatever its scene needs.
                let need = match pocketed {
                    true => Need::Always,
                    false => Need::All(vec![presence.clone(), cost.clone(), need]),
                };
                locations.push(Location {
                    hash: check.hash,
                    place: self.at(&place.0, place.1)?,
                    need,
                });
            }
        }
        for (hash, entry) in self.book.checks() {
            ensure!(
                entry.optional
                    || seen.iter().any(|(h, _)| *h == hash)
                    || gifts::by_hash(hash).is_some(),
                "a rule names check {hash}, which no check has"
            );
        }
        Ok(locations)
    }

    /// Where `check` is: the start when `pocketed`, its room, the place
    /// its `rule` pins, its map, or the maps of the placements standing
    /// for it.
    fn places_of_check(
        &self,
        check: &ItemCheck,
        rule: Option<&CheckEntry>,
        pocketed: bool,
    ) -> Result<Vec<Place>> {
        let places = &self.scan.places;
        let mut where_: Vec<Place> = Vec::new();
        if pocketed {
            where_.push(start_of(self.ctx.age));
        } else if let Some((room, node)) = self.rooms.check(check.hash) {
            where_.extend(places_of(places, node, &room));
        } else if let Some((node, era)) = rule.and_then(|r| r.place.clone()) {
            where_.extend(
                self.pinned(&node, era)
                    .into_iter()
                    .map(|e| (node.clone(), e)),
            );
        } else if check.file.starts_with("Banc/") {
            let (node, eras) = node_of(&check.file);
            where_.extend(eras.into_iter().map(|e| (node.clone(), e)));
        } else {
            let scrub = scrubs::of(check).map(|s| s.placements.to_vec());
            let shop = || {
                let slot = self.shop_slots.iter().find(|s| s.hash() == check.hash);
                slot.map(|s| s.placements.clone())
            };
            let cow = || {
                let cow = self.cows.iter().find(|c| c.hash() == check.hash);
                cow.map(|c| c.placements.iter().map(|p| p.hash).collect())
            };
            for hash in scrub.or_else(shop).or_else(cow).into_iter().flatten() {
                for node in self.scan.nodes_of.get(&hash).into_iter().flatten() {
                    where_.extend(places_of(places, node, node));
                }
            }
        }
        if where_.is_empty() {
            bail!("check {} ({}) has no place", check.hash, check.holder);
        }
        Ok(where_)
    }

    /// What `check` needs at `place` beyond its presence and cost: its
    /// `rule`, else the place's obstacles and its gate, then its kind's
    /// word and its keys.
    fn need_at(
        &self,
        check: &ItemCheck,
        rule: Option<&CheckEntry>,
        place: &Place,
        tools: &mut Tools,
        by_tool: &mut HashMap<String, Need>,
    ) -> Result<Need> {
        let ctx = self.ctx;
        let need = match rule {
            Some(r) => self.book.need(&r.needs, ctx)?,
            None => {
                let obstacles = self.obstacles_at(place, by_tool)?;
                match self.gates.get(&check.hash) {
                    Some(gate) => {
                        let gate = self.resolve(&switches(gate, tools)?)?;
                        Need::All(vec![gate, obstacles])
                    }
                    None => obstacles,
                }
            }
        };
        // A room rule measured what hits a gold skulltula, which the
        // kind's blanket word would only narrow.
        let kind = match scrubs::of(check) {
            Some(_) => Some(STUN_SCRUB),
            None if rule.is_some_and(|r| r.room) => None,
            None => pickups::kind_of(check).and_then(|k| k.needs),
        };
        let need = match kind {
            Some(word) => Need::All(vec![need, self.resolve(word)?]),
            None => need,
        };
        let keys = [
            keys::dungeon_need(check, ctx.keys),
            keys::fortress_need(&check.item, ctx),
        ];
        Ok(match keys.into_iter().flatten().collect::<Vec<_>>() {
            keys if keys.is_empty() => need,
            keys => Need::All([vec![need], keys].concat()),
        })
    }

    /// What `place`'s obstacles need, each tool's need resolved once into
    /// `by_tool`. A map for one era drops the obstacles only the other era
    /// clears.
    fn obstacles_at(&self, place: &Place, by_tool: &mut HashMap<String, Need>) -> Result<Need> {
        let Some(tools) = self.scan.obstacles.get(&place.0) else {
            return Ok(Need::Always);
        };
        let one_era = eras_of(&self.scan.places, &place.0).len() == 1;
        let mut needs = Vec::new();
        for tool in tools.keys() {
            if !by_tool.contains_key(tool) {
                by_tool.insert(tool.clone(), self.resolve(tool)?);
            }
            let need = &by_tool[tool];
            if !(one_era && !need.possible(place.1)) {
                needs.push(need.clone());
            }
        }
        Ok(Need::All(needs))
    }
}
