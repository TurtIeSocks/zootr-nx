//! Bombchus without a bomb bag. Their pouch param caps them by the bag
//! (`MaxNumGameData: Bomb`), so none can be held until it's found. Swapping
//! that for a fixed cap, as magic beans and skulltula tokens have, frees
//! them. The pouch table (`PouchActorInfo`) repeats the cap per row, so its
//! bombchu rows get the same swap, through the stand-ins' `Tables`: writing
//! the table on its own once dropped every stand-in's pouch row.

use anyhow::{Result, ensure};

use crate::known::{Name, actor, field, item};
use crate::standins::Tables;
use ounce_formats::{Byml, BymlDoc, BymlMap, ModBuilder};

/// Every pack carrying a copy of the param. Each gets the edit, so it holds
/// whichever pack loads first.
const PACKS: [&Name; 11] = [
    item::BOMBCHU,
    actor::OBJ_BOMBCHU,
    actor::POUCH_BOMBCHU,
    actor::SHOP_BOMBCHU_BUNDLE_01,
    actor::SHOP_BOMBCHU_BUNDLE_02,
    actor::SHOP_BOMBCHU_BUNDLE_03,
    actor::SHOP_BOMBCHU_BUNDLE_04,
    actor::SHOP_BOMBCHU_BUNDLE_05,
    actor::SHOP_BOMBCHU_BUNDLE_06,
    actor::SHOP_BOMBCHU_BUNDLE_07,
    actor::SHOP_BOMBCHU_BUNDLE_08,
];
const ACTOR: &str = "Work/Actor/Item_Bombchu.engine__actor__ActorParam.gyml";
const PARAM: &str =
    "Component/PouchContentParam/Item_Bombchu.game__component__PouchContentParam.bgyml";
/// As many as the PC build allows. The dev switches hold them here too.
const CAP: i32 = zootr_hook::switches::FREE_BOMBCHUS_CAP;

/// Cap `map` by `CAP` instead of the bomb bag.
fn cap(map: &mut BymlMap) {
    map.remove(&**field::MAX_NUM_GAME_DATA);
    map.remove(&**field::SMALL_BAG_ACTOR);
    map.insert(field::FIXED_MAX_NUM.to_string().into(), Byml::I32(CAP));
}

/// The param, capped by `CAP` instead of the bomb bag.
fn free(param: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(param)?;
    cap(doc.root.as_mut_map()?);
    Ok(doc.to_binary())
}

/// Whether a `PouchActorInfo` row is a bombchu, or a bundle of them, that
/// the bomb bag caps.
fn capped_by_bag(row: &BymlMap) -> bool {
    let is = |key: &str, value: &str| {
        row.get(key)
            .and_then(|v| v.as_string().ok())
            .is_some_and(|v| v == value)
    };
    is(field::MAX_NUM_GAME_DATA, "Bomb")
        && (is("__RowId", item::BOMBCHU) || is(field::BUNDLE_ACTOR, ACTOR))
}

/// Free every bombchu row of the pouch table.
fn free_rows(tables: &mut Tables) -> Result<usize> {
    tables.edit_pouch(|row| {
        let hit = capped_by_bag(row);
        if hit {
            cap(row);
        }
        hit
    })
}

