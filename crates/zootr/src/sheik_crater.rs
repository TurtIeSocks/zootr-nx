//! Sheik in the Crater stays until Link has her gift. Her crater graph made
//! the area that starts her Bolero scene only while the scene hadn't played
//! and the fire sage quest was before `EnterTemple`, which the scene moves
//! it to. Entering the Fire Temple first moves the quest past that too, so
//! she was gone for good. The area now waits on the scene alone, the flag
//! the tracker reads as her check done. The scene's own quest move still
//! waits on the quest being before it, so it never sets the quest back.

use anyhow::{Context, Result, ensure};
use ounce_formats::ainb::{File, Graph, Link};
use ounce_formats::{ModBuilder, Romfs};

use crate::graphs::logic;
use crate::known::{Name, field, map};

/// Writes the crater graph with the area waiting on the scene alone.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let module = logic(map::DEATH_MOUNTAIN_CRATER_ADULT_PREFAB_E63C);
    builder.write(&module, &staying(&romfs.read(&module)?)?)
}

/// `module` with the area made while the scene hasn't played, and the
/// quest move behind its own before-`EnterTemple` question.
fn staying(module: &[u8]) -> Result<Vec<u8>> {
    let g = Graph::parse(module)?;
    let class = |name: &Name| {
        let mut at = (g.nodes.iter().enumerate()).filter(|(_, n)| n.name == name.get());
        match (at.next(), at.next()) {
            (Some((i, _)), None) => Ok(i),
            _ => anyhow::bail!("not one {name} node in the crater graph"),
        }
    };
    let source = |node: usize, input: &Name| -> Result<Vec<Link>> {
        let input = g.nodes[node].inputs.iter().find(|i| i.name == input.get());
        Ok(input
            .with_context(|| format!("#{node} has no {input:?}"))?
            .sources
            .clone())
    };
    let (area, step) = (
        class(field::ACTOR_LOGIC_AREA_ACTOR)?,
        class(field::TRIGGER_CHALLENGE_SET_STEP)?,
    );
    let [both] = &source(area, field::LOGIC_CREATE)?[..] else {
        anyhow::bail!("the area isn't made by one node");
    };
    let asks = source(both.node, field::BOOL_MULTI)?;
    let by = |output: &Name| asks.iter().find(|l| l.output == output.get());
    let (unplayed, before) = (by(field::INV_BOOL), by(field::IS_BEFORE));
    let (Some(unplayed), Some(_)) = (unplayed, before) else {
        anyhow::bail!("the area isn't made on the scene and the quest: {asks:?}");
    };
    ensure!(
        asks.len() == 2,
        "the area waits on {} things, not 2",
        asks.len()
    );
    let [scene] = &source(step, field::IS_DRIVE)?[..] else {
        anyhow::bail!("the quest move isn't driven by one node");
    };
    ensure!(
        scene.output == field::EVENT_FINISH.get(),
        "{scene:?} isn't the scene's end"
    );

    let mut f = File::parse(module)?;
    let create = field::LOGIC_CREATE.get();
    let and = (both.node, field::TRUE.get());
    f.swap_source(area, create, and, (unplayed.node, &unplayed.output))?;
    // A copy of the area's question, asking the scene's end where it asked
    // the scene's played flag: the quest moves only from before `EnterTemple`.
    let guard = f.duplicate(both.node)?;
    let (multi, end) = (
        field::BOOL_MULTI.get(),
        (scene.node, field::EVENT_FINISH.get()),
    );
    f.swap_source(guard, multi, (unplayed.node, &unplayed.output), end)?;
    f.swap_source(step, field::IS_DRIVE.get(), end, (guard, field::TRUE.get()))?;
    f.to_bytes()
}

#[cfg(test)]
mod tests {
    use ounce_formats::ainb::Value;

    use super::*;
    use crate::gifts::GIFTS;

    /// The area waits on the scene's played flag alone, which is the flag
    /// her gift is found by, and the quest move on the scene's end and the
    /// quest being before `EnterTemple`.
    #[test]
    fn sheik_stays_in_the_crater_until_her_scene_plays() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let module = logic(map::DEATH_MOUNTAIN_CRATER_ADULT_PREFAB_E63C);
        let g = Graph::parse(&staying(&romfs.read(&module).unwrap()).unwrap()).unwrap();
        let node = |name: &Name| g.nodes.iter().position(|n| n.name == name.get()).unwrap();
        let reads = |n: usize, input: &Name| {
            let input = g.nodes[n].inputs.iter().find(|i| i.name == input.get());
            input.unwrap().sources.clone()
        };

        let [made] = &reads(node(field::ACTOR_LOGIC_AREA_ACTOR), field::LOGIC_CREATE)[..] else {
            panic!("the area isn't made by one node");
        };
        assert_eq!(made.output, field::INV_BOOL.get());
        let [played] = &reads(made.node, field::BOOL_MULTI)[..] else {
            panic!("the area's inverse doesn't read one flag");
        };
        let wanted = (GIFTS.iter())
            .find(|g| g.giver == "Sheik in the Crater")
            .and_then(|g| g.found())
            .unwrap();
        let n = &g.nodes[played.node];
        let values = (n.properties.iter().map(|(_, v)| v)).chain(n.inputs.iter().map(|i| &i.value));
        assert!(
            values.clone().any(|v| *v == Value::String(wanted.clone())),
            "the area waits on {:?}, not {wanted}",
            values.collect::<Vec<_>>()
        );

        let step = node(field::TRIGGER_CHALLENGE_SET_STEP);
        let [guard] = &reads(step, field::IS_DRIVE)[..] else {
            panic!("the quest move isn't driven by one node");
        };
        let asks: Vec<_> = reads(guard.node, field::BOOL_MULTI)
            .into_iter()
            .map(|l| l.output)
            .collect();
        assert_eq!(
            asks.iter()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>(),
            [field::EVENT_FINISH.get(), field::IS_BEFORE.get()].into(),
        );
    }
}
