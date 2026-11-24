//! The valley bridge and everything beside it follow the four carpenters,
//! as in Ship. The game ties them to the Spirit quest past its first step,
//! which [`crate::gerudo`] moves as soon as Link holds the card so the
//! guards stand down. So every valley reader asks [`FREED`] instead: the
//! clear event that frees the fourth carpenter has played. The fortress
//! keeps the quest step, so the card still stands the guards down.
//!
//! Each carpenter's rescue runs on his Step01 quest schedule, applied only
//! while the quest sits at that step. An early card skips it, so each
//! carpenter's default schedule takes the rescue's conditional part too.

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::ainb::{Graph, Patch};
use ounce_formats::{Byml, BymlDoc, EventKind, Flowchart, ModBuilder, Romfs};
use roead::sarc::Sarc;

use crate::checks::swap_strings;
use crate::flows::{self, GAME_DATA, Rewrites, param, set, string};
use crate::gamedata::{GameData, Op};
use crate::graphs::logic;
use crate::known::{Name, actor, field, flag, flow, map};

/// True once all four carpenters are free.
pub const FREED: &str = "Rando_CarpentersFreed_Exp";
/// What the valley read before: the quest past its first step.
const PAST_FIRST: &Name = flag::SAGE_OF_SPIRIT_IS_AFTER_STEP01_EXP;

/// The folders [`write`] reads beyond the usual dump: the NPC schedules and
/// the map pictures.
pub const SCHEDULES: &str = "Component/AIScheduleParam";
pub const CAPTURE_DIR: &str = "Game/MapCaptureInfo";
const CAPTURE: &str = "Game/MapCaptureInfo/MapCaptureInfo.game__MapCaptureInfo.bgyml";
/// The quest schedules, the rescue's among them.
const QUEST_SCHEDULES: &str = "Pack/AIScheduleParam.pack.zs";
const CONDITIONAL: &str = "ConditionalSchedule";

/// The jailed carpenters.
const CARPENTERS: [&Name; 4] = [
    actor::NPC_HYLIA_013_ADULT,
    actor::NPC_HYLIA_014_ADULT,
    actor::NPC_HYLIA_015_ADULT,
    actor::NPC_HYLIA_016_ADULT,
];

/// Each flow that asks for the bridge, and how many queries it has: the
/// running man's race, the head carpenter, and Saburo once freed.
const FLOWS: [(&Name, usize); 3] = [
    (actor::NPC_HYLIA_RUNNING_MAN_ADULT, 2),
    (actor::NPC_HYLIA_012_ADULT, 4),
    (actor::NPC_HYLIA_015_ADULT, 1),
];
/// The valley's Banc reads: the carpenters' tent props, their navmesh cut
/// and the arrival from the fortress.
const BANC_READS: usize = 12;
/// The bridge module's reads: the bridge and Epona's jumps, and the area
/// near it.
const BRIDGE_READS: usize = 2;

/// The clear event's played flag.
pub fn played() -> String {
    format!(
        "{}.{}",
        flag::HAVE_PLAYED_EVENT,
        flow::MEET_FOUR_CARPENTER_CLEAR_EVENT
    )
}

/// [`FREED`]'s program.
pub fn exps(gd: &GameData) -> Result<Vec<(String, Vec<Op>)>> {
    Ok(vec![(FREED.into(), vec![Op::read(gd, &played())?])])
}

/// Points each of [`FLOWS`] at [`FREED`].
pub fn flows(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    for (name, want) in FLOWS {
        flows::edit(romfs, rewrites, name.get(), |flow| {
            let n = ask_freed(flow)?;
            ensure!(n == want, "{n} queries ask for the bridge, not {want}");
            Ok(())
        })?;
    }
    Ok(())
}

