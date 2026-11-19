//! A pickup dealt a line's level or ammo shows what Link would get
//! ([`crate::shows`]), as a shop slot does: a placement per step on the
//! holder's spot, each with its own stand-in, standing while that step is
//! the one the holder gives and its drop isn't picked up yet. The holder is
//! step 0, each step past it a copy. The gates are lazy, so a pot standing
//! in a loaded map swaps once Link levels up. A drop already lying removes
//! itself ([`super::stale`]).
//!
//! A logic module that makes or watches a holder (`ItemCheck::watched`)
//! gets its nodes copied for each step ([`super::groups`],
//! [`super::nodes`]). A thief's call names each step's key in turn
//! (`fortress::give`).

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{Byml, ModBuilder, Romfs};

use super::{moved, stale};
use crate::checks::{Drop, ItemCheck};
use crate::gamedata::{GameData, Op};
use crate::picked::{self, After, Holder};
use crate::shows::{self, and, by_name, copy_hash, not};

/// Which step of its holder's a placement is.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// The holder's placement.
    pub of: u64,
    pub k: usize,
    /// True while it's the one the holder gives.
    program: Vec<Op>,
}

/// Each step of a holder placed as `of` dealt `item`: its placement, what
/// it shows, and which step it is. `None` when it always gives `item`.
pub(super) fn of(of: u64, item: &str, progressive: bool) -> Option<Vec<(u64, &'static str, Step)>> {
    let steps = shows::steps(item, progressive)?.into_iter().enumerate();
    let step = |(k, (shows, program))| (copy_hash(of, k), shows, Step { of, k, program });
    Some(steps.map(step).collect())
}

/// The BoolExp a step's placement stands by, true while it's the step its
/// holder gives and isn't picked.
pub(crate) fn gate(placement: u64) -> String {
    format!("Rando_Step_{placement}")
}

/// Whether a pickup dealt `item` might show it a step at a time: ammo, or
/// a line's level once progressive items are on. A lying pickup dealt one
/// stands until picked whatever the setting (`picked::After::Taken`), so
/// the tracker can tell by the spot alone.
pub(crate) fn asks(item: &str) -> bool {
    crate::gifts::levels::asks(item, true)
}

/// Every stepped holder's placements: each copy, and the BoolExp each
/// stands by.
pub struct Steps {
    /// Each step's placement past a holder's own, by the placement it
    /// copies.
    pub copies: Vec<(u64, u64)>,
    /// Each step's placement's BoolExp, by its hash.
    gates: HashMap<u64, String>,
    /// Each step past a thief's key, by its placement's hash: its own
    /// instance name, so her call can name it apart from the key's.
    named: HashMap<u64, usize>,
    /// Each placement's BoolExp, then each step stand-in's own.
    pub exps: Vec<(String, Vec<Op>)>,
    /// Whether a step stand-in is built, so the drop graph needs its branch.
    built: bool,
}

impl Steps {
    pub fn new(
        checks: &[ItemCheck],
        drops: &HashMap<u64, Drop>,
        progressive: bool,
        holders: &[Holder],
        gd: &GameData,
    ) -> Result<Self> {
        let mut steps = Self {
            copies: Vec::new(),
            gates: HashMap::new(),
            named: HashMap::new(),
            exps: Vec::new(),
            built: false,
        };
        for m in moved(checks, drops, progressive) {
            let Some(step) = &m.step else {
                continue;
            };
            let holder = holders.iter().find(|h| h.hash == step.of);
            let holder = holder.with_context(|| format!("holder {} isn't picked", step.of))?;
            // A Gold Skulltula on bean soil, or a heart piece that shows
            // once something happens, keeps its own gate.
            let own = match &holder.after {
                After::Killed(Some(p)) | After::Taken(Some(p)) => Some(p),
                _ => None,
            };
            let unpicked = not(by_name(&picked::flag(step.of), Op::Bool));
            let program = shows::within(own, and(step.program.clone(), unpicked), gd)?;
            let gate = gate(m.hash);
            steps.exps.push((gate.clone(), program));
            // A thief's key keeps her vanilla gate, and her call picks the
            // step by the BoolExp instead (`fortress::give`). Her call finds
            // the key by its instance name, so each step goes by its own:
            // sharing one, her warp moved a step that wasn't the one shown
            // (Mac, 2026-11-19).
            let named = checks
                .iter()
                .any(|c| c.hash == step.of && super::is_named(c));
            if !named {
                steps.gates.insert(m.hash, gate);
            } else if step.k > 0 {
                steps.named.insert(m.hash, step.k);
            }
            if step.k > 0 {
                steps.copies.push((step.of, m.hash));
            }
            if m.built() {
                steps
                    .exps
                    .push((stale::exp(&m.actor), step.program.clone()));
                steps.built = true;
            }
        }
        Ok(steps)
    }

