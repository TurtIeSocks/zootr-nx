//! A built stand-in's take, marked without the hook. The pickup event graph
//! runs one sequence on a take that opens a get window: into the pouch,
//! then the respawn change. The seed hangs a branch last on it that sets
//! the stand-in's own Bool. Vanilla pickups stop at the name check.
//!
//! ```text
//! name = file name of the pickup's own actor
//! if name starts with PREFIX
//!   set Bool(TAKEN + name)
//! ```
//!
//! Every built actor with the prefix gets its Bool ([`flags`]), so the set
//! never names a missing entry.

use anyhow::{Context, Result};
use ounce_formats::ainb::{File, Value};
use roead::sarc::Sarc;

use super::shared;
use crate::known::{Name, field};

/// The name every built pickup starts with, and nothing vanilla does.
pub const PREFIX: &str = "Obj_Rando";
/// What a built pickup's Bool is named after it.
const TAKEN: &str = "Rando_Taken_";

/// The Bool the take of built pickup `stand_in` sets.
pub fn flag(stand_in: &str) -> String {
    format!("{TAKEN}{stand_in}")
}

/// The Bool of every built pickup among `written`, a mod's romfs paths.
pub fn flags(written: &[String]) -> Vec<String> {
    (written.iter())
        .filter_map(|w| w.strip_prefix("Pack/Actor/")?.strip_suffix(".pack.zs"))
        .filter(|name| name.starts_with(PREFIX))
        .map(flag)
        .collect()
}

/// The pickup event graph's path in the pack.
pub fn graph() -> String {
    format!("AI/{}.event.root.ainb", field::POUCH_CONTENT.get())
}

