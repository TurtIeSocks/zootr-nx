//! Picking up what a broken holder dropped: a spiral search for it, and
//! telling a neighbour's drop from the spot's own.

use std::thread::sleep;

use anyhow::Result;
use ounce_formats::Romfs;

use super::{BEAT, Sweeper};
use crate::play::Input;
use crate::sweep::judge::Judge;
use crate::sweep::watch::{lands_as, owned_as};

/// Legs of each length in the square spiral [`search`] walks.
const SPIRAL: usize = 8;
/// How long each step pushes the stick: about 0.75 m on the Mac. A 100 ms
/// push moves Link 0.15 m, most of it spent getting going.
const STEP_MS: u64 = 250;
impl<I: Input, J: Judge> Sweeper<'_, I, J> {
    /// [`search`] for what a broken pot dropped until that lands or a get
    /// event starts. `use` broke it with the sword and stepped onto its
    /// spot, where the drop falls, but one can bounce off (a rock's in the
    /// Colossus).
    pub(super) fn fetch(&mut self, landed: &impl Fn(&Self) -> Result<bool>) -> Result<()> {
        self.input.press("ZL")?;
        sleep(BEAT / 2);
        for push in search() {
            if self.dev.state()?.event || landed(self)? {
                break;
            }
            self.input.stick(push, STEP_MS)?;
        }
        Ok(())
    }
}

/// The stick pushes Link takes looking for a drop, the camera behind him:
/// a square spiral out from where he stands. Its rows are a step apart, so
/// it passes over a drop that bounced aside or behind.
fn search() -> impl Iterator<Item = &'static str> {
    const TURNS: [&str; 4] = ["LUp", "LRight", "LDown", "LLeft"];
    (0..2 * SPIRAL).flat_map(|leg| std::iter::repeat_n(TURNS[leg % 4], leg / 2 + 1))
}

/// The item of the one of `near` that `other`, an item landing beside the
/// spot's own, is the get of.
pub(super) fn neighbour(romfs: &Romfs, near: &[String], other: &str) -> Option<String> {
    (near.iter())
        .find(|item| owned_as(romfs, item).is_ok_and(|names| lands_as(other, &names)))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;

    #[test]
    fn the_search_passes_within_a_step_of_every_spot_around_the_break() {
        let mut at = (0i32, 0i32);
        let mut seen = std::collections::BTreeSet::from([at]);
        for push in search() {
            match push {
                "LUp" => at.1 += 1,
                "LDown" => at.1 -= 1,
                "LRight" => at.0 += 1,
                _ => at.0 -= 1,
            }
            seen.insert(at);
        }
        let reach = SPIRAL as i32 / 2 - 1;
        for x in -reach..=reach {
            for y in -reach..=reach {
                assert!(seen.contains(&(x, y)), "never near ({x}, {y})");
            }
        }
    }

    #[test]
    fn an_item_landing_is_told_to_be_a_neighbours_by_its_names() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let near = vec![
            actor::OBJ_RUPEE_RED.to_string(),
            actor::OBJ_ARMOR_GORON.to_string(),
        ];
        let got = |item: &str| format!("IsGet.{item}");
        let found = neighbour(&romfs, &near, &got(actor::OBJ_ARMOR_GORON));
        assert_eq!(found.as_deref(), Some(actor::OBJ_ARMOR_GORON.get()));
        let gear = got(actor::ARMOR_ADULT_GORON);
        assert_eq!(neighbour(&romfs, &near, &gear).as_deref(), found.as_deref());
        assert_eq!(neighbour(&romfs, &near, "IsGet.Item_A"), None);
        assert_eq!(neighbour(&romfs, &[], &gear), None);
    }
}
