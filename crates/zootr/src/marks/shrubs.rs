//! Shrubs, which the foliage system draws from leaf types, not models. A
//! marked shrub is a copy of its actor whose FoliageParam names a copy of
//! its leaf type with the leaves' colour set gold. The game reads leaf
//! types from the foliage pack, not an actor's own, so the copy's leaf
//! type and the files named for it go there, and its geometry beside the
//! host's.

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, murmur3};
use roead::sarc::{Sarc, SarcWriter};

use super::marked;
use super::paint::GOLD;
use crate::known::{Name, field};
use crate::rsdb::Tags;
use crate::standins::{Tables, actor_path, doc, path_of, raw, renamed, with_ref, write_clone};

/// The pack every leaf type and the files named for it are in.
const FOLIAGE: &str = "Pack/Foliage.pack.zs";

/// How strongly the gold colours a marked shrub's leaves, 0 to 1.
const STRENGTH: f32 = 0.9;

/// Writes `host`'s marked copy and its gold leaf type, with its host's
/// rows and tags.
pub(super) fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    tags: &mut Tags,
    host: &Name,
) -> Result<()> {
    let name = marked(host)?;
    let template = ActorPack::load(romfs, host)?;
    let (foliage_like, foliage_doc) = foliage(&template)?;
    let leaf = leaf_of(&foliage_doc)?;
    let marked_leaf = marked(&leaf)?;
    write_leaf(romfs, builder, &leaf, &marked_leaf)?;

    let foliage_at = renamed(&foliage_like, &marked_leaf);
    let foliage = pointed(foliage_doc.root.as_map()?, &leaf, &marked_leaf)?;
    let actor_like = actor_path(&template.name);
    let actor_doc = BymlDoc::parse(&raw(&template, &actor_like)?)?;
    let actor = with_ref(
        actor_doc.root.as_map()?,
        field::FOLIAGE_REF.get(),
        &foliage_at,
    )?;
    write_clone(
        builder,
        &template,
        &name,
        [
            (actor_path(&name), doc(actor, &actor_doc), actor_like),
            (foliage_at, doc(foliage, &foliage_doc), foliage_like),
        ],
    )?;
    tables.copy(&name, host)?;
    tags.copy(&name, host)
}

/// The geometry file each of `hosts` draws its leaves with, which the
/// marked copy's is copied from.
#[cfg(test)]
pub(super) fn geometry_files(romfs: &Romfs, hosts: &[&Name]) -> Result<Vec<String>> {
    hosts
        .iter()
        .map(|host| {
            let (_, foliage) = foliage(&ActorPack::load(romfs, host)?)?;
            Ok(geometry(&leaf_of(&foliage)?))
        })
        .collect()
}

/// `actor`'s FoliageParam: its path in the pack, and the document.
fn foliage(actor: &ActorPack) -> Result<(String, BymlDoc)> {
    let path = path_of(actor, field::FOLIAGE_REF.get())?;
    let doc = BymlDoc::parse(&raw(actor, &path)?)?;
    Ok((path, doc))
}

/// The one leaf type a FoliageParam names.
fn leaf_of(foliage: &BymlDoc) -> Result<String> {
    match foliage.root.as_map()?.get(field::SHOOT_LEAF_LIST.get()) {
        Some(Byml::String(s)) => leaf_name(s)
            .map(str::to_owned)
            .context("a leaf type's name"),
        _ => anyhow::bail!("a FoliageParam that names no single leaf type"),
    }
}

/// The leaf type a `Work/Foliage/ResShootLeaf/NAME.….gyml` reference names.
fn leaf_name(reference: &str) -> Option<&str> {
    let file = reference.rsplit('/').next()?;
    file.split('.').next().filter(|n| !n.is_empty())
}

/// Leaf type `leaf`'s geometry.
fn geometry(leaf: &str) -> String {
    format!("Foliage/NodeTreeShootLeaf/{leaf}.foliage.flnt.zs")
}

/// FoliageParam `foliage` naming leaf type `to` where it named `from`.
fn pointed(foliage: &BymlMap, from: &str, to: &str) -> Result<BymlMap> {
    let list = field::SHOOT_LEAF_LIST.get();
    let mut foliage = foliage.clone();
    let reference = match foliage.get(list) {
        Some(Byml::String(s)) => s.replace(from, to),
        _ => anyhow::bail!("a FoliageParam that names no single leaf type"),
    };
    foliage.insert(list.into(), Byml::String(reference.into()));
    Ok(foliage)
}

