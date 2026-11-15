//! Bombs without the Goron Bracelet. Link lifts a bomb out of the bag, and
//! the bomb's lift param asks for the bracelet's grade (`LiftGrade: Low`).
//! Vanilla hands over the bracelet before any bag, so only a shuffled bag
//! shows it: Link raises his arms and puts the bomb back. The bomb drops to
//! `None`, as pots and Bombchus have. The bomb flower's fruit names `Low` in
//! its own param, so it still needs the bracelet.

use anyhow::Result;
use ounce_formats::{Byml, BymlDoc, ModBuilder, murmur3};

use crate::known::{Name, actor, field, item};

/// The bomb's liftable param, by its path in the pack.
fn param() -> String {
    format!(
        "Component/{}/{}.game__component__{}.bgyml",
        field::LIFTABLE_PARAM,
        item::BOMB,
        field::LIFTABLE_PARAM
    )
}
const GRADE: &Name = field::LIFT_GRADE;
/// Every pack carrying a copy of the param. Each gets the edit, so it holds
/// whichever pack loads first.
const PACKS: [&Name; 5] = [
    item::BOMB,
    actor::NPC_HYLIA_IMPERIAL_SOLDIER_BOMB,
    actor::OBJ_BOMB_FLOWER_A_01,
    actor::OBJ_BOMB_FLOWER_A_04,
    actor::OBJ_BOMB_FLOWER_FRUIT_FOR_BREAK_OBJ_DEATH_MT_TRAIL_BROKEN_ROCK_A_01,
];

fn grade(name: &str) -> Byml {
    Byml::U32(murmur3(name.as_bytes()))
}

/// The param, liftable by anyone.
fn freed(param: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(param)?;
    doc.root
        .as_mut_map()?
        .insert(GRADE.get().into(), grade("None"));
    Ok(doc.to_binary())
}

/// Write every pack with the freed param.
pub fn write(builder: &mut ModBuilder) -> Result<()> {
    builder.edit_param(&PACKS.map(|p| &**p), &param(), freed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::field;
    use ounce_formats::{ActorPack, pack_file};
    use roead::sarc::Sarc;

    fn fruit() -> String {
        format!(
            "Component/{}/{}_Fruit.game__component__{}.bgyml",
            field::LIFTABLE_PARAM,
            actor::OBJ_BOMB_FLOWER_A_01.trim_end_matches("_A_01"),
            field::LIFTABLE_PARAM
        )
    }

    fn own_grade(data: &[u8]) -> Option<Byml> {
        Byml::from_binary(data)
            .unwrap()
            .as_map()
            .unwrap()
            .get(&**GRADE)
            .cloned()
    }

    #[test]
    fn the_bomb_and_the_flower_s_fruit_each_ask_for_the_bracelet() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let pack = |name| ActorPack::load(&romfs, name).unwrap();
        assert_eq!(
            own_grade(pack(PACKS[0]).raw(&param()).unwrap()),
            Some(grade("Low"))
        );
        assert_eq!(
            own_grade(pack(PACKS[2]).raw(&fruit()).unwrap()),
            Some(grade("Low"))
        );
    }

    #[test]
    fn every_pack_is_written_freed() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-bombs-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        write(&mut builder).unwrap();
        assert_eq!(builder.finish().unwrap().len(), PACKS.len() + 1);
        for pack in PACKS {
            let raw = std::fs::read(dir.join("romfs").join(pack_file(pack))).unwrap();
            let data = romfs.dicts().decompress(&pack_file(pack), &raw).unwrap();
            let sarc = Sarc::new(data).unwrap();
            assert_eq!(
                own_grade(sarc.get_data(&param()).unwrap()),
                Some(grade("None")),
                "{pack}"
            );
            if let Some(fruit) = sarc.get_data(&fruit()) {
                assert_eq!(own_grade(fruit), Some(grade("Low")), "{pack}");
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn packs_lists_every_copy_of_the_param() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        assert_eq!(crate::game::packs_holding(&romfs, &param()), PACKS);
    }
}
