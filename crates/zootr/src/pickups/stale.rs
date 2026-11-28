//! The shared drop graph skips the fall and bounce for shuffled Gold
//! Skulltula rewards. It also removes stepped rewards across a level-up:
//! each step has its own pickup, and a drop still in the world would
//! otherwise give its old step. Each change is gated by the actor's name.

use anyhow::{Context, Result};
use ounce_formats::ainb::{File, Value};
use roead::sarc::Sarc;

use super::shared;
use crate::known::field;

/// The name every step stand-in starts with, and nothing else does.
pub const STEP: &str = "Obj_RandoStep_";
/// Every changed Skulltula reward, including the ones that show a step.
pub const HOVER: &str = "Obj_Rando_Hover_";
/// A Skulltula reward whose old step must disappear on a level-up.
pub const HOVER_STEP: &str = "Obj_Rando_Hover_Step_";
/// What the BoolExp a step stand-in stands by is named after it.
const DROP: &str = "Rando_Drop_";
/// `LifeDepleted`, the death reason every breakable's own graph asks. The
/// default death table ignores reason 0.
const LIFE_DEPLETED: i32 = 10;

/// The BoolExp true while step stand-in `stand_in` is the one its holder
/// gives. False, a lying drop of it removes itself.
pub fn exp(stand_in: &str) -> String {
    format!("{DROP}{stand_in}")
}

/// The drop graph's path in the pack.
pub fn graph() -> String {
    format!("AI/{}.root.ainb", field::DROP_ITEM.get())
}

/// The pack's drop graph with the branch hung last under its root.
pub fn branched(pack: &Sarc, steps: bool, hovering: bool) -> Result<Vec<u8>> {
    let vanilla = File::parse(pack.get_data(&graph()).context("no drop graph")?)?;
    let mut f = vanilla.clone();
    if hovering {
        // Keep the idle spin and pickup handling. Only the bound action
        // that applies drop motion should be skipped.
        let (select, nothing) = shared::selector_and_leaf(&vanilla)?;
        let is_hover = f.copy_node(&vanilla, select)?;
        let bound = vanilla
            .find(field::EXECUTE_DROP_BOUND.get())
            .context("no drop bound")?;
        let (starts, _) = shared::starts_with(pack, &mut f, HOVER)?;
        let idle = f.copy_node(&vanilla, nothing)?;
        f.link(is_hover, "Input", starts, "Result")?;
        f.set_child(is_hover, "True", idle)?;
        f.set_child(is_hover, "False", bound)?;
        f.repoint_child(vanilla.root()?, bound, is_hover)?;
    }
    if steps {
        expire(pack, &vanilla, &mut f, STEP)?;
        if hovering {
            expire(pack, &vanilla, &mut f, HOVER_STEP)?;
        }
    }
    f.to_bytes()
}

