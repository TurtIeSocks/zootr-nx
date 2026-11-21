//! The rainbow bridge at six medallions. See
//! `docs/superpowers/specs/2026-11-12-win-conditions-design.md`.

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, EventKind, Romfs, murmur3};

use crate::flows::{self, Rewrites};
use crate::gamedata::{self, op};
use crate::known::{Name, field, flag, flow, item};

/// The bridge's logic module arms its walk-in trigger while the Ganon step
/// is exactly Step01, read as "after Ready and not after this". Nothing else
/// reads this one.
const ARMED_UNTIL: &Name = flag::DEFEAT_GANON_IS_AFTER_STEP01_EXP;
/// Step01 from the start, so the trigger is armed whenever the medallions
/// are in.
pub fn start() -> String {
    crate::steps::at(flow::DEFEAT_GANON, "Step01")
}
pub const MEDALLIONS: [&Name; 6] = [
    item::MEDAL_FOREST,
    item::MEDAL_FIRE,
    item::MEDAL_WATER,
    item::MEDAL_SPIRIT,
    item::MEDAL_SHADOW,
    item::MEDAL_LIGHT,
];
/// Zelda's Light Arrows scene, which also sets Step01.
const ARROWS: (&Name, &Name) = (flow::ALL_SAGE, field::MARKET_ADULT_FIELD_GET_ARROW_LIGHT);
const STEP: (&Name, &Name) = (
    field::CHALLENGE_SYSTEM_ACTOR,
    flow::EVENT_EXECUTE_PROGRESS_CHALLENGE_STEP,
);

/// Keep the trigger armed past Step01 while any medallion is missing:
/// `step > Step01 || !(every medallion count > 0)`.
pub fn need_medallions(root: &mut Byml) -> Result<()> {
    let values = gamedata::program(root, ARMED_UNTIL)?;
    ensure!(values.len() == 2, "{ARMED_UNTIL} isn't one comparison");
    let first = values[0].as_array()?.to_vec();
    let ins = |code, operand| gamedata::instruction(&first, code, operand);
    for (i, medal) in MEDALLIONS.iter().enumerate() {
        let count = murmur3(format!("PouchData.{medal}.{}", flag::STOCK_NUM).as_bytes());
        values.push(ins(op::INT_GREATER, Some(i64::from(count)))?);
        values.push(ins(op::INT_CONSTANT, Some(0))?);
        if i > 0 {
            values.push(ins(op::AND, None)?);
        }
    }
    values.push(ins(op::NOT, None)?);
    values.push(ins(op::OR, None)?);
    Ok(())
}

/// Zelda's Light Arrows scene keeps the Ganon step where it is, so a
/// bridge raised before it isn't rolled back to Step01.
pub fn keep_step(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, ARROWS.0, |flow| {
        let reached = flows::reachable(flow, ARROWS.1)?;
        let step01 = flows::running(flow, (STEP.0, STEP.1))
            .into_iter()
            .find(|&e| {
                reached.contains(&e)
                    && match &flow.events[e as usize].kind {
                        EventKind::Action { params, .. } => {
                            flows::param(params, field::CHALLENGE_NAME)
                                .is_some_and(|v| v == flow::DEFEAT_GANON)
                                && flows::param(params, field::STEP_NAME) == Some("Step01")
                        }
                        _ => false,
                    }
            });
        flows::step_over(flow, step01.context("the Light Arrows scene sets no Step01")?)
    })
}

#[cfg(test)]
mod tests {
    use ounce_formats::BymlDoc;

    use super::*;
    use crate::game::dev_dump;
    use crate::gamedata::{self, GameData};
    use crate::known::{field, flag, flow};

    #[test]
    fn the_bridge_waits_for_every_medallion() {
        let Some(romfs) = dev_dump() else { return };
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        let mut gd = GameData::load(&romfs).unwrap();
        let before = gd.expr(ARMED_UNTIL).unwrap().unwrap();
        need_medallions(&mut doc.root).unwrap();
        for medal in MEDALLIONS {
            gd.learn(&format!("PouchData.{medal}.{}", flag::STOCK_NUM));
        }
        let text = render(&gd, &doc.root);
        assert!(text.starts_with(&before), "{text}");
        for medal in MEDALLIONS {
            assert!(
                text.contains(&format!("PouchData.{medal}.{} > 0", flag::STOCK_NUM)),
                "{text}"
            );
        }
        assert!(text.contains("|| !("), "{text}");
    }

    /// The edited BoolExp, rendered the way `probe data` shows it.
    fn render(gd: &GameData, root: &Byml) -> String {
        let hash = i64::from(murmur3(ARMED_UNTIL.as_bytes()));
        let table = root.as_map().unwrap()["Data"].as_map().unwrap()[flag::BOOL_EXP.get()]
            .as_array()
            .unwrap();
        let entry = table
            .iter()
            .find(|e| e["Hash"].as_int::<i64>().is_ok_and(|h| h == hash))
            .unwrap();
        gd.render(entry).unwrap()
    }

    #[test]
    fn the_light_arrows_leave_the_ganon_step_alone() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        keep_step(&romfs, &mut rewrites).unwrap();
        let flow = ounce_formats::Flowchart::parse(&rewrites[&flows::rel(ARROWS.0)])
            .unwrap()
            .unwrap();
        let reached = flows::reachable(&flow, ARROWS.1).unwrap();
        let sets = |e: &u16| match &flow.events[*e as usize].kind {
            EventKind::Action { params, .. } => {
                flows::param(params, field::CHALLENGE_NAME).is_some_and(|v| v == flow::DEFEAT_GANON)
            }
            _ => false,
        };
        let steps = flows::running(&flow, (STEP.0, STEP.1));
        assert!(!steps.iter().filter(|e| reached.contains(e)).any(sets));
        assert!(steps.iter().any(|e| reached.contains(e)));
    }
}