    /// Copy in each step's placement past a holder's own, then gate every
    /// step's on its BoolExp. Run after `picked::place`, so each copy is
    /// one of a holder standing until picked, and before the fields, which
    /// give each step its stand-in. Whether `root`, a map's doc, changed.
    pub fn place(&self, root: &mut Byml) -> Result<bool> {
        let copied = shows::copy_in(root, &self.copies)?;
        let named = shows::name_apart(root, &self.named)?;
        Ok(shows::gate(root, &self.gates)? || copied || named)
    }

    /// The drop graph's branch, when a step stand-in needs it.
    pub fn write(&self, romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
        match self.built {
            true => stale::write(romfs, builder),
            false => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use ounce_formats::murmur3;

    use super::*;
    use crate::checks::{self, hash_of};
    use crate::game::dev_dump;
    use crate::known::field;
    use crate::known::item;
    use crate::pickups::{FORTRESS_KEYS, FREESTANDING, Kind, POTS};
    use crate::world::presence_of;

    /// A Kokiri Forest pot, and the Death Mountain Crater heart piece.
    const POT: u64 = 2108281022000564101;
    const PIECE: u64 = 10331043360028932045;

    /// `kind`'s check placed as `hash`, dealt `item`, placed a step at a
    /// time in its map: the steps, and the map with the fields written.
    fn placed(kind: &Kind, hash: u64, item: &str) -> (Steps, Byml, Vec<Spot>) {
        let romfs = dev_dump().unwrap();
        let gd = GameData::load(&romfs).unwrap();
        let checks: Vec<ItemCheck> = (kind.checks(&romfs).unwrap().into_iter())
            .filter(|c| c.hash == hash)
            .collect();
        let drops = HashMap::from([(hash, Drop::from(item.to_string()))]);
        let holders = picked::holders(&checks, &drops);
        let steps = Steps::new(&checks, &drops, true, &holders, &gd).unwrap();
        let mut doc = checks::load_banc(&romfs, &checks[0].file).unwrap();
        picked::place(&mut doc.root, &holders).unwrap();
        assert!(steps.place(&mut doc.root).unwrap());
        let fields: Vec<String> = (moved(&checks, &drops, true).iter())
            .map(|m| format!("{}.{}={}", m.hash, m.field, m.actor))
            .collect();
        checks::set_fields(&mut doc.root, &fields).unwrap();
        let spots = picked::spots(&doc.root, &holders, &steps.copies).unwrap();
        (steps, doc.root, spots)
    }

    use zootr_hook::Spot;

    /// Each step's placement in `root`: its gate and what it gives.
    fn at(root: &Byml, hash: u64, field: &str) -> (String, String) {
        let actors = checks::actors(root).unwrap();
        let mut found = actors.iter().filter(|a| hash_of(a) == Some(hash));
        let a = found
            .next()
            .unwrap_or_else(|| panic!("no placement {hash}"));
        assert!(found.next().is_none(), "{hash} placed twice");
        let gate = presence_of(a).unwrap();
        assert!(!gate.negated);
        let path: Vec<&str> = field.split('.').collect();
        let gives = match path[..] {
            [one] => checks::str_field(a, one),
            [outer, inner] => checks::str_field(&a.as_map().unwrap()[outer], inner),
            _ => None,
        };
        (gate.flag, gives.unwrap().to_owned())
    }

    fn exp<'a>(steps: &'a Steps, name: &str) -> &'a [Op] {
        let e = steps.exps.iter().find(|(n, _)| n == name);
        &e.unwrap_or_else(|| panic!("no BoolExp {name}")).1
    }

    #[test]
    fn a_pot_dealt_the_hookshot_drops_each_level_from_a_placement_of_its_own() {
        if dev_dump().is_none() {
            return;
        }
        let (steps, root, spots) = placed(&POTS, POT, item::HOOKSHOT);
        let copy = copy_hash(POT, 1);
        assert_eq!(steps.copies, [(POT, copy)]);
        let drop = "Dynamic.Drop__DropActor";
        let (gate, hookshot) = at(&root, POT, drop);
        let (copy_gate, longshot) = at(&root, copy, drop);
        assert_eq!(gate, format!("Rando_Step_{POT}"));
        assert_eq!(copy_gate, format!("Rando_Step_{copy}"));
        assert!(hookshot.starts_with(stale::STEP) && hookshot.ends_with("_0"));
        assert_eq!(
            longshot,
            format!("{}1", hookshot.strip_suffix('0').unwrap())
        );
        // Each stands only until the pot's drop is picked up.
        let picked = Op::Bool(murmur3(picked::flag(POT).as_bytes()));
        for gate in [&gate, &copy_gate] {
            assert!(exp(&steps, gate).ends_with(&[picked, Op::Not, Op::And]));
        }
        // Each stand-in's drop stays only while it's the one the pot gives.
        let shows = |level: usize| shows::steps(item::HOOKSHOT, true).unwrap()[level].1.clone();
        assert_eq!(exp(&steps, &stale::exp(&hookshot)), shows(0));
        assert_eq!(exp(&steps, &stale::exp(&longshot)), shows(1));
        assert!(steps.built);
        // A take of either drop ticks the pot's flag.
        assert_eq!(spots.len(), 2);
        assert!(
            spots
                .iter()
                .all(|s| s.flag == murmur3(picked::flag(POT).as_bytes()))
        );
        assert_ne!(spots[0].name, spots[1].name);
        assert_eq!(spots[0].at, spots[1].at);
    }

