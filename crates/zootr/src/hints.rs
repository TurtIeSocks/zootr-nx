//! Gossip stone hints: each stone names where an item Link needs is. See
//! `docs/superpowers/specs/2026-11-12-hints-design.md`.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, ensure};
use ounce_formats::{EventKind, Link, Msbt, Part, Romfs, messages, replace_file};
use roead::sarc::Sarc;

use crate::flows::{self, Rewrites};
use crate::generate::Spot;
use crate::known::{Name, actor, flow};
use crate::names::{AREAS, ITEMS, pretty};
use crate::rng::Rng;
use crate::text;

/// The stones' message file, without `.msbt`.
pub const FILE: &str = "EventFlowMsg/GossipStone";
const FLOW: &Name = flow::GOSSIP_STONE;
/// Every stone's line is labelled with its instance name, `GOSSIP_<AREA>_NN`.
const STONE: &str = "GOSSIP_";
/// The two lines a stone may add after its own, also `GOSSIP_` labels.
const RUMOR: &str = "GOSSIP_RumorCommon_";
pub const STONES: usize = 40;
/// The stone's own line, read by the placed actor's instance name.
const READ: (&Name, &Name) = (
    actor::NPC_EVENT_STARTER,
    flow::EVENT_TALK_WITH_INSTANCE_NAME_LABEL,
);
/// The call that turns Link to the stone before the reading.
const INIT: (&Name, &Name) = (flow::INIT_TALK, flow::INIT_TALK_GOSSIP_STONE);

/// One hint: an item and the Banc folder it's in.
#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    pub area: String,
    pub item: String,
}

/// Up to `STONES` hints, each a different spot holding an item in
/// `wanted`, picked by `seed`. Stones repeat them when there are fewer.
pub fn choose(spots: &[Spot], wanted: &BTreeSet<&str>, seed: u64) -> Vec<Hint> {
    let mut found: Vec<&Spot> = spots
        .iter()
        .filter(|s| wanted.contains(s.item.as_str()))
        .collect();
    found.sort_by_key(|s| s.hash);
    Rng::new(seed ^ u64::from_le_bytes(*b"gossip!!")).shuffle(&mut found);
    found.truncate(STONES);
    let hint = |s: &&Spot| Hint {
        area: s.area.clone(),
        item: s.item.clone(),
    };
    found.iter().map(hint).collect()
}

/// Reading a stone shows its line and nothing else: no Mask of Truth, no
/// first-read talk, no closing rumor.
pub fn quiet(romfs: &Romfs, rewrites: &mut Rewrites) -> Result<()> {
    flows::edit(romfs, rewrites, FLOW, |flow| {
        let init = (0u16..)
            .zip(&flow.events)
            .find(|(_, e)| {
                matches!(&e.kind, EventKind::SubFlow { flowchart, entry_point, .. }
                    if flowchart == INIT.0 && entry_point == INIT.1)
            })
            .map(|(i, _)| i)
            .with_context(|| format!("no call to {}::{}", INIT.0, INIT.1))?;
        let read = flows::only(flow, READ)?;
        let talk = flow.entry_points.iter().position(|ep| ep.name == "Talk");
        flow.link(Link::Entry(talk.context("no Talk entry")?), Some(init))?;
        flow.link(Link::Next(init), Some(read))?;
        flow.link(Link::Next(read), None)
    })
}

/// The archive `data` (at `rel`) with every stone's line naming one of
/// `hints`. `labels` maps each item to its name's label.
pub(crate) fn language(
    data: &[u8],
    rel: &str,
    hints: &[Hint],
    labels: &HashMap<&str, String>,
) -> Result<Vec<u8>> {
    let sarc = Sarc::new(data).with_context(|| format!("reading {rel}"))?;
    let get = |path: &str| {
        sarc.get_data(path)
            .with_context(|| format!("{rel} has no {path}"))
    };
    let (pouch, areas) = (messages(get(ITEMS)?)?, messages(get(AREAS)?)?);
    let path = format!("{FILE}.msbt");
    let file = get(&path)?;
    let lines = messages(file)?;
    let stones = stones(&lines);
    ensure!(stones.len() == STONES, "{rel} has {} stones", stones.len());
    let mut msbt = Msbt::parse(file)?;
    let named = |names: &HashMap<String, String>, label: &str| {
        names.get(label).filter(|n| !n.is_empty()).cloned()
    };
    for (stone, hint) in stones.iter().zip(hints.iter().cycle()) {
        let area = named(&areas, &hint.area).unwrap_or_else(|| pretty(&hint.area));
        let item = labels
            .get(hint.item.as_str())
            .and_then(|l| named(&pouch, l));
        let item = item.unwrap_or_else(|| pretty(&hint.item));
        msbt.set(stone, &[Part::Text(line(rel, &area, &item))])?;
    }
    replace_file(data, &path, &msbt.to_binary())
}

