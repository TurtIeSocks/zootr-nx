//! Resources a check gives only once Link has what uses them.

use anyhow::Result;
use ounce_formats::{ActorPack, BymlDoc, ModBuilder, Romfs};

use crate::bombchu_bags;
use crate::checks::Swap;
use crate::copies::MADE;
use crate::known::{Name, actor, item};
use crate::progressive::{self, Lines};
use crate::standins::{Tables, actor_path, raw, write_clone};

/// Each resource and what fires it. A starter bag's rows count only in a
/// seed that shuffles the bag ([`need`]).
pub const NEEDS: [(&Name, &Name); 12] = [
    (item::BOMB_BUNDLE_SMALL, item::BOMB_BAG_SMALL),
    (actor::T_BOX_ARROW_BUNDLE_SMALL, item::RANGE_BOW),
    (actor::T_BOX_ARROW_BUNDLE_MEDIUM, item::RANGE_BOW),
    (actor::T_BOX_ARROW_BUNDLE_LARGE, item::RANGE_BOW),
    (actor::T_BOX_DEKU_SEED_BUNDLE, item::RANGE_SLING_SHOT),
    (actor::T_BOX_DEKU_NUT_BUNDLE, item::NUT_BAG_SMALL),
    (item::DEKU_STICK_BUNDLE_SMALL, item::STICK_BAG_SMALL),
    (actor::OBJ_DEKU_STICK, item::STICK_BAG_SMALL),
    (item::BOMBCHU_BUNDLE_SMALL, bombchu_bags::SMALL),
    (item::BOMBCHU_BUNDLE_MEDIUM, bombchu_bags::SMALL),
    (item::BOMBCHU_BUNDLE_LARGE, bombchu_bags::SMALL),
    (actor::OBJ_BOMB, item::BOMB_BAG_SMALL),
];
/// Given instead until then, as the Gerudo Training Ground's chests do.
pub const WITHOUT: &Name = actor::T_BOX_RUPEE_RED;

/// `item`'s row, if it's a resource `lines`' seed holds back: one whose
/// firing item the seed has ([`Lines::has`]).
pub fn need(item: &str, lines: Lines) -> Option<(&'static Name, &'static Name)> {
    let row = NEEDS.iter().find(|(i, _)| *i == item)?;
    lines.has(row.1).then_some(*row)
}

/// A chest holding `item` drops it once Link has what it needs.
pub fn swap(item: &str, lines: Lines) -> Option<Swap> {
    need(item, lines).map(|(then, check)| Swap {
        check: check.get(),
        then: then.get(),
    })
}

/// The pouch names owning any of which means owning `fires`: its line's,
/// or `fires` itself outside every line.
pub fn pouches(fires: &'static str) -> impl Iterator<Item = &'static str> {
    let line = progressive::line(fires).unwrap_or_default();
    let named = line.iter().map(|(_, pouch)| pouch.get());
    named.chain(line.is_empty().then_some(fires))
}

/// What follows [`MADE`] in a Red Rupee copy's name.
const COPY: &str = "Ammo";

/// The Red Rupee copy a minigame prize chest drops for `item`, a chest of
/// a minigame being no substitute chest. The hook swaps it for `item` once
/// Link owns what fires it ([`line`]). As long as `item`'s name, since the
/// hook writes the swap over the drop's string.
pub fn copy(item: &str, lines: Lines) -> Option<String> {
    need(item, lines)?;
    let k = NEEDS.iter().position(|(a, _)| *a == item)?;
    let base = format!("{MADE}{COPY}");
    let width = item.len().saturating_sub(base.len()).max(1);
    Some(format!("{base}{k:0>width$}"))
}

/// The ammo `name` is the Red Rupee copy of, if it's one.
pub fn copied(name: &str) -> Option<&'static str> {
    let of = NEEDS
        .iter()
        .find(|(a, _)| copy(a, Lines::widest(false)).is_some_and(|c| c == name));
    of.map(|(a, _)| a.get())
}

/// The hook's keyed line for a prize chest holding `item`: its copy, then
/// `item` once Link owns any pouch name that fires it. The copy's own
/// pouch name is the first of those, so the level after it is the ammo.
pub fn line(item: &str) -> Option<Vec<(String, String)>> {
    let &(bundle, fires) = NEEDS.iter().find(|(a, _)| *a == item)?;
    let fired: Vec<_> = pouches(fires.get()).collect();
    let bottom = (
        copy(item, Lines::widest(false))?,
        fired.first()?.to_string(),
    );
    let levels = fired.iter().map(|p| (bundle.to_string(), p.to_string()));
    Some([bottom].into_iter().chain(levels).collect())
}

