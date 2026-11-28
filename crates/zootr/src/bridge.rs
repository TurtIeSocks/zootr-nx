//! The rainbow bridge at six medallions, or open, and Ganon's trials done.
//! See
//! `docs/superpowers/specs/2026-11-12-win-conditions-design.md` and
//! `docs/superpowers/specs/2026-11-21-basic-win-condition-design.md`.

use anyhow::{Context, Result, ensure};
use ounce_formats::ainb::Patch;
use ounce_formats::{Byml, EventKind, ModBuilder, Romfs, murmur3};

use crate::cutscenes::played_set;
use crate::flows::{self, Rewrites};
use crate::gamedata::{self, op};
use crate::graphs::{logic, node};
use crate::known::{Name, actor, field, flag, flow, item, map};
use crate::settings::Trials;

/// The bridge's logic module arms its walk-in trigger while the Ganon step
/// is exactly Step01, read as "after Ready and not after this". Nothing else
/// reads this one.
const ARMED_UNTIL: &Name = flag::DEFEAT_GANON_IS_AFTER_STEP01_EXP;
/// Step01 from the start, so the trigger is armed whenever the medallions
/// are in.
pub fn start() -> String {
    crate::steps::at(flow::DEFEAT_GANON, "Step01")
}
/// The bridge up from the start: the walk-in scene counts as played, which
/// the bridge's module ORs into creating the bridge. Nothing else: no Light
/// Arrows, no sages, no Ganon step.
pub fn open() -> Vec<String> {
    vec![played_set(field::GANON_CASTLE_ADULT_FIELD_RAINBOW)]
}
/// The six trials in Ganon's Castle, as their barrier flags name them.
const TRIALS: [&str; 6] = ["Light", "Forest", "Water", "Shadow", "Fire", "Spirit"];

/// Every barrier in Ganon's Castle down from the start: what the six
/// barrier scenes leave set, played and removed each, none left and all
/// clear. Nothing else: no Ganon step, no bridge.
pub fn trials_done() -> Vec<String> {
    let mut edits = vec![
        format!("{}=0", field::INSIDE_GANON_CASTLE_DUNGEON_BARRIER_COUNT),
        format!("{}=true", field::INSIDE_GANON_CASTLE_IS_ALL_BARRIER_CLEAR),
    ];
    for trial in TRIALS {
        let removed = format!("InsideGanonCastle_Dungeon_BarrierRemove_{trial}");
        edits.push(played_set(&removed));
        edits.push(format!("{removed}=true"));
    }
    edits
}

/// Trials Done clears the barrier without playing its scene. The pots use
/// the same saved flag that removes the barrier.
pub fn write_trial_pots(builder: &mut ModBuilder, trials: Trials) -> Result<()> {
    if trials == Trials::Done {
        let module = logic(map::INSIDE_GANON_CASTLE_LEVEL_ROOM_01_PREFAB_75AF);
        builder.write(&module, &pots_after_barriers(builder.read(&module)?)?)?;
    }
    Ok(())
}

fn pots_after_barriers(data: Vec<u8>) -> Result<Vec<u8>> {
    let mut patch = Patch::new(data)?;
    let query = (
        field::HOLD_GAME_DATA_GET_BOOL_SIMPLE,
        field::GAME_DATA_NAME_BOOL,
    );
    let scene = node(
        patch.graph(),
        query,
        flag::HAVE_PLAYED_EVENT_DM_INSIDE_GANON_CASTLE_0090,
    )?;
    let clear = node(
        patch.graph(),
        query,
        field::INSIDE_GANON_CASTLE_IS_ALL_BARRIER_CLEAR,
    )?;
    let create = field::LOGIC_CREATE.get();
    let pots: Vec<_> = patch
        .graph()
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            n.name == field::ACTOR_LOGIC_SIMPLE_ACTOR
                && n.str(field::INSTANCE_NAME)
                    .is_some_and(|name| name.starts_with(actor::OBJ_POT_A_06.get()))
                && n.input(create).is_some_and(|input| {
                    matches!(input.sources.as_slice(), [s] if s.node == scene && s.output == "Bool")
                })
        })
        .map(|(i, _)| i)
        .collect();
    ensure!(
        pots.len() == 4,
        "expected four barrier pots, found {}",
        pots.len()
    );
    for pot in pots {
        patch.relink(pot, create, 0, clear, "Bool")?;
    }
    Ok(patch.into_bytes())
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
        flows::step_over(
            flow,
            step01.context("the Light Arrows scene sets no Step01")?,
        )
    })
}

