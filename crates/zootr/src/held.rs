//! What Link holds up in a get. A chest's flow, and a pickup's, bind the
//! item to Link's hands with `EventTriggerModelBindForItemGetDemo`. The
//! ocarinas miss it twice: their performer's event graph (`Common`) has no
//! such event, and they calculate after every other actor (`CalcPriority`,
//! in their row and in their system setting). So a chest dealt one showed
//! Link holding up nothing under the right text. Each ocarina takes every
//! other item's performer (`PouchContent`), whose graph answers that event
//! and every one `Common` does, and the Hookshot's system setting, and its
//! row drops the priority: then Link holds it up (Mac, 2026-11-17). With
//! the performer alone he still didn't. A chest gives a line's first
//! level and the level Link needs is picked as it opens, so both are
//! written in every seed.

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs, pack_file};

use crate::known::{Name, item};
use crate::standins::{self, COMPONENTS, Tables};

/// The items the game leaves out of Link's hands.
const UNBOUND: [&Name; 2] = [item::OCARINA_FAIRY, item::OCARINA_TIME];
/// Whose parts each takes.
const LENDER: &Name = item::HOOKSHOT;
const PERFORMER: &str = "EventPerformerRef";
const SETTING: &str = "SystemSetting";
/// The row field that has an actor calculate out of turn. The lender's row
/// has none.
const PRIORITY: &str = "CalcPriority";

/// Write each item the game leaves out of Link's hands with the lender's
/// performer and system setting, and its row without its priority.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    let lender = ActorPack::load(romfs, LENDER)?;
    for name in UNBOUND {
        let pack = ActorPack::load(romfs, name)?;
        let mut writer = pack.writer();
        let path = standins::actor_path(name);
        let old = standins::raw(&pack, &path)?;
        let mut actor = BymlDoc::parse(&old)?;
        let mut refs = standins::components(actor.root.as_map()?)?;
        for key in [PERFORMER, SETTING] {
            standins::lend(&mut writer, &mut refs, key, &lender)?;
        }
        (actor.root.as_mut_map()?).insert(COMPONENTS.into(), Byml::Map(refs));
        let new = actor.to_binary();
        builder.resize(&path, old.len(), new.len())?;
        writer.add_file(path, new);
        builder.write(&pack_file(name), &writer.to_binary())?;
        tables.take(name, LENDER, &[PRIORITY])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::standins::row;
    use roead::sarc::Sarc;

    const BIND: &str = "EventTriggerModelBindForItemGetDemo";
    const GRAPH: &str = "EventUniqueAin";
    /// The pack the event graphs no actor carries itself live in.
    const SHARED: &str = "Pack/AI.Global.Product.111.pack.zs";
    /// The system setting that calculates an actor after the rest.
    const AFTER: &str = "GameActorAfter";

    /// The event graph `pack`'s performer runs: the pack's own copy, or the
    /// shared one.
    fn graph(romfs: &Romfs, pack: &ActorPack) -> Vec<u8> {
        let param = pack.actor_param().unwrap().unwrap();
        let performer = standins::component(&param, PERFORMER)
            .unwrap_or_else(|| panic!("{} has no {PERFORMER}", pack.name));
        let performer = pack.resolve(&performer).unwrap().unwrap();
        let ain = performer[GRAPH].as_string().unwrap();
        let file = format!("AI/{}b", ain.rsplit('/').next().unwrap());
        if let Some(own) = pack.raw(&file) {
            return own.to_vec();
        }
        let shared = romfs.read(SHARED).unwrap();
        Sarc::new(&shared)
            .unwrap()
            .get_data(&file)
            .unwrap()
            .to_vec()
    }

    fn binds(graph: &[u8]) -> bool {
        graph.windows(BIND.len()).any(|w| w == BIND.as_bytes())
    }

    /// Whether `pack`'s system setting calculates it after the rest.
    fn after(pack: &ActorPack) -> bool {
        let param = pack.actor_param().unwrap().unwrap();
        standins::component(&param, SETTING).is_some_and(|s| s.contains(AFTER))
    }

    #[test]
    fn every_item_a_check_holds_is_held_up_in_its_get() {
        let Some(romfs) = dev_dump() else { return };
        // Every kind of check: placed holders (pots and the rest), gifts,
        // scrubs, every shop slot and cows.
        let slots = crate::shops::slots(&romfs).unwrap();
        let checks = (crate::checks::scan(&romfs).unwrap().into_iter())
            .chain(crate::gifts::checks())
            .chain(crate::scrubs::checks())
            .chain(crate::shops::checks(&slots, usize::MAX))
            .chain(crate::cows::dealt(&romfs, true).unwrap());
        let mut items: Vec<String> = checks.map(|c| c.item).collect();
        items.sort();
        items.dedup();
        for u in UNBOUND {
            assert!(items.iter().any(|i| *i == **u), "{u} is in no check");
        }
        let dir = std::env::temp_dir().join(format!("zootr-held-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(&romfs, &mut builder, &mut tables).unwrap();
        builder.finish().unwrap();
        let mut unbound = Vec::new();
        for item in &items {
            let written = dir.join("romfs").join(pack_file(item));
            let pack = match std::fs::read(&written) {
                Ok(raw) => {
                    let data = romfs.dicts().decompress(&pack_file(item), &raw).unwrap();
                    ActorPack::from_bytes(item, data).unwrap()
                }
                Err(_) => ActorPack::load(&romfs, item).unwrap(),
            };
            let late = row(&tables.actor, item).is_ok_and(|r| r.contains_key(PRIORITY));
            if !binds(&graph(&romfs, &pack)) || after(&pack) || late {
                unbound.push(item.as_str());
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
        assert!(unbound.is_empty(), "held up empty-handed: {unbound:?}");
    }

    #[test]
    fn the_lender_s_graph_answers_every_event_the_ocarinas_does() {
        let Some(romfs) = dev_dump() else { return };
        let events = |graph: &[u8]| -> Vec<String> {
            let text = String::from_utf8_lossy(graph);
            let mut out: Vec<String> = (text.split(|c: char| !c.is_ascii_alphanumeric()))
                .filter(|w| w.starts_with("Event") && w.len() > "Event".len())
                .map(str::to_owned)
                .collect();
            out.sort();
            out.dedup();
            out
        };
        let lender = events(&graph(&romfs, &ActorPack::load(&romfs, LENDER).unwrap()));
        for u in UNBOUND {
            let own = events(&graph(&romfs, &ActorPack::load(&romfs, u).unwrap()));
            let missing: Vec<_> = own.iter().filter(|e| !lender.contains(e)).collect();
            assert!(missing.is_empty(), "{u}: {missing:?}");
        }
    }

    #[test]
    fn the_ocarinas_param_lives_in_its_own_pack_alone() {
        let Some(romfs) = dev_dump() else { return };
        for u in UNBOUND {
            let path = standins::actor_path(u);
            assert_eq!(crate::game::packs_holding(&romfs, &path), [u.get()]);
        }
    }
}
