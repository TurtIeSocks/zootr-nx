//! Buying from a keeper: moving the shop cursor over the shelves and
//! pressing through the price line and the get window.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::judge::Judge;
use super::spot::{ALONE, BEAT, RUPEES, Sweeper, Swept, Verdict};
use crate::known::{Name, flag};
use crate::live;
use crate::play::Input;

/// The shop cursor: the shelf spot browsing points at, -1 before it.
pub(super) const CURSOR: &Name = flag::SHOP_SELECT_ITEM_INDEX;
/// What the shelf spot under the cursor sells, by actor name.
pub(super) const GOODS: &Name = flag::SHOP_SELECT_ITEM_NAME;
/// The stick pushes that move the shop cursor.
pub(super) const MOVES: [&str; 4] = ["LRight", "LDown", "LLeft", "LUp"];
/// The price of the goods under the shop cursor, a GameData Int with no
/// name, set as the cursor moves onto them.
pub(super) const PRICE: u32 = 0xf0363f80;
/// A push that moves the cursor one spot, and the wait for it to land.
pub(super) const NUDGE_MS: u64 = 120;
pub(super) const NUDGE: Duration = Duration::from_millis(500);
/// Beats of A on the goods, through the price line and the get window,
/// before the buy has had its chance. The Goron keeper says two more lines
/// than the Kokiri one, a warning and a haggle, and 8 ran out before his
/// get window (Mac, 2026-11-14). The loop ends as the get lands, so spare beats cost nothing.
pub(super) const BUY: usize = 16;

impl<I: Input, J: Judge> Sweeper<'_, I, J> {
    /// Buy `item` off a shop shelf: A through the keeper's talk into
    /// browsing ("I'd like to browse" is its first line), move the cursor
    /// onto the goods, A through the price line and the get window, then B
    /// out, which backs out of browsing, says goodbye and ends the talk.
    /// The judge bought Hob's Token but left Link browsing, where A buys
    /// again, a stall (2026-11-14). `None` for the judge to go on when
    /// browsing never starts.
    pub(super) fn buy(
        &mut self,
        item: &str,
        landed: &impl Fn(&Self) -> Result<bool>,
        swept: &mut Swept,
    ) -> Result<Option<Verdict>> {
        let (game, global) = (self.dev.game(), self.dev.global());
        live::set(game, global, live::hash(CURSOR)?, "-1")?;
        let start = Instant::now();
        while self.cursor()? < 0 {
            if start.elapsed() > ALONE {
                return Ok(None);
            }
            self.input.press("A")?;
            sleep(BEAT);
        }
        if self.shelve(item, &mut swept.judge)? {
            // Exactly the price: a buy that leaves more than the wallet
            // holds is refused as a full bag, rupees the most often (Mac,
            // 2026-11-14).
            let price = live::read(self.dev.game(), self.gd, self.dev.global(), PRICE)?;
            live::set(
                self.dev.game(),
                self.dev.global(),
                live::hash(RUPEES.0)?,
                &price,
            )?;
            for _ in 0..BUY {
                self.input.press("A")?;
                sleep(BEAT);
                if landed(self)? {
                    break;
                }
            }
        }
        Ok(Some(match self.finish("B", None)? {
            true => Verdict::Stall,
            false => Verdict::Nothing,
        }))
    }

    /// Move the shop cursor onto goods that shelve as `item`, trying each
    /// push from each shelf spot seen and reached until one does, and say
    /// whether it
    /// found them. Each spot's goods go to `seen`. The shelves aren't a
    /// grid one push a spot: Kokiri's cursor went 0, 1, 4, 5 pushed right
    /// (Mac, 2026-11-14), so where a push goes is learned, not assumed.
    pub(super) fn shelve(&mut self, item: &str, seen: &mut Vec<String>) -> Result<bool> {
        let mut pushes = BTreeMap::new();
        let mut spots = BTreeSet::new();
        loop {
            let at = self.cursor()?;
            if spots.insert(at) {
                let goods = self.goods()?;
                seen.push(format!("shelf spot {at}: {goods}"));
                if shelves_as(item, &goods) {
                    return Ok(true);
                }
            }
            let untried = (spots.iter())
                .flat_map(|&spot| (0..MOVES.len()).map(move |m| (spot, m)))
                .filter(|push| !pushes.contains_key(push))
                .find_map(|(from, m)| Some((from, m, route(&pushes, at, from)?)));
            let Some((from, m, path)) = untried else {
                return Ok(false);
            };
            for step in path {
                self.nudge(step)?;
            }
            if self.cursor()? != from {
                return Ok(false);
            }
            self.nudge(m)?;
            pushes.insert((from, m), self.cursor()?);
        }
    }

    pub(super) fn nudge(&mut self, m: usize) -> Result<()> {
        self.input.stick(MOVES[m], NUDGE_MS)?;
        sleep(NUDGE);
        Ok(())
    }

    pub(super) fn cursor(&self) -> Result<i64> {
        let value = self.value(CURSOR)?;
        value
            .parse()
            .with_context(|| format!("{CURSOR} reads {value}"))
    }

    pub(super) fn goods(&self) -> Result<String> {
        let value = self.value(GOODS)?;
        Ok(value.trim_matches('"').trim_end_matches("\\0").to_owned())
    }

    pub(super) fn value(&self, name: &str) -> Result<String> {
        let hash = live::hash(name)?;
        live::read(self.dev.game(), self.gd, self.dev.global(), hash)
    }
}

