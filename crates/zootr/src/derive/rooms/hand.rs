//! Checks taken by hand (room reach spec, rule 14): from the room's floor
//! within the holder's measured reach, in any part, paying what stands on
//! the way from the room's main part to that floor.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::checks::{ItemCheck, is_big};
use crate::nav::graph::Graph;
use crate::nav::limits::{Holder, limits};
use crate::rules::kinds::Kind;

/// The kind of holder `check` is, for its hand reach: `None` for one no
/// reach is kept for (a scrub, a person, an item on the floor).
pub(super) fn holder(check: &ItemCheck) -> Option<Holder> {
    if is_big(&check.holder) {
        Some(Holder::BigChest)
    } else if check.holder.contains("TBox") {
        Some(Holder::SmallChest)
    } else if crate::pickups::POTS.holds(check) {
        Some(Holder::Pot)
    } else {
        None
    }
}

/// An arc from one part of a room to another: the obstacles on it, and
/// whether it's a crawl (a child's only).
pub(super) struct Crossing<'k> {
    pub from: usize,
    pub to: usize,
    pub on: Vec<(u64, &'k Kind)>,
    pub crawl: bool,
}

impl Crossing<'_> {
    /// What it costs: each obstacle, and a crawl as one more.
    fn cost(&self) -> usize {
        self.on.len() + usize::from(self.crawl)
    }
}

/// The cheapest way by `crossings` from a part `main` holds to part `to`,
/// as the crossings taken in order: none when `to` is main, `None` when
/// no way gets there.
pub(super) fn way<'a, 'k>(
    crossings: &'a [Crossing<'k>],
    main: impl Fn(usize) -> bool,
    to: usize,
) -> Option<Vec<&'a Crossing<'k>>> {
    if main(to) {
        return Some(vec![]);
    }
    let mut best: HashMap<usize, (usize, Option<usize>)> = HashMap::new();
    let mut heap = BinaryHeap::new();
    for c in crossings.iter().filter(|c| main(c.from)) {
        best.entry(c.from).or_insert((0, None));
        heap.push(Reverse((0, c.from)));
    }
    while let Some(Reverse((cost, part))) = heap.pop() {
        if best.get(&part).is_some_and(|(b, _)| *b < cost) {
            continue;
        }
        if part == to {
            break;
        }
        for (i, c) in crossings.iter().enumerate().filter(|(_, c)| c.from == part) {
            let next = cost + c.cost();
            if best.get(&c.to).is_none_or(|(b, _)| next < *b) && !main(c.to) {
                best.insert(c.to, (next, Some(i)));
                heap.push(Reverse((next, c.to)));
            }
        }
    }
    let mut taken = vec![];
    let mut part = to;
    while let (_, Some(i)) = *best.get(&part)? {
        taken.push(&crossings[i]);
        part = crossings[i].from;
    }
    taken.reverse();
    Some(taken)
}

/// "NAME HASH in front: USES" for each of `blocking`.
pub(super) fn in_front(blocking: &[(u64, &Kind)]) -> String {
    (blocking.iter())
        .map(|(h, k)| format!("{} {h} in front: {}", k.name, k.uses))
        .collect::<Vec<_>>()
        .join("; ")
}

/// What a map's checks are taken from: its graph, each face's room and
/// part, and the arcs between parts of a room.
pub(super) struct TakenBy<'a, 'g, 'k> {
    pub graph: &'a Graph<'g>,
    pub room: &'a [Option<u32>],
    pub part: &'a [usize],
    pub crossings: &'a [Crossing<'k>],
}

impl<'k> TakenBy<'_, '_, 'k> {
    /// A check of holder `kind` in room `r` at `at`, taken by hand (rule
    /// 14): from its room's floor within the holder's reach, the main
    /// part's first. Its obstacles (those in front of it, and on the way
    /// from the main part to that floor), why, and whether it's a guess.
    pub fn hand(
        &self,
        kind: Holder,
        r: u32,
        at: [f32; 3],
        main: &dyn Fn(usize) -> bool,
        on: &dyn Fn([f32; 3], [f32; 3]) -> Vec<(u64, &'k Kind)>,
    ) -> Taken<'k> {
        let word = kind.word();
        // Derive measures the child (Floor::of).
        let Some(reach) = limits().hand(true, kind) else {
            return Taken::guess(format!("no hand reach measured for a {word}"));
        };
        let graph = self.graph;
        let own = |f: usize| self.room[f] == Some(r) && !graph.deep(graph.centers[f]);
        let flat = |p: [f32; 3]| (p[0] - at[0]).hypot(p[2] - at[2]);
        let within = |keep: &dyn Fn(usize) -> bool| {
            graph
                .nearest_floor(at, reach, keep)
                .filter(|n| flat(n.at) <= reach)
        };
        let in_main = within(&|f| own(f) && main(self.part[f]));
        let Some(n) = in_main.or_else(|| within(&own)) else {
            return Taken::guess(format!(
                "no floor of its room within a {word}'s reach ({reach} m)"
            ));
        };
        let mut blocking = on(at, n.at);
        let off = flat(n.at);
        let mut why =
            format!("{off:.1} m from its room's floor, within a {word}'s reach ({reach} m)");
        let Some(way) = way(self.crossings, main, self.part[n.face]) else {
            why.push_str(", but no way there from where Link enters the room");
            return Taken {
                on: blocking,
                child: false,
                why,
                guess: true,
            };
        };
        let mut crawl = false;
        for c in way {
            blocking.extend(c.on.iter().filter(|(_, k)| k.uses != "always").cloned());
            crawl |= c.crawl;
        }
        blocking.sort_by_key(|(h, _)| *h);
        blocking.dedup_by_key(|(h, _)| *h);
        if !blocking.is_empty() {
            why = format!("{why}; {}", in_front(&blocking));
        }
        if crawl {
            why.push_str("; through a crawl: child");
        }
        Taken {
            guess: blocking.iter().any(|(_, k)| k.guess),
            on: blocking,
            child: crawl,
            why,
        }
    }
}

/// How a check is taken: the obstacles on the way, whether only a child
/// gets there (a crawl), why, and whether it's a guess.
pub(super) struct Taken<'k> {
    pub on: Vec<(u64, &'k Kind)>,
    pub child: bool,
    pub why: String,
    pub guess: bool,
}

impl Taken<'_> {
    fn guess(why: String) -> Self {
        Taken {
            on: vec![],
            child: false,
            why,
            guess: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web() -> Kind {
        let kind =
            "hash = 1\nname = \"Fake Web\"\nuses = \"fire\"\nsets = \"opens\"\nwhy = \"fake\"";
        toml::from_str(kind).unwrap()
    }

    #[test]
    fn the_way_to_a_part_takes_the_fewest_obstacles() {
        let web = web();
        let cross = |from, to, n: usize| Crossing {
            from,
            to,
            on: (0..n).map(|i| (i as u64, &web)).collect(),
            crawl: false,
        };
        // Main is part 1. Part 3 is behind one web by way of part 2, or two
        // webs straight.
        let crossings = [cross(1, 3, 2), cross(1, 2, 0), cross(2, 3, 1)];
        let main = |p| p == 1;
        let way = way(&crossings, main, 3).unwrap();
        assert_eq!(
            way.iter().map(|c| (c.from, c.to)).collect::<Vec<_>>(),
            [(1, 2), (2, 3)]
        );
        assert!(super::way(&crossings, main, 1).unwrap().is_empty());
        assert!(super::way(&crossings, main, 9).is_none());
    }
}
