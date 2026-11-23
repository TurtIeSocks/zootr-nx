//! What derive saw crossing from one room to another, and the link a
//! room file writes for it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use crate::nav::limits::limits;
use crate::rules::kinds::Kind;
use crate::rules::rooms;

use super::all_of;
use super::locks::Lock;

/// What a crossing from one room to another was seen as.
#[derive(Default)]
pub(super) struct Seen<'k> {
    pub count: usize,
    /// Drop or step heights, gap widths.
    pub least: f32,
    pub most: f32,
    /// The placements it goes by (a ladder, a door, water).
    pub by: BTreeSet<u64>,
    /// The obstacle kinds standing on it, by placement.
    pub on: BTreeMap<u64, &'k Kind>,
    /// Where it was seen with the least metric: the place to test it.
    pub at: ([f32; 3], [f32; 3]),
    /// The furthest a jump on it falls, in m: 0 for a level gap.
    pub fall: f32,
    /// The confirmed link that stands in for every derived arc here.
    pub puzzle: Option<&'k rooms::SpotLink>,
}

impl Seen<'_> {
    pub fn add(&mut self, metric: f32, by: Option<u64>, at: ([f32; 3], [f32; 3])) {
        if self.count == 0 || metric < self.least {
            self.at = at;
        }
        if self.count == 0 {
            (self.least, self.most) = (metric, metric);
        }
        self.count += 1;
        self.least = self.least.min(metric);
        self.most = self.most.max(metric);
        self.by.extend(by);
    }
}

/// A link's needs, why and whether it's a guess, from what was seen
/// crossing from `from`.
pub(super) fn link(
    via: rooms::Via,
    from: u32,
    s: &Seen,
    locks: &HashMap<u64, Vec<Lock>>,
) -> (String, String, bool) {
    if let Some(p) = s.puzzle {
        return (p.needs.clone(), format!("confirmed: {}", p.why), p.guess);
    }
    let span = |unit: &str| {
        if (s.most - s.least).abs() < 0.05 {
            format!("{:.1} {unit}", s.least)
        } else {
            format!("{:.1} to {:.1} {unit}", s.least, s.most)
        }
    };
    let by = || {
        s.by.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut why = match via {
        rooms::Via::Walk => format!("walk, {} edges", s.count),
        rooms::Via::Drop if s.most > limits().drop() => format!("drop {} into water", span("m")),
        rooms::Via::Drop => format!("drop {}", span("m")),
        rooms::Via::Step => format!("step up {}", span("m")),
        rooms::Via::Jump if s.fall > 0.0 => format!(
            "gap {}, falling up to {:.1} m: within a running jump off the edge ({:.1} m for that fall)",
            span("m"),
            s.fall,
            limits().leap(s.fall)
        ),
        rooms::Via::Jump => format!(
            "level gap {}: within a running jump ({:.1} m)",
            span("m"),
            limits().level_jump()
        ),
        _ if s.by.is_empty() => via.word().to_owned(),
        _ => format!("{} {}", via.word(), by()),
    };
    let mut guess = false;
    let mut needs: Vec<String> = vec![];
    if via == rooms::Via::Crawl {
        needs.push("child".to_owned());
    }
    for (h, k) in &s.on {
        if k.uses != "always" {
            write!(why, "; {} {h}: {}", k.name, k.uses).unwrap();
            guess |= k.guess;
            needs.push(k.uses.clone());
            continue;
        }
        // A shutter may carry one flag in two locks: name it once.
        let mut flags: Vec<(u32, bool)> = (locks.get(h).into_iter().flatten())
            .filter(|l| l.holds(from))
            .map(|l| (l.hash(), l.room.is_none()))
            .collect();
        flags.sort_unstable();
        flags.dedup_by_key(|f| f.0);
        for (flag, roomless) in flags {
            write!(why, "; {} {h} shut until flag #{flag:08x}", k.name).unwrap();
            guess |= roomless;
            needs.push(format!("flag:#{flag:08x}"));
        }
    }
    let needs: Vec<&str> = needs.iter().map(String::as_str).collect();
    (all_of(&needs), why, guess)
}
