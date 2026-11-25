//! Boots and tunics for a child. The menu's table greys them out for him
//! ([`super::toggles`] opens each), and his model had no group for their
//! armor to hide, so his own fur boots and green cloth showed through. The
//! seed's `CHILD_GEAR` flag lets the hook make them on him and the boots'
//! state reach him (`patch/gear.rs`), his steps in boots are the adult's
//! clips, lent ([`crate::ageless_items`]), and his tunics are his own
//! ([`tunics`]). See `superpowers/specs/2026-11-23-child-gear-design.md`.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_file};

use super::tunics;
use crate::colors::Colors;
use crate::known::{Name, actor, field};
use crate::standins;

/// The child's packs that carry his base equipment user file: his own and
/// the menu's model of him.
const PACKS: [&Name; 2] = [actor::PLAYER_CHILD, actor::POUCH_PLAYER_CHILD];
/// The groups the gear's armor hides on him, each with his materials it
/// covers: the boots' his fur boots, then the tunics'.
const GROUPS: [(&Name, &[&Name]); 3] = [
    (
        field::G_BOOTS,
        &[field::MT_FUR_BOOTS, field::MT_FUR_BOOTS_ALPHA_MASK],
    ),
    tunics::GROUPS[0],
    tunics::GROUPS[1],
];

/// His base equipment user file, which only his two packs carry.
fn user_path() -> String {
    let param = field::EQUIPMENT_USER_PARAM;
    format!(
        "Component/{param}/{}.game__component__{param}.bgyml",
        actor::PLAYER
    )
}

/// The equipment user file at `param` with `GROUPS` added to its hidden
/// groups.
fn grouped(param: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(param)?;
    let list = (doc.root.as_mut_map()?)
        .get_mut(field::HIDDEN_MATERIAL_GROUP_LIST.get())
        .context("no hidden material groups")?;
    let Byml::Array(groups) = list else {
        anyhow::bail!("the hidden material groups aren't a list");
    };
    for (name, materials) in GROUPS {
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
        let materials = materials.iter().map(|m| Byml::String(m.get().into()));
        group.insert(
            field::MATERIAL_NAME_LIST.get().into(),
            Byml::Array(materials.collect()),
        );
        groups.push(Byml::Map(group));
    }
    Ok(doc.to_binary())
}

/// Give the pouch table's row of each of `boots` the adult's armor for a
/// child too, so the D-pad's boots shortcut offers them to him. Returns how
/// many rows changed.
pub fn shortcut(tables: &mut standins::Tables, boots: &[&Name]) -> Result<usize> {
    let changed = tables.edit_pouch(|row| {
        let text = |key: &Name| row.get(key.get()).and_then(|v| v.as_string().ok());
        let id = row
            .get(crate::rsdb::ROW_ID)
            .and_then(|v| v.as_string().ok());
        let wanted = id.is_some_and(|id| boots.iter().any(|b| b.get() == id));
        let boots = wanted
            && text(field::ACTION_GUIDE_TYPE).is_some_and(|t| t == field::BOOT_SHORTCUT.get());
        let Some(adult) = text(field::ADULT_EQUIPMENT).cloned().filter(|_| boots) else {
            return false;
        };
        row.insert(field::CHILD_EQUIPMENT.get().into(), Byml::String(adult));
        true
    })?;
    ensure!(
        changed == boots.len(),
        "the pouch table has {changed} of {} boots rows",
        boots.len()
    );
    Ok(changed)
}

/// Write what a child's boots and tunics need: his packs with the groups
/// their armor hides, and the armor of his Kokiri Tunic and of each of
/// `tunics`, with its rows in `tables`, in the colors `colors` picks.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut standins::Tables,
    tunics: &[&Name],
    colors: &Colors,
) -> Result<()> {
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
    builder.scale(&path, old, new)?;
    tunics::write(romfs, builder, tables, tunics, colors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, item};

    #[test]
    fn each_boots_row_on_gives_a_child_the_adults_armor_and_no_other() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let both = [item::HEAVY_BOOT, item::HOVER_BOOT];
        for (on, off) in [(both[0], both[1]), (both[1], both[0])] {
            let mut tables = standins::Tables::load(&romfs).unwrap();
            let vanilla = standins::row(&tables.pouch, off.get()).unwrap();
            assert_eq!(shortcut(&mut tables, &[on]).unwrap(), 1);
            let row = standins::row(&tables.pouch, on.get()).unwrap();
            assert_eq!(
                row[field::CHILD_EQUIPMENT.get()],
                row[field::ADULT_EQUIPMENT.get()]
            );
            let other = standins::row(&tables.pouch, off.get()).unwrap();
            assert_eq!(other, vanilla, "{off} stays as it was");
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

    #[test]
    fn the_gear_hides_his_boots_cloth_and_vest_in_both_his_packs() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let path = user_path();
        for name in PACKS {
            let pack = ActorPack::load(&romfs, name).unwrap();
            let old = standins::raw(&pack, &path).unwrap();
            let (before, after) = (groups(&old), groups(&grouped(&old).unwrap()));
            assert_eq!(after[..before.len()], before[..], "{name} keeps its groups");
            let added = GROUPS.map(|(name, materials)| {
                let materials = materials.iter().map(|m| m.to_string()).collect();
                (name.to_string(), materials)
            });
            assert_eq!(after[before.len()..], added);
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
