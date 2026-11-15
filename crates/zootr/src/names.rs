//! Display names from the game's English text, read from the dump at run
//! time. Anything without a name gets its id made readable.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Romfs, messages};
use roead::sarc::Sarc;

use crate::items::component;
use crate::known::field;

pub(crate) const TEXT: &str = "Mals/USen.Product.110.sarc.zs";
pub(crate) const AREAS: &str = "LocationMsg/Location.msbt";
pub(crate) const ITEMS: &str = "ActorMsg/PouchContent.msbt";
/// Actor prefixes that mean nothing to a player.
const PREFIXES: [&str; 3] = ["Item_", "TBox_", "Obj_"];

pub struct Names {
    areas: HashMap<String, String>,
    items: HashMap<String, String>,
}

impl Names {
    /// Area names, and names for each of `actors`.
    pub fn load<'a>(romfs: &Romfs, actors: impl IntoIterator<Item = &'a str>) -> Result<Self> {
        let sarc = Sarc::new(romfs.read(TEXT)?).with_context(|| format!("reading {TEXT}"))?;
        let text = |path: &str| -> Result<HashMap<String, String>> {
            let data = sarc
                .get_data(path)
                .with_context(|| format!("{TEXT} has no {path}"))?;
            messages(data).with_context(|| format!("reading {path}"))
        };
        let pouch = text(ITEMS)?;
        let mut items = HashMap::new();
        for actor in actors {
            if !items.contains_key(actor)
                && let Some(label) = name_label(romfs, actor)?
                && let Some(name) = pouch.get(&label).cloned()
            {
                items.insert(actor.to_owned(), name);
            }
        }
        Ok(Self {
            areas: text(AREAS)?,
            items,
        })
    }

    pub fn area(&self, id: &str) -> String {
        self.areas.get(id).cloned().unwrap_or_else(|| pretty(id))
    }

    pub fn item(&self, actor: &str) -> String {
        self.items
            .get(actor)
            .cloned()
            .unwrap_or_else(|| pretty(actor))
    }
}

/// An id made readable: `Thing_Rupee_Blue` reads `Rupee Blue`, `KokiriForest`
/// reads `Kokiri Forest`.
pub fn pretty(id: &str) -> String {
    let id = PREFIXES
        .iter()
        .find_map(|p| id.strip_prefix(p))
        .unwrap_or(id);
    let mut out = String::new();
    let mut after_lower = false;
    for c in id.chars() {
        if c == '_' || (c.is_uppercase() && after_lower) {
            out.push(' ');
        }
        if c != '_' {
            out.push(c);
        }
        after_lower = c.is_lowercase();
    }
    out
}

/// The label naming an actor in `ITEMS`, through the `ActorName` its param
/// points at: `?ActorSystem/ActorName/<stem>.engine__actor__ActorName.bgyml`
/// is named by `<stem>_Name`.
pub(crate) fn name_label(romfs: &Romfs, actor: &str) -> Result<Option<String>> {
    let Some(param) = ActorPack::load(romfs, actor)?.actor_param()? else {
        return Ok(None);
    };
    let stem = component(&param, field::ACTOR_NAME_REF)
        .and_then(|r| r.rsplit('/').next())
        .and_then(|r| r.split('.').next());
    Ok(stem.map(|s| format!("{s}_Name")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, flow, item};

    #[test]
    fn dump_names_areas_and_items_and_tidies_unknown_ids() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = crate::checks::scan(&romfs).unwrap();
        let names = Names::load(&romfs, checks.iter().map(|c| c.item.as_str())).unwrap();
        let mut missing: Vec<&str> = checks
            .iter()
            .flat_map(|c| [c.file.split('/').nth(1).unwrap(), &c.item])
            .filter(|id| !names.areas.contains_key(*id) && !names.items.contains_key(*id))
            .collect();
        missing.sort();
        missing.dedup();
        assert_eq!(missing, Vec::<&str>::new());
        assert_eq!(names.area("NoPlace"), "No Place");
        assert_eq!(names.item("No_Thing"), "No Thing");
    }

    #[test]
    fn every_message_file_writes_back_unchanged() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let sarc = Sarc::new(romfs.read(TEXT).unwrap()).unwrap();
        for f in sarc.files() {
            let msbt = ounce_formats::Msbt::parse(f.data()).unwrap();
            assert_eq!(msbt.to_binary(), f.data(), "{:?}", f.name());
            assert!(msbt.misplaced().is_empty(), "{:?}", f.name());
        }
    }

    #[test]
    fn the_text_archive_writes_back_unchanged() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let data = romfs.read(TEXT).unwrap();
        for f in Sarc::new(&data).unwrap().files() {
            let name = f.name().unwrap();
            let same = ounce_formats::replace_file(&data, name, f.data()).unwrap();
            assert!(same == data, "{name}");
        }
    }

    #[test]
    fn ids_read_as_words() {
        assert_eq!(pretty(actor::T_BOX_RUPEE_BLUE), "Rupee Blue");
        assert_eq!(pretty(actor::OBJ_HEART_PIECE), "Heart Piece");
        assert_eq!(pretty(item::GLASS_TRUTH), "Glass Truth");
        assert_eq!(pretty(flow::BOTTOM_OF_THE_WELL), "Bottom Of The Well");
        assert_eq!(pretty("Arrow_10"), "Arrow 10");
    }
}
