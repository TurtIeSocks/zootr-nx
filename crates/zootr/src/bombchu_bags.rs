//! Bombchu Bags: with the `bombchus` setting at `bag`, three bags of our
//! own cap Bombchus at 20, 30 and 40, and Link holds none before the first.

mod look;
pub use look::FILES;

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, murmur3, pack_path};

use crate::gamedata;
use crate::key_tints;
use crate::known::{Name, field, hash, item};
use crate::rsdb;
use crate::standins::{
    TABLE, Tables, clone_pack, component, doc, path_of, pouch_actor, raw, renamed, resolved,
    with_ref, write_clone,
};

/// Each bag, smallest first. The hook's refill reads a bag's size from
/// the end of its name, as it does the game's bags.
pub const BAGS: [&str; 3] = [
    "Item_Rando_BombchuBag_Small",
    "Item_Rando_BombchuBag_Medium",
    "Item_Rando_BombchuBag_Large",
];
pub const SMALL: &Name = &Name(hash(BAGS[0]));
pub const MEDIUM: &Name = &Name(hash(BAGS[1]));
pub const LARGE: &Name = &Name(hash(BAGS[2]));

/// The bomb bag each bag copies, by size.
const BOMB_BAGS: [&Name; 3] = [
    item::BOMB_BAG_SMALL,
    item::BOMB_BAG_MEDIUM,
    item::BOMB_BAG_LARGE,
];

/// Each bag with the bomb bag it copies, smallest first.
pub fn copies() -> [(&'static str, &'static Name); 3] {
    [0, 1, 2].map(|i| (BAGS[i], BOMB_BAGS[i]))
}

/// The dump's value a seed's GameData copies `hash` from, when `hash` is
/// one a bag adds: a bag's `IsGet` copies its bomb bag's, and the slot
/// naming the held bag copies the bomb bag's slot.
pub fn copied_from(hash: u32) -> Option<u32> {
    let is_get = |item: &str| murmur3(format!("{}.{item}", gamedata::IS_GET).as_bytes());
    if hash == murmur3(slot_entry().as_bytes()) {
        return Some(murmur3(slot_entry().replace(SLOT, "Bomb").as_bytes()));
    }
    let at = BAGS.iter().position(|b| is_get(b) == hash)?;
    Some(is_get(BOMB_BAGS[at].get()))
}

/// The pouch slot a Bombchu Bag sets and Bombchus are capped by, as the
/// nut bags set `DekuNut`. The game names the save entry holding the bag
/// from it (`CurrentExpansionActor_` and the slot), as it does the six
/// slots it ships with.
pub const SLOT: &str = "Bombchu";

/// The saved entry naming the Bombchu Bag Link holds.
pub fn slot_entry() -> String {
    format!("CurrentExpansionActor_{SLOT}")
}

/// A bag's expansion: the GameParameterTable component naming it, and
/// its fields.
const EXPANSION: &str = "ExpansionMaxNumActor";
const EXPANDS: &str = "Actor";
const SLOT_FIELD: &str = "GameData";

