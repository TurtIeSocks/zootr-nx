//! Boots for a child. The menu's table greys them out for him, and his
//! model had no group for the boots' armor to hide, so his own fur boots
//! showed through. The seed's `CHILD_GEAR` flag lets the hook make the
//! boots on him and their state reach him (`patch/gear.rs`), and his steps
//! in them are the adult's clips, lent ([`crate::ageless_items`]). See
//! `superpowers/specs/2026-11-23-child-gear-design.md`.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, pack_file};

use crate::game::POUCH_TABLE;
use crate::known::{Name, actor, field};
use crate::standins;

/// The boots' armor actors, as the table names them.
const BOOTS: [&Name; 2] = [actor::ARMOR_ADULT_HEAVY_BOOT, actor::ARMOR_ADULT_HOVER_BOOT];
/// The child's packs that carry his base equipment user file: his own and
/// the menu's model of him.
const PACKS: [&Name; 2] = [actor::PLAYER_CHILD, actor::POUCH_PLAYER_CHILD];
/// The group the boots' armor hides, and his materials it covers.
const GROUP: (&Name, [&Name; 2]) = (
    field::G_BOOTS,
    [field::MT_FUR_BOOTS, field::MT_FUR_BOOTS_ALPHA_MASK],
);

/// His base equipment user file, which only his two packs carry.
fn user_path() -> String {
    let param = field::EQUIPMENT_USER_PARAM;
    format!(
        "Component/{param}/{}.game__component__{param}.bgyml",
        actor::PLAYER
    )
}

/// The pause menu's table at `table` with the boots open to both ages.
fn boots_opened(table: &[u8]) -> Result<Vec<u8>> {
    let (table, changed) = super::unlock(table, |name| BOOTS.iter().any(|b| b.get() == name))?;
    ensure!(
        changed == BOOTS.len(),
        "the pause menu's table has {changed} of the boots limited to one age"
    );
    Ok(table)
}

/// The equipment user file at `param` with `GROUP` added to its hidden
/// groups.
fn grouped(param: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(param)?;
    let list = (doc.root.as_mut_map()?)
        .get_mut(field::HIDDEN_MATERIAL_GROUP_LIST.get())
        .context("no hidden material groups")?;
    let Byml::Array(groups) = list else {
        anyhow::bail!("the hidden material groups aren't a list");
    };
    let (name, materials) = GROUP;
    let has = |g: &Byml| {
        let group = g.as_map().ok().and_then(|m| m.get(field::GROUP_NAME.get()));
        group
            .and_then(|n| n.as_string().ok())
            .is_some_and(|n| n.as_str() == name.get())
    };
    ensure!(!groups.iter().any(has), "{name} is already a group");
    let mut group = BymlMap::default();
    group.insert(
        field::GROUP_NAME.get().into(),
        Byml::String(name.get().into()),
    );
    let materials = materials.map(|m| Byml::String(m.get().into()));
    group.insert(
        field::MATERIAL_NAME_LIST.get().into(),
        Byml::Array(materials.into()),
    );
    groups.push(Byml::Map(group));
    Ok(doc.to_binary())
}

/// Give each boots item's row of the pouch table the adult's armor for a
/// child too, so the D-pad's boots shortcut offers them to him. Returns how many
/// rows changed.
pub fn shortcut(tables: &mut standins::Tables) -> Result<usize> {
    let changed = tables.edit_pouch(|row| {
        let text = |key: &Name| row.get(key.get()).and_then(|v| v.as_string().ok());
        let boots = text(field::ACTION_GUIDE_TYPE).is_some_and(|t| t == field::BOOT_SHORTCUT.get());
        let Some(adult) = text(field::ADULT_EQUIPMENT).cloned().filter(|_| boots) else {
            return false;
        };
        row.insert(field::CHILD_EQUIPMENT.get().into(), Byml::String(adult));
        true
    })?;
    ensure!(changed > 0, "no boots row in the pouch table");
    Ok(changed)
}

