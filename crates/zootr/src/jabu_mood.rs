//! Jabu's mood under the Adult Jabu-Jabu setting. His graph plays his happy
//! ("Fine") animations once one flag reads true, the Stone of Water step
//! after Barinade. With the setting on it reads [`FINE`] instead: that step
//! for the child, and for adult the Water sage, so adult Jabu is sick while
//! the fountain is frozen and happy once it thaws.
//!
//! ```text
//! FINE = (child and after Barinade) or (adult and Water sage done)
//! ```

use anyhow::{Context, Result, ensure};
use ounce_formats::ainb::{Graph, Patch};
use ounce_formats::{ModBuilder, Romfs};
use roead::sarc::{Sarc, SarcWriter};

use crate::flags::actor_pack;
use crate::gamedata::{GameData, Op};
use crate::known::{actor, field, flag};
use crate::settings::AdultJabu;

/// The BoolExp Jabu's graph reads for his mood with the setting on.
const FINE: &str = "ZootrJabuFine";

/// [`FINE`]'s program, when the setting is on.
pub fn exps(gd: &GameData, mode: AdultJabu) -> Result<Vec<(String, Vec<Op>)>> {
    if mode == AdultJabu::Off {
        return Ok(Vec::new());
    }
    let child = Op::read(gd, flag::SEQUENCE_IS_CHILDHOOD.get())?;
    let barinade = Op::read(gd, flag::GET_STONE_OF_WATER_IS_AFTER_STEP14_EXP.get())?;
    let sage = Op::read(gd, flag::SAGE_OF_WATER_IS_COMPLETED_EXP.get())?;
    let program = vec![
        child,
        barinade,
        Op::And,
        child,
        Op::Not,
        sage,
        Op::And,
        Op::Or,
    ];
    Ok(vec![(FINE.into(), program)])
}

/// Writes Jabu's pack with his mood read from [`FINE`], when the setting is
/// on.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, mode: AdultJabu) -> Result<()> {
    if mode == AdultJabu::Off {
        return Ok(());
    }
    let path = actor_pack(actor::NPC_JABU_JABU.get());
    let data = romfs.read(&path)?;
    let pack = Sarc::new(&data)?;
    let (file, vanilla) = (pack.files())
        .filter_map(|f| Some((f.name?, f.data)))
        .find(|(name, data)| name.ends_with(".ainb") && mood_node(data).is_ok())
        .context("no graph in Jabu's pack reads his mood")?;
    let (file, old) = (file.to_string(), vanilla.len());
    let new = retarget(vanilla)?;
    let len = new.len();
    let packed = SarcWriter::from_sarc(&pack)
        .with_file(file.as_str(), new)
        .to_binary();
    builder.write(&path, &packed)?;
    builder.resize(&file, old, len)
}

/// The one node in `graph` that reads the step after Barinade.
fn mood_node(graph: &[u8]) -> Result<usize> {
    let g = Graph::parse(graph)?;
    let reads = |name: &str| {
        (g.nodes.iter().enumerate())
            .filter(|(_, n)| n.name == field::QUERY_GET_NON_STRUCT_GAME_DATA_BOOL_EXP.get())
            .filter(|(_, n)| n.str(field::GAME_DATA_FULL_NAME.get()) == Some(name))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    };
    let found = reads(flag::GET_STONE_OF_WATER_IS_AFTER_STEP14_EXP.get());
    ensure!(found.len() == 1, "{} nodes read Jabu's mood", found.len());
    Ok(found[0])
}

/// `vanilla` with its mood node reading [`FINE`].
fn retarget(vanilla: &[u8]) -> Result<Vec<u8>> {
    let node = mood_node(vanilla)?;
    let mut patch = Patch::new(vanilla.to_vec())?;
    patch.set_str(node, field::GAME_DATA_FULL_NAME.get(), FINE)?;
    Ok(patch.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(romfs: &Romfs) -> Vec<u8> {
        let data = romfs.read(&actor_pack(actor::NPC_JABU_JABU.get())).unwrap();
        let pack = Sarc::new(&data).unwrap();
        let found = (pack.files())
            .find(|f| f.name.is_some_and(|n| n.ends_with(".ainb")) && mood_node(f.data).is_ok());
        found.unwrap().data.to_vec()
    }

    #[test]
    fn jabus_mood_reads_ours_with_the_setting_on() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let new = retarget(&graph(&romfs)).unwrap();
        let g = Graph::parse(&new).unwrap();
        let reading = (g.nodes.iter())
            .filter(|n| n.str(field::GAME_DATA_FULL_NAME.get()) == Some(FINE))
            .count();
        assert_eq!(reading, 1);
        assert!(mood_node(&new).is_err(), "nothing reads the vanilla step");
    }

    #[test]
    fn the_child_keeps_barinade_and_adult_waits_for_the_thaw() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        assert!(exps(&gd, AdultJabu::Off).unwrap().is_empty());
        let (name, program) = exps(&gd, AdultJabu::Always).unwrap().remove(0);
        assert_eq!(name, FINE);
        let mut doc =
            ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH).unwrap()).unwrap();
        crate::gamedata::add_exps(&mut doc.root, &[(name, program)]).unwrap();
        let new = GameData::parse(&doc.to_binary()).unwrap();
        let name = |n: &crate::known::Name| new.name(ounce_formats::murmur3(n.get().as_bytes()));
        let (child, barinade, sage) = (
            name(flag::SEQUENCE_IS_CHILDHOOD),
            name(flag::GET_STONE_OF_WATER_IS_AFTER_STEP14_EXP),
            name(flag::SAGE_OF_WATER_IS_COMPLETED_EXP),
        );
        assert_eq!(
            new.expr(FINE).unwrap().unwrap(),
            format!("({child} && {barinade}) || (!{child} && {sage})")
        );
    }
}
