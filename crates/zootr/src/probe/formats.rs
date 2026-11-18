//! Raw-format subcommands: BYML, AINB, restbl and string dumps, and the
//! file greps over the unpacked game.

use std::path::Path;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, Romfs};
use roead::sarc::Sarc;

use crate::items::scalar;

pub(super) fn byml(romfs: &Romfs, game: &Path, pattern: &str) -> Result<()> {
    unpacked(romfs, game, pattern, |path, data, _| {
        if let Ok(doc) = ounce_formats::BymlDoc::parse(data) {
            println!("{path}");
            print!("{}", tree(&doc.root, 1));
        }
        Ok(())
    })
}

/// A value as indented lines: a map's keys in order, an array's items by
/// index, a scalar on its key's line.
fn tree(v: &Byml, depth: usize) -> String {
    let line = |key: &str, v: &Byml| match v {
        Byml::Map(_) | Byml::Array(_) => {
            format!("{:w$}{key}:\n{}", "", tree(v, depth + 1), w = depth * 2)
        }
        Byml::BinaryData(b) | Byml::FileData(b) => {
            format!("{:w$}{key}: <{} bytes>\n", "", b.len(), w = depth * 2)
        }
        v => format!("{:w$}{key}: {}\n", "", scalar(v), w = depth * 2),
    };
    match v {
        Byml::Map(m) => {
            let mut pairs: Vec<_> = m.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            pairs.iter().map(|(k, v)| line(k, v)).collect()
        }
        Byml::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, v)| line(&i.to_string(), v))
            .collect(),
        Byml::BinaryData(b) | Byml::FileData(b) => {
            format!("{:w$}<{} bytes>\n", "", b.len(), w = depth * 2)
        }
        v => format!("{:w$}{}\n", "", scalar(v), w = depth * 2),
    }
}

/// Print a graph, one node per line.
pub(super) fn ainb(romfs: &Romfs, path: &str) -> Result<()> {
    use ounce_formats::ainb::{Graph, Value};
    let graph = Graph::parse(&graph_bytes(romfs, path)?)?;
    let show = |v: &Value| match v {
        Value::String(s) => format!("{s:?}"),
        Value::Pointer => "ptr".into(),
        v => format!("{v:?}"),
    };
    for (i, node) in graph.nodes.iter().enumerate() {
        let props = node
            .properties
            .iter()
            .map(|(n, v)| format!("{n}={}", show(v)));
        let inputs = node
            .inputs
            .iter()
            .map(|input| match input.sources.as_slice() {
                [] => format!("{}={}", input.name, show(&input.value)),
                links => {
                    let from: Vec<_> = links
                        .iter()
                        .map(|l| format!("#{}.{}", l.node, l.output))
                        .collect();
                    format!("{}<-{}", input.name, from.join("|"))
                }
            });
        let all: Vec<_> = props.chain(inputs).collect();
        let outputs: Vec<_> = node.outputs.iter().map(|o| o.name.as_str()).collect();
        println!(
            "#{i} {:?} {} {} -> {}",
            node.kind,
            node.name,
            all.join(" "),
            outputs.join(",")
        );
    }
    Ok(())
}

/// A graph's bytes: a romfs file, or a file inside an actor pack when the
/// path runs through `Pack/Actor/NAME.pack.zs/`.
fn graph_bytes(romfs: &Romfs, path: &str) -> Result<Vec<u8>> {
    let Some((actor, inner)) = in_pack(path) else {
        let Some((pack, inner)) = path.split_once(".pack.zs/") else {
            return romfs.read(path);
        };
        // A shared pack, like `Pack/AI.Global.Product.111.pack.zs`.
        let data = romfs.read(&format!("{pack}.pack.zs"))?;
        let sarc = Sarc::new(&data)?;
        let file = sarc.get_data(inner);
        return Ok(file
            .with_context(|| format!("{pack} has no {inner}"))?
            .to_vec());
    };
    let pack = ActorPack::load(romfs, actor)?;
    let data = pack.raw(inner).with_context(|| {
        format!("{actor}'s pack has no {inner}. `zootr probe pack {actor}` lists its files")
    })?;
    Ok(data.to_vec())
}