    #[test]
    fn a_thiefs_key_steps_stand_by_her_live_gate() {
        let Some(romfs) = dev_dump() else { return };
        let key = FORTRESS_KEYS.checks(&romfs).unwrap()[0].clone();
        let vanilla = checks::load_banc(&romfs, &key.file).unwrap();
        let actors = checks::actors(&vanilla.root).unwrap();
        let holder = actors.iter().find(|a| hash_of(a) == Some(key.hash));
        let own = presence_of(holder.unwrap()).unwrap().flag;
        let (steps, root, _) = placed(&FORTRESS_KEYS, key.hash, item::HOOKSHOT);
        for hash in [key.hash, copy_hash(key.hash, 1)] {
            let actors = checks::actors(&root).unwrap();
            let a = actors.iter().find(|a| hash_of(a) == Some(hash)).unwrap();
            assert_eq!(presence_of(a).unwrap().flag, own, "{hash}");
            let lazy = a.as_map().unwrap()["Presence"].as_map().unwrap()
                [field::IS_CHECK_LAZY.get()]
            .as_bool()
            .unwrap();
            assert!(!lazy, "{hash} isn't watched live");
            // Her call reads the step's BoolExp before she falls.
            let read = exp(&steps, &gate(hash));
            let own = Op::Bool(murmur3(own.as_bytes()));
            assert!(!read.contains(&own), "{hash} waits on her fall");
        }
        // Each step goes by its own name, which her call names it by.
        let name = |root: &Byml, hash| {
            let actors = checks::actors(root).unwrap();
            let a = actors.iter().find(|a| hash_of(a) == Some(hash)).unwrap();
            checks::str_field(a, "Name").unwrap().to_owned()
        };
        let key_name = name(&vanilla.root, key.hash);
        assert_eq!(name(&root, key.hash), key_name);
        let copy = copy_hash(key.hash, 1);
        assert_eq!(name(&root, copy), shows::instance(&key_name, 1));
    }

    #[test]
    fn ammo_drops_a_floor_rupee_until_link_owns_what_fires_it() {
        if dev_dump().is_none() {
            return;
        }
        let arrows = crate::ammo::NEEDS[1].0;
        let (steps, root, _) = placed(&POTS, POT, arrows);
        let drop = "Dynamic.Drop__DropActor";
        let (_, rupee) = at(&root, POT, drop);
        let (_, bundle) = at(&root, copy_hash(POT, 1), drop);
        assert_eq!(rupee, crate::known::actor::OBJ_RUPEE_RED.get());
        assert!(bundle.starts_with(stale::STEP));
        // Only a built stand-in reads a BoolExp of its own.
        assert!(!steps.exps.iter().any(|(n, _)| *n == stale::exp(&rupee)));
        exp(&steps, &stale::exp(&bundle));
    }

    #[test]
    fn a_lying_heart_piece_dealt_a_level_stands_until_picked() {
        if dev_dump().is_none() {
            return;
        }
        let (steps, root, spots) = placed(&FREESTANDING, PIECE, item::HOOKSHOT);
        let (gate, hookshot) = at(&root, PIECE, "Gyaml");
        let (_, longshot) = at(&root, copy_hash(PIECE, 1), "Gyaml");
        assert_eq!(gate, format!("Rando_Step_{PIECE}"));
        assert_ne!(hookshot, longshot);
        let picked = Op::Bool(murmur3(picked::flag(PIECE).as_bytes()));
        assert!(exp(&steps, &gate).ends_with(&[picked, Op::Not, Op::And]));
        assert_eq!(spots.len(), 2);
        let row = crate::generate::Spot {
            hash: PIECE,
            holder: crate::known::actor::OBJ_HEART_PIECE.get().into(),
            item: item::HOOKSHOT.get().into(),
            ..Default::default()
        };
        assert_eq!(picked::flag_at(&row), Some(picked::flag(PIECE)));
    }
}
