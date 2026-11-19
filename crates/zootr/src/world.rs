//! World-graph facts read from Banc files: scene exits, and the GameData
//! flags that make an actor exist or vanish.
//!
//! A node is a Banc param name such as a field's child map. Overworld maps
//! have one per era; dungeons have a single era-less one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use anyhow::Result;
use ounce_formats::{Byml, BymlDoc, Romfs, files_with_suffix};

use crate::checks::{self, actors, bancs, str_field};
use crate::known::{Name, field};
use crate::per_dump::PerDump;

const EXIT_ACTOR: &Name = field::FIELD_AND_DUNGEON_ENTRANCE_EVENT_REQUEST_ACTOR;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Era {
    Child,
    Adult,
}

impl Era {
    const BOTH: [Era; 2] = [Era::Child, Era::Adult];

    fn next_banc_key(self) -> &'static str {
        match self {
            Era::Child => field::NEXT_BANC_PARAM_NAME_CHILD,
            Era::Adult => field::NEXT_BANC_PARAM_NAME_ADULT,
        }
    }
}

impl fmt::Display for Era {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Era::Child => "child",
            Era::Adult => "adult",
        })
    }
}

/// An overworld map's child-era `_Static` file is that map's child node, child
/// only. A dungeon file serves both eras.
pub fn node_of(file: &str) -> (String, Vec<Era>) {
    let stem = file.rsplit('/').next().unwrap_or(file);
    let node = ["_Static", "_Dynamic"]
        .iter()
        .find_map(|s| stem.split_once(s).map(|(n, _)| n))
        .unwrap_or(stem);
    let eras = if node.contains("_Child") {
        vec![Era::Child]
    } else if node.contains("_Adult") {
        vec![Era::Adult]
    } else {
        Era::BOTH.to_vec()
    };
    (node.to_owned(), eras)
}

/// A flag gate on an actor: it exists only while `flag` is set, or only
/// while it is unset when `negated`.
#[derive(Clone, Debug, PartialEq)]
pub struct Presence {
    pub flag: String,
    pub negated: bool,
}

impl fmt::Display for Presence {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let bang = if self.negated { "!" } else { "" };
        write!(f, "{bang}{}", self.flag)
    }
}

pub fn presence_of(actor: &Byml) -> Option<Presence> {
    let p = actor.as_map().ok()?.get("Presence")?;
    Some(Presence {
        flag: str_field(p, field::FLAG_NAME)?.to_owned(),
        negated: p
            .as_map()
            .ok()?
            .get(&**field::IS_NEGATION)?
            .as_bool()
            .unwrap_or(false),
    })
}

/// A scene transition: walking into it in `from` as `era` lands at
/// `to_start` in `to`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Exit {
    pub from: String,
    pub era: Era,
    pub to: String,
    pub to_start: String,
    /// The gate's own placement name, when it has one.
    pub name: String,
    /// Shown as `!flag` when the gate exists only while the flag is unset.
    pub presence: String,
}

/// Every distinct exit. The same gate is often placed several times (wide
/// entrances are several trigger boxes), so duplicates collapse. Kept per
/// dump.
pub fn exits(romfs: &Romfs) -> Result<BTreeSet<Exit>> {
    static KEPT: PerDump<BTreeSet<Exit>> = PerDump::new();
    KEPT.get(romfs, || find_exits(romfs))
}

fn find_exits(romfs: &Romfs) -> Result<BTreeSet<Exit>> {
    let mut out = BTreeSet::new();
    for (file, doc) in bancs(romfs)? {
        let doc = doc?;
        let (from, eras) = node_of(&file);
        for actor in actors(&doc.root)? {
            if str_field(actor, checks::GYAML) != Some(EXIT_ACTOR) {
                continue;
            }
            let Some(dynamic) = actor.as_map()?.get("Dynamic") else {
                continue;
            };
            let presence = presence_of(actor).map(|p| p.to_string());
            for &era in &eras {
                let Some(to) = str_field(dynamic, era.next_banc_key()) else {
                    continue;
                };
                out.insert(Exit {
                    from: from.clone(),
                    era,
                    to: to.to_owned(),
                    to_start: str_field(dynamic, field::NEXT_START_POS_NAME)
                        .unwrap_or("-")
                        .to_owned(),
                    name: str_field(actor, "Name").unwrap_or("-").to_owned(),
                    presence: presence.clone().unwrap_or_else(|| "-".into()),
                });
            }
        }
    }
    Ok(out)
}

/// Maps the game swaps for an `_Alternative` copy while a flag is set,
/// e.g. Zora's Domain as an adult, which has its own flag.
/// Named on the neighbours' GameBancParam `OptimizeChangeBancUnits`.
pub fn alternatives(romfs: &Romfs) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for path in files_with_suffix(
        &romfs
            .root()
            .join(format!("Banc/{}", field::GAME_BANC_PARAM)),
        ".bgyml",
    )? {
        let doc = BymlDoc::parse(&romfs.read(&romfs.rel(&path)?)?)?;
        let Some(units) = doc.root.as_map()?.get(&**field::OPTIMIZE_CHANGE_BANC_UNITS) else {
            continue;
        };
        for unit in units.as_array()? {
            if let (Some(banc), Some(flag)) = (
                str_field(unit, field::BANC_PARAM_NAME),
                str_field(unit, field::ALTERNATIVE_GAME_DATA).filter(|f| !f.is_empty()),
            ) {
                out.insert(banc.to_owned(), flag.to_owned());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{flag, map};

    #[test]
    fn nodes_and_eras_come_from_the_file_name() {
        needs_names!();
        let (n, e) = node_of(&format!(
            "Banc/{}/{}_Static.bcett.byml.zs",
            flag::KOKIRI_FOREST,
            map::KOKIRI_FOREST_CHILD
        ));
        assert_eq!(n, map::KOKIRI_FOREST_CHILD);
        assert_eq!(e, [Era::Child]);
        let (n, e) = node_of(&format!(
            "Banc/{}/{}_Static.bcett.byml.zs",
            flag::ZORA_DOMAIN,
            map::ZORA_DOMAIN_ADULT_ALTERNATIVE
        ));
        assert_eq!(n, map::ZORA_DOMAIN_ADULT_ALTERNATIVE);
        assert_eq!(e, [Era::Adult]);
        let (n, e) = node_of(&format!(
            "Banc/{}/{}_Dynamic.bcett.byml.zs",
            flag::FIRE_TEMPLE,
            flag::FIRE_TEMPLE
        ));
        assert_eq!(n, flag::FIRE_TEMPLE);
        assert_eq!(e.len(), 2);
    }
}
