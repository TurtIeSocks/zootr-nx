//! What `zootr photo` photographs and what it writes: each check's
//! subject (its age, map, spot and prep) and its line in `photos.jsonl`,
//! as the ledger's `photos/index.json` keeps it.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

use super::nightly;
use crate::ledger::{self, Why};
use crate::nav::kinds;
use crate::sweep::plan::{Planned, Steps, Trigger};
use crate::sweep::spot::presets;
use crate::warp::{self, Side};

/// The run's photo lines, one per check.
pub const LINES: &str = "photos.jsonl";

/// Where a check is photographed from.
#[derive(Clone, Debug, PartialEq)]
pub enum Spot {
    /// Its own placement, by hash.
    Placed(u64),
    /// The giver standing here: a gift's or shop's, as the sweep's planner
    /// found them.
    At([f32; 3]),
}

/// What a check's holder needs to be there, untouched: a `NoBake` chest
/// raised, a keeper out. The sweep's plan of it, without the use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prep {
    /// GameData values held through the load, as `NAME=VALUE`.
    pub ready: Vec<String>,
    /// Quest steps run through the quest system before the teleport.
    pub steps: Steps,
}

impl Prep {
    /// The prep of `p`: its presets, and the values held back for its foes
    /// (no foe is beaten here).
    fn of(p: &Planned) -> Self {
        let mut ready = presets(p);
        let mut steps = vec![];
        if let Trigger::Open {
            latch, steps: s, ..
        } = &p.trigger
        {
            ready.extend(latch.iter().cloned());
            steps.clone_from(s);
        }
        Self { ready, steps }
    }
}

/// One check to photograph.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub hash: u64,
    pub child: Option<bool>,
    /// Its map and spot, or none to stand beside.
    pub at: Option<(String, Spot)>,
    /// What Hob looks for: `chest`, `person`, `heart container`.
    pub what: String,
    pub prep: Prep,
}

/// The verdict of a photo the maintainer took by hand.
pub const BY_HAND: &str = "by hand";

/// One check's photo, as the ledger's `photos/index.json` keeps it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub hash: String,
    pub map: Option<String>,
    pub age: Option<String>,
    pub time: Option<String>,
    /// The side Link stood on when Hob saw the holder.
    pub side: Option<Side>,
    /// How many steps back from that side.
    pub step: u32,
    /// `framed`, `best effort` (no side passed, the first frame kept),
    /// `no spot` or [`BY_HAND`].
    pub verdict: String,
    /// The placement photographed in the check's place: a gift's giver.
    pub giver: Option<String>,
    pub at: String,
    pub build: Option<String>,
    pub seed: Option<String>,
}

impl Line {
    /// The placement this line photographed: its giver, or the check.
    pub(super) fn subject(&self) -> Option<u64> {
        self.giver.as_ref().unwrap_or(&self.hash).parse().ok()
    }
}

impl std::fmt::Display for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let age = match self.child {
            Some(true) => "child",
            Some(false) => "adult",
            None => "either",
        };
        let (map, spot) = match &self.at {
            Some((map, Spot::Placed(h))) if *h == self.hash => (map.as_str(), "itself".into()),
            Some((map, Spot::Placed(h))) => (map.as_str(), format!("placed {h}")),
            Some((map, Spot::At(at))) => (map.as_str(), format!("giver at {at:?}")),
            None => ("-", "no spot".into()),
        };
        write!(f, "{}\t{map}\t{age}\t{spot}", self.hash)
    }
}

