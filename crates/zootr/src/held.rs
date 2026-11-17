//! What Link holds up in a get. A chest's flow, and a pickup's, bind the
//! item to Link's hands with `EventTriggerModelBindForItemGetDemo`, an
//! event only some event graphs answer: the one the item's
//! `EventPerformerParam` names. The ocarinas' (`Common`) doesn't, so a
//! chest dealt one showed Link holding up nothing under the right text
//! (Mac, 2026-11-17). Each dealt ocarina takes every other item's
//! performer (`PouchContent`), whose graph answers that event and every
//! one `Common` does, so the scenes that drive the ocarina keep working.

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs, pack_file};

use crate::known::{Name, item};
use crate::standins::{self, COMPONENTS};

/// The items whose event graph leaves them out of Link's hands.
const UNBOUND: [&Name; 2] = [item::OCARINA_FAIRY, item::OCARINA_TIME];
/// Whose performer each takes.
const LENDER: &Name = item::HOOKSHOT;
const PERFORMER: &str = "EventPerformerRef";

/// Write each item in `items` the game leaves out of Link's hands with
/// the lender's performer.
pub fn write<'a>(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    items: impl Iterator<Item = &'a str>,
) -> Result<()> {
    let items: Vec<&str> = items.collect();
    let dealt: Vec<&Name> = (UNBOUND.into_iter())
        .filter(|u| items.iter().any(|i| **u == **i))
        .collect();
    if dealt.is_empty() {
        return Ok(());
    }
    let lender = ActorPack::load(romfs, LENDER)?;
    for name in dealt {
        let pack = ActorPack::load(romfs, name)?;
        let mut writer = pack.writer();
        let path = standins::actor_path(name);
        let old = standins::raw(&pack, &path)?;
        let mut actor = BymlDoc::parse(&old)?;
        let mut refs = standins::components(actor.root.as_map()?)?;
        standins::lend(&mut writer, &mut refs, PERFORMER, &lender)?;
        (actor.root.as_mut_map()?).insert(COMPONENTS.into(), Byml::Map(refs));
        let new = actor.to_binary();
        builder.resize(&path, old.len(), new.len())?;
        writer.add_file(path, new);
        builder.write(&pack_file(name), &writer.to_binary())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use roead::sarc::Sarc;

    const BIND: &str = "EventTriggerModelBindForItemGetDemo";
    const GRAPH: &str = "EventUniqueAin";
    /// The pack the event graphs no actor carries itself live in.
    const SHARED: &str = "Pack/AI.Global.Product.111.pack.zs";

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
        write(&romfs, &mut builder, items.iter().map(String::as_str)).unwrap();
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
            if !binds(&graph(&romfs, &pack)) {
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
    fn only_the_dealt_ocarina_s_pack_is_written() {
        let Some(romfs) = dev_dump() else { return };
        let dir = std::env::temp_dir().join(format!("zootr-held-one-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let dealt = [
            LENDER.get(),
            item::OCARINA_TIME.get(),
            item::OCARINA_TIME.get(),
        ];
        write(&romfs, &mut builder, dealt.into_iter()).unwrap();
        let files = builder.finish().unwrap();
        assert_eq!(
            files,
            [pack_file(item::OCARINA_TIME), romfs.restbl_rel().unwrap()]
        );
        std::fs::remove_dir_all(dir).unwrap();
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