/// Writes `marked`, leaf type `leaf`'s gold copy: a copy of each file the
/// foliage pack names for `leaf`, its leaf type gold and its FoliageParam
/// naming it, and a copy of its geometry.
fn write_leaf(romfs: &Romfs, builder: &mut ModBuilder, leaf: &str, marked: &str) -> Result<()> {
    let data = romfs.read(FOLIAGE)?;
    let pack = Sarc::new(&data)?;
    let mut writer = SarcWriter::from_sarc(&pack);
    let named = format!("/{leaf}.");
    for file in pack.files() {
        let Some(path) = file.name.filter(|p| p.contains(&named)) else {
            continue;
        };
        let at = path.replacen(&named, &format!("/{marked}."), 1);
        let copy = if path.starts_with("Foliage/ResShootLeaf/") {
            let host = BymlDoc::parse(file.data)?;
            doc(gold(host.root.as_map()?, marked)?, &host).to_binary()
        } else if path.starts_with("Component/FoliageParam/") {
            let host = BymlDoc::parse(file.data)?;
            doc(pointed(host.root.as_map()?, leaf, marked)?, &host).to_binary()
        } else {
            file.data.to_vec()
        };
        // A file the game doesn't size (an empty node tree) needs no entry.
        if builder.has_entry(path) {
            builder.add_entry(&at, path, file.data.len(), copy.len())?;
        }
        writer.add_file(&at, copy);
    }
    builder.write(FOLIAGE, &writer.to_binary())?;
    let like = geometry(leaf);
    builder.add(&geometry(marked), &romfs.read(&like)?, &like)
}

/// Leaf type `leaf` as the marked one `name`: its leaves coloured gold, and
/// its hash its own name's.
fn gold(leaf: &BymlMap, name: &str) -> Result<BymlMap> {
    let mut leaf = leaf.clone();
    let key = field::LEAF_MOD_COLOR.get();
    let mut modded = leaf.get(key).context("no leaf colour")?.as_map()?.clone();
    let [r, g, b] = GOLD.map(|c| Byml::Float(c as f32 / 255.0));
    let colour = BymlMap::from_iter([
        ("R".into(), r),
        ("G".into(), g),
        ("B".into(), b),
        ("A".into(), Byml::Float(STRENGTH)),
    ]);
    modded.insert(field::COLOR.get().into(), Byml::Map(colour));
    leaf.insert(key.into(), Byml::Map(modded));
    leaf.insert(field::LEAF_MOD_COLOR_ENABLE.get().into(), Byml::Bool(true));
    let hash = murmur3(name.as_bytes());
    let hash = match leaf.get(field::HASH.get()) {
        Some(Byml::I32(_)) => Byml::I32(hash as i32),
        _ => Byml::U32(hash),
    };
    leaf.insert(field::HASH.get().into(), hash);
    Ok(leaf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leaf_reference_names_its_type() {
        assert_eq!(
            leaf_name(
                "Work/Foliage/ResShootLeaf/Land_FakePlant_A_01.game__foliage__ResShootLeaf.gyml"
            ),
            Some("Land_FakePlant_A_01")
        );
        assert_eq!(leaf_name(""), None);
    }

    #[test]
    fn a_pointed_foliage_names_the_marked_leaf() {
        let reference =
            "Work/Foliage/ResShootLeaf/Land_FakePlant_A_01.game__foliage__ResShootLeaf.gyml";
        let foliage = BymlMap::from_iter([(
            field::SHOOT_LEAF_LIST.get().into(),
            Byml::String(reference.into()),
        )]);
        let pointed = pointed(&foliage, "Land_FakePlant_A_01", "Land_FakePlant_Z_01").unwrap();
        let Byml::String(s) = &pointed[field::SHOOT_LEAF_LIST.get()] else {
            panic!("not a string");
        };
        assert_eq!(leaf_name(s), Some("Land_FakePlant_Z_01"));
    }

    #[test]
    fn a_marked_leaf_is_gold_and_hashes_its_own_name() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let pack = Sarc::new(romfs.read(FOLIAGE).unwrap()).unwrap();
        let data = (pack.files())
            .find(|f| {
                f.name
                    .is_some_and(|n| n.starts_with("Foliage/ResShootLeaf/"))
            })
            .expect("a leaf type")
            .data;
        let host = BymlDoc::parse(data).unwrap();
        let host = host.root.as_map().unwrap();
        if host.get(field::LEAF_MOD_COLOR.get()).is_none() {
            return;
        }
        let leaf = gold(host, "Land_FakePlant_Z_01").unwrap();
        let colour = leaf[field::LEAF_MOD_COLOR.get()].as_map().unwrap()[field::COLOR.get()]
            .as_map()
            .unwrap()
            .clone();
        assert_eq!(colour["R"], Byml::Float(1.0));
        assert_eq!(leaf[field::LEAF_MOD_COLOR_ENABLE.get()], Byml::Bool(true));
        let hash = murmur3(b"Land_FakePlant_Z_01");
        assert!(
            matches!(
                leaf[field::HASH.get()],
                Byml::U32(h) if h == hash
            ) || matches!(leaf[field::HASH.get()], Byml::I32(h) if h as u32 == hash)
        );
    }
}
