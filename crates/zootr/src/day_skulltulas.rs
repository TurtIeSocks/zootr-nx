//! Gold Skulltulas by day. Three things keep a night-only one in the night:
//! the night logic module deletes it by day, its graph's day branch plays
//! its despawn, and its emit timer (game code that asks the clock itself)
//! holds it twitching in place. The module's delete reads false, as the
//! all-day module's does, the day branch reads "is it dead" for the time
//! (false wherever it runs), and a do-nothing node runs for the timer.
//! Out by day, it turns on its wall as it does at night (Mac, 2026-11-19).
//!
//! ```text
//! night module:  delete = (day at load and not night) or (day, alive, leaving)
//!                (now: false)
//! own graph:     if night only and not dead and day (now: dead): leave
//!                emit timer (now: nothing)
//! ```

use anyhow::{Context, Result};
use ounce_formats::ainb::{File, Graph, Value};
use ounce_formats::{ModBuilder, Romfs};
use roead::sarc::{Sarc, SarcWriter};

use crate::flags::actor_pack;
use crate::graphs::logic;
use crate::known::{actor, field, map};

/// The graph in the Gold Skulltula's pack that sends it off by day.
fn graph() -> String {
    format!("AI/{}.utility.root.ainb", actor::ENEMY_SKULLTULA_GOLD.get())
}

/// Writes the Gold Skulltula's pack with its day check and emit timer cut
/// (its size table entry resized), and the night module with no delete.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let path = actor_pack(actor::ENEMY_SKULLTULA_GOLD.get());
    let data = romfs.read(&path)?;
    let pack = Sarc::new(&data)?;
    let file = graph();
    let vanilla = pack.get_data(&file).context("no Gold Skulltula graph")?;
    let (old, new) = (vanilla.len(), awake(vanilla)?);
    let len = new.len();
    let packed = SarcWriter::from_sarc(&pack)
        .with_file(file.as_str(), new)
        .to_binary();
    builder.write(&path, &packed)?;
    builder.resize(&file, old, len)?;
    let module = logic(map::GIMMICK_NIGHTTIME_SKULLTULA_GOLD_APPEAR_05A9);
    builder.write(&module, &kept(&romfs.read(&module)?)?)
}

/// The night module with its Gold Skulltula's delete cut to false.
fn kept(module: &[u8]) -> Result<Vec<u8>> {
    let g = Graph::parse(module)?;
    let skulltula = (g.nodes.iter())
        .position(|n| n.name == field::ACTOR_LOGIC_ENEMY_SKULLTULA_GOLD.get())
        .context("no Gold Skulltula in the night module")?;
    let mut f = File::parse(module)?;
    f.unlink(skulltula, field::LOGIC_DELETE.get(), &Value::Bool(false))?;
    f.to_bytes()
}

/// `vanilla` with the day question read from the death query instead.
fn awake(vanilla: &[u8]) -> Result<Vec<u8>> {
    let g = Graph::parse(vanilla)?;
    let class = |name: &str| {
        (g.nodes.iter().position(|n| n.name == name)).with_context(|| format!("no {name} node"))
    };
    let (time, dead) = (
        class(field::QUERY_TIME_DIVISION.get())?,
        class(field::QUERY_DEATH_IS_DEAD.get())?,
    );
    let day = field::IS_DAY_TIME.get();
    let reader = (g.nodes.iter().enumerate())
        .find_map(|(i, n)| {
            n.inputs
                .iter()
                .find(|input| (input.sources.iter()).any(|l| l.node == time && l.output == day))
                .map(|input| (i, input.name.clone()))
        })
        .context("nothing reads the day")?;
    let mut f = File::parse(vanilla)?;
    f.swap_source(
        reader.0,
        &reader.1,
        (time, day),
        (dead, field::IS_DEAD.get()),
    )?;
    // The emit timer asks the clock in the game's own code and holds a
    // day Skulltula in place: a do-nothing node runs in its stead.
    let (emit, idle) = (
        class(field::EXECUTE_SKULLTULA_GOLD_EMIT_TIME.get())?,
        class(field::EXECUTE_GENERIC_DUMMY_ACTION.get())?,
    );
    let parent = (0..g.nodes.len())
        .find(|&p| {
            f.children(p)
                .is_ok_and(|c| c.iter().any(|(_, t)| *t == emit as i32))
        })
        .context("nothing runs the emit timer")?;
    f.repoint_child(parent, emit, idle)?;
    f.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use ounce_formats::ainb::check;

    /// Each input fed by `node`'s `output`, as reader and input name.
    fn readers(g: &Graph, node: usize, output: &str) -> Vec<(usize, String)> {
        let reads = |input: &&ounce_formats::ainb::Input| {
            (input.sources.iter()).any(|l| l.node == node && l.output == output)
        };
        (g.nodes.iter().enumerate())
            .flat_map(|(i, n)| {
                n.inputs
                    .iter()
                    .filter(reads)
                    .map(move |inp| (i, inp.name.clone()))
            })
            .collect()
    }

    #[test]
    fn the_day_question_reads_the_death_query() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let data = romfs
            .read(&actor_pack(actor::ENEMY_SKULLTULA_GOLD.get()))
            .unwrap();
        let pack = Sarc::new(&data).unwrap();
        let vanilla = pack.get_data(&graph()).unwrap();
        let bytes = awake(vanilla).unwrap();
        check(&bytes).unwrap();
        let (was, now) = (
            Graph::parse(vanilla).unwrap(),
            Graph::parse(&bytes).unwrap(),
        );
        assert_eq!(was.nodes.len(), now.nodes.len());
        let at = |g: &Graph, name: &str| g.nodes.iter().position(|n| n.name == name).unwrap();
        let (time, dead) = (
            at(&now, field::QUERY_TIME_DIVISION.get()),
            at(&now, field::QUERY_DEATH_IS_DEAD.get()),
        );
        let day = field::IS_DAY_TIME.get();
        let selector = readers(&was, time, day);
        assert_eq!(selector.len(), 1);
        assert!(readers(&now, time, day).is_empty());
        let after = readers(&now, dead, field::IS_DEAD.get());
        assert!(after.contains(&selector[0]), "{after:?}");
    }

    #[test]
    fn the_night_module_never_deletes_its_skulltula() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let module = logic(map::GIMMICK_NIGHTTIME_SKULLTULA_GOLD_APPEAR_05A9);
        let bytes = kept(&romfs.read(&module).unwrap()).unwrap();
        check(&bytes).unwrap();
        let g = Graph::parse(&bytes).unwrap();
        let gold = field::ACTOR_LOGIC_ENEMY_SKULLTULA_GOLD.get();
        let node = g.nodes.iter().find(|n| n.name == gold).unwrap();
        let delete = node
            .inputs
            .iter()
            .find(|i| i.name == field::LOGIC_DELETE.get());
        let delete = delete.unwrap();
        assert!(delete.sources.is_empty());
        assert_eq!(delete.value, Value::Bool(false));
    }
}