/// Swaps every query of [`PAST_FIRST`] in `flow` for [`FREED`].
fn ask_freed(flow: &mut Flowchart) -> Result<usize> {
    let mut n = 0;
    for event in &mut flow.events {
        let EventKind::Switch { params, .. } = &mut event.kind else {
            continue;
        };
        if param(params, GAME_DATA.get()) == Some(PAST_FIRST.get()) {
            set(params, GAME_DATA.get(), string(FREED))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Points the reads in Banc file `rel` at [`FREED`] if it's the adult
/// valley's. Returns how many.
pub fn banc(rel: &str, root: &mut Byml) -> Result<usize> {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    if !file.starts_with(&format!("{}_", map::GERUDO_VALLEY_ADULT)) {
        return Ok(0);
    }
    Ok(swap_strings(root.as_mut_map()?, PAST_FIRST.get(), FREED))
}

/// Fails unless [`banc`] swapped every valley read.
pub fn check_banc(n: usize) -> Result<()> {
    ensure!(n == BANC_READS, "{n} valley Banc reads, not {BANC_READS}");
    Ok(())
}

/// Writes the bridge module, the valley's navmesh, the head carpenter's
/// schedule and the map picture reading [`FREED`], and each carpenter's
/// default schedule with his rescue.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let module = logic(map::GERUDO_VALLEY_ADULT_PREFAB_9E43);
    builder.write(&module, &bridge(&romfs.read(&module)?)?)?;
    for path in [
        navmesh(),
        schedule(actor::NPC_HYLIA_012_ADULT),
        CAPTURE.into(),
    ] {
        let mut doc = BymlDoc::parse(&romfs.read(&path)?)?;
        let n = swap_strings(doc.root.as_mut_map()?, PAST_FIRST.get(), FREED);
        ensure!(n == 1, "{path} reads the bridge {n} times, not once");
        builder.write(&path, &doc.to_binary())?;
    }
    let quest = romfs.read(QUEST_SCHEDULES)?;
    let quest = Sarc::new(&quest)?;
    for carpenter in CARPENTERS {
        let path = schedule(carpenter);
        let like = rescue(carpenter);
        let step = quest
            .get_data(&like)
            .with_context(|| format!("no {like}"))?;
        // Sized by its bytes alone, the fortress crashed loading him.
        let new = rescued(&romfs.read(&path)?, step)?;
        builder.write_like(&path, &new, &like, step.len())?;
    }
    Ok(())
}

/// The bridge module reading [`FREED`].
fn bridge(module: &[u8]) -> Result<Vec<u8>> {
    let g = Graph::parse(module)?;
    let key = field::GAME_DATA_NAME_BOOL_EXP.get();
    let reads: Vec<usize> = (g.nodes.iter().enumerate())
        .filter(|(_, n)| n.str(key) == Some(PAST_FIRST.get()))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        reads.len() == BRIDGE_READS,
        "the bridge module reads the quest {} times, not {BRIDGE_READS}",
        reads.len()
    );
    let mut patch = Patch::new(module.to_vec())?;
    for node in reads {
        patch.set_str(node, key, FREED)?;
    }
    Ok(patch.into_bytes())
}

/// `default`, a carpenter's schedule, with the conditional part of `step`,
/// his rescue's.
fn rescued(default: &[u8], step: &[u8]) -> Result<Vec<u8>> {
    let step = BymlDoc::parse(step)?;
    let rescue = (step.root.as_map()?.get(CONDITIONAL)).context("the rescue has no conditions")?;
    let mut doc = BymlDoc::parse(default)?;
    let map = doc.root.as_mut_map()?;
    match map.get(CONDITIONAL) {
        None => {}
        Some(Byml::Array(a)) if a.is_empty() => {}
        Some(_) => bail!("the default schedule has conditions of its own"),
    }
    map.insert(CONDITIONAL.into(), rescue.clone());
    Ok(doc.to_binary())
}

/// `actor`'s default schedule.
fn schedule(actor: &Name) -> String {
    format!("{SCHEDULES}/{actor}.game__component__AIScheduleParam.bgyml")
}

/// The rescue's schedule for `carpenter`, in [`QUEST_SCHEDULES`].
fn rescue(carpenter: &Name) -> String {
    format!(
        "{SCHEDULES}/{}_Step01_{carpenter}_{}.game__component__AIScheduleParam.bgyml",
        flow::SAGE_OF_SPIRIT,
        flag::GERUDO_FORTRESS
    )
}

/// The adult valley's map parameters, where its navmesh swap is.
fn navmesh() -> String {
    format!(
        "Banc/{}/{}.game__banc__GameBancParam.bgyml",
        field::GAME_BANC_PARAM,
        map::GERUDO_VALLEY_ADULT
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump() -> Option<Romfs> {
        crate::game::dev_dump()
    }

    #[test]
    fn every_valley_flow_asks_for_the_carpenters() {
        let Some(romfs) = dump() else { return };
        let mut rewrites = Rewrites::new();
        flows(&romfs, &mut rewrites).unwrap();
        for (name, _) in FLOWS {
            let flow = flows::current(&romfs, &rewrites, name.get()).unwrap();
            let strings = flows::strings(&flow);
            assert!(strings.contains(&FREED), "{name} doesn't ask");
            assert!(
                !strings.contains(&PAST_FIRST.get()),
                "{name} still asks the quest"
            );
        }
    }

    #[test]
    fn the_bridge_module_reads_the_carpenters() {
        let Some(romfs) = dump() else { return };
        let module = logic(map::GERUDO_VALLEY_ADULT_PREFAB_9E43);
        let new = bridge(&romfs.read(&module).unwrap()).unwrap();
        let g = Graph::parse(&new).unwrap();
        let key = field::GAME_DATA_NAME_BOOL_EXP.get();
        let reads = |name: &str| g.nodes.iter().filter(|n| n.str(key) == Some(name)).count();
        assert_eq!((reads(FREED), reads(PAST_FIRST.get())), (BRIDGE_READS, 0));
    }

    #[test]
    fn the_valley_bancs_read_the_carpenters_and_the_fortress_keeps_the_quest() {
        let Some(romfs) = dump() else { return };
        let files = crate::checks::banc_files(&romfs).unwrap();
        let mut n = 0;
        for (rel, doc) in crate::checks::load_bancs(&romfs, files) {
            let mut doc = doc.unwrap();
            let swapped = banc(&rel, &mut doc.root).unwrap();
            assert!(
                swapped == 0 || rel.contains(map::GERUDO_VALLEY_ADULT.get()),
                "{rel}"
            );
            n += swapped;
        }
        check_banc(n).unwrap();
    }

    #[test]
    fn the_other_valley_files_read_the_carpenters() {
        let Some(romfs) = dump() else { return };
        let dir = std::env::temp_dir().join(format!("zootr-valley-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        write(&romfs, &mut builder).unwrap();
        // The module, the navmesh, the head carpenter, the map, four rescues
        // and the size table.
        assert_eq!(builder.finish().unwrap().len(), 9);
        // Sized by its bytes alone, the fortress crashed: each rescue costs
        // at least what his Step01 schedule does.
        let table = romfs.restbl_rel().unwrap();
        let written = crate::game::read_built(&romfs, &dir, &table);
        let restbl = ounce_formats::Restbl::parse(&written).unwrap();
        for carpenter in CARPENTERS {
            let (own, step) = (
                restbl.get(&schedule(carpenter)),
                restbl.get(&rescue(carpenter)),
            );
            assert!(own.unwrap() >= step.unwrap(), "{carpenter}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn each_carpenter_keeps_his_rescue_past_step01() {
        let Some(romfs) = dump() else { return };
        let quest = romfs.read(QUEST_SCHEDULES).unwrap();
        let quest = Sarc::new(&quest).unwrap();
        for carpenter in CARPENTERS {
            let step = quest.get_data(&rescue(carpenter)).unwrap();
            let new = rescued(&romfs.read(&schedule(carpenter)).unwrap(), step).unwrap();
            let new = BymlDoc::parse(&new).unwrap();
            let step = BymlDoc::parse(step).unwrap();
            let map = new.root.as_map().unwrap();
            assert_eq!(
                map.get(CONDITIONAL),
                step.root.as_map().unwrap().get(CONDITIONAL)
            );
            assert!(
                map.contains_key("DefaultSchedule"),
                "{carpenter} lost his own talk"
            );
        }
    }

    #[test]
    fn freed_reads_the_clear_event() {
        let Some(romfs) = dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let (name, program) = exps(&gd).unwrap().remove(0);
        let mut doc = BymlDoc::parse(&romfs.read(crate::gamedata::PATH).unwrap()).unwrap();
        crate::gamedata::add_exps(&mut doc.root, &[(name, program)]).unwrap();
        let new = GameData::parse(&doc.to_binary()).unwrap();
        let played = new.name(ounce_formats::murmur3(played().as_bytes()));
        assert_eq!(new.expr(FREED).unwrap().unwrap(), played);
    }
}
