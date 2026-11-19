//! What Link holds up in a get. A chest's flow, and a pickup's, bind the
//! item to Link's hands with `EventTriggerModelBindForItemGetDemo`, then
//! turn it over his head with its animation sequence's get turn.
//! The ocarinas miss all of it: their performer's event graph (`Common`)
//! has no such event, they calculate after every other actor
//! (`CalcPriority`, in their row and in their system setting), and they
//! have no sequence. So a chest dealt one showed Link holding up nothing
//! under the right text.
//!
//! Each ocarina becomes the Hookshot's actor, keeping every part of its
//! own but two: its performer and its system setting give way to the
//! Hookshot's (`PouchContent`, whose graph answers that event and every
//! one `Common` does, and the default setting). It takes the Hookshot's
//! sequence and animations, which turn it, and the base actor's parts it
//! has none of. Its row drops the priority and makes room for both
//! actors' heaps. Then Link holds it up and it turns (live, 2026-11-17).
//!
//! What failed on the way, each live:
//!
//! | Tried | Result |
//! |---|---|
//! | Only the performer | Held up empty-handed |
//! | Performer and setting | Held up, still |
//! | Its own actor taking the sequence and animations too | Crash at load: heaps sized for one actor. With both heaps, the chest's get hangs |
//! | The Hookshot's actor with only its name, pouch, model and table | Turns, but Left no longer takes it out to play |
//!
//! The crash (`main+0x668f50`) is the actor's components map failing to
//! allocate in its heap, and summing the heaps ended it. The hung build
//! lacked the base actor's physics and LOD, and its row took the
//! Hookshot's `AnimationResources`: which of those hung it isn't known.
//! Its own model bind isn't it: kept or not, the get runs.
//!
//! A chest gives a line's first level and the level Link needs is picked
//! as it opens, so both are written in every seed.

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs, pack_file};

use crate::known::{Name, item};
use crate::standins::{self, COMPONENTS, Tables};

/// The items the game leaves out of Link's hands.
const UNBOUND: [&Name; 2] = [item::OCARINA_FAIRY, item::OCARINA_TIME];
/// Whose actor each becomes.
const LENDER: &Name = item::HOOKSHOT;
const PERFORMER: &str = "EventPerformerRef";
const SETTING: &str = "SystemSetting";
/// The item's own parts that give way to the lender's: what binds it to
/// Link's hands in a get.
const LENT: [&str; 2] = [PERFORMER, SETTING];
/// The row field that has an actor calculate out of turn. The lender's row
/// has none.
const PRIORITY: &str = "CalcPriority";

/// Write each item the game leaves out of Link's hands as the lender's
/// actor with its own parts but `LENT`, carrying the lender's files.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
    let lender = ActorPack::load(romfs, LENDER)?;
    let like = standins::actor_path(LENDER);
    let lender_actor = standins::raw(&lender, &like)?;
    for name in UNBOUND {
        let pack = ActorPack::load(romfs, name)?;
        let mut writer = pack.writer();
        let path = standins::actor_path(name);
        let old = standins::raw(&pack, &path)?;
        let own = standins::components(BymlDoc::parse(&old)?.root.as_map()?)?;
        let mut actor = BymlDoc::parse(&lender_actor)?;
        let mut refs = standins::components(actor.root.as_map()?)?;
        refs.extend(own.into_iter().filter(|(k, _)| !LENT.contains(&k.as_str())));
        (actor.root.as_mut_map()?).insert(COMPONENTS.into(), Byml::Map(refs));
        for file in lender.files() {
            if file != like && writer.get_file(file.as_str()).is_none() {
                writer.add_file(file.clone(), standins::raw(&lender, &file)?);
            }
        }
        let new = actor.to_binary();
        builder.resize_like(&path, old.len(), new.len(), &like, lender_actor.len())?;
        writer.add_file(path, new);
        builder.write(&pack_file(name), &writer.to_binary())?;
        tables.add_heaps(name, LENDER)?;
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
        let shared = romfs.read(crate::pickups::GLOBAL_AI).unwrap();
        Sarc::new(&shared)
            .unwrap()
            .get_data(&file)
            .unwrap()
            .to_vec()
    }

    /// A mod folder `write` wrote into, named for its test, and the tables.
    fn written(romfs: &Romfs, test: &str) -> (std::path::PathBuf, Tables) {
        let id = format!("zootr-held-{test}-{}", std::process::id());
        let dir = std::env::temp_dir().join(id);
        let mut builder = ModBuilder::new(romfs, &dir).unwrap();
        let mut tables = Tables::load(romfs).unwrap();
        write(romfs, &mut builder, &mut tables).unwrap();
        builder.finish().unwrap();
        (dir, tables)
    }

    /// `item`'s pack as the mod in `dir` has it, or the game's.
    fn pack_in(romfs: &Romfs, dir: &std::path::Path, item: &str) -> ActorPack {
        match std::fs::read(dir.join("romfs").join(pack_file(item))) {
            Ok(raw) => {
                let data = romfs.dicts().decompress(&pack_file(item), &raw).unwrap();
                ActorPack::from_bytes(item, data).unwrap()
            }
            Err(_) => ActorPack::load(romfs, item).unwrap(),
        }
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
        let (dir, tables) = written(&romfs, "every");
        let mut unbound = Vec::new();
        for item in &items {
            let pack = pack_in(&romfs, &dir, item);
            let late = row(&tables.actor, item).is_ok_and(|r| r.contains_key(PRIORITY));
            if !binds(&graph(&romfs, &pack)) || after(&pack) || late {
                unbound.push(item.as_str());
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
        assert!(unbound.is_empty(), "held up empty-handed: {unbound:?}");
    }

    /// Link plays an ocarina with its own parts, and the lender's turn it.
    #[test]
    fn each_keeps_its_own_parts_but_the_lent_ones_and_takes_the_rest() {
        let Some(romfs) = dev_dump() else { return };
        let (dir, _) = written(&romfs, "parts");
        let resolved = |pack: &ActorPack| {
            let param = pack.actor_param().unwrap().unwrap();
            standins::components(&param).unwrap()
        };
        let lender = resolved(&ActorPack::load(&romfs, LENDER).unwrap());
        for u in UNBOUND {
            let own = resolved(&ActorPack::load(&romfs, u).unwrap());
            let now = resolved(&pack_in(&romfs, &dir, u));
            for (key, value) in &now {
                let theirs = LENT.contains(&key.as_str()) || !own.contains_key(key);
                let from = if theirs { &lender } else { &own };
                assert_eq!(Some(value), from.get(key), "{u}: {key}");
            }
            let keys = |m: &ounce_formats::BymlMap| m.keys().cloned().collect::<Vec<_>>();
            for key in keys(&own).into_iter().chain(keys(&lender)) {
                assert!(now.contains_key(&key), "{u} lost {key}");
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
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
