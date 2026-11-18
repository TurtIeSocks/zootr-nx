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
//! A Gold Skulltula gets no twin either, as a vanilla one never comes
//! back. Its key is set only by a token or a heart piece taken from it,
//! so one dropping anything else would stand again after a reset. It stands
//! until its drop is picked up instead. One that shows once beans are
//! planted keeps that gate, read with the flag through a BoolExp.
//!
//! Ruto's letter, a pickup its logic module deletes once Link has the
//! letter, is deleted by the flag instead: Link getting the letter
//! elsewhere would take away what lies there, and getting what lies
//! there wouldn't.
//!
//! A dropped pickup doesn't know its holder, so the hook's table lists each
//! holder's [`Spot`]: its flag, what it drops and where it stands. A take
//! sets the nearest unpicked one dropping that name.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::ainb::Patch;
use ounce_formats::{Byml, Romfs, murmur3};
use zootr_hook::Spot;

use crate::checks::{self, DROP_ACTOR, GYAML, ItemCheck, TRANSLATE, hash_of, str_field, vec3};
use crate::flows::Rewrites;
use crate::gamedata::Op;
use crate::graphs::logic;
use crate::known::{field, flag, map};
use crate::pickups::{CRATES, HIVES, Kind, POTS, ROCKS, RUTOS_LETTER, SKULLTULAS};
use crate::world::Presence;

/// The kinds whose holders stand until picked.
const KINDS: [&Kind; 6] = [&POTS, &CRATES, &ROCKS, &HIVES, &RUTOS_LETTER, &SKULLTULAS];

/// A holder of a shuffled item, standing until it's picked.
pub struct Holder {
    pub hash: u64,
    pub after: After,
}

/// What stands in a holder's place once it's picked.
#[derive(Debug, PartialEq)]
pub enum After {
    /// Its vanilla twin, placed under this hash.
    Twin(u64),
    /// Nothing, as the game takes it away once crushed.
    Gone,
    /// Nothing, as it's a Gold Skulltula. It keeps its own gate, if any.
    Killed(Option<Presence>),
    /// Nothing, as its logic module deletes it (Ruto's letter).
    Deleted,
}

/// The seed Bool set once the drop of the holder placed as `hash` is
/// picked up.
pub fn flag(hash: u64) -> String {
    format!("Rando_Picked_{hash}")
}

/// The BoolExp a gated Gold Skulltula shows by: its own gate, and its
/// drop not picked up yet.
pub fn shown(hash: u64) -> String {
    format!("Rando_Shown_{hash}")
}

/// The flag a spot ticks by, when its holder stands until picked or is
/// a shop slot (`shops::sold`).
pub fn flag_at(holder: &str, hash: u64) -> Option<String> {
    let picked = KINDS.iter().any(|k| k.places(holder));
    (picked || crate::shops::is_holder(holder)).then(|| flag(hash))
}

/// Each holder among `pieces` that stands until picked, once each.
pub fn holders(pieces: &[ItemCheck]) -> Vec<Holder> {
    let after = |c: &ItemCheck| match () {
        _ if RUTOS_LETTER.holds(c) => After::Deleted,
        _ if SKULLTULAS.holds(c) => After::Killed(c.presence.clone()),
        _ if c.presence.is_some() => After::Gone,
        _ => After::Twin(crate::known::hash(&flag(c.hash))),
    };
    let found: BTreeMap<u64, After> = pieces
        .iter()
        .filter(|c| KINDS.iter().any(|k| k.holds(c)))
        .map(|c| (c.hash, after(c)))
        .collect();
    let holders = found
        .into_iter()
        .map(|(hash, after)| Holder { hash, after });
    holders.collect()
}

pub fn flags(holders: &[Holder]) -> Vec<String> {
    holders.iter().map(|h| flag(h.hash)).collect()
}