/// Write every pack with the freed param, and free the pouch table's rows
/// for `tables` to write.
pub fn write(builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    ensure!(
        free_rows(tables)? > 0,
        "no bombchu row in the pouch table is capped by the bomb bag"
    );
    builder.edit_param(&PACKS.map(|p| &**p), PARAM, free)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field, item};
    use crate::standins::POUCH_ACTOR_INFO;
    use ounce_formats::{Romfs, pack_file};
    use roead::sarc::Sarc;
    use std::path::Path;

    fn param(romfs: &Romfs, pack: &str) -> Option<Vec<u8>> {
        let sarc = Sarc::new(romfs.read(&pack_file(pack)).ok()?).ok()?;
        sarc.get_data(PARAM).map(<[u8]>::to_vec)
    }

    #[test]
    fn the_cap_replaces_the_bag() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let freed = free(&param(&romfs, PACKS[0]).unwrap()).unwrap();
        let root = BymlDoc::parse(&freed).unwrap().root;
        let map = root.as_map().unwrap();
        assert_eq!(map.get(&**field::FIXED_MAX_NUM), Some(&Byml::I32(CAP)));
        assert!(
            !map.contains_key(&**field::MAX_NUM_GAME_DATA)
                && !map.contains_key(&**field::SMALL_BAG_ACTOR)
        );
        assert_eq!(map.get(&**field::ENABLE_STOCK), Some(&Byml::Bool(true)));
    }

    #[test]
    fn every_pack_is_written_freed_and_reads_back() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-chus-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(&mut builder, &mut tables).unwrap();
        tables.write(&mut builder).unwrap();
        assert_eq!(builder.finish().unwrap().len(), PACKS.len() + 4);
        for pack in PACKS {
            let raw = std::fs::read(dir.join("romfs").join(pack_file(pack))).unwrap();
            let data = romfs.dicts().decompress(&pack_file(pack), &raw).unwrap();
            let sarc = Sarc::new(data).unwrap();
            let doc = BymlDoc::parse(sarc.get_data(PARAM).unwrap()).unwrap();
            let fixed = doc
                .root
                .as_map()
                .unwrap()
                .get(&**field::FIXED_MAX_NUM)
                .cloned();
            assert_eq!(fixed, Some(Byml::I32(CAP)), "{pack}");
            assert_eq!(
                sarc.len(),
                Sarc::new(romfs.read(&pack_file(pack)).unwrap())
                    .unwrap()
                    .len()
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn rows(romfs: &Romfs, path: &Path) -> Vec<(String, Byml)> {
        let raw = std::fs::read(path).unwrap();
        let doc = BymlDoc::parse(&romfs.dicts().decompress(POUCH_ACTOR_INFO, &raw).unwrap());
        let doc = doc.unwrap();
        let rows = doc.root.as_array().unwrap();
        rows.iter()
            .map(|r| {
                (
                    r.as_map().unwrap()["__RowId"]
                        .as_string()
                        .unwrap()
                        .to_string(),
                    r.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn the_pouch_table_frees_only_the_bombchu_rows() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-chus-rows-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        assert_eq!(free_rows(&mut tables).unwrap(), 2);
        tables.write(&mut builder).unwrap();
        let written = builder.finish().unwrap();
        assert!(written.iter().any(|f| f == POUCH_ACTOR_INFO));
        let old = rows(&romfs, &romfs.root().join(POUCH_ACTOR_INFO));
        let new = rows(&romfs, &dir.join("romfs").join(POUCH_ACTOR_INFO));
        assert_eq!(old.len(), new.len());
        let mut freed = Vec::new();
        for ((id, before), (_, after)) in old.iter().zip(&new) {
            if before == after {
                continue;
            }
            freed.push(id.as_str());
            let map = after.as_map().unwrap();
            assert_eq!(
                map.get(&**field::FIXED_MAX_NUM),
                Some(&Byml::I32(CAP)),
                "{id}"
            );
            assert!(
                !map.contains_key(&**field::MAX_NUM_GAME_DATA)
                    && !map.contains_key(&**field::SMALL_BAG_ACTOR)
            );
            // Nothing else about the row moved.
            let mut rest = before.as_map().unwrap().clone();
            rest.remove(&**field::MAX_NUM_GAME_DATA);
            rest.remove(&**field::SMALL_BAG_ACTOR);
            let mut map = map.clone();
            map.remove(&**field::FIXED_MAX_NUM);
            assert_eq!(rest, map, "{id}");
        }
        assert_eq!(freed, [item::BOMBCHU, actor::SHOP_BOMBCHU_BUNDLE_03]);
        // The bomb and the bags keep their cap.
        let bomb = new.iter().find(|(id, _)| id == item::BOMB).unwrap();
        assert!(
            bomb.1
                .as_map()
                .unwrap()
                .contains_key(&**field::MAX_NUM_GAME_DATA)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_row_the_bag_doesnt_cap_is_left_alone() {
        let mut row = BymlMap::default();
        row.insert(
            "__RowId".into(),
            Byml::String(item::MAGIC_BEAN.to_string().into()),
        );
        row.insert(field::FIXED_MAX_NUM.to_string().into(), Byml::I32(10));
        assert!(!capped_by_bag(&row));
        let mut bomb = BymlMap::default();
        bomb.insert(
            "__RowId".into(),
            Byml::String(item::BOMB.to_string().into()),
        );
        bomb.insert(
            field::MAX_NUM_GAME_DATA.to_string().into(),
            Byml::String("Bomb".into()),
        );
        assert!(!capped_by_bag(&bomb));
        bomb.insert(
            field::BUNDLE_ACTOR.to_string().into(),
            Byml::String(ACTOR.into()),
        );
        assert!(capped_by_bag(&bomb));
        cap(&mut bomb);
        assert!(
            !capped_by_bag(&bomb) && bomb.get(&**field::FIXED_MAX_NUM) == Some(&Byml::I32(CAP))
        );
        assert_eq!(bomb.len(), 3);
    }

    #[test]
    fn packs_lists_every_copy_of_the_param() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        assert_eq!(crate::game::packs_holding(&romfs, PARAM), PACKS);
    }
}