/// The checks to photograph, `area`'s alone or `hashes` alone when given,
/// grouped by map so each map loads once.
pub fn subjects(romfs: &Romfs, area: Option<&str>, hashes: &[u64]) -> Result<Vec<Subject>> {
    let (ledger, planned) = ledger::planned(romfs)?;
    let preps: HashMap<u64, Prep> = (planned.iter())
        .map(|p| (p.spot.hash, Prep::of(p)))
        .collect();
    let talks: HashMap<u64, (String, [f32; 3])> = (planned.into_iter())
        .filter_map(|p| match p.trigger {
            Trigger::Talk { map, face, .. } => Some((p.spot.hash, (map, face))),
            _ => None,
        })
        .collect();
    let givers = crate::sweep::plan::givers(romfs)?;
    let rows = (ledger.rows.iter())
        .filter(|r| matches!(r.left_out, None | Some(Why::Eie { .. })))
        .filter(|r| area.is_none_or(|a| r.area == a))
        .filter(|r| hashes.is_empty() || hashes.contains(&r.hash));
    let mut poses = HashMap::new();
    let mut out = Vec::new();
    for r in rows {
        if !poses.contains_key(&r.area) {
            poses.insert(r.area.clone(), warp::poses_in(romfs, &r.area)?);
        }
        let child = first_age(r.logic.iter().map(|n| n.era.as_str()));
        let maps = (poses[&r.area].iter())
            .filter(|(_, placed)| placed.contains_key(&r.hash))
            .map(|(map, _)| map.as_str());
        let placed = map_for(maps, child).map(|m| (m.to_owned(), Spot::Placed(r.hash)));
        let at_giver = |(map, face): &(String, [f32; 3])| (map.clone(), Spot::At(*face));
        let talk = talks.get(&r.hash).map(at_giver);
        let at = match r.kind {
            "gift" => givers.get(&r.hash).map(at_giver).or(talk).or(placed),
            "shop" => talk.or(placed),
            _ => placed.or(talk),
        };
        let child = (at.as_ref().and_then(|(map, _)| warp::era(map))).or(child);
        let what = match &at {
            Some((_, Spot::At(_))) if !r.holder.contains("Cow") => "person".into(),
            _ => what(&r.holder, &r.name),
        };
        out.push(Subject {
            hash: r.hash,
            child,
            at,
            what,
            prep: preps.get(&r.hash).cloned().unwrap_or_default(),
        });
    }
    out.sort_by(|a, b| {
        (&a.at.as_ref().map(|(m, _)| m), a.child).cmp(&(&b.at.as_ref().map(|(m, _)| m), b.child))
    });
    Ok(out)
}

/// The first age logic lets reach a check: child when both can.
fn first_age<'a>(eras: impl Iterator<Item = &'a str>) -> Option<bool> {
    let eras: HashSet<&str> = eras.collect();
    match (eras.contains("child"), eras.contains("adult")) {
        (true, _) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// The map of `maps` to shoot in as `child`: one of that age (child when
/// either), else one with no age, else any.
fn map_for<'a>(maps: impl Iterator<Item = &'a str>, child: Option<bool>) -> Option<&'a str> {
    let maps: Vec<&str> = maps.collect();
    let want = Some(child.unwrap_or(true));
    (maps.iter().find(|m| warp::era(m) == want))
        .or_else(|| maps.iter().find(|m| warp::era(m).is_none()))
        .or_else(|| maps.first())
        .copied()
}

/// What Hob looks for in the frame of `holder`, the check named `name`:
/// its kind, or the item itself when it stands alone (a heart container).
fn what(holder: &str, name: &str) -> String {
    if nightly(holder) {
        "Gold Skulltula".into()
    } else if holder.contains("Cow") {
        "cow".into()
    } else {
        match kinds::word(holder) {
            Some("item") | None => name.to_lowercase(),
            Some(word) => word.into(),
        }
    }
}

/// The lines `dir` already holds, by check: a later line wins.
pub fn done(dir: &Path) -> HashMap<u64, Line> {
    read(&dir.join(LINES))
}

/// The lines of `file`, by check: a later line wins.
fn read(file: &Path) -> HashMap<u64, Line> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    (text.lines())
        .filter_map(|l| serde_json::from_str::<Line>(l).ok())
        .filter_map(|l| Some((l.hash.parse().ok()?, l)))
        .collect()
}

/// The side each holder in `file` was framed from, by check: a sweep
/// tries it first.
pub fn sides(file: &Path) -> HashMap<u64, Side> {
    (read(file).into_iter())
        .filter(|(_, l)| l.verdict == "framed")
        .filter_map(|(hash, l)| Some((hash, l.side?)))
        .collect()
}