/// The pack's pickup event graph with the branch hung last on the sequence
/// that puts the pickup in the pouch.
pub fn ticked(pack: &Sarc) -> Result<Vec<u8>> {
    let vanilla = File::parse(pack.get_data(&graph()).context("no pickup event graph")?)?;
    let push = field::ONE_SHOT_POUCH_CONTENT_PUSH_TO_POUCH_OWN_IN_EVENT.get();
    let push = vanilla.find(push).context("no push into the pouch")?;
    let runs_push = |n: &usize| {
        (vanilla.children(*n)).is_ok_and(|c| c.iter().any(|(_, k)| *k as usize == push))
    };
    let take = (0..vanilla.node_count())
        .find(runs_push)
        .context("no sequence runs the push")?;

    let mut f = vanilla.clone();
    let copy = |f: &mut File, class: &Name| shared::copy(pack, f, class.get());
    let own = copy(&mut f, field::QUERY_GENERIC_GET_OWN_ACTOR_LINK)?;
    let name = copy(&mut f, field::QUERY_GENERIC_GET_ACTOR_NAME)?;
    let starts = copy(&mut f, field::QUERY_GENERIC_STRING_STARTS_WITH)?;
    let concat = copy(&mut f, field::QUERY_GENERIC_CONCAT_STRING)?;
    let set = copy(&mut f, field::ONE_SHOT_SET_NON_STRUCT_GAME_DATA_BOOL)?;
    let select = vanilla.find_bool_selector().context("no bool selector")?;
    let nothing = (vanilla.find(field::ONESHOT_DO_NOTHING.get())).context("no do-nothing leaf")?;
    let ours = f.copy_node(&vanilla, select)?;
    let others = f.copy_node(&vanilla, nothing)?;

    let file_name = field::ACTOR_FILE_NAME.get();
    let full_name = field::GAME_DATA_FULL_NAME.get();
    let text = |s: &str| Value::String(s.into());
    f.link(name, "Actor", own, "Actor")?;
    f.link(starts, "Input", name, file_name)?;
    f.set_input(starts, "Token", &text(PREFIX))?;
    f.set_input(concat, "Input0", &text(TAKEN))?;
    f.link(concat, "Input1", name, file_name)?;
    f.set_input(concat, "Input2", &text(""))?;
    f.set_input(set, full_name, &text(""))?;
    f.link(set, full_name, concat, "Output")?;
    f.set_input(set, field::VALUE.get(), &Value::Bool(true))?;
    f.link(ours, "Input", starts, "Result")?;
    f.set_child(ours, "True", set)?;
    f.set_child(ours, "False", others)?;
    f.add_child(take, "", ours)?;
    f.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::pickups::GLOBAL_AI;
    use ounce_formats::ainb::{Graph, Link, check};

    #[test]
    fn only_a_built_pickup_pack_gets_a_bool() {
        let written = [
            "Pack/Actor/Obj_Rando_007.pack.zs",
            "Pack/Actor/Obj_RandoStep_012_1.pack.zs",
            "Pack/Actor/Obj_FakePot.pack.zs",
            "GameData/Obj_Rando_009.pack.zs",
        ]
        .map(String::from);
        assert_eq!(
            flags(&written),
            [
                "Rando_Taken_Obj_Rando_007",
                "Rando_Taken_Obj_RandoStep_012_1"
            ]
        );
    }

    #[test]
    fn a_built_pickup_sets_its_bool_after_the_push() {
        let Some(romfs) = dev_dump() else {
            return;
        };
        let data = romfs.read(GLOBAL_AI).unwrap();
        let pack = Sarc::new(&data).unwrap();
        let vanilla = File::parse(pack.get_data(&graph()).unwrap()).unwrap();
        let bytes = ticked(&pack).unwrap();
        check(&bytes).unwrap();
        let f = File::parse(&bytes).unwrap();
        let g = Graph::parse(&bytes).unwrap();
        let class = |n: usize| g.nodes[n].name.as_str();
        let from = |n: usize, input: &str| -> Link {
            let s = &g.nodes[n].input(input).unwrap().sources;
            assert_eq!(s.len(), 1, "{input}");
            s[0].clone()
        };
        let child = |n: usize, plug: &str| {
            let c = f.children(n).unwrap();
            c.iter().find(|(p, _)| p == plug).unwrap().1 as usize
        };

        // The take's vanilla children stay first, the branch comes last.
        let push = field::ONE_SHOT_POUCH_CONTENT_PUSH_TO_POUCH_OWN_IN_EVENT.get();
        let take = (0..f.node_count())
            .find(|&n| {
                f.children(n)
                    .unwrap()
                    .iter()
                    .any(|(_, k)| class(*k as usize) == push)
            })
            .unwrap();
        let children = f.children(take).unwrap();
        let (last, kept) = children.split_last().unwrap();
        assert_eq!(kept, vanilla.children(take).unwrap());
        let ours = last.1 as usize;

        let starts = from(ours, "Input").node;
        assert_eq!(class(starts), field::QUERY_GENERIC_STRING_STARTS_WITH.get());
        assert_eq!(g.nodes[starts].str("Token"), Some(PREFIX));
        let name = from(starts, "Input");
        assert_eq!(name.output, field::ACTOR_FILE_NAME.get());
        let own = from(name.node, "Actor").node;
        assert_eq!(class(own), field::QUERY_GENERIC_GET_OWN_ACTOR_LINK.get());

        assert_eq!(class(child(ours, "False")), field::ONESHOT_DO_NOTHING.get());
        let set = child(ours, "True");
        assert_eq!(
            class(set),
            field::ONE_SHOT_SET_NON_STRUCT_GAME_DATA_BOOL.get()
        );
        assert_eq!(
            g.nodes[set].value(field::VALUE.get()),
            Some(&Value::Bool(true))
        );
        let concat = from(set, field::GAME_DATA_FULL_NAME.get()).node;
        assert_eq!(class(concat), field::QUERY_GENERIC_CONCAT_STRING.get());
        assert_eq!(g.nodes[concat].str("Input0"), Some(TAKEN));
        assert_eq!(from(concat, "Input1"), name);
        assert_eq!(g.nodes[concat].str("Input2"), Some(""));
    }
}