/// Whether a shelf shows `item` as `goods`: a chest's rupee shelves as the
/// rupee itself (`actor::T_BOX_RUPEE_RED` as `actor::OBJ_RUPEE_RED`), so the name past
/// its kind decides.
pub(super) fn shelves_as(item: &str, goods: &str) -> bool {
    fn bare(name: &str) -> &str {
        name.split_once('_').map_or(name, |(_, rest)| rest)
    }
    bare(item) == bare(goods)
}

/// The pushes that take the shop cursor from `from` to `to`, over the
/// ones seen so far (`pushes` maps a spot and push to where it went).
pub(super) fn route(
    pushes: &BTreeMap<(i64, usize), i64>,
    from: i64,
    to: i64,
) -> Option<Vec<usize>> {
    let mut paths = BTreeMap::from([(from, vec![])]);
    let mut queue = VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        if at == to {
            return paths.remove(&to);
        }
        for (&(_, m), &next) in pushes.range((at, 0)..(at + 1, 0)) {
            if !paths.contains_key(&next) {
                let mut path = paths[&at].clone();
                path.push(m);
                paths.insert(next, path);
                queue.push_back(next);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, item};

    #[test]
    fn a_chest_rupee_shelves_as_the_rupee_and_a_token_as_itself() {
        if crate::game::no_names() {
            return;
        }
        assert!(shelves_as(actor::T_BOX_RUPEE_RED, actor::OBJ_RUPEE_RED));
        assert!(shelves_as(item::TOKEN_GOLD, item::TOKEN_GOLD));
        assert!(!shelves_as(actor::T_BOX_RUPEE_RED, actor::OBJ_RUPEE_PURPLE));
    }

    #[test]
    fn the_shop_cursor_is_routed_over_the_pushes_seen() {
        // Kokiri's shelf on the Mac: right from 0 went to 1, from 1 to 4.
        let pushes = BTreeMap::from([((0, 0), 1), ((1, 0), 4), ((4, 2), 1), ((1, 2), 0)]);
        assert_eq!(route(&pushes, 0, 4), Some(vec![0, 0]));
        assert_eq!(route(&pushes, 4, 0), Some(vec![2, 2]));
        assert_eq!(route(&pushes, 4, 4), Some(vec![]));
        assert_eq!(route(&pushes, 0, 6), None);
    }
}
