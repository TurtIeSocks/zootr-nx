//! The prize in Dampe's dig. His flow only names the piece (#224 spawns
//! `actor::OBJ_HEART_PIECE`), and his AI graph wakes the `HeartPiece` holder slot,
//! `actor::OBJ_HEART_PIECE_DIGGING_HOLE`. That actor plays its dig animation, the rise out
//! of the soil and the fall back, on its model's `Root` bones. A moved
//! prize keeps the actor and lends it the prize's model, so the item rises
//! the way the piece does.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{Byml, BymlDoc, ModBuilder, Romfs};
use roead::sarc::{Sarc, SarcWriter};

use crate::checks::Drop;
use crate::gifts::{self, GIFTS};
use crate::known::{Name, actor, field};
use crate::settings::DungeonKeys;
use crate::standins::item_pack;

/// Dampe's flow, and the actor his dig shows.
const DAMPE: &Name = actor::NPC_HYLIA_DAMPE_001;
const PIECE: &Name = actor::OBJ_HEART_PIECE_DIGGING_HOLE;
const COMPONENTS: &str = "Components";
const MODEL: &Name = field::MODEL_INFO_REF;
const PARENT: &str = "$parent";

fn rel(actor: &str) -> String {
    format!("Pack/Actor/{actor}.pack.zs")
}

fn actor_param(actor: &str) -> String {
    format!("Actor/{actor}.engine__actor__ActorParam.bgyml")
}

/// A model to lend: the reference an actor param names it by, and its
/// files, `$parent` chain included, as the item's pack stores them.
struct Model {
    reference: String,
    files: Vec<(String, Vec<u8>)>,
}

/// The model `item`'s own actor is drawn with.
fn model(romfs: &Romfs, item: &str, keys: DungeonKeys) -> Result<Model> {
    let pack = item_pack(romfs, item, keys)?;
    let param = pack
        .actor_param()?
        .with_context(|| format!("{item} has no actor of its own"))?;
    let reference = param
        .get(COMPONENTS)
        .and_then(|c| c.as_map().ok()?.get(MODEL.get())?.as_string().ok())
        .with_context(|| format!("{item} has no model"))?
        .to_string();
    let mut files = Vec::new();
    let mut next = Some(reference.clone());
    while let Some(at) = next {
        let (path, data) = pack
            .find(&at)
            .with_context(|| format!("{item}'s pack has no {at}"))?;
        let doc = Byml::from_binary(data).with_context(|| format!("{item}: {path}"))?;
        next = doc
            .as_map()?
            .get(PARENT)
            .and_then(|p| p.as_string().ok())
            .map(ToString::to_string);
        files.push((path, data.to_vec()));
    }
    Ok(Model { reference, files })
}

/// The piece's own actor param, drawing `reference`.
fn lend(param: &[u8], reference: &str) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(param)?;
    let components = doc
        .root
        .as_mut_map()?
        .get_mut(COMPONENTS)
        .with_context(|| format!("{PIECE} has no {COMPONENTS}"))?;
    components
        .as_mut_map()?
        .insert(MODEL.get().into(), Byml::String(reference.into()));
    Ok(doc.to_binary())
}

/// Make the dig show the prize this seed puts there, unless it's the heart
/// piece.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    drops: &HashMap<u64, Drop>,
    keys: DungeonKeys,
) -> Result<()> {
    let gift = GIFTS
        .iter()
        .find(|g| g.flow == DAMPE)
        .context("no gift for Dampe's dig")?;
    let item = gifts::item_at(drops, gift.hash(), gift.item);
    if item == gift.item {
        return Ok(());
    }
    let model = model(romfs, item, keys)?;
    let rel = rel(PIECE);
    let raw = romfs.read(&rel)?;
    let sarc = Sarc::new(&raw).with_context(|| format!("reading {rel}"))?;
    let path = actor_param(PIECE);
    let old = sarc
        .get_data(&path)
        .with_context(|| format!("{rel} has no {path}"))?;
    let new = lend(old, &model.reference)?;
    builder.resize(&path, old.len(), new.len())?;
    let mut packed = SarcWriter::from_sarc(&sarc);
    packed.add_file(&path, new);
    for (file, data) in model.files {
        // The game ships these, so each already has its RESTBL entry.
        builder.resize(&file, data.len(), data.len())?;
        packed.add_file(&file, data);
    }
    builder.write(&rel, &packed.to_binary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::item;

    const PRIZE: &Name = item::BOMBCHU_BUNDLE_SMALL;

    /// The piece's pack as `write` leaves it for `PRIZE`, and the files it
    /// wrote.
    fn written(romfs: &Romfs) -> (Vec<u8>, Vec<String>) {
        let gift = GIFTS.iter().find(|g| g.flow == DAMPE).unwrap();
        let dir = std::env::temp_dir().join(format!("zootr-dig-{}", std::process::id()));
        let mut builder = ModBuilder::new(romfs, &dir).unwrap();
        let drops = HashMap::from([(gift.hash(), Drop::from(PRIZE.to_string()))]);
        write(romfs, &mut builder, &drops, DungeonKeys::Rings).unwrap();
        let files = builder.finish().unwrap();
        let raw = std::fs::read(dir.join("romfs").join(rel(PIECE))).unwrap();
        let data = romfs.dicts().decompress(&rel(PIECE), &raw).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        (data, files)
    }

    fn components(sarc: &Sarc) -> Byml {
        let doc = BymlDoc::parse(sarc.get_data(&actor_param(PIECE)).unwrap()).unwrap();
        doc.root.as_map().unwrap()[COMPONENTS].clone()
    }

    #[test]
    fn the_piece_draws_the_prize_and_keeps_its_rise() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (data, files) = written(&romfs);
        assert_eq!(files, [rel(PIECE), romfs.restbl_rel().unwrap()]);
        let sarc = Sarc::new(&data).unwrap();
        let prize = model(&romfs, PRIZE, DungeonKeys::Rings).unwrap();
        let raw = romfs.read(&rel(PIECE)).unwrap();
        let (mut now, mut vanilla) = (components(&sarc), components(&Sarc::new(&raw).unwrap()));
        let lent = now.as_mut_map().unwrap().remove(MODEL.get()).unwrap();
        assert_eq!(lent.as_string().unwrap(), prize.reference.as_str());
        // Only the model moved: the animation player and the rest stay.
        vanilla.as_mut_map().unwrap().remove(MODEL.get());
        assert_eq!(now, vanilla);
        for (file, bytes) in &prize.files {
            assert_eq!(sarc.get_data(file), Some(bytes.as_slice()), "{file}");
        }
    }

    #[test]
    fn every_item_in_the_pool_has_a_model_to_lend() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for check in crate::checks::all(&romfs).unwrap() {
            model(&romfs, &check.item, DungeonKeys::Rings).unwrap();
        }
    }

    #[test]
    fn the_heart_piece_leaves_the_pack_alone() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-dig-vanilla-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        write(&romfs, &mut builder, &HashMap::new(), DungeonKeys::Rings).unwrap();
        assert_eq!(builder.finish().unwrap().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
