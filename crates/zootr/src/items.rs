//! The item catalogue: every actor pack whose ActorParam points at a
//! PouchContentParam, which is what makes an actor something Link can own.

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlMap, Romfs, files_with_suffix};

use crate::known::field;

const DIR: &str = "Pack/Actor";
const SUFFIX: &str = ".pack.zs";

/// `Category` hashes, named by their members since the real enum names
/// aren't known. Swords have no `Category` at all.
const CATEGORIES: &[(u32, &str)] = &[
    (957554845, "item"),
    (1879595265, "key"),
    (1399920009, "ranged"),
    (1499870550, "ammo"),
    (995674644, "rupee"),
    (1343561467, "recovery"),
    (4046851590, "head"),
    (3520794636, "shield"),
    (2000767247, "boots"),
    (318723218, "tunic"),
];

pub struct Item {
    pub actor: String,
    pub pouch: BymlMap,
    /// Set for items that go on a button.
    pub assignable: Option<BymlMap>,
}

impl Item {
    /// A field as text, `-` when missing.
    pub fn category(&self) -> &'static str {
        let hash = self.field("Category");
        CATEGORIES
            .iter()
            .find(|(h, _)| h.to_string() == hash)
            .map_or(if hash == "-" { "sword" } else { "?" }, |c| c.1)
    }

    pub fn field(&self, key: &str) -> String {
        self.pouch.get(key).map_or("-".into(), scalar)
    }
}

pub fn scalar(v: &Byml) -> String {
    match v {
        Byml::String(s) => s.to_string(),
        Byml::Bool(b) => b.to_string(),
        Byml::I32(n) => n.to_string(),
        Byml::U32(n) => n.to_string(),
        Byml::I64(n) => n.to_string(),
        Byml::U64(n) => n.to_string(),
        Byml::Float(n) => n.to_string(),
        Byml::Double(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

/// `Components.<key>`, the usual way one param file points at another.
pub(crate) fn component<'a>(map: &'a BymlMap, key: &str) -> Option<&'a str> {
    map.get("Components")?
        .as_map()
        .ok()?
        .get(key)?
        .as_string()
        .ok()
        .map(|s| s.as_str())
}

/// Follows `Components.<key>` to the file it names, resolved.
pub(crate) fn follow(pack: &ActorPack, map: &BymlMap, key: &str) -> Result<Option<BymlMap>> {
    match component(map, key) {
        Some(r) => pack.resolve(r),
        None => Ok(None),
    }
}

fn load(romfs: &Romfs, actor: &str) -> Result<Option<Item>> {
    let pack = ActorPack::load(romfs, actor)?;
    let Some(param) = pack.actor_param()? else {
        return Ok(None);
    };
    let Some(pouch) = follow(&pack, &param, field::POUCH_CONTENT_REF)? else {
        return Ok(None);
    };
    let assignable = match follow(&pack, &param, field::GAME_PARAMETER_TABLE_REF)? {
        Some(table) => follow(&pack, &table, field::ASSIGNABLE_ITEM_PARAM)?,
        None => None,
    };
    Ok(Some(Item {
        actor: actor.to_owned(),
        pouch,
        assignable,
    }))
}

/// Every pouch item, sorted by actor name.
pub fn scan(romfs: &Romfs) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    for path in files_with_suffix(&romfs.root().join(DIR), SUFFIX)? {
        let rel = romfs.rel(&path)?;
        let actor = rel.trim_start_matches(DIR).trim_start_matches('/');
        if let Some(item) = load(romfs, actor.trim_end_matches(SUFFIX))? {
            items.push(item);
        }
    }
    items.sort_by(|a, b| a.actor.cmp(&b.actor));
    Ok(items)
}
