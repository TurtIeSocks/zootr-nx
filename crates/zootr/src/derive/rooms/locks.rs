//! The flags a shutter waits on, the rooms that set them, and what
//! setting each one needs.

use std::collections::{BTreeMap, BTreeSet};

use ounce_formats::murmur3;

use super::all_of;
use crate::gates::{Atom, Expr as GateExpr};
use crate::rules::kinds::Kind;

/// A flag a shutter waits on.
#[derive(Clone, Default)]
pub(super) struct Lock {
    /// The game's name: code only, never written (the hook blocks it).
    pub name: String,
    /// The placements that set it: through a graph, or playing an event
    /// their params name.
    pub setters: BTreeSet<u64>,
    /// Each setter in words with its hash, for a setter with no kind.
    pub named: Vec<String>,
    /// Its room: its setters', or the room its name gives.
    pub room: Option<u32>,
}

impl Lock {
    pub fn hash(&self) -> u32 {
        murmur3(self.name.as_bytes())
    }

    /// A flag only clearing its room sets (a fight) holds Link in; a flag
    /// a graph sets (a switch, torches) holds a door shut both ways.
    pub fn holds(&self, from: u32) -> bool {
        !self.setters.is_empty() || self.room == Some(from)
    }
}

/// The room a flag's name gives: `..._RoomB_...` is room 1.
pub(super) fn room_in_name(name: &str) -> Option<u32> {
    let (_, rest) = name.split_once("_Room")?;
    let mut chars = rest.chars();
    let letter = chars.next().filter(char::is_ascii_uppercase)?;
    matches!(chars.next(), Some('_') | None).then(|| letter as u32 - 'A' as u32)
}

/// The flags `expr` waits on, with the placements that set each.
pub(super) fn locks_in(expr: &GateExpr, out: &mut BTreeMap<String, BTreeSet<u64>>) {
    match expr {
        GateExpr::SetFlag(name, drives) => {
            let setters = drives
                .iter()
                .flat_map(GateExpr::atoms)
                .filter_map(|a| match a {
                    Atom::Actor { hash, .. } => *hash,
                    _ => None,
                });
            out.entry(name.clone()).or_default().extend(setters);
        }
        GateExpr::Atom(Atom::Flag(name)) => {
            out.entry(name.clone()).or_default();
        }
        GateExpr::And(v) | GateExpr::Or(v) => v.iter().for_each(|e| locks_in(e, out)),
        GateExpr::Not(e) => locks_in(e, out),
        GateExpr::Const(_) | GateExpr::Atom(_) => {}
    }
}

/// A flag's needs, why and whether it's a guess, from what sets it.
pub(super) fn flag(lock: &Lock, kinds: &[&Kind]) -> (String, String, bool) {
    if lock.setters.is_empty() {
        let why = "set by clearing the room: what clears it isn't traced yet".to_owned();
        return ("always".into(), why, true);
    }
    if kinds.is_empty() {
        let why = format!(
            "an event {} plays sets it: what beats that isn't traced yet",
            lock.named.join(", ")
        );
        return ("always".into(), why, true);
    }
    let uses: Vec<&str> = kinds.iter().map(|k| k.uses.as_str()).collect();
    let mut seen = BTreeSet::new();
    let why = (kinds.iter())
        .filter(|k| seen.insert(k.hash))
        .map(|k| format!("{} sets it ({}): {}", k.name, lock.setters.len(), k.uses))
        .collect::<Vec<_>>()
        .join("; ");
    let guess = kinds.is_empty() || kinds.iter().any(|k| k.guess);
    (all_of(&uses), why, guess)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flag_names_its_room_by_letter() {
        assert_eq!(room_in_name("FakeTemple_RoomB_Cleared"), Some(1));
        assert_eq!(room_in_name("FakeTemple_RoomA"), Some(0));
        assert_eq!(room_in_name("FakeTemple_Roomy_Cleared"), None);
        assert_eq!(room_in_name("FakeTemple_Cleared"), None);
    }
}