/// `line` as a photo shot by hand: [`BY_HAND`], with no side.
pub(super) fn by_hand(line: Line) -> Line {
    Line {
        verdict: BY_HAND.into(),
        side: None,
        step: 0,
        ..line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;

    #[test]
    fn a_hand_shot_says_so_and_has_no_side() {
        let line = Line {
            hash: "1".into(),
            map: Some("FakeHill_Child".into()),
            time: Some("night".into()),
            side: Some(Side::Left),
            step: 1,
            verdict: "best effort".into(),
            ..Line::default()
        };
        let hand = by_hand(line.clone());
        assert_eq!(
            (hand.verdict.as_str(), hand.side, hand.step),
            (BY_HAND, None, 0)
        );
        assert_eq!((hand.map, hand.time), (line.map, line.time));
    }

    #[test]
    fn the_first_age_is_child_when_both_reach_it() {
        assert_eq!(first_age(["adult", "child"].into_iter()), Some(true));
        assert_eq!(first_age(["adult"].into_iter()), Some(false));
        assert_eq!(first_age([].into_iter()), None);
    }

    #[test]
    fn the_map_is_the_ages_then_an_ageless_one() {
        let maps = ["FakeHill_Adult", "FakeHill_Child"];
        assert_eq!(
            map_for(maps.into_iter(), Some(false)),
            Some("FakeHill_Adult")
        );
        assert_eq!(map_for(maps.into_iter(), None), Some("FakeHill_Child"));
        assert_eq!(
            map_for(["FakeTemple"].into_iter(), Some(false)),
            Some("FakeTemple")
        );
        assert_eq!(map_for([].into_iter(), None), None);
    }

    #[test]
    fn a_run_resumes_from_its_lines_and_a_later_line_wins() {
        let dir = std::env::temp_dir().join(format!("zootr-photo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let line = |hash: &str, verdict: &str| Line {
            hash: hash.into(),
            verdict: verdict.into(),
            ..Line::default()
        };
        let text = [
            line("1", "best effort"),
            line("2", "framed"),
            line("1", "framed"),
        ]
        .map(|l| serde_json::to_string(&l).unwrap())
        .join("\n");
        std::fs::write(dir.join(LINES), text + "\nnot json\n").unwrap();
        let done = done(&dir);
        assert_eq!(done.len(), 2);
        assert_eq!(done[&1].verdict, "framed");
        let side = Line {
            hash: "3".into(),
            side: Some(Side::Left),
            verdict: "framed".into(),
            ..Line::default()
        };
        let text = serde_json::to_string(&side).unwrap();
        std::fs::write(dir.join("sides.jsonl"), text).unwrap();
        assert_eq!(sides(&dir.join("sides.jsonl")), [(3, Side::Left)].into());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_line_reads_as_the_ledgers_photo() {
        let line = Line {
            hash: "1".into(),
            side: Some(Side::Left),
            giver: Some("9".into()),
            ..Line::default()
        };
        let json = serde_json::to_value(&line).unwrap();
        assert_eq!(json["side"], "left");
        assert_eq!(line.subject(), Some(9));
    }

    #[test]
    fn hob_looks_for_what_the_holder_is() {
        needs_names!();
        let gold = format!("{}_01", &**actor::ENEMY_SKULLTULA_GOLD);
        assert_eq!(what(&gold, "Gold Skulltula"), "Gold Skulltula");
        assert_eq!(what(actor::OBJ_T_BOX_A_01, "Small Key"), "chest");
        let heart = what(actor::OBJ_HEART_CONTAINER, "Heart container");
        assert_eq!(heart, "heart container");
    }

    #[test]
    fn a_shop_and_a_gift_are_shot_at_their_giver() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = subjects(&romfs, None, &[]).unwrap();
        let (ledger, _) = ledger::planned(&romfs).unwrap();
        let of_kind = |kind: &str| {
            let hashes: HashSet<u64> = (ledger.rows.iter())
                .filter(|r| r.kind == kind && r.left_out.is_none())
                .map(|r| r.hash)
                .collect();
            all.iter().filter(move |s| hashes.contains(&s.hash))
        };
        for kind in ["shop", "gift"] {
            let at = of_kind(kind)
                .filter(|s| matches!(s.at, Some((_, Spot::At(_)))))
                .count();
            assert!(at > 0, "no {kind} is shot at its giver");
        }
        let chests =
            of_kind("chest").filter(|s| matches!(s.at, Some((_, Spot::Placed(h))) if h == s.hash));
        assert!(chests.count() > 100);
    }
}
