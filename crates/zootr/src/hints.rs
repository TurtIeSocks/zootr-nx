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

/// One hint: an item and where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    pub place: Place,
    pub item: String,
}

/// Where a hinted item is.
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// A Banc folder, named in each language.
    Area(String),
    /// Words written as they are: another Archipelago slot's check.
    Words(String),
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
        place: Place::Area(s.area.clone()),
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
        let area = match &hint.place {
            Place::Area(a) => named(&areas, a).unwrap_or_else(|| pretty(a)),
            Place::Words(words) => words.clone(),
        };
        let item = labels
            .get(hint.item.as_str())
            .and_then(|l| named(&pouch, l));
        let item = item.unwrap_or_else(|| pretty(&hint.item));
        let parts = paged(&msbt, &line(rel, &area, &item));
        msbt.set(stone, &parts)?;
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

/// The most characters a stone's box holds on a line. Hints are mostly
/// Title Case names, wider than vanilla's prose: 64 of them ran past
/// the bubble live, and 50 keeps inside it (`research/hints.md`).
const WIDEST: usize = 50;
/// The lines a stone's box shows at once.
const PER_PAGE: usize = 2;
/// The tag vanilla stones turn the page with (a Lost Woods stone's three
/// pages, `research/hints.md`).
const PAGE: (u16, u16) = (0, 4);

/// `text`'s lines wrapped to [`WIDEST`], [`PER_PAGE`] to a page, a page
/// break between pages. A line that fits keeps its one page.
fn paged(msbt: &Msbt, text: &str) -> Vec<Part> {
    let lines: Vec<String> = text.lines().flat_map(wrapped).collect();
    let pages = lines.chunks(PER_PAGE).map(|p| Part::Text(p.join("\n")));
    let page = msbt.tag(PAGE.0, PAGE.1);
    let mut parts = Vec::new();
    for p in pages {
        if !parts.is_empty() {
            parts.push(page.clone());
        }
        parts.push(p);
    }
    parts
}

/// `line` broken at spaces into lines of at most [`WIDEST`] characters. A
/// word longer than that keeps a line of its own.
fn wrapped(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in line.split(' ') {
        match out.last_mut() {
            Some(last) if last.chars().count() + 1 + word.chars().count() <= WIDEST => {
                last.push(' ');
                last.push_str(word);
            }
            _ => out.push(word.to_owned()),
        }
    }
    out
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
        let hint = |place| Hint {
            place,
            item: item::HAMMER.to_string(),
        };
        let hints = [
            hint(Place::Area(flag::KOKIRI_FOREST.to_string())),
            hint(Place::Words("Fake Pot 3 (Fake Player)".into())),
        ];
        let item = crate::names::name_label(&romfs, item::HAMMER).unwrap();
        let labels = HashMap::from([(item::HAMMER.get(), item.unwrap())]);
        let rels = text::archives(&romfs).unwrap();
        assert_eq!(rels.len(), 16);
        for rel in rels {
            let data = language(&romfs.read(&rel).unwrap(), &rel, &hints, &labels).unwrap();
            let sarc = Sarc::new(&data).unwrap();
            let lines = messages(sarc.get_data(&format!("{FILE}.msbt")).unwrap()).unwrap();
            let read: Vec<&String> = stones(&lines).iter().map(|s| &lines[*s]).collect();
            // The stones take the hints in turn.
            assert!(read.iter().step_by(2).all(|l| *l == read[0]), "{rel}");
            assert!(
                read.iter().skip(1).step_by(2).all(|l| *l == read[1]),
                "{rel}"
            );
            assert!(
                read[1].starts_with("Fake Pot 3 (Fake Player)") || text::is_english(&rel),
                "{rel}"
            );
            if rel.contains("USen") {
                assert_eq!(
                    read[0],
                    "They say that Kokiri Forest\nhides Megaton Hammer."
                );
                assert_eq!(
                    read[1],
                    "They say that Fake Pot 3 (Fake Player)\nhides Megaton Hammer."
                );
            }
        }
    }

    #[test]
    fn a_long_line_wraps_at_spaces() {
        let long = format!("They say that {} (FakePlayer)", "Fake Location ".repeat(6));
        let lines = wrapped(&long);
        assert!(
            lines.iter().all(|l| l.chars().count() <= WIDEST),
            "{lines:?}"
        );
        assert_eq!(lines.join(" "), long);
        assert_eq!(wrapped("hides Megaton Hammer."), ["hides Megaton Hammer."]);
    }

    #[test]
    fn a_stone_too_long_for_its_box_turns_the_page() {
        let Some(romfs) = dev_dump() else { return };
        let place = format!("{}(FakePlayer)", "Fake Location ".repeat(4));
        let hints = [Hint {
            place: Place::Words(place.clone()),
            item: item::HAMMER.to_string(),
        }];
        let item = crate::names::name_label(&romfs, item::HAMMER).unwrap();
        let labels = HashMap::from([(item::HAMMER.get(), item.unwrap())]);
        let rel = (text::archives(&romfs).unwrap().into_iter())
            .find(|r| r.contains("USen"))
            .unwrap();
        let data = language(&romfs.read(&rel).unwrap(), &rel, &hints, &labels).unwrap();
        let sarc = Sarc::new(&data).unwrap();
        let msbt = Msbt::parse(sarc.get_data(&format!("{FILE}.msbt")).unwrap()).unwrap();
        let lines = messages(sarc.get_data(&format!("{FILE}.msbt")).unwrap()).unwrap();
        let parts = msbt.get(stones(&lines)[0]).unwrap().unwrap();
        let first = wrapped(&format!("They say that {place}"));
        assert_eq!(first.len(), PER_PAGE, "the place fills the first page");
        let first = first.join("\n");
        let hides = Part::Text("hides Megaton Hammer.".into());
        assert_eq!(parts, [Part::Text(first), msbt.tag(PAGE.0, PAGE.1), hides]);
    }

    #[test]
    fn hints_name_wanted_items_only_and_follow_the_seed() {
        needs_names!();
        let spot = |hash: u64, item: &str| Spot {
            hash,
            area: format!("Area{hash}"),
            item: item.into(),
            ..Default::default()
        };
        let spots: Vec<Spot> = (0..60)
            .map(|h| spot(h, if h % 2 == 0 { item::HOOKSHOT } else { "Item_A" }))
            .collect();
        let wanted = BTreeSet::from([item::HOOKSHOT.get()]);
        let hints = choose(&spots, &wanted, 7);
        assert_eq!(hints.len(), 30);
        assert!(hints.iter().all(|h| h.item == item::HOOKSHOT));
        assert_eq!(hints, choose(&spots, &wanted, 7));
        assert_ne!(hints, choose(&spots, &wanted, 8));
    }
}