#[cfg(test)]
mod tests {
    use ounce_formats::BymlDoc;
    use ounce_formats::ainb::Graph;

    use super::*;
    use crate::game::dev_dump;
    use crate::gamedata::{self, GameData};
    use crate::known::{field, flag, flow};
    use crate::rules::{Book, Ctx, Have};
    use crate::settings::{NewGame, Settings};
    use crate::world::Era;

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

    #[test]
    fn open_and_done_set_flags_the_game_has() {
        let Some(romfs) = dev_dump() else { return };
        let gd = GameData::load(&romfs).unwrap();
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        crate::presets::apply(&gd, &mut doc.root, &open()).unwrap();
        crate::presets::apply(&gd, &mut doc.root, &trials_done()).unwrap();
    }

    #[test]
    fn the_four_barrier_pots_spawn_when_the_trials_are_clear() {
        let Some(romfs) = dev_dump() else { return };
        let module = logic(map::INSIDE_GANON_CASTLE_LEVEL_ROOM_01_PREFAB_75AF);
        let vanilla = romfs.read(&module).unwrap();
        let mut expected = Graph::parse(&vanilla).unwrap();
        let flag_of = |name: &Name| {
            expected
                .nodes
                .iter()
                .position(|n| n.str(field::GAME_DATA_NAME_BOOL) == Some(name.get()))
                .unwrap()
        };
        let scene = flag_of(flag::HAVE_PLAYED_EVENT_DM_INSIDE_GANON_CASTLE_0090);
        let clear = flag_of(field::INSIDE_GANON_CASTLE_IS_ALL_BARRIER_CLEAR);
        let mut linked = 0;
        for node in &mut expected.nodes {
            let pot = node
                .str(field::INSTANCE_NAME)
                .is_some_and(|name| name.starts_with(actor::OBJ_POT_A_06.get()));
            for input in &mut node.inputs {
                for source in input.sources.iter_mut().filter(|s| s.node == scene) {
                    assert_eq!(input.name, field::LOGIC_CREATE.get());
                    assert!(pot);
                    source.node = clear;
                    linked += 1;
                }
            }
        }
        assert_eq!(linked, 4);
        let actual = pots_after_barriers(vanilla).unwrap();
        assert_eq!(Graph::parse(&actual).unwrap(), expected);
    }

    #[test]
    fn the_four_barrier_pots_require_the_trials_only_on_all_six() {
        let Some(_) = dev_dump() else { return };
        let book = Book::load().unwrap();
        let context = |trials: &str| {
            let settings = Settings::from([
                (crate::settings::TRIALS.into(), trials.into()),
                (crate::settings::BOMBCHUS.into(), "free".into()),
            ]);
            Ctx::new(&NewGame::from_settings(&settings, &[]).unwrap())
        };
        let all_six = context("all-six");
        let done = context("done");
        let empty = Have::default();
        let mut tools = Have::default();
        for tool in [
            crate::magic::meter::METER,
            item::ARROW_LIGHT,
            item::BOMBCHU_BUNDLE_SMALL,
            item::BOTTLE,
            item::DIN_FIRE,
            item::GAUNTLET_GOLD,
            item::GAUNTLET_SILVER,
            item::GLASS_TRUTH,
            item::HAMMER,
            item::HOOKSHOT,
            item::HOVER_BOOT,
            item::LONGSHOT,
            item::RANGE_BOW,
            item::SHIELD_MIRROR,
        ] {
            tools.add(tool);
        }
        for hash in [
            13727300571360846696,
            10177407372807102311,
            4656702765770784238,
            9619715366192254862,
        ] {
            let rule = book
                .check(hash)
                .unwrap_or_else(|| panic!("no rule for pot {hash}"));
            let cleared = book.need(&rule.needs, &all_six).unwrap();
            assert!(!cleared.met(&empty, Era::Adult));
            assert!(cleared.met(&tools, Era::Adult));
            assert!(
                book.need(&rule.needs, &done)
                    .unwrap()
                    .met(&empty, Era::Adult)
            );
        }
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