/// A pouch row's or param's list of actors, as each holds it: a list, or
/// one string of them joined by commas.
fn actors(value: &Byml) -> Vec<String> {
    match value {
        Byml::Array(a) => (a.iter())
            .filter_map(|v| Some(v.as_string().ok()?.to_string()))
            .collect(),
        Byml::String(s) => s.split(',').map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

/// `actors` written back in the form `like` holds them.
fn actors_like(like: &Byml, actors: Vec<String>) -> Byml {
    match like {
        Byml::String(_) => Byml::String(actors.join(",").into()),
        _ => Byml::Array(actors.into_iter().map(|a| Byml::String(a.into())).collect()),
    }
}

/// `map` expands only `keep`'s actors in `key`, and stops filling
/// Bombchus with the bag: Bombchus leave the bomb bags, and a Bombchu Bag
/// holds nothing else.
fn expand_only(map: &mut BymlMap, key: &str, keep: impl Fn(&str) -> bool) -> Result<()> {
    let value = map.get(key).with_context(|| format!("no {key}"))?;
    let kept = actors(value).into_iter().filter(|a| keep(a)).collect();
    let value = actors_like(value, kept);
    map.insert(key.into(), value);
    map.remove(&**field::ACTOR_FULL_ONLY_IF_IS_GET_WHEN_GET_BAG);
    Ok(())
}

fn is_chu(actor: &str) -> bool {
    pouch_actor(item::BOMBCHU)
        .as_string()
        .is_ok_and(|chu| actor == chu.as_str())
}

/// A Bombchu Bag's pouch row or expansion: Bombchus alone, in their slot.
fn own(map: &mut BymlMap, list: &str, slot: &str) -> Result<()> {
    expand_only(map, list, is_chu)?;
    map.insert(slot.into(), Byml::String(SLOT.into()));
    Ok(())
}

/// `name`'s GameParameterTable and expansion, copied from `template`'s
/// and holding Bombchus alone. Each is `(path, doc, the file it copies)`.
fn expansion(template: &ActorPack, name: &str) -> Result<[(String, BymlDoc, String); 2]> {
    let table_like = path_of(template, TABLE)?;
    let table_doc = BymlDoc::parse(&raw(template, &table_like)?)?;
    let table = table_doc.root.as_map()?;
    let expansion_like = pack_path(&component(table, EXPANSION).context("no expansion")?);
    let expansion_at = renamed(&expansion_like, name);
    let expansion_doc = BymlDoc::parse(&raw(template, &expansion_like)?)?;
    let mut expansion = resolved(template, &expansion_like)?;
    own(&mut expansion, EXPANDS, SLOT_FIELD)?;
    let table = with_ref(table, EXPANSION, &expansion_at)?;
    Ok([
        (
            renamed(&table_like, name),
            doc(table, &table_doc),
            table_like,
        ),
        (expansion_at, doc(expansion, &expansion_doc), expansion_like),
    ])
}

/// `name`'s files over `template`'s pack: its own actor, model info
/// (showing `model`) and pouch content as a key ring's, and its own
/// expansion.
fn files(template: &ActorPack, name: &str, model: &str) -> Result<Vec<(String, BymlDoc, String)>> {
    let mut files = key_tints::copied(template, name, model)?;
    let [table, expansion] = expansion(template, name)?;
    let (_, actor, _) = files.first_mut().context("no actor")?;
    *actor = doc(with_ref(actor.root.as_map()?, TABLE, &table.0)?, actor);
    files.extend([table, expansion]);
    Ok(files)
}

/// The label naming `item`, if it's a Bombchu Bag: its pack lives only in
/// the seed, and it answers to its own name.
pub fn name_label(item: &str) -> Option<String> {
    BAGS.contains(&item)
        .then(|| format!("{item}{}", key_tints::TEXT_PARTS[0]))
}

/// `item`'s pack if it's a Bombchu Bag, built as [`write`] writes it.
pub fn copy_pack(romfs: &Romfs, item: &str) -> Result<Option<ActorPack>> {
    let Some(at) = BAGS.iter().position(|b| *b == item) else {
        return Ok(None);
    };
    let template = ActorPack::load(romfs, BOMB_BAGS[at])?;
    clone_pack(&template, item, files(&template, item, &look::model(at))?).map(Some)
}

/// Each bag's GameData: its `IsGet` and the rest of the bomb bag's entries,
/// its pouch count, and the saved slot naming the bag Link holds.
pub fn game_data(root: &mut Byml) -> Result<()> {
    for (bag, like) in BAGS.iter().zip(BOMB_BAGS) {
        gamedata::copy_item(root, like.get(), bag)?;
    }
    gamedata::add_string64s(root, &[slot_entry()])?;
    let cap = format!("{}.{SLOT}", field::MAX_NUM_GAME_DATA);
    gamedata::copy_member(root, CAPS, "DekuNut", SLOT, &cap)
}

/// The Struct of saved ammo caps, a member per pouch slot (a row's
/// `MaxNumGameData`). A bag's get sets its slot's cap there, then names
/// the bag in the slot. A slot with no member gets neither, so Link holds
/// none of its ammo and a chest of it gives a Red Rupee.
const CAPS: u32 = 0xf80f8077;

/// Each Bombchu Bag, its model, its pouch row and the bomb bags it takes
/// Bombchus from, for `tables` to write. Each bag shows its bomb bag's
/// model in the Bombchu's blue.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    for (at, (bag, like)) in BAGS.iter().zip(BOMB_BAGS).enumerate() {
        let model = look::paint(romfs, builder, at)?;
        let template = ActorPack::load(romfs, like)?;
        write_clone(builder, &template, bag, files(&template, bag, &model)?)?;
        tables.copy_item(bag, like)?;
        tables.name_itself(bag)?;
        tables.show(bag, like, &model)?;
    }
    let mut failed = None;
    let rows = tables.edit_pouch(|row| {
        let id = row
            .get(rsdb::ROW_ID)
            .and_then(|v| v.as_string().ok())
            .map(|s| s.to_string());
        let id = id.as_deref().unwrap_or_default();
        let list = &**field::EXPANSION_MAX_NUM_ACTOR;
        let done = if BAGS.contains(&id) {
            own(row, list, field::MAX_NUM_GAME_DATA)
        } else if BOMB_BAGS.iter().any(|b| b.get() == id) {
            expand_only(row, list, |a| !is_chu(a))
        } else {
            return false;
        };
        if let Err(e) = done {
            failed.get_or_insert(e);
        }
        true
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    ensure!(rows == BAGS.len() + BOMB_BAGS.len(), "{rows} bag rows");
    for like in BOMB_BAGS {
        let template = ActorPack::load(romfs, like)?;
        let table = resolved(&template, &path_of(&template, TABLE)?)?;
        let at = pack_path(&component(&table, EXPANSION).context("no expansion")?);
        builder.edit_param(&[like.get()], &at, |param| {
            let mut doc = BymlDoc::parse(param)?;
            expand_only(doc.root.as_mut_map()?, EXPANDS, |a| !is_chu(a))?;
            Ok(doc.to_binary())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_tints::MADE;

    use crate::standins::POUCH_ACTOR_INFO;
    use ounce_formats::pack_file;

    fn row<'a>(doc: &'a BymlDoc, id: &str) -> &'a BymlMap {
        let rows = doc.root.as_array().unwrap();
        let found = rows.iter().find(|r| crate::standins::row_id(r) == id);
        found.unwrap().as_map().unwrap()
    }

    fn listed(map: &BymlMap) -> Vec<String> {
        actors(&map[&**field::EXPANSION_MAX_NUM_ACTOR])
    }

    #[test]
    fn the_bags_hold_bombchus_alone_and_the_bomb_bags_let_them_go() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-chubags-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(&romfs, &mut builder, &mut tables).unwrap();
        tables.write(&mut builder).unwrap();
        builder.finish().unwrap();
        let read = |rel: &str| {
            let raw = std::fs::read(dir.join("romfs").join(rel)).unwrap();
            romfs.dicts().decompress(rel, &raw).unwrap()
        };
        let pouch = BymlDoc::parse(&read(POUCH_ACTOR_INFO)).unwrap();
        let chu = pouch_actor(item::BOMBCHU).as_string().unwrap().to_string();
        for (at, (bag, like)) in BAGS.iter().zip(BOMB_BAGS).enumerate() {
            let model = look::model(at);
            let painted = dir
                .join("romfs")
                .join(crate::medallions::model_path(&model));
            assert!(painted.exists(), "{model}");
            let own = row(&pouch, bag);
            assert_eq!(listed(own), std::slice::from_ref(&chu), "{bag}");
            let slot = own[&**field::MAX_NUM_GAME_DATA].as_string().unwrap();
            assert_eq!(slot.as_str(), SLOT);
            assert!(!own.contains_key(&**field::ACTOR_FULL_ONLY_IF_IS_GET_WHEN_GET_BAG));
            let bombs = listed(row(&pouch, like.get()));
            assert!(!bombs.is_empty() && !bombs.contains(&chu), "{like}");
            let pack = ActorPack::from_bytes(bag, read(&pack_file(bag))).unwrap();
            let table = resolved(&pack, &path_of(&pack, TABLE).unwrap()).unwrap();
            let at = pack_path(&component(&table, EXPANSION).unwrap());
            assert!(at.contains(bag), "{at}");
            let expansion = resolved(&pack, &at).unwrap();
            assert_eq!(actors(&expansion[EXPANDS]), std::slice::from_ref(&chu));
            let slot = expansion[SLOT_FIELD].as_string().unwrap();
            assert_eq!(slot.as_str(), SLOT);
            let pack = ActorPack::from_bytes(like, read(&pack_file(like))).unwrap();
            let table = resolved(&pack, &path_of(&pack, TABLE).unwrap()).unwrap();
            let at = pack_path(&component(&table, EXPANSION).unwrap());
            let expansion = resolved(&pack, &at).unwrap();
            let bombs = actors(&expansion[EXPANDS]);
            assert!(!bombs.is_empty() && !bombs.contains(&chu), "{like}");
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_stand_in_reads_the_bag_it_shows() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let pack = copy_pack(&romfs, BAGS[1]).unwrap().unwrap();
        assert_eq!(pack.name, BAGS[1]);
        assert!(copy_pack(&romfs, item::BOMB_BAG_SMALL).unwrap().is_none());
    }

    #[test]
    fn the_save_keeps_which_bag_link_holds() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        game_data(&mut doc.root).unwrap();
        let gd = gamedata::GameData::parse(&doc.to_binary()).unwrap();
        let slot = ounce_formats::murmur3(slot_entry().as_bytes());
        assert_eq!(slot, zootr_hook::switches::BOMBCHU_SLOT);
        assert_eq!(gd.kind(slot), Some("String64"));
        // The pouch count the hook asks whether Link holds a bag.
        let counts: std::collections::HashMap<u32, u32> = gd
            .members(gamedata::POUCH_COUNTS)
            .unwrap()
            .into_iter()
            .collect();
        let mut seen = std::collections::HashSet::new();
        for bag in BAGS {
            let count = counts[&ounce_formats::murmur3(bag.as_bytes())];
            let inner = gd.members(count).unwrap();
            assert!(inner.iter().all(|&(_, v)| seen.insert(v)), "{bag}");
        }
    }

    #[test]
    fn every_bag_is_ours_and_names_itself() {
        for (bag, name) in BAGS.iter().zip([SMALL, MEDIUM, LARGE]) {
            assert!(bag.starts_with(MADE), "{bag}");
            assert_eq!(name.get(), *bag);
        }
    }
}