/// Write the setting's files: the table with the boots open to a child,
/// and his packs with the group their armor hides.
pub fn write(builder: &mut ModBuilder) -> Result<()> {
    let table = boots_opened(&builder.read(POUCH_TABLE)?)?;
    builder.write(POUCH_TABLE, &table)?;
    let path = user_path();
    let mut sizes = None;
    for name in PACKS {
        let pack = ActorPack::from_bytes(name, builder.read(&pack_file(name))?)?;
        let old = standins::raw(&pack, &path)?;
        let new = grouped(&old)?;
        sizes = Some((old.len(), new.len()));
        let mut writer = pack.writer();
        writer.add_file(path.clone(), new);
        builder.write(&pack_file(name), &writer.to_binary())?;
    }
    // Both packs share the file's one RESTBL entry. Grown by bytes alone,
    // the Mac build crashed loading it at New Game.
    let (old, new) = sizes.context("no child pack")?;
    builder.scale(&path, old, new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, item};

    #[test]
    fn every_boots_row_gives_a_child_the_adults_armor() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut tables = standins::Tables::load(&romfs).unwrap();
        // The armor rows name no armor for either age.
        assert_eq!(shortcut(&mut tables).unwrap(), 2);
        for id in [item::HEAVY_BOOT, item::HOVER_BOOT] {
            let row = standins::row(&tables.pouch, id.get()).unwrap();
            assert_eq!(
                row[field::CHILD_EQUIPMENT.get()],
                row[field::ADULT_EQUIPMENT.get()]
            );
        }
    }

    /// Each hidden group in the equipment user file at `bytes`: its name
    /// and materials.
    fn groups(bytes: &[u8]) -> Vec<(String, Vec<String>)> {
        let root = BymlDoc::parse(bytes).unwrap().root;
        let list = root.as_map().unwrap()[field::HIDDEN_MATERIAL_GROUP_LIST.get()]
            .as_array()
            .unwrap();
        let text = |b: &Byml| b.as_string().unwrap().to_string();
        list.iter()
            .map(|g| {
                let g = g.as_map().unwrap();
                let materials = g[field::MATERIAL_NAME_LIST.get()].as_array().unwrap();
                (
                    text(&g[field::GROUP_NAME.get()]),
                    materials.iter().map(text).collect(),
                )
            })
            .collect()
    }

    /// Each Equipment page actor in the table at `bytes`: its name and ages.
    fn rows(bytes: &[u8]) -> Vec<(String, [bool; 2])> {
        let mut root = BymlDoc::parse(bytes).unwrap().root;
        let rows = super::super::equipment(&mut root, &|_: &str| true).unwrap();
        let row = |a: &mut BymlMap| {
            (
                super::super::name(a).unwrap().to_owned(),
                super::super::ages(a),
            )
        };
        rows.into_iter().map(row).collect()
    }

    #[test]
    fn the_boots_open_to_a_child_and_nothing_else_moves() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let (before, after) = (rows(&vanilla), rows(&boots_opened(&vanilla).unwrap()));
        assert_eq!(before.len(), after.len());
        let mut opened = 0;
        for ((name, was), (_, now)) in before.iter().zip(&after) {
            match BOOTS.iter().any(|b| b.get() == name) {
                true => {
                    opened += 1;
                    assert_eq!((*was, *now), ([false, true], [true, true]), "{name}");
                }
                false => assert_eq!(was, now, "{name} isn't boots"),
            }
        }
        assert_eq!(opened, BOOTS.len());
    }

    #[test]
    fn the_boots_hide_his_fur_boots_in_both_his_packs() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let path = user_path();
        for name in PACKS {
            let pack = ActorPack::load(&romfs, name).unwrap();
            let old = standins::raw(&pack, &path).unwrap();
            let (before, after) = (groups(&old), groups(&grouped(&old).unwrap()));
            assert_eq!(after[..before.len()], before[..], "{name} keeps its groups");
            let fur = [field::MT_FUR_BOOTS, field::MT_FUR_BOOTS_ALPHA_MASK].map(|m| m.to_string());
            assert_eq!(
                after[before.len()..],
                [(field::G_BOOTS.to_string(), fur.to_vec())]
            );
        }
        let mine = ActorPack::load(&romfs, actor::PLAYER_CHILD).unwrap();
        let menu = ActorPack::load(&romfs, actor::POUCH_PLAYER_CHILD).unwrap();
        assert_eq!(
            standins::raw(&mine, &path).unwrap(),
            standins::raw(&menu, &path).unwrap(),
            "one RESTBL entry covers both copies"
        );
    }
}