fn expire(pack: &Sarc, vanilla: &File, f: &mut File, prefix: &str) -> Result<()> {
    let read = field::QUERY_GET_NON_STRUCT_GAME_DATA_BOOL_EXP;
    let (starts, read) = shared::name_check(pack, f, prefix, DROP, read)?;
    let death = shared::copy(pack, f, field::ONE_SHOT_DEATH_REQUEST_DEATH.get())?;
    let (select, nothing) = shared::selector_and_leaf(vanilla)?;
    let is_step = f.copy_node(vanilla, select)?;
    let standing = f.copy_node(vanilla, select)?;
    let others = f.copy_node(vanilla, nothing)?;
    let kept = f.copy_node(vanilla, nothing)?;

    f.link(is_step, "Input", starts, "Result")?;
    f.set_child(is_step, "True", standing)?;
    f.set_child(is_step, "False", others)?;
    f.link(standing, "Input", read, "Value")?;
    f.set_child(standing, "True", kept)?;
    f.set_child(standing, "False", death)?;
    f.set_input(death, field::DEATH_RESON.get(), &Value::Int(LIFE_DEPLETED))?;
    f.add_child(vanilla.root()?, "", is_step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::pickups::GLOBAL_AI;
    use ounce_formats::ainb::{Graph, Link, check};

    #[test]
    fn hovering_rewards_skip_the_fall_and_old_steps_expire() {
        let Some(romfs) = dev_dump() else { return };
        let data = romfs.read(GLOBAL_AI).unwrap();
        let pack = Sarc::new(&data).unwrap();
        let bytes = branched(&pack, true, true).unwrap();
        check(&bytes).unwrap();
        let f = File::parse(&bytes).unwrap();
        let g = Graph::parse(&bytes).unwrap();
        let children = f.children(f.root().unwrap()).unwrap();
        let named = |prefix| {
            children
                .iter()
                .map(|(_, n)| *n as usize)
                .find(|&n| {
                    let Some(input) = g.nodes[n].input("Input") else {
                        return false;
                    };
                    input
                        .sources
                        .iter()
                        .any(|link| g.nodes[link.node].str("Token") == Some(prefix))
                })
                .unwrap()
        };
        let child = |n, plug| {
            f.children(n)
                .unwrap()
                .iter()
                .find(|(p, _)| p == plug)
                .unwrap()
                .1 as usize
        };
        let hover = named(HOVER);
        assert_eq!(
            g.nodes[child(hover, "False")].name,
            field::EXECUTE_DROP_BOUND.get()
        );
        assert_eq!(
            g.nodes[child(hover, "True")].name,
            field::ONESHOT_DO_NOTHING.get()
        );
        let current = child(named(HOVER_STEP), "True");
        assert_eq!(
            g.nodes[child(current, "False")].name,
            field::ONE_SHOT_DEATH_REQUEST_DEATH.get()
        );
    }

    #[test]
    fn a_step_drop_dies_while_its_boolexp_is_false() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let data = romfs.read(GLOBAL_AI).unwrap();
        let pack = Sarc::new(&data).unwrap();
        let vanilla = File::parse(pack.get_data(&graph()).unwrap()).unwrap();
        let bytes = branched(&pack, true, false).unwrap();
        check(&bytes).unwrap();
        let f = File::parse(&bytes).unwrap();
        let g = Graph::parse(&bytes).unwrap();
        let root = f.root().unwrap();
        assert_eq!(root, vanilla.root().unwrap());

        // The vanilla children stay first, the branch comes last.
        let children = f.children(root).unwrap();
        let (last, kept) = children.split_last().unwrap();
        assert_eq!(kept, vanilla.children(root).unwrap());
        let is_step = last.1 as usize;
        let child = |n: usize, plug: &str| {
            let c = f.children(n).unwrap();
            c.iter().find(|(p, _)| p == plug).unwrap().1 as usize
        };
        let from = |n: usize, input: &str| -> Link {
            let s = &g.nodes[n].input(input).unwrap().sources;
            assert_eq!(s.len(), 1, "{input}");
            s[0].clone()
        };
        let class = |n: usize| g.nodes[n].name.as_str();

        let starts = from(is_step, "Input").node;
        assert_eq!(class(starts), field::QUERY_GENERIC_STRING_STARTS_WITH.get());
        assert_eq!(g.nodes[starts].str("Token"), Some(STEP));
        let name = from(starts, "Input");
        assert_eq!(name.output, field::ACTOR_FILE_NAME.get());
        assert_eq!(class(name.node), field::QUERY_GENERIC_GET_ACTOR_NAME.get());
        let own = from(name.node, "Actor").node;
        assert_eq!(class(own), field::QUERY_GENERIC_GET_OWN_ACTOR_LINK.get());

        let others = child(is_step, "False");
        assert_eq!(class(others), field::ONESHOT_DO_NOTHING.get());
        let standing = child(is_step, "True");
        let read = from(standing, "Input").node;
        assert_eq!(
            class(read),
            field::QUERY_GET_NON_STRUCT_GAME_DATA_BOOL_EXP.get()
        );
        let concat = from(read, field::GAME_DATA_FULL_NAME.get()).node;
        assert_eq!(class(concat), field::QUERY_GENERIC_CONCAT_STRING.get());
        assert_eq!(g.nodes[concat].str("Input0"), Some(DROP));
        assert_eq!(from(concat, "Input1"), name);
        assert_eq!(g.nodes[concat].str("Input2"), Some(""));

        assert_eq!(
            class(child(standing, "True")),
            field::ONESHOT_DO_NOTHING.get()
        );
        let death = child(standing, "False");
        assert_eq!(class(death), field::ONE_SHOT_DEATH_REQUEST_DEATH.get());
        assert_eq!(
            g.nodes[death].value(field::DEATH_RESON.get()),
            Some(&Value::Int(LIFE_DEPLETED))
        );
        assert!(f.find(field::ONE_SHOT_DEATH_REQUEST_DEATH.get()).is_some());
        assert!(
            vanilla
                .find(field::ONE_SHOT_DEATH_REQUEST_DEATH.get())
                .is_none()
        );
    }
}
