//! Jabu-Jabu opening his mouth as Link swims up, under the Jabu-Jabu open
//! preset. The fountain's group makes the two areas that open and keep open
//! his mouth only once the Water quest is past `Step08`, which the first
//! arrival in his belly moves it to. So without the fish he stayed shut
//! until Link had been inside once.

use anyhow::{Result, ensure};
use ounce_formats::ainb::{File, Graph, Value};
use ounce_formats::{ModBuilder, Romfs};

use crate::graphs::logic;
use crate::known::{field, map};

/// Writes the fountain group with its mouth areas made from the start. The
/// adult copy (Adult Jabu-Jabu) runs the same group, so it opens for both.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let module = logic(map::ZORA_FOUNTAIN_CHILD_PREFAB_41C4);
    builder.write(&module, &opened(&romfs.read(&module)?)?)
}

/// `module` with every area whose making waits on something made always.
/// The group's third area, the fish's drop, is made always already.
fn opened(module: &[u8]) -> Result<Vec<u8>> {
    let g = Graph::parse(module)?;
    let create = field::LOGIC_CREATE.get();
    let waiting: Vec<usize> = (g.nodes.iter().enumerate())
        .filter(|(_, n)| n.name == field::ACTOR_LOGIC_AREA_ACTOR.get())
        .filter(|(_, n)| (n.inputs.iter()).any(|i| i.name == create && !i.sources.is_empty()))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        waiting.len() == 2,
        "Jabu's group has {} waiting areas, not his mouth's two",
        waiting.len()
    );
    let mut f = File::parse(module)?;
    for node in waiting {
        f.unlink(node, create, &Value::Bool(true))?;
    }
    f.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_mouth_areas_stand_from_the_start() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let module = logic(map::ZORA_FOUNTAIN_CHILD_PREFAB_41C4);
        let new = opened(&romfs.read(&module).unwrap()).unwrap();
        let g = Graph::parse(&new).unwrap();
        let create = field::LOGIC_CREATE.get();
        let areas: Vec<_> = (g.nodes.iter())
            .filter(|n| n.name == field::ACTOR_LOGIC_AREA_ACTOR.get())
            .collect();
        assert_eq!(areas.len(), 3);
        for area in areas {
            let input = area.inputs.iter().find(|i| i.name == create).unwrap();
            assert!(input.sources.is_empty(), "an area still waits to be made");
        }
    }
}