/// The Red Rupee copies `drops` name, each once, in order.
pub fn copies<'a>(drops: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut copies: Vec<String> = drops
        .filter(|d| copied(d).is_some())
        .map(str::to_owned)
        .collect();
    copies.sort();
    copies.dedup();
    copies
}

/// Writes each Red Rupee copy: the Red Rupee's pack under its name, with
/// the Red Rupee's rows, so the game gives and names a Red Rupee, as a
/// small key copy counts as a small key.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    copies: &[String],
) -> Result<()> {
    if copies.is_empty() {
        return Ok(());
    }
    let template = ActorPack::load(romfs, WITHOUT.get())?;
    let like = actor_path(&template.name);
    for copy in copies {
        let param = BymlDoc::parse(&raw(&template, &like)?)?;
        write_clone(
            builder,
            &template,
            copy,
            [(actor_path(copy), param, like.clone())],
        )?;
        tables.copy_item(copy, &template.name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_as_long_as_its_ammo_and_one_per_kind() {
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        let copies: Vec<_> = NEEDS
            .iter()
            .map(|(a, _)| copy(a, Lines::widest(false)).unwrap())
            .collect();
        for ((ammo, _), c) in NEEDS.iter().zip(&copies) {
            assert!(c.len() >= ammo.len(), "{c} for {ammo}");
            assert_eq!(copied(c), Some(ammo.get()));
        }
        let unique: std::collections::HashSet<_> = copies.iter().collect();
        assert_eq!(unique.len(), copies.len());
        assert_eq!(copied(WITHOUT.get()), None);
        assert_eq!(copy(item::HOOKSHOT.get(), Lines::widest(false)), None);
    }

    #[test]
    fn a_line_gives_the_copy_until_link_owns_any_bow() {
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        let arrows = actor::T_BOX_ARROW_BUNDLE_SMALL.get();
        let line = line(arrows).unwrap();
        let bows: Vec<_> = pouches(item::RANGE_BOW.get()).collect();
        assert!(bows.len() > 1, "the bow's line");
        assert_eq!(
            line[0],
            (copy(arrows, Lines::default()).unwrap(), bows[0].to_owned())
        );
        let rest: Vec<_> = line[1..]
            .iter()
            .map(|(d, p)| (d.as_str(), p.as_str()))
            .collect();
        let want: Vec<_> = bows.iter().map(|&p| (arrows, p)).collect();
        assert_eq!(rest, want);
        assert_eq!(super::line(item::HOOKSHOT.get()), None);
    }

    #[test]
    fn a_bags_ammo_waits_for_the_bag_only_where_the_bag_is_shuffled() {
        needs_names!();
        let rows = [
            (actor::T_BOX_DEKU_NUT_BUNDLE, item::NUT_BAG_SMALL),
            (item::DEKU_STICK_BUNDLE_SMALL, item::STICK_BAG_SMALL),
            (item::BOMBCHU_BUNDLE_MEDIUM, bombchu_bags::SMALL),
        ];
        for (ammo, bag) in rows {
            assert_eq!(need(ammo, Lines::default()), None, "{ammo}");
            assert_eq!(need(ammo, Lines::widest(false)), Some((ammo, bag)));
        }
        let nuts = Lines {
            nut_bag: true,
            ..Lines::default()
        };
        assert!(need(rows[0].0, nuts).is_some() && need(rows[1].0, nuts).is_none());
        // Arrows wait for the bow in every seed.
        assert!(need(actor::T_BOX_ARROW_BUNDLE_SMALL, Lines::default()).is_some());
    }

    /// The swap turns the holder into a substitute chest, which would
    /// break a pot. A gift (a pot's pickup among them) keeps its item and
    /// steps by what Link owns instead (`shuffle::drops`).
    #[test]
    fn every_vanilla_resource_outside_a_gift_sits_in_a_chest() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = crate::checks::scan(&romfs).unwrap();
        let held: Vec<_> = checks
            .iter()
            .filter(|c| swap(&c.item, Lines::widest(false)).is_some())
            .filter(|c| !crate::gifts::is_gift(c))
            .collect();
        assert!(!held.is_empty());
        for c in held {
            assert!(
                c.holder.starts_with("Obj_TBox_A_"),
                "{}: {}",
                c.file,
                c.holder
            );
        }
    }
}
