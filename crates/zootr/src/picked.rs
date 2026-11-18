//! Pots, boxes, rocks and hives that drop their shuffled item until Link
//! picks it up, and their vanilla drop on every break after that. Each
//! gets a vanilla twin under a new hash, placed with it. A seed Bool, set
//! by the hook at the pickup, swaps one for the other through their
//! Presence. The twin keeps a key of its own, so it stays broken until its
//! area resets, as the vanilla holder does.
//!
//! A holder the game already takes away once it's crushed (two Ice Cavern
//! pots) gets no twin, since the vanilla one is gone after the pickup too.
//! It stands until its drop is picked up instead, so a drop left behind
//! comes back.
//!
//! A dropped pickup doesn't know its holder, so the hook's table lists each
//! holder's [`Spot`]: its flag, what it drops and where it stands. A take
//! sets the nearest unpicked one dropping that name.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, murmur3};
use zootr_hook::Spot;

use crate::checks::{self, DROP_ACTOR, ItemCheck, TRANSLATE, hash_of, str_field, vec3};
use crate::known::field;
use crate::pickups::{CRATES, HIVES, Kind, POTS, ROCKS};

/// The kinds whose holders get a twin.
const KINDS: [&Kind; 4] = [&POTS, &CRATES, &ROCKS, &HIVES];

/// A holder of a shuffled drop, and the twin that stands once it's picked.
pub struct Holder {
    pub hash: u64,
    /// None for a holder the game takes away once crushed.
    pub twin: Option<u64>,
}

/// The seed Bool set once the drop of the holder placed as `hash` is
/// picked up.
pub fn flag(hash: u64) -> String {
    format!("Rando_Picked_{hash}")
}

/// The flag a spot ticks by, when its holder gets a twin.
pub fn flag_at(holder: &str, hash: u64) -> Option<String> {
    KINDS.iter().any(|k| k.places(holder)).then(|| flag(hash))
}

/// Each holder among `pieces` that gets a twin, once each.
pub fn holders(pieces: &[ItemCheck]) -> Vec<Holder> {
    let gated: BTreeMap<u64, bool> = pieces
        .iter()
        .filter(|c| KINDS.iter().any(|k| k.holds(c)))
        .map(|c| (c.hash, c.presence.is_some()))
        .collect();
    gated
        .into_iter()
        .map(|(hash, gated)| Holder {
            hash,
            twin: (!gated).then(|| crate::known::hash(&flag(hash))),
        })
        .collect()
}

pub fn flags(holders: &[Holder]) -> Vec<String> {
    holders.iter().map(|h| flag(h.hash)).collect()
}

/// Each holder's hash, then its twin's.
pub fn pairs(holders: &[Holder]) -> Vec<(u64, u64)> {
    holders
        .iter()
        .filter_map(|h| Some((h.hash, h.twin?)))
        .collect()
}

/// Place a twin beside each of `holders` in `root`, a Banc file's doc:
/// a copy as it is now, under the twin's hash, standing once the flag is
/// set, while the holder stands until then. Run before the holder's drop
/// changes, so the twin keeps the vanilla one. Returns how many it placed.
pub fn place(root: &mut Byml, holders: &[Holder]) -> Result<usize> {
    let mut twins = Vec::new();
    for actor in checks::actors_mut(root)? {
        let Some(hash) = hash_of(actor) else {
            continue;
        };
        let Some(h) = holders.iter().find(|h| h.hash == hash) else {
            continue;
        };
        let flag = flag(hash);
        let Some(twin_hash) = h.twin else {
            let gone = actor.as_mut_map()?.remove("Presence");
            let gone = gone.as_ref().and_then(|p| p.as_map().ok());
            let negated = gone.and_then(|p| p.get(field::IS_NEGATION.get()));
            ensure!(
                negated == Some(&Byml::Bool(true)),
                "holder {hash} stands only once something happens, so it can't take turns"
            );
            checks::gate(actor, &flag, true)?;
            continue;
        };
        let mut twin = actor.clone();
        checks::gate(actor, &flag, true).with_context(|| format!("holder {hash}"))?;
        let map = twin.as_mut_map()?;
        map.insert("Hash".into(), Byml::U64(twin_hash));
        if let Some(Byml::Map(phive)) = map.get_mut("Phive")
            && let Some(Byml::Map(placement)) = phive.get_mut("Placement")
            && placement.contains_key("ID")
        {
            placement.insert("ID".into(), Byml::U64(twin_hash));
        }
        checks::gate(&mut twin, &flag, false)?;
        twins.push(twin);
    }
    let placed = twins.len();
    if placed == 0 {
        return Ok(0);
    }
    let list = (root.as_mut_map()?)
        .get_mut("Actors")
        .context("no actors")?
        .as_mut_array()?;
    for twin in &twins {
        let hash = hash_of(twin).context("twin without Hash")?;
        ensure!(
            !list.iter().any(|a| hash_of(a) == Some(hash)),
            "twin {hash} is already placed"
        );
    }
    list.extend(twins);
    Ok(placed)
}

