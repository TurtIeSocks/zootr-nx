//! The flag map's edges from actor AI: every AINB in every actor pack. An
//! actor has its graphs; each graph reads and writes GameData, runs its
//! modules and uses node classes as a level's logic graph does.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use ounce_formats::ainb::Graph as Ainb;
use ounce_formats::{Romfs, files_with_suffix, par_map};
use roead::sarc::Sarc;

use super::{Graph, Kind, Link, scene};
use crate::gamedata::GameData;

const PACKS: &str = "Pack/Actor";
const PACK: &str = ".pack.zs";
const AINB: &str = ".ainb";

/// One actor's graphs: its name, each member path with the graph, and the
/// members the AINB reader can't parse yet.
type Pack = (String, Vec<(String, Ainb)>, Vec<String>);

/// Adds every pack's graphs, and returns them by the path `source` cites.
pub(super) fn add(g: &mut Graph, romfs: &Romfs, gd: &GameData) -> Result<HashMap<String, Ainb>> {
    let mut files = files_with_suffix(&romfs.root().join(PACKS), PACK)?;
    files.sort();
    let packs: Vec<Result<Pack>> = par_map(&files, |path| {
        let rel = romfs.rel(path)?;
        let actor = (rel.rsplit('/').next().unwrap_or(&rel))
            .trim_end_matches(PACK)
            .to_owned();
        let data = romfs.read(&rel)?;
        let sarc = Sarc::new(&data)?;
        let (mut graphs, mut unread) = (Vec::new(), Vec::new());
        let members = (sarc.files()).filter_map(|f| Some((f.name?, f.data)));
        for (name, data) in members.filter(|(name, _)| name.ends_with(AINB)) {
            let file = format!("{rel}/{name}");
            match Ainb::parse(data) {
                Ok(ainb) => graphs.push((file, ainb)),
                Err(_) => unread.push(file),
            }
        }
        Ok((actor, graphs, unread))
    });
    let mut kept = HashMap::new();
    for pack in packs {
        let (actor, graphs, unread) = pack?;
        g.unread.extend(unread);
        let stems: HashSet<String> = graphs.iter().map(|(file, _)| stem(file)).collect();
        let has = (!graphs.is_empty()).then(|| g.node(Kind::Actor, &actor, || None));
        for (file, ainb) in graphs {
            let graph = g.node(Kind::Graph, &format!("{actor}/{}", stem(&file)), || None);
            if let Some(has) = has {
                g.edge(has, graph, Link::Has, String::new(), &file, None);
            }
            // A module the pack holds is its own; any other is shared.
            let module = |child: &str| match stems.contains(child) {
                true => format!("{actor}/{child}"),
                false => child.to_owned(),
            };
            scene::own(g, graph, &file, &ainb, gd, module);
            kept.insert(file, ainb);
        }
    }
    Ok(kept)
}

/// `.../AI/NAME.utility.root.ainb` is `NAME.utility.root`.
fn stem(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.trim_end_matches(AINB).to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::Graph;
    use crate::known::{actor, field, map};

    /// Gold Skulltulas by day had three blockers, each connected to the
    /// Gold Skulltula: the night module that places it, its own graph,
    /// and the day question and emit timer that graph runs.
    #[test]
    fn the_gold_skulltula_reaches_all_three_blockers() {
        let Some(g) = Graph::dev() else {
            return;
        };
        let find = |id: String| g.find(&id).unwrap_or_else(|e| panic!("{e:#}"));
        let path = |a: usize, b: usize| !g.trace(a, b).paths.is_empty();
        let gold = &**actor::ENEMY_SKULLTULA_GOLD;
        let skulltula = find(format!("actor:{gold}"));
        let night = &**map::GIMMICK_NIGHTTIME_SKULLTULA_GOLD_APPEAR_05A9;
        let night = find(format!("graph:{night}.logic.module"));
        assert!(path(night, skulltula), "the night module");
        let own = find(format!("graph:{gold}/{gold}.utility.root"));
        assert!(path(skulltula, own), "its own graph");
        let day = find(format!("class:{}", field::QUERY_TIME_DIVISION));
        assert!(path(day, own), "its day question");
        let timer = find(format!("class:{}", field::EXECUTE_SKULLTULA_GOLD_EMIT_TIME));
        assert!(path(timer, own), "its emit timer");
    }
}
