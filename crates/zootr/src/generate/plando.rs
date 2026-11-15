//! What a plando can pin, for the app's Plando page.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use ounce_formats::Romfs;
use serde::Serialize;

use super::Spot;
use crate::checks::ItemCheck;
use crate::names::Names;
use crate::settings::{self, NewGame, Settings, group};
use crate::shuffle::{self, Fixed, Placement};
use crate::{checks, keys, places};

/// A check a plando can pin an item to, named as a seed's report names it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlandoCheck {
    #[serde(with = "super::report::u64_text")]
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub hash: u64,
    pub area: String,
    /// Where it is, else what it held in the vanilla game.
    pub place: String,
    /// What it holds in the vanilla game.
    pub vanilla: String,
    /// The setting that would deal it, when these settings don't.
    pub needs: Option<String>,
}

/// An item a plando can pin.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlandoItem {
    pub actor: String,
    pub name: String,
}

/// What a plando can pick from with these settings, and why the deal
/// refuses `fixed`, if it does.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlandoOptions {
    /// Each check once, by area then place: the dealt ones, then those one
    /// more setting would deal.
    pub checks: Vec<PlandoCheck>,
    /// The pool's items once each, by name.
    pub items: Vec<PlandoItem>,
    /// Why the deal refuses `fixed`, if it does.
    pub error: Option<String>,
}

/// The checks `settings` deal, the items their pool holds, and whether the
/// deal takes `fixed`.
pub fn options(romfs: &Romfs, settings: &Settings, fixed: &Fixed) -> Result<PlandoOptions> {
    let new_game = NewGame::from_settings(settings, &[])?;
    let every = checks::all(romfs)?;
    let dealt = checks::dealt_of(romfs, every.clone(), &new_game)?;
    let mut seen = BTreeSet::new();
    let unique: Vec<_> = dealt.iter().filter(|c| seen.insert(c.hash)).collect();
    let skipped = skipped(romfs, &every, settings, &seen)?;
    let listed = (unique.iter().map(|c| (*c, None)))
        .chain(skipped.values().map(|(id, c)| (c, Some(id.clone()))));
    let (listed, needs): (Vec<&ItemCheck>, Vec<Option<String>>) = listed.unzip();
    let free = Fixed::new();
    let pool: BTreeSet<&str> = shuffle::deal(&dealt, &free, &new_game)?
        .items
        .into_iter()
        .collect();
    let actors = (listed.iter().map(|c| c.item.as_str())).chain(pool.iter().copied());
    let names = Names::load(romfs, actors)?;
    let at = |c| Placement {
        check: c,
        item: keys::pooled(c),
    };
    let mut spots: Vec<Spot> = listed.iter().map(|c| Spot::new(&at(c), &names)).collect();
    places::fill(romfs, &mut spots)?;
    let mut checks: Vec<PlandoCheck> = (spots.iter().zip(needs))
        .map(|(s, needs)| PlandoCheck {
            hash: s.hash,
            area: s.area_name.clone(),
            place: s.name().to_owned(),
            vanilla: names.item(&s.vanilla),
            needs,
        })
        .collect();
    checks.sort_by(|a, b| (&a.area, &a.place, a.hash).cmp(&(&b.area, &b.place, b.hash)));
    let mut items: Vec<PlandoItem> = (pool.into_iter())
        .map(|actor| PlandoItem {
            actor: actor.to_owned(),
            name: names.item(actor),
        })
        .collect();
    items.sort_by(|a, b| (&a.name, &a.actor).cmp(&(&b.name, &b.actor)));
    let error = shuffle::deal(&dealt, fixed, &new_game).err();
    Ok(PlandoOptions {
        checks,
        items,
        error: error.map(|e| format!("{e:#}")),
    })
}

/// The checks `settings` don't deal that one more setting would, by hash,
/// with that setting's id: each toggle on the Shuffles and Dungeons panels
/// that's off, turned on alone. The first in the schema wins a check two
/// would deal. `every` is what [`checks::all`] gave.
fn skipped(
    romfs: &Romfs,
    every: &[ItemCheck],
    settings: &Settings,
    dealt: &BTreeSet<u64>,
) -> Result<BTreeMap<u64, (String, ItemCheck)>> {
    let on = settings::toggles_on(settings);
    let off: Vec<_> = (settings::schema().into_iter())
        .filter(|d| matches!(d.kind, settings::Kind::Toggle { .. }))
        .filter(|d| [group::SHUFFLES, group::DUNGEONS].contains(&d.group))
        .filter(|d| !on.contains(&d.id))
        .map(|d| d.id)
        .collect();
    // Each setting reads its own holders from the dump, so they read at once.
    let dealt_with = |id: &String| -> Result<Vec<ItemCheck>> {
        let mut more = settings.clone();
        more.insert(id.clone(), true.into());
        checks::dealt_of(romfs, every.to_vec(), &NewGame::from_settings(&more, &[])?)
    };
    let each: Vec<Result<Vec<ItemCheck>>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = (off.iter())
            .map(|id| scope.spawn(move || dealt_with(id)))
            .collect();
        jobs.into_iter()
            .map(|job| job.join().expect("a dealt read panicked"))
            .collect()
    });
    let mut out = BTreeMap::new();
    for (id, checks) in off.iter().zip(each) {
        for check in checks? {
            if !dealt.contains(&check.hash) && !out.contains_key(&check.hash) {
                out.insert(check.hash, (id.clone(), check));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::item;

    #[test]
    fn options_name_each_check_once_and_report_a_refused_deal() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let settings = Settings::new();
        let got = options(&romfs, &settings, &Fixed::new()).unwrap();
        assert_eq!(got.error, None);
        let hashes: BTreeSet<u64> = got.checks.iter().map(|c| c.hash).collect();
        assert_eq!(hashes.len(), got.checks.len(), "one row a check");
        assert!(
            got.checks
                .iter()
                .all(|c| !c.area.is_empty() && !c.place.is_empty())
        );
        assert!(got.items.iter().any(|i| i.actor == item::SWORD_KOKIRI));
        let rolling = (got.checks.iter())
            .find(|c| c.place.ends_with("(Rolling Goron)"))
            .expect("a check NPC gifts would deal is listed");
        assert_eq!(rolling.area, "Goron City");
        assert_eq!(rolling.needs.as_deref(), Some(settings::NPC_GIFTS));
        let dealt = got.checks.iter().find(|c| c.needs.is_none()).unwrap();
        let first = dealt.hash;
        let fine = Fixed::from([(first, got.items[0].actor.clone())]);
        assert_eq!(options(&romfs, &settings, &fine).unwrap().error, None);
        let gone = Fixed::from([(1, item::SWORD_KOKIRI.to_string())]);
        let error = options(&romfs, &settings, &gone).unwrap().error.unwrap();
        assert!(error.contains("no check with hash 1"), "{error}");
    }
}