/// Each of `holders` placed in `root`, a Banc file's doc, as the hook's
/// table lists it. Run once the drops are written, so each names the
/// shuffled one.
pub fn spots(root: &Byml, holders: &[Holder]) -> Result<Vec<Spot>> {
    let mut out = Vec::new();
    for actor in checks::actors(root)? {
        let Some(hash) = hash_of(actor).filter(|h| holders.iter().any(|x| x.hash == *h)) else {
            continue;
        };
        let dynamic = actor.as_map()?.get("Dynamic");
        let drop = dynamic.and_then(|d| str_field(d, DROP_ACTOR.get()));
        let drop = drop.with_context(|| format!("holder {hash} drops nothing"))?;
        let at = vec3(actor, TRANSLATE).with_context(|| format!("holder {hash} stands nowhere"))?;
        out.push(Spot {
            flag: murmur3(flag(hash).as_bytes()),
            name: Spot::name_hash(drop.as_bytes()),
            at: at.map(|v| v.round() as i16),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::actor;
    use crate::pickups;
    use crate::settings::NewGame;
    use crate::world::presence_of;

    /// Kokiri Forest's rock.
    const ROCK: u64 = 6429280567576879719;
    /// The Ice Cavern pots gone once crushed.
    const ICE_RED: u64 = 3105481948394149361;
    const ICE_BLUE: u64 = 6330430963809465220;

    fn pool() -> (ounce_formats::Romfs, Vec<ItemCheck>) {
        let romfs = dev_dump().unwrap();
        let mut new_game = NewGame::rings();
        new_game.pots = true;
        new_game.crates = true;
        new_game.rocks = true;
        new_game.hives = true;
        new_game.skulltulas = true;
        let pieces = pickups::dealt(&romfs, &new_game).unwrap();
        (romfs, pieces)
    }

    #[test]
    fn every_breakable_holder_and_hive_gets_a_twin_and_nothing_else() {
        if dev_dump().is_none() {
            return;
        }
        let (_, pieces) = pool();
        let holders = holders(&pieces);
        assert_eq!(holders.len(), 134);
        let skulltulas = pickups::SKULLTULAS.checks(&dev_dump().unwrap()).unwrap();
        assert!(
            skulltulas
                .iter()
                .all(|s| holders.iter().all(|h| h.hash != s.hash))
        );
        assert!(
            holders
                .iter()
                .all(|h| h.twin != Some(h.hash) && h.twin != Some(0))
        );
        let gone: Vec<u64> = holders
            .iter()
            .filter(|h| h.twin.is_none())
            .map(|h| h.hash)
            .collect();
        assert_eq!(gone, [ICE_RED, ICE_BLUE]);
        assert_eq!(
            flag_at(actor::OBJ_LIFT_ROCK_A_01.get(), ROCK),
            Some(flag(ROCK))
        );
        assert_eq!(flag_at(actor::ENEMY_SKULLTULA_GOLD.get(), ROCK), None);
    }

    #[test]
    fn a_holder_stands_until_picked_and_its_vanilla_twin_after() {
        let Some(_) = dev_dump() else { return };
        let (romfs, pieces) = pool();
        let holders = holders(&pieces);
        let rock = pieces.iter().find(|c| ROCKS.holds(c)).unwrap();
        let rock_hash = rock.hash;
        let mut doc = checks::load_banc(&romfs, &rock.file).unwrap();
        let before = checks::actors(&doc.root).unwrap().len();
        let mine = pieces
            .iter()
            .filter(|c| c.file == rock.file && holders.iter().any(|h| h.hash == c.hash))
            .map(|c| c.hash)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert_eq!(place(&mut doc.root, &holders).unwrap(), mine);
        let actors = checks::actors(&doc.root).unwrap();
        assert_eq!(actors.len(), before + mine);
        let twin = holders
            .iter()
            .find(|h| h.hash == rock_hash)
            .unwrap()
            .twin
            .unwrap();
        let find = |hash| actors.iter().find(|a| hash_of(a) == Some(hash)).unwrap();
        let (held, copy) = (find(rock_hash), find(twin));
        let gate = |a| presence_of(a).unwrap();
        let (a, b) = (gate(held), gate(copy));
        let flag = flag(rock_hash);
        assert_eq!((a.flag.as_str(), a.negated), (flag.as_str(), true));
        assert_eq!((b.flag.as_str(), b.negated), (flag.as_str(), false));
        let drop = |a: &Byml| {
            a.as_map().unwrap()["Dynamic"].as_map().unwrap()[checks::DROP_ACTOR.get()].clone()
        };
        assert_eq!(drop(held), drop(copy));
        let id = |a: &Byml| a.as_map().unwrap()["Phive"].as_map().unwrap()["Placement"].clone();
        assert_eq!(id(copy).as_map().unwrap()["ID"], Byml::U64(twin));
    }

    #[test]
    fn a_pot_gone_once_crushed_stands_until_picked_instead() {
        let Some(_) = dev_dump() else { return };
        let (romfs, pieces) = pool();
        let holders = holders(&pieces);
        let pot = pieces.iter().find(|c| c.hash == ICE_RED).unwrap();
        let mut doc = checks::load_banc(&romfs, &pot.file).unwrap();
        let before = checks::actors(&doc.root).unwrap().len();
        let twins = holders
            .iter()
            .filter(|h| h.twin.is_some())
            .filter(|h| {
                pieces
                    .iter()
                    .any(|c| c.hash == h.hash && c.file == pot.file)
            })
            .count();
        place(&mut doc.root, &holders).unwrap();
        let actors = checks::actors(&doc.root).unwrap();
        assert_eq!(actors.len(), before + twins);
        for hash in [ICE_RED, ICE_BLUE] {
            let pot = actors.iter().find(|a| hash_of(a) == Some(hash)).unwrap();
            let gate = presence_of(pot).unwrap();
            assert_eq!((gate.flag, gate.negated), (flag(hash), true));
        }
    }

    #[test]
    fn a_holder_lists_its_flag_its_shuffled_drop_and_where_it_stands() {
        let Some(_) = dev_dump() else { return };
        let (romfs, pieces) = pool();
        let holders = holders(&pieces);
        let rock = pieces.iter().find(|c| ROCKS.holds(c)).unwrap();
        let mut doc = checks::load_banc(&romfs, &rock.file).unwrap();
        let field = format!(
            "{}.Dynamic.{}=Obj_FakeStandIn_001",
            rock.hash,
            DROP_ACTOR.get()
        );
        place(&mut doc.root, &holders).unwrap();
        checks::set_fields(&mut doc.root, &[field]).unwrap();
        let spots = spots(&doc.root, &holders).unwrap();
        let mine = spots
            .iter()
            .find(|s| s.flag == murmur3(flag(rock.hash).as_bytes()))
            .unwrap();
        assert_eq!(mine.name, Spot::name_hash(b"Obj_FakeStandIn_001"));
        let actors = checks::actors(&doc.root).unwrap();
        let placed = actors
            .iter()
            .find(|a| hash_of(a) == Some(rock.hash))
            .unwrap();
        let at = vec3(placed, TRANSLATE).unwrap();
        assert_eq!(mine.at, at.map(|v| v.round() as i16));
        // Twins aren't listed: one spot per holder in the file.
        let here = holders
            .iter()
            .filter(|h| {
                pieces
                    .iter()
                    .any(|c| c.hash == h.hash && c.file == rock.file)
            })
            .count();
        assert_eq!(spots.len(), here);
    }
}
