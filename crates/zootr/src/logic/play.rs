//! Playing the world: what Link reaches with what he has, round by round.

use std::collections::{BTreeSet, HashMap};

use super::*;

impl World {
    /// Grow `seen` to every place reachable with `have`.
    fn grow(&self, seen: &mut [bool], have: &Have) {
        let mut grew = true;
        while grew {
            grew = false;
            let mut era_seen = [false; 2];
            for (i, s) in seen.iter().enumerate() {
                if *s {
                    era_seen[self.places[i].1 as usize] = true;
                }
            }
            for e in &self.edges {
                if seen[e.to] {
                    continue;
                }
                let (from, era) = match e.from {
                    Some(f) => (seen[f], self.places[f].1),
                    None => {
                        let era = self.places[e.to].1;
                        (era_seen[era as usize], era)
                    }
                };
                if from && e.need.met(have, era) {
                    seen[e.to] = true;
                    grew = true;
                }
            }
        }
    }

    /// Collect everything reachable from `START`, starting with `start`,
    /// with `items` at the checks that hold them, round by round.
    pub fn play(&self, start: &Have, items: &HashMap<u64, &str>) -> Play {
        let mut have = start.clone();
        let mut seen = vec![false; self.places.len()];
        seen[self.start()] = true;
        let (mut reached, mut sourced) = (HashSet::new(), vec![false; self.sources.len()]);
        let mut rounds = Vec::new();
        loop {
            self.grow(&mut seen, &have);
            let mut round = Vec::new();
            for l in &self.locations {
                let era = self.places[l.place].1;
                if seen[l.place] && !reached.contains(&l.hash) && l.need.met(&have, era) {
                    reached.insert(l.hash);
                    round.push(Got::Check(l.hash));
                }
            }
            for (i, s) in self.sources.iter().enumerate() {
                let here = |&p: &usize| seen[p] && s.need.met(&have, self.places[p].1);
                if !sourced[i] && s.places.iter().any(here) {
                    sourced[i] = true;
                    round.push(Got::Source(i));
                }
            }
            if round.is_empty() {
                break;
            }
            for got in &round {
                match got {
                    Got::Check(h) => items.get(h).into_iter().for_each(|i| have.add(i)),
                    Got::Source(i) => {
                        let s = &self.sources[*i];
                        match s.gives {
                            Gives::Item => have.add(&s.item),
                            Gives::Flag => drop(have.flags.insert(s.item.clone())),
                        }
                    }
                }
            }
            rounds.push(round);
        }
        Play {
            have,
            rounds,
            reached,
        }
    }

    /// Which places are reachable with `have`, without collecting anything.
    pub(crate) fn seen(&self, have: &Have) -> Vec<bool> {
        let mut seen = vec![false; self.places.len()];
        seen[self.start()] = true;
        self.grow(&mut seen, have);
        seen
    }

    /// Every place reachable with `have`, without collecting anything.
    pub fn reach(&self, have: &Have) -> BTreeSet<&Place> {
        let reached = self.places.iter().zip(self.seen(have)).filter(|(_, s)| *s);
        reached.map(|(p, _)| p).collect()
    }

    /// Whether only a child (`Some(true)`) or only an adult can get the
    /// check at `hash`: the eras of its copies whose rule anything could
    /// meet there. `None` when both can, or logic can't tell.
    pub fn only_child(&self, hash: u64) -> Option<bool> {
        let eras = self.eras(hash, Need::possible)?;
        match eras.len() {
            1 => eras.first().map(|e| *e == Era::Child),
            _ => None,
        }
    }

    /// Whether logic has the check at `hash` but no item or flag gets
    /// Link to it at either age, like a child's copy that needs the Iron
    /// Boots.
    pub fn never(&self, hash: u64) -> bool {
        self.eras(hash, Need::ever)
            .is_some_and(|eras| eras.is_empty())
    }

    /// The eras whose copy of the check at `hash` has a rule `could`
    /// meet. `None` when logic has no copy of it.
    fn eras(&self, hash: u64, could: fn(&Need, Era) -> bool) -> Option<BTreeSet<Era>> {
        let mut copies = (self.locations.iter())
            .filter(|l| l.hash == hash)
            .map(|l| (l, self.places[l.place].1))
            .peekable();
        copies.peek()?;
        Some(
            copies
                .filter(|(l, era)| could(&l.need, *era))
                .map(|(_, era)| era)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::map;

    #[test]
    fn a_room_flag_is_held_once_its_room_is_reached() {
        let place: Place = (map::INSIDE_THE_DEKU_TREE.get().to_owned(), Era::Child);
        let flag = "#1a2b3c4d";
        let world = World {
            places: vec![place.clone()],
            edges: vec![],
            locations: vec![Location {
                hash: 1,
                place: 0,
                need: Need::Flag(flag.into()),
            }],
            sources: vec![Source {
                name: "Fake Room Unlocked".into(),
                places: vec![0],
                item: flag.into(),
                gives: Gives::Flag,
                need: Need::Always,
            }],
            obstacles: BTreeMap::new(),
            goal: vec![],
            start: place,
        };
        let play = world.play(&Have::default(), &HashMap::new());
        assert!(play.reached.contains(&1));
        assert!(play.have.flags.contains(flag) && play.have.items.is_empty());
        assert_eq!(play.rounds.len(), 2, "the flag first, then the check");
    }

    #[test]
    fn a_check_only_one_age_can_get_names_that_age() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let new_game = crate::settings::NewGame::default();
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        // The Water Temple's fourth small key chest, underwater: Iron Boots.
        assert_eq!(world.only_child(3693930347660122883), Some(false));
        assert_eq!(world.only_child(1), None, "no such check");
    }

    #[test]
    fn a_check_no_age_can_get_is_never() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let book = crate::rules::Book::load().unwrap();
        let new_game = crate::settings::NewGame {
            crates: true,
            ..Default::default()
        };
        let checks = checks::dealt(&romfs, &new_game).unwrap();
        let world = World::build(&romfs, &book, &Ctx::new(&new_game), &checks).unwrap();
        // Hyrule Field's grotto pond crates: the child's copy needs the
        // Iron Boots, the adult's gets them.
        assert!(world.never(13505816572168486267));
        assert!(!world.never(13942975907881711341));
        assert!(!world.never(1), "no such check");
    }
}