/// Each spot of the widest sweep mix: hash, area, trigger or skip reason,
/// name.
pub(super) fn pool(romfs: &Romfs, seed: u64, untracked: bool) -> Result<()> {
    use crate::sweep::night::{self, MIXES, mix};
    use crate::sweep::plan::Trigger;
    let dir = std::env::temp_dir().join("zootr-probe-pool");
    let (_, settings) = mix(MIXES.len() - 1);
    let places = crate::dev::places(romfs)?;
    let pool = night::pool(romfs, &dir, seed, &settings, &places)?;
    if untracked {
        let spots: Vec<_> = pool.iter().map(|p| p.spot.clone()).collect();
        let gd = crate::gamedata::GameData::load(romfs)?;
        let tracker = crate::tracker::Tracker::new(&gd, &spots)?;
        let left = spots.iter().filter(|s| tracker.flag(s.hash).is_none());
        for s in left {
            println!("{:<20} {:<24} {}", s.hash, s.area, s.holder);
        }
        return Ok(());
    }
    for p in pool {
        let how = match &p.trigger {
            Trigger::Skip(why) => format!("skip: {why}"),
            t => night::kind(t).into(),
        };
        println!(
            "{:<20} {:<24} {how:<10} {}",
            p.spot.hash,
            p.spot.area,
            night::name(&p)
        );
    }
    Ok(())
}

pub(super) fn restbl(romfs: &Romfs, paths: &[String]) -> Result<()> {
    let table = ounce_formats::Restbl::parse(&romfs.read(&romfs.restbl_rel()?)?)?;
    for p in paths {
        let size = table.get(p).map_or("-".into(), |s| s.to_string());
        println!("{size} {p}");
    }
    Ok(())
}