/// A stone's line: a sentence in English, the two names alone otherwise.
fn line(rel: &str, area: &str, item: &str) -> String {
    match text::is_english(rel) {
        true => format!("They say that {area}\nhides {item}."),
        false => format!("{area}:\n{item}"),
    }
}

/// The stones' labels, sorted.
fn stones(lines: &HashMap<String, String>) -> Vec<&str> {
    let mut out: Vec<&str> = lines
        .keys()
        .map(String::as_str)
        .filter(|l| l.starts_with(STONE) && !l.starts_with(RUMOR))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{flag, item};

    #[test]
    fn a_stone_reads_its_line_and_nothing_else() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        quiet(&romfs, &mut rewrites).unwrap();
        let flow = ounce_formats::Flowchart::parse(&rewrites[&flows::rel(FLOW)])
            .unwrap()
            .unwrap();
        let talk = flow.entry_points.iter().find(|ep| ep.name == "Talk");
        let init = talk.unwrap().main.unwrap();
        let next = |e: u16| match &flow.events[e as usize].kind {
            EventKind::SubFlow { next, .. } | EventKind::Action { next, .. } => *next,
            kind => panic!("{kind:?} has no next"),
        };
        let read = next(init).unwrap();
        assert_eq!(read, flows::only(&flow, READ).unwrap());
        assert_eq!(next(read), None);
    }

    #[test]
    fn every_stone_names_a_hint_in_every_language() {
        let Some(romfs) = dev_dump() else { return };
        let hints = [Hint {
            area: flag::KOKIRI_FOREST.to_string(),
            item: item::HAMMER.to_string(),
        }];
        let item = crate::names::name_label(&romfs, item::HAMMER).unwrap();
        let labels = HashMap::from([(item::HAMMER.get(), item.unwrap())]);
        let rels = text::archives(&romfs).unwrap();
        assert_eq!(rels.len(), 16);
        for rel in rels {
            let data = language(&romfs.read(&rel).unwrap(), &rel, &hints, &labels).unwrap();
            let sarc = Sarc::new(&data).unwrap();
            let lines = messages(sarc.get_data(&format!("{FILE}.msbt")).unwrap()).unwrap();
            let read: Vec<&String> = stones(&lines).iter().map(|s| &lines[*s]).collect();
            assert!(read.iter().all(|l| *l == read[0]), "{rel}");
            if rel.contains("USen") {
                assert_eq!(
                    read[0],
                    "They say that Kokiri Forest\nhides Megaton Hammer."
                );
            }
        }
    }

    #[test]
    fn hints_name_wanted_items_only_and_follow_the_seed() {
        let spot = |hash: u64, item: &str| Spot {
            hash,
            area: format!("Area{hash}"),
            item: item.into(),
            ..Default::default()
        };
        let spots: Vec<Spot> = (0..60)
            .map(|h| {
                spot(
                    h,
                    if h % 2 == 0 {
                        item::HOOKSHOT
                    } else {
                        "Item_Rupee"
                    },
                )
            })
            .collect();
        let wanted = BTreeSet::from([item::HOOKSHOT.get()]);
        let hints = choose(&spots, &wanted, 7);
        assert_eq!(hints.len(), 30);
        assert!(hints.iter().all(|h| h.item == item::HOOKSHOT));
        assert_eq!(hints, choose(&spots, &wanted, 7));
        assert_ne!(hints, choose(&spots, &wanted, 8));
    }
}
