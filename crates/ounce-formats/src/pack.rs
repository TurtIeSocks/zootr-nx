//! Actor packs: `Pack/Actor/<name>.pack.zs`, a SARC of the actor's BGYML
//! params. A param file can name a `$parent` it inherits from, usually also
//! inside the pack.

use anyhow::{Context, Result};
use roead::byml::{Byml, Map};
use roead::sarc::{Sarc, SarcWriter};

use crate::Romfs;

const PARENT: &str = "$parent";

pub struct ActorPack {
    pub name: String,
    sarc: Sarc<'static>,
}

/// `Work/Actor/X.engine__actor__ActorParam.gyml` and
/// `?Actor/X.engine__actor__ActorParam.bgyml` both live at
/// `Actor/X.engine__actor__ActorParam.bgyml` in the pack.
pub fn pack_path(reference: &str) -> String {
    let path = reference
        .trim_start_matches('?')
        .trim_start_matches("Work/");
    match path.strip_suffix(".gyml") {
        Some(stem) => format!("{stem}.bgyml"),
        None => path.to_owned(),
    }
}

/// `child` over `parent`, recursing into maps both have.
fn merge(parent: Map, child: &Map) -> Map {
    let mut out = parent;
    for (k, v) in child {
        let merged = match (out.remove(k), v) {
            (Some(Byml::Map(p)), Byml::Map(c)) => Byml::Map(merge(p, c)),
            _ => v.clone(),
        };
        out.insert(k.clone(), merged);
    }
    out.remove(PARENT);
    out
}

/// The romfs path of actor `name`'s pack.
pub fn pack_file(name: &str) -> String {
    format!("Pack/Actor/{name}.pack.zs")
}

impl ActorPack {
    pub fn load(romfs: &Romfs, name: &str) -> Result<Self> {
        let rel = pack_file(name);
        let sarc = Sarc::new(romfs.read(&rel)?).with_context(|| format!("reading {rel}"))?;
        Ok(Self {
            name: name.to_owned(),
            sarc,
        })
    }

    /// Every file in the pack, by path.
    pub fn files(&self) -> Vec<String> {
        self.sarc
            .files()
            .filter_map(|f| f.name().map(str::to_owned))
            .collect()
    }

    /// A writer holding every file in the pack, to build a changed copy.
    pub fn writer(&self) -> SarcWriter {
        SarcWriter::from_sarc(&self.sarc)
    }

    /// A file's bytes as stored, for files that aren't BYML.
    pub fn raw(&self, path: &str) -> Option<&[u8]> {
        self.sarc.get_data(path)
    }

    /// The pack path `reference` points at, and that file's bytes as stored.
    pub fn find(&self, reference: &str) -> Option<(String, &[u8])> {
        let path = pack_path(reference);
        let data = self.sarc.get_data(&path)?;
        Some((path, data))
    }

    /// The param file `reference` points at, with its `$parent` chain merged
    /// in. A parent outside the pack ends the chain. `None` if the file
    /// itself isn't in the pack.
    pub fn resolve(&self, reference: &str) -> Result<Option<Map>> {
        let path = pack_path(reference);
        let Some(data) = self.sarc.get_data(&path) else {
            return Ok(None);
        };
        let doc = Byml::from_binary(data).with_context(|| format!("{}: {path}", self.name))?;
        let own = doc.as_map()?;
        let parent = match own.get(PARENT).and_then(|p| p.as_string().ok()) {
            Some(p) => self.resolve(p)?.unwrap_or_default(),
            None => Map::default(),
        };
        Ok(Some(merge(parent, own)))
    }

    /// `Actor/<name>.engine__actor__ActorParam.bgyml`, resolved.
    pub fn actor_param(&self) -> Result<Option<Map>> {
        self.resolve(&format!(
            "Actor/{}.engine__actor__ActorParam.bgyml",
            self.name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_and_ref_paths_map_into_the_pack() {
        let want = "Actor/X.engine__actor__ActorParam.bgyml";
        assert_eq!(
            pack_path("Work/Actor/X.engine__actor__ActorParam.gyml"),
            want
        );
        assert_eq!(pack_path("?Actor/X.engine__actor__ActorParam.bgyml"), want);
    }

    #[test]
    fn child_wins_and_maps_merge() {
        let map = |pairs: &[(&str, Byml)]| -> Map {
            pairs
                .iter()
                .map(|(k, v)| ((*k).into(), v.clone()))
                .collect()
        };
        let parent = map(&[
            ("A", Byml::I32(1)),
            (
                "C",
                Byml::Map(map(&[("x", Byml::I32(1)), ("y", Byml::I32(1))])),
            ),
        ]);
        let child = map(&[
            (PARENT, Byml::String("p".into())),
            ("A", Byml::I32(2)),
            ("C", Byml::Map(map(&[("y", Byml::I32(2))]))),
        ]);
        let out = merge(parent, &child);
        assert_eq!(out.get("A"), Some(&Byml::I32(2)));
        assert_eq!(out["C"].as_map().unwrap().get("x"), Some(&Byml::I32(1)));
        assert_eq!(out["C"].as_map().unwrap().get("y"), Some(&Byml::I32(2)));
        assert!(!out.contains_key(PARENT));
    }
}