/// The names in a file's bytes, sorted: AINB keeps node, parameter and
/// blackboard names in one string pool, an ASB its commands and clips.
pub(super) fn strings(data: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = data
        .split(|b| !b.is_ascii_graphic())
        .filter(|s| s.len() >= 4 && s[0].is_ascii_alphabetic())
        .filter(|s| {
            s.iter()
                .all(|b| b.is_ascii_alphanumeric() || b"._:".contains(b))
        })
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Hands `f` each file whose path contains `pattern`, decompressed, or each
/// member in its place when it's a SARC archive, marked as one.
pub(super) fn unpacked(
    romfs: &Romfs,
    game: &Path,
    pattern: &str,
    mut f: impl FnMut(&str, &[u8], bool) -> Result<()>,
) -> Result<()> {
    crate::game::each(game, pattern, |path, data| {
        let data = romfs.dicts().decompress(&path, &data)?;
        match Sarc::new(&data) {
            Ok(sarc) => {
                for file in sarc.files() {
                    if let Some(name) = file.name() {
                        f(&format!("{path}/{name}"), file.data(), true)?;
                    }
                }
                Ok(())
            }
            Err(_) => f(&path, &data, false),
        }
    })
}

pub(super) fn grep(romfs: &Romfs, game: &Path, needle: &str, path: &str) -> Result<()> {
    let needle = needle.as_bytes();
    let mut found = 0;
    unpacked(romfs, game, path, |path, data, _| {
        let n = data.windows(needle.len()).filter(|w| *w == needle).count();
        if n > 0 {
            println!("{n:>5} {path}");
            found += 1;
        }
        Ok(())
    })?;
    eprintln!("{found} files");
    Ok(())
}

pub(super) fn files(romfs: &Romfs, game: &Path, pattern: &str, out: Option<&Path>) -> Result<()> {
    let found = crate::game::search(game, pattern)?;
    for (path, data) in &found {
        println!("{:>10} {path}", data.len());
        if let Some(out) = out {
            write_under(out, path, &romfs.dicts().decompress(path, data)?)?;
        }
    }
    eprintln!("{} files", found.len());
    Ok(())
}

/// Writes every file whose path contains `pattern` under `out`,
/// decompressed and with each SARC archive opened into a folder of its
/// members, so plain tools (`rg`, `ls`) can search the result.
pub(super) fn unpack(romfs: &Romfs, game: &Path, pattern: &str, out: &Path) -> Result<()> {
    let mut written = 0;
    unpacked(romfs, game, pattern, |path, data, _| {
        written += 1;
        write_under(out, path, data)
    })?;
    eprintln!("{written} files");
    Ok(())
}

/// Writes `data` at its romfs `path` under `out`, `.zs` dropped from each
/// part of it (an archive's own name becomes its members' folder).
fn write_under(out: &Path, path: &str, data: &[u8]) -> Result<()> {
    let dest = out.join(path.replace(".zs/", "/").trim_end_matches(".zs"));
    std::fs::create_dir_all(dest.parent().context("no parent")?)?;
    std::fs::write(&dest, data)?;
    Ok(())
}

/// The actor and the file inside its pack, for a path into an actor pack.
fn in_pack(path: &str) -> Option<(&str, &str)> {
    let (pack, inner) = path.split_once(".pack.zs/")?;
    Some((pack.strip_prefix("Pack/Actor/")?, inner))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field};

    #[test]
    fn an_archive_member_lands_in_a_folder_named_for_its_archive() {
        let out = std::env::temp_dir().join(format!("zootr-unpack-{}", std::process::id()));
        write_under(&out, "Pack/Actor/FakeActor.pack.zs/AI/Fake.ainb", b"ai").unwrap();
        write_under(&out, "Banc/FakeMap.bcett.byml.zs", b"map").unwrap();
        let read = |p: &str| std::fs::read(out.join(p)).unwrap();
        assert_eq!(read("Pack/Actor/FakeActor.pack/AI/Fake.ainb"), b"ai");
        assert_eq!(read("Banc/FakeMap.bcett.byml"), b"map");
        std::fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn names_are_four_or_more_name_bytes() {
        needs_names!();
        let data = b"\0\x01Rotate_Shop\0ab\0Item_C\xff";
        assert_eq!(strings(data), ["Item_C", field::ROTATE_SHOP]);
    }

    #[test]
    fn a_byml_tree_puts_one_value_on_each_line_under_its_key() {
        let map = |pairs: Vec<(&str, Byml)>| {
            Byml::Map(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
        };
        let array = Byml::Array(vec![Byml::I32(1), Byml::String("x".into())]);
        let doc = map(vec![
            ("b", array),
            ("a", map(vec![("c", Byml::Bool(true))])),
        ]);
        assert_eq!(tree(&doc, 0), "a:\n  c: true\nb:\n  0: 1\n  1: x\n");
    }

    #[test]
    fn a_path_through_an_actor_pack_names_the_actor_and_its_file() {
        let path = &format!(
            "Pack/Actor/{}.pack.zs/AI/{}.brain.root.ainb",
            actor::ENEMY_SCRUB_BUSINESS,
            actor::ENEMY_SCRUB_BUSINESS
        );
        assert_eq!(
            in_pack(path),
            Some((
                &**actor::ENEMY_SCRUB_BUSINESS,
                format!("AI/{}.brain.root.ainb", actor::ENEMY_SCRUB_BUSINESS).as_str()
            ))
        );
        assert_eq!(in_pack("Logic/FakeMap_Common.logic.module.ainb"), None);
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let graph = ounce_formats::ainb::Graph::parse(&graph_bytes(&romfs, path).unwrap()).unwrap();
        assert!(!graph.nodes.is_empty());
    }
}
