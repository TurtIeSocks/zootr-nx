//! The room files' playtest queue: where a hand rule and a room rule
//! disagree, what is still a guess, and the routes that take knowledge.

use std::collections::BTreeMap;
use std::fmt;

use super::Book;
use super::rooms::{Room, folder};

/// One dungeon's lists, by its room folder.
pub struct Audit {
    pub folder: String,
    /// Each room's name, by number.
    pub names: BTreeMap<u32, String>,
    pub disagree: Vec<Disagree>,
    pub guess: Vec<Guess>,
    pub knowledge: Vec<Line>,
}

/// A hand rule beside the room rule that would replace it. A room rule
/// is only part of the need: reaching the room is the rest, so its ways
/// in come along.
pub struct Disagree {
    pub room: u32,
    pub hash: u64,
    pub name: String,
    pub hand: String,
    pub rule: String,
    pub ways_in: Vec<String>,
}

/// An entry nothing measured settles yet.
pub struct Guess {
    pub room: u32,
    pub what: String,
    pub needs: String,
    pub why: String,
}

/// A route between the margin and the limit.
pub struct Line {
    pub room: u32,
    pub what: String,
    pub why: String,
}

/// The audit of every dungeon with room files, or of the one in `only`.
pub fn audit(book: &Book, only: Option<&str>) -> Vec<Audit> {
    let mut out: Vec<Audit> = vec![];
    for room in &book.rooms {
        let at = folder(&room.node);
        if only.is_some_and(|o| o != at) {
            continue;
        }
        let i = match out.iter().position(|a| a.folder == at) {
            Some(i) => i,
            None => {
                out.push(Audit {
                    folder: at.clone(),
                    names: BTreeMap::new(),
                    disagree: vec![],
                    guess: vec![],
                    knowledge: vec![],
                });
                out.len() - 1
            }
        };
        let siblings: Vec<&Room> = (book.rooms.iter())
            .filter(|r| r.node == room.node)
            .collect();
        add(&mut out[i], book, room, &siblings);
    }
    out
}

fn add(audit: &mut Audit, book: &Book, room: &Room, siblings: &[&Room]) {
    let n = room.file.room;
    audit.names.insert(n, room.file.name.clone());
    let ways_in = || {
        (siblings.iter())
            .flat_map(|r| r.file.link.iter().map(move |l| (r.file.room, l)))
            .filter(|(_, l)| l.to == n)
            .map(|(from, l)| format!("{from} {}: {}", l.via.word(), l.needs))
            .collect()
    };
    for c in &room.file.check {
        // A room that isn't live leaves the hand rule in the book.
        let hand = match room.live {
            true => book.hand.get(&c.hash),
            false => book.check(c.hash),
        };
        if let Some(hand) = hand.filter(|h| h.needs != c.expr) {
            audit.disagree.push(Disagree {
                room: n,
                hash: c.hash,
                name: c.name.clone(),
                hand: hand.needs.to_string(),
                rule: c.needs.clone(),
                ways_in: ways_in(),
            });
        }
    }
    let links = (room.file.link.iter()).filter(|l| l.guess).map(|l| Guess {
        room: n,
        what: format!("link to {} ({})", l.to, l.via.word()),
        needs: l.needs.clone(),
        why: l.why.clone(),
    });
    let checks = (room.file.check.iter()).filter(|c| c.guess).map(|c| Guess {
        room: n,
        what: format!("check {} {}", c.name, c.hash),
        needs: c.needs.clone(),
        why: c.why.clone(),
    });
    let flags = (room.file.flag.iter()).filter(|f| f.guess).map(|f| Guess {
        room: n,
        what: format!("flag {} #{:08x}", f.name, f.hash),
        needs: f.needs.clone(),
        why: f.why.clone(),
    });
    audit.guess.extend(links.chain(checks).chain(flags));
    audit
        .knowledge
        .extend(room.file.knowledge.iter().map(|k| Line {
            room: n,
            what: format!("to {} ({})", k.to, k.via.word()),
            why: k.why.clone(),
        }));
}

impl fmt::Display for Audit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let room = |n: &u32| format!("room {n} {}", self.names.get(n).map_or("", String::as_str));
        writeln!(f, "{}", self.folder)?;
        writeln!(f, "  disagree ({})", self.disagree.len())?;
        for d in &self.disagree {
            writeln!(f, "    {}: {} {}", room(&d.room), d.name, d.hash)?;
            writeln!(f, "      hand: {}", d.hand)?;
            writeln!(
                f,
                "      room: {}, ways in: {}",
                d.rule,
                d.ways_in.join("; ")
            )?;
        }
        writeln!(f, "  guess ({})", self.guess.len())?;
        for g in &self.guess {
            writeln!(f, "    {}: {}: {}", room(&g.room), g.what, g.needs)?;
            writeln!(f, "      {}", g.why)?;
        }
        writeln!(f, "  knowledge ({})", self.knowledge.len())?;
        for k in &self.knowledge {
            writeln!(f, "    {}: {}: {}", room(&k.room), k.what, k.why)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deku_tree_audit_lists_its_hand_rules_and_guesses() {
        needs_names!();
        let book = Book::load().unwrap();
        let audits = audit(&book, Some("inside-the-deku-tree"));
        let [deku] = &audits[..] else {
            panic!("one audit for the Deku Tree, got {}", audits.len());
        };
        // The hand rule asks for what burns or blows up the web, the room
        // rule only for reaching the basement: the web is on its ways in.
        let sticks = (deku.disagree.iter())
            .find(|d| d.name == "Lower Three Deku Sticks")
            .expect("the hand rule and the room rule differ");
        assert_eq!((sticks.room, sticks.rule.as_str()), (4, "always"));
        assert_eq!(
            sticks.hand, "explosives or fire",
            "a hand rule prints as written"
        );
        assert!(
            sticks.ways_in.iter().any(|w| w.contains("fire")),
            "{:?}",
            sticks.ways_in
        );
        assert!(deku.guess.iter().any(|g| g.what.contains("jump")));
        assert!(deku.to_string().contains("disagree ("));
    }
}