/// The BoolExp each gated Gold Skulltula among `holders` shows by.
pub fn exps(holders: &[Holder]) -> Vec<(String, Vec<Op>)> {
    let bool_of = |name: &str| Op::Bool(murmur3(name.as_bytes()));
    let gated = holders.iter().filter_map(|h| match &h.after {
        After::Killed(Some(gate)) => Some((h.hash, gate)),
        _ => None,
    });
    let exps = gated.map(|(hash, gate)| {
        let mut program = vec![bool_of(&gate.flag)];
        program.extend(gate.negated.then_some(Op::Not));
        program.extend([bool_of(&flag(hash)), Op::Not, Op::And]);
        (shown(hash), program)
    });
    exps.collect()
}

/// Each holder's hash, then its twin's.
pub fn pairs(holders: &[Holder]) -> Vec<(u64, u64)> {
    holders
        .iter()
        .filter_map(|h| match h.after {
            After::Twin(twin) => Some((h.hash, twin)),
            _ => None,
        })
        .collect()
}

/// Ruto's letter's logic module deletes what lies there once its flag is
/// set, not once Link has the letter. Nothing when it isn't among
/// `holders`.
pub fn delete_letter(romfs: &Romfs, rewrites: &mut Rewrites, holders: &[Holder]) -> Result<()> {
    let Some(letter) = holders.iter().find(|h| h.after == After::Deleted) else {
        return Ok(());
    };
    let rel = logic(map::HYRULE_FIELD_LAKE_HYLIA_CHILD_PREFAB_9E98);
    let mut patch = Patch::new(romfs.read(&rel)?)?;
    let key = (
        field::HOLD_GAME_DATA_GET_BOOL_SIMPLE,
        field::GAME_DATA_NAME_BOOL,
    );
    let got = flag::GET_STONE_OF_WATER_GET_RUTO_LETTER.get();
    let nodes = patch.graph().nodes.iter().enumerate();
    let reads: Vec<usize> = nodes
        .filter(|(_, n)| n.name == key.0 && n.str(key.1) == Some(got))
        .map(|(i, _)| i)
        .collect();
    ensure!(!reads.is_empty(), "{rel} never reads {got}");
    for i in reads {
        patch.set_str(i, key.1, &flag(letter.hash))?;
    }
    rewrites.insert(rel, patch.into_bytes());
    Ok(())
}

