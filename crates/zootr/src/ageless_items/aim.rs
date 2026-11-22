//! Each Link aims the Bow with the Bow's clips and the Slingshot with the
//! Slingshot's. Both graphs pick between the two aims by age, since in the
//! game only the child has the Slingshot and only the adult the Bow: the
//! child drew the Bow with the Slingshot's hands and held its arrow like a
//! seed, and the adult held the Slingshot sideways like a bow. Each of
//! those picks becomes one by the item in his hand: the Slingshot's clips
//! for the Slingshot, the Bow's for anything else.

use anyhow::{Context, Result, ensure};
use ounce_formats::asb::{self, Asb, Kind, Node};

/// The blackboard entries the picks read: by age, then by item.
const AGE: &str = "IsChild";
const ITEM: &str = "UsingItem";
/// `UsingItem` with the Slingshot in hand: the ZR item's type.
const SLINGSHOT: i32 = 4;
/// The word in every clip name on each side of a pick.
const SLING: &str = "Sling";
const BOW: &str = "Bow";

/// Each pick by age in the graph `asb` whose child side plays only the
/// Slingshot's clips and adult side only the Bow's: the node, then each
/// side.
fn picks(asb: &Asb) -> Result<Vec<(usize, u32, u32)>> {
    let age = (asb.entry(Kind::Bool, AGE)?).context("the graph reads no age")?;
    let only = |side: u32, word: &str| -> Result<bool> {
        let clips = asb.clips(side)?;
        Ok(!clips.is_empty() && clips.iter().all(|c| c.contains(word)))
    };
    let mut found = vec![];
    for i in 0..asb.len() {
        if let Node::Bool {
            entry: Some(e),
            children,
        } = asb.node(i)?
            && e == age
            && let [child, adult] = children[..]
            && only(child, SLING)?
            && only(adult, BOW)?
        {
            found.push((i, child, adult));
        }
    }
    Ok(found)
}

/// A Link's graph at `bytes`, each Slingshot or Bow pick made by the item
/// in his hand.
pub fn aimed(bytes: &[u8]) -> Result<Vec<u8>> {
    let asb = Asb::parse(bytes)?;
    let item = (asb.entry(Kind::Int, ITEM)?).context("the graph reads no item")?;
    let picks = picks(&asb)?;
    ensure!(!picks.is_empty(), "the graph picks no aim by age");
    let mut out = bytes.to_vec();
    for (i, child, adult) in picks {
        asb::select_int(&mut out, i, item, &[(SLINGSHOT, child)], adult)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ageless_items::files::GRAPHS;
    use crate::game::dev_dump;

    #[test]
    fn each_link_aims_by_the_item_in_his_hand_and_nothing_else_moves() {
        let Some(romfs) = dev_dump() else { return };
        for graph in GRAPHS {
            aims_by_item(&romfs.read(graph).unwrap());
        }
    }

    fn aims_by_item(vanilla: &[u8]) {
        let was = Asb::parse(vanilla).unwrap();
        let found = picks(&was).unwrap();
        // The lock-on aim's draws, charges and holds.
        assert_eq!(found.len(), 5);
        let edited = aimed(vanilla).unwrap();
        let now = Asb::parse(&edited).unwrap();
        let item = now.entry(Kind::Int, ITEM).unwrap();
        assert_eq!(now.len(), was.len());
        for i in 0..was.len() {
            match found.iter().find(|p| p.0 == i) {
                Some(&(_, child, adult)) => assert_eq!(
                    now.node(i).unwrap(),
                    Node::Int {
                        entry: item,
                        children: vec![(SLINGSHOT, child), (0, adult)]
                    }
                ),
                None => assert_eq!(now.node(i).unwrap(), was.node(i).unwrap(), "node {i}"),
            }
        }
        assert!(picks(&now).unwrap().is_empty(), "a pick by age is left");
    }
}