/// Place a twin beside each of `holders` in `root`, a Banc file's doc:
/// a copy as it is now, under the twin's hash, standing once the flag is
/// set, while the holder stands until then. Run before the holder's drop
/// changes, so the twin keeps the vanilla one. Returns how many holders
/// it changed.
pub fn place(root: &mut Byml, holders: &[Holder]) -> Result<usize> {
    let (mut twins, mut gated) = (Vec::new(), 0);
    for actor in checks::actors_mut(root)? {
        let Some(hash) = hash_of(actor) else {
            continue;
        };
        let Some(h) = holders.iter().find(|h| h.hash == hash) else {
            continue;
        };
        let flag = flag(hash);
        let twin_hash = match h.after {
            After::Twin(twin) => twin,
            After::Deleted => continue,
            After::Gone => {
                let gone = actor.as_mut_map()?.remove("Presence");
                let gone = gone.as_ref().and_then(|p| p.as_map().ok());
                let negated = gone.and_then(|p| p.get(field::IS_NEGATION.get()));
                ensure!(
                    negated == Some(&Byml::Bool(true)),
                    "holder {hash} stands only once something happens, so it can't take turns"
                );
                checks::gate(actor, &flag, true)?;
                gated += 1;
                continue;
            }
            After::Killed(None) => {
                checks::gate(actor, &flag, true).with_context(|| format!("Skulltula {hash}"))?;
                gated += 1;
                continue;
            }
            After::Killed(Some(_)) => {
                let presence = actor.as_mut_map()?.get_mut("Presence");
                let presence = presence.with_context(|| format!("Skulltula {hash} isn't gated"))?;
                let presence = presence.as_mut_map()?;
                presence.insert(
                    field::FLAG_NAME.get().into(),
                    Byml::String(shown(hash).into()),
                );
                presence.insert(field::IS_NEGATION.get().into(), Byml::Bool(false));
                gated += 1;
                continue;
            }
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
    let changed = twins.len() + gated;
    if twins.is_empty() {
        return Ok(changed);
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
    Ok(changed)
}

/// Each of `holders` placed in `root`, a Banc file's doc, as the hook's
/// table lists it: by what it drops, or what it is when it drops nothing
/// (Ruto's letter). Run once the fields are written, so each names the
/// shuffled item.
pub fn spots(root: &Byml, holders: &[Holder]) -> Result<Vec<Spot>> {
    let mut out = Vec::new();
    for actor in checks::actors(root)? {
        let Some(hash) = hash_of(actor).filter(|h| holders.iter().any(|x| x.hash == *h)) else {
            continue;
        };
        let dynamic = actor.as_map()?.get("Dynamic");
        let drop = dynamic.and_then(|d| str_field(d, DROP_ACTOR.get()));
        let drop = drop.or_else(|| str_field(actor, GYAML));
        let drop = drop.with_context(|| format!("holder {hash} is nothing"))?;
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
    use std::collections::BTreeSet;

    use ounce_formats::BymlDoc;

    use super::*;
    use crate::game::dev_dump;
    use crate::gamedata;
    use crate::known::actor;
    use crate::pickups;
    use crate::settings::NewGame;
    use crate::world::presence_of;

    /// Kokiri Forest's rock.
    const ROCK: u64 = 6429280567576879719;
    /// Ruto's letter.
    const LETTER: u64 = 474136620438465634;
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
    fn breakables_get_twins_and_skulltulas_stand_until_picked() {
        if dev_dump().is_none() {
            return;
        }
        let (_, pieces) = pool();
        let holders = holders(&pieces);
        let skulltulas = pickups::SKULLTULAS.checks(&dev_dump().unwrap()).unwrap();
        let skulltulas: BTreeSet<u64> = skulltulas.iter().map(|s| s.hash).collect();
        assert_eq!(skulltulas.len(), 100);
        assert_eq!(holders.len(), 134 + 100);
        let killed = |h: &&Holder| matches!(h.after, After::Killed(_));
        let killed: BTreeSet<u64> = holders.iter().filter(killed).map(|h| h.hash).collect();
        assert_eq!(killed, skulltulas);
        // The bean soil ones keep their gate.
        let gated = holders
            .iter()
            .filter(|h| matches!(h.after, After::Killed(Some(_))));
        assert_eq!(gated.count(), 10);
        assert!(
            holders
                .iter()
                .all(|h| h.after != After::Twin(h.hash) && h.after != After::Twin(0))
        );
        let gone: Vec<u64> = holders
            .iter()
            .filter(|h| h.after == After::Gone)
            .map(|h| h.hash)
            .collect();
        assert_eq!(gone, [ICE_RED, ICE_BLUE]);
        assert!(holders.iter().all(|h| h.after != After::Deleted));
        assert_eq!(
            flag_at(actor::OBJ_LIFT_ROCK_A_01.get(), ROCK),
            Some(flag(ROCK))
        );
        let skulltula = *skulltulas.first().unwrap();
        let at = flag_at(actor::ENEMY_SKULLTULA_GOLD.get(), skulltula);
        assert_eq!(at, Some(flag(skulltula)));
    }

    #[test]
    fn a_skulltula_stands_until_picked_and_a_bean_one_until_planted_too() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::SKULLTULAS.checks(&romfs).unwrap();
        let holders = holders(&pieces);
        let bean = holders
            .iter()
            .find(|h| matches!(h.after, After::Killed(Some(_))))
            .unwrap();
        let After::Killed(Some(soil)) = &bean.after else {
            unreachable!()
        };
        let plain = holders
            .iter()
            .find(|h| h.after == After::Killed(None))
            .unwrap();
        for h in [bean, plain] {
            let piece = pieces.iter().find(|c| c.hash == h.hash).unwrap();
            let mut doc = checks::load_banc(&romfs, &piece.file).unwrap();
            let before = checks::actors(&doc.root).unwrap().len();
            assert!(place(&mut doc.root, &holders).unwrap() > 0);
            let actors = checks::actors(&doc.root).unwrap();
            assert_eq!(actors.len(), before, "no twins");
            let placed = actors.iter().find(|a| hash_of(a) == Some(h.hash)).unwrap();
            let gate = presence_of(placed).unwrap();
            match h.hash == plain.hash {
                true => assert_eq!((gate.flag, gate.negated), (flag(h.hash), true)),
                false => assert_eq!((gate.flag, gate.negated), (shown(h.hash), false)),
            }
        }
        let exps = exps(&holders);
        assert_eq!(exps.len(), 10);
        let (name, program) = exps.iter().find(|(n, _)| *n == shown(bean.hash)).unwrap();
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        gamedata::add_bools(&mut doc.root, &[flag(bean.hash)]).unwrap();
        gamedata::add_exps(&mut doc.root, &[(name.clone(), program.clone())]).unwrap();
        let gd = gamedata::GameData::parse(&doc.to_binary()).unwrap();
        let name_of = |n: &str| gd.name(murmur3(n.as_bytes()));
        let expr = format!("{} && !{}", name_of(&soil.flag), name_of(&flag(bean.hash)));
        assert_eq!(gd.expr(name).unwrap(), Some(expr));
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
        let twins = (holders.iter())
            .filter(|h| matches!(h.after, After::Twin(_)))
            .filter(|h| {
                pieces
                    .iter()
                    .any(|c| c.hash == h.hash && c.file == rock.file)
            })
            .count();
        assert_eq!(actors.len(), before + twins);
        let After::Twin(twin) = holders.iter().find(|h| h.hash == rock_hash).unwrap().after else {
            panic!("the rock has no twin");
        };
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
            .filter(|h| matches!(h.after, After::Twin(_)))
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

    #[test]
    fn rutos_letter_goes_once_picked_not_once_link_has_the_letter() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = RUTOS_LETTER.checks(&romfs).unwrap();
        let holders = holders(&pieces);
        assert_eq!(holders.len(), 1);
        assert_eq!(
            (holders[0].hash, &holders[0].after),
            (LETTER, &After::Deleted)
        );
        assert_eq!(
            flag_at(actor::OBJ_BOTTLE_LETTER.get(), LETTER),
            Some(flag(LETTER))
        );
        assert!(pairs(&holders).is_empty());
        let mut rewrites = Rewrites::new();
        delete_letter(&romfs, &mut rewrites, &holders).unwrap();
        let rel = logic(map::HYRULE_FIELD_LAKE_HYLIA_CHILD_PREFAB_9E98);
        let graph = ounce_formats::ainb::Graph::parse(&rewrites[&rel]).unwrap();
        let reads = |name: &str| {
            let nodes = graph.nodes.iter();
            nodes
                .filter(|n| n.str(field::GAME_DATA_NAME_BOOL) == Some(name))
                .count()
        };
        assert_eq!(reads(&flag(LETTER)), 2);
        assert_eq!(reads(flag::GET_STONE_OF_WATER_GET_RUTO_LETTER), 0);
        // No twin, and it lists by what it is, as it drops nothing.
        let mut doc = checks::load_banc(&romfs, &pieces[0].file).unwrap();
        assert_eq!(place(&mut doc.root, &holders).unwrap(), 0);
        let field = format!("{LETTER}.{GYAML}=Obj_FakeStandIn_001");
        checks::set_fields(&mut doc.root, &[field]).unwrap();
        let spots = spots(&doc.root, &holders).unwrap();
        assert_eq!(spots.len(), 1);
        assert_eq!(spots[0].name, Spot::name_hash(b"Obj_FakeStandIn_001"));
    }

    #[test]
    fn no_letter_no_rewrite() {
        let Some(romfs) = dev_dump() else { return };
        let mut rewrites = Rewrites::new();
        delete_letter(&romfs, &mut rewrites, &[]).unwrap();
        assert!(rewrites.is_empty());
    }
}
