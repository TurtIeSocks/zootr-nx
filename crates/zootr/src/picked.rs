//! Pots, boxes, rocks, grass and hives that drop their shuffled item until Link
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
//! Goron City's Deku Stick pot, which the city's logic module makes,
//! gets no twin either: its twin hung the map's load (issue #458). It
//! stands until its drop is picked up and then stays gone, where the
//! vanilla pot would come back. That's EIE debt, not a choice.
//!
//! A Gold Skulltula gets no twin either, as a vanilla one never comes
//! back. Its key is set only by a token or a heart piece taken from it,
//! so one dropping anything else would stand again after a reset. It stands
//! until its drop is picked up instead. One that shows once beans are
//! planted keeps that gate, read with the flag through a BoolExp. One
//! dealt its own token is left vanilla: the token sets its key, which the
//! tracker reads.
//!
//! Ruto's letter, a pickup its logic module deletes once Link has the
//! letter, is deleted by the flag instead: Link getting the letter
//! elsewhere would take away what lies there, and getting what lies
//! there wouldn't.
//!
//! A dropped pickup doesn't know its holder, so the hook's table lists each
//! holder's [`Spot`]: its flag, what it drops and where it stands. A take
//! sets the nearest unpicked one dropping that name.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, ensure};
use ounce_formats::ainb::Patch;
use ounce_formats::{Byml, Romfs, murmur3};
use zootr_hook::Spot;

use crate::checks::{
    self, DROP_ACTOR, Drop, GYAML, ItemCheck, TRANSLATE, hash_of, str_field, vec3,
};
use crate::flows::Rewrites;
use crate::gamedata::Op;
use crate::graphs::logic;
use crate::known::{field, flag, map};
use crate::pickups::seats;
use crate::pickups::steps::asks;
use crate::pickups::{
    BOSS_HEARTS, CRATES, FIRE_ARROWS, FLYING_POTS, FORTRESS_KEYS, FREESTANDING, GRASS, HIVES, Kind,
    LOOSE, LOOSE_KEYS, POTS, ROCKS, RUTOS_LETTER, SKULLTULAS,
};
use crate::world::Presence;

/// Pickups lying in place, which stand until picked only when dealt what
/// might show a step at a time. Their key tracks the rest.
const LYING: [&Kind; 6] = [
    &FREESTANDING,
    &LOOSE,
    &BOSS_HEARTS,
    &FIRE_ARROWS,
    &LOOSE_KEYS,
    &FORTRESS_KEYS,
];

/// Whether a holder that `lies` in place, dealt `item`, stands until
/// picked.
fn taken(lies: bool, item: &str) -> bool {
    lies && asks(item)
}

/// Holders whose twin hangs their map's load: Goron City's Deku Stick
/// pot, made by the city's logic module (issue #458).
pub const NO_TWIN: [u64; 1] = [16954699510563627060];

/// The kinds whose holders stand until picked.
const KINDS: [&Kind; 8] = [
    &POTS,
    &FLYING_POTS,
    &CRATES,
    &ROCKS,
    &GRASS,
    &HIVES,
    &RUTOS_LETTER,
    &SKULLTULAS,
];

/// A holder of a shuffled item, standing until it's picked.
pub struct Holder {
    pub hash: u64,
    pub after: After,
    /// How it looks until picked, if it's a breakable that's marked.
    pub look: crate::marks::Look,
}

/// What stands in a holder's place once it's picked.
#[derive(Debug, PartialEq)]
pub enum After {
    /// Its vanilla twin, placed under this hash.
    Twin(u64),
    /// Nothing, as the game takes it away once crushed.
    Gone,
    /// Nothing, as its twin would hang its map's load ([`NO_TWIN`]).
    Alone,
    /// Nothing, as it's a Gold Skulltula. It keeps its own gate, if any.
    Killed(Option<Presence>),
    /// Nothing, as its logic module deletes it (Ruto's letter).
    Deleted,
    /// Nothing, as it's a pickup lying in place, dealt what might show a
    /// step at a time (`pickups::steps`). It keeps its own gate, if any.
    Taken(Option<Presence>),
    /// Nothing, as it's a boulder, whose item lies in its seat
    /// (`pickups::seats`). The boulder stays vanilla; the seat stands until
    /// picked.
    Seated,
}

/// The seed Bool set once the drop of the holder placed as `hash` is
/// picked up.
pub fn flag(hash: u64) -> String {
    format!("Rando_Picked_{hash}")
}

/// The BoolExp every gate of the holder placed as `hash` reads: its
/// [`flag`], which the hook sets, or any of its stand-ins' taken Bools,
/// which the pickup event graph sets without the hook
/// (`pickups::taken`).
pub fn got(hash: u64) -> String {
    format!("Rando_Got_{hash}")
}

/// The BoolExp a gated Gold Skulltula shows by: its own gate, and its
/// drop not picked up yet.
pub fn shown(hash: u64) -> String {
    format!("Rando_Shown_{hash}")
}

/// A Gold Skulltula dealt its own token stays vanilla: the token sets its
/// key, which the tracker reads.
fn keeps_token(skulltula: bool, vanilla: &str, item: &str) -> bool {
    skulltula && item == vanilla
}

/// The flag a spot ticks by, when its holder stands until picked or is
/// a shop slot (`shops::sold`).
pub fn flag_at(spot: &crate::generate::Spot) -> Option<String> {
    let holder = spot.holder.as_str();
    let kept = keeps_token(SKULLTULAS.places(holder), &spot.vanilla, &spot.item);
    let lies = LYING.iter().any(|k| k.places(holder));
    let stands = KINDS.iter().any(|k| k.places(holder)) || seats::places(holder);
    let picked = stands && !kept || taken(lies, &spot.item);
    (picked || crate::shops::is_holder(holder)).then(|| flag(spot.hash))
}

/// Each holder among `pieces`, dealt `drops`, that stands until picked,
/// once each.
pub fn holders(pieces: &[ItemCheck], drops: &HashMap<u64, Drop>) -> Vec<Holder> {
    let lying = |c: &ItemCheck| {
        let lies = LYING.iter().any(|k| k.holds(c));
        taken(lies, crate::gifts::item_at(drops, c.hash, &c.item))
    };
    let after = |c: &ItemCheck| match () {
        _ if seats::holds(c) => After::Seated,
        _ if RUTOS_LETTER.holds(c) => After::Deleted,
        _ if SKULLTULAS.holds(c) => After::Killed(c.presence.clone()),
        _ if lying(c) => After::Taken(c.presence.clone()),
        _ if c.presence.is_some() => After::Gone,
        _ if NO_TWIN.contains(&c.hash) => After::Alone,
        _ => After::Twin(crate::known::hash(&flag(c.hash))),
    };
    let found: BTreeMap<u64, (After, crate::marks::Look)> = pieces
        .iter()
        .filter(|c| KINDS.iter().any(|k| k.holds(c)) || seats::holds(c) || lying(c))
        .filter(|c| {
            let item = crate::gifts::item_at(drops, c.hash, &c.item);
            !keeps_token(SKULLTULAS.holds(c), &c.item, item)
        })
        .map(|c| {
            let look = crate::marks::look(crate::gifts::item_at(drops, c.hash, &c.item));
            (c.hash, (after(c), look))
        })
        .collect();
    let holders = found
        .into_iter()
        .map(|(hash, (after, look))| Holder { hash, after, look });
    holders.collect()
}

pub fn flags(holders: &[Holder]) -> Vec<String> {
    holders.iter().map(|h| flag(h.hash)).collect()
}

/// Each of `holders`' [`got`], from its flag and its `stand_ins`, by
/// holder, then the BoolExp each gated Gold Skulltula shows by.
pub fn exps(holders: &[Holder], stand_ins: &[(u64, String)]) -> Vec<(String, Vec<Op>)> {
    let bool_of = |name: &str| Op::Bool(murmur3(name.as_bytes()));
    let got_of = |hash: u64| {
        let taken = (stand_ins.iter().filter(|(of, _)| *of == hash))
            .map(|(_, s)| vec![bool_of(&crate::pickups::taken::flag(s))]);
        let program = std::iter::once(vec![bool_of(&flag(hash))]).chain(taken);
        (
            got(hash),
            program.reduce(crate::shows::or).unwrap_or_default(),
        )
    };
    let mut out: Vec<_> = holders.iter().map(|h| got_of(h.hash)).collect();
    let gated = holders.iter().filter_map(|h| match &h.after {
        After::Killed(Some(gate)) => Some((h.hash, gate)),
        _ => None,
    });
    let exps = gated.map(|(hash, gate)| {
        let mut program = vec![bool_of(&gate.flag)];
        program.extend(gate.negated.then_some(Op::Not));
        program.extend([Op::Exp(murmur3(got(hash).as_bytes())), Op::Not, Op::And]);
        (shown(hash), program)
    });
    out.extend(exps);
    out
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
/// set, while the holder stands until then, as its marked copy when it's
/// a breakable that's marked (`marks`). Run before the holder's drop
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
        let flag = got(hash);
        let twin_hash = match h.after {
            After::Twin(twin) => twin,
            // Its steps' gates take its turns (`pickups::steps`).
            After::Deleted | After::Taken(_) | After::Seated => continue,
            After::Gone | After::Alone => {
                let gone = actor.as_mut_map()?.remove("Presence");
                let gone = gone.as_ref().and_then(|p| p.as_map().ok());
                let negated = gone.and_then(|p| p.get(field::IS_NEGATION.get()));
                ensure!(
                    h.after == After::Alone || negated == Some(&Byml::Bool(true)),
                    "holder {hash} stands only once something happens, so it can't take turns"
                );
                checks::gate(actor, &flag, true)?;
                crate::marks::mark(actor, h.look)?;
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
        crate::marks::mark(actor, h.look)?;
        crate::shows::rehash(&mut twin, twin_hash)?;
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
/// (Ruto's letter). A step's placement past a holder's own, or a boulder's
/// seat, among `copies` by the placement each copies, ticks its holder's
/// flag. A boulder itself holds nothing. Run once the fields are written,
/// so each names the shuffled item.
pub fn spots(root: &Byml, holders: &[Holder], copies: &[(u64, u64)]) -> Result<Vec<Spot>> {
    let holder = |placed: u64| {
        let of = copies.iter().find(|(_, copy)| *copy == placed);
        let hash = of.map_or(placed, |(of, _)| *of);
        holders.iter().find(|h| h.hash == hash)
    };
    let mut out = Vec::new();
    for actor in checks::actors(root)? {
        let Some(placed) = hash_of(actor) else {
            continue;
        };
        let Some(Holder { hash, after, .. }) = holder(placed) else {
            continue;
        };
        if *after == After::Seated && placed == *hash {
            continue;
        }
        let dynamic = actor.as_map()?.get(crate::checks::DYNAMIC);
        let drop = dynamic.and_then(|d| str_field(d, DROP_ACTOR.get()));
        let drop = drop.or_else(|| str_field(actor, GYAML));
        let drop = drop.with_context(|| format!("holder {placed} is nothing"))?;
        let at =
            vec3(actor, TRANSLATE).with_context(|| format!("holder {placed} stands nowhere"))?;
        out.push(Spot {
            flag: murmur3(flag(*hash).as_bytes()),
            name: Spot::name_hash(drop.as_bytes()),
            at: at.map(|v| v.round() as i16),
            placed,
            twin: match after {
                After::Twin(twin) => *twin,
                _ => 0,
            },
        });
    }
    Ok(out)
}

/// The GameData entries the hook reads `spots` from: two words each, at
/// [`Spot::key`].
pub fn entries(spots: &[Spot]) -> Vec<(u32, u64)> {
    (spots.iter().zip(0..))
        .flat_map(|(s, i)| (0..).zip(s.words()).map(move |(w, v)| (Spot::key(i, w), v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ounce_formats::BymlDoc;

    use super::*;
    use crate::game::dev_dump;
    use crate::gamedata;
    use crate::generate::Spot as Row;
    use crate::known::Name;
    use crate::known::{actor, item};
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

    /// Every one of `pieces` dealt something else than its own item.
    fn moved(pieces: &[ItemCheck]) -> HashMap<u64, Drop> {
        let other = |_| Drop::from("Obj_FakeDrop".to_string());
        pieces.iter().map(|c| (c.hash, other(c))).collect()
    }

    fn at(holder: &Name, hash: u64) -> Row {
        Row {
            hash,
            holder: holder.get().into(),
            item: "Obj_FakeDrop".into(),
            ..Default::default()
        }
    }

    fn pool() -> (ounce_formats::Romfs, Vec<ItemCheck>) {
        let romfs = dev_dump().unwrap();
        let mut new_game = NewGame::rings();
        new_game.pots = true;
        new_game.crates = true;
        new_game.rocks = true;
        new_game.grass = true;
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
        let holders = holders(&pieces, &moved(&pieces));
        let skulltulas = pickups::SKULLTULAS.checks(&dev_dump().unwrap()).unwrap();
        let skulltulas: BTreeSet<u64> = skulltulas.iter().map(|s| s.hash).collect();
        assert_eq!(skulltulas.len(), 100);
        // 518 pots and 23 flying ones, 38 crates, 167 rocks, 160 shrubs and
        // 21 hives, then the Skulltulas, the 63 boulders and 42 silver rocks.
        assert_eq!(holders.len(), 927 + 100 + 63 + 42);
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
        let alone: Vec<u64> = holders
            .iter()
            .filter(|h| h.after == After::Alone)
            .map(|h| h.hash)
            .collect();
        assert_eq!(alone, NO_TWIN);
        assert!(holders.iter().all(|h| h.after != After::Deleted));
        assert_eq!(
            flag_at(&at(actor::OBJ_LIFT_ROCK_A_01, ROCK)),
            Some(flag(ROCK))
        );
        let skulltula = *skulltulas.first().unwrap();
        let spot = at(actor::ENEMY_SKULLTULA_GOLD, skulltula);
        assert_eq!(flag_at(&spot), Some(flag(skulltula)));
    }

    #[test]
    fn a_dealt_breakable_stands_marked_and_its_twin_and_the_undealt_stay_vanilla() {
        use pickups::{CRATES, FLYING_POTS, GRASS, HIVES, POTS, ROCKS};
        let Some(romfs) = dev_dump() else { return };
        for kind in [&POTS, &FLYING_POTS, &CRATES, &ROCKS, &GRASS, &HIVES] {
            let pieces = kind.checks(&romfs).unwrap();
            // One dealt, the rest of its file as if their kind were off.
            let dealt = &pieces[..1];
            let holders = holders(dealt, &moved(dealt));
            let mut doc = checks::load_banc(&romfs, &dealt[0].file).unwrap();
            place(&mut doc.root, &holders).unwrap();
            let actors = checks::actors(&doc.root).unwrap();
            let gyaml = |hash: u64| {
                let a = actors.iter().find(|a| hash_of(a) == Some(hash)).unwrap();
                checks::str_field(a, checks::GYAML).unwrap().to_owned()
            };
            let host = &dealt[0].holder;
            assert_eq!(gyaml(dealt[0].hash), crate::marks::marked(host).unwrap());
            if let After::Twin(twin) = holders[0].after {
                assert_eq!(&gyaml(twin), host);
            }
            let undealt = pieces[1..].iter().filter(|c| c.file == dealt[0].file);
            for other in undealt {
                assert_eq!(gyaml(other.hash), other.holder, "{}", other.hash);
            }
        }
    }

    #[test]
    fn a_key_lying_in_place_dealt_a_line_stands_until_picked() {
        let Some(romfs) = dev_dump() else { return };
        let mut pieces = pickups::LOOSE_KEYS.checks(&romfs).unwrap();
        pieces.extend(pickups::FORTRESS_KEYS.checks(&romfs).unwrap());
        let hookshot = |c: &ItemCheck| (c.hash, Drop::from(item::HOOKSHOT.to_string()));
        let drops = pieces.iter().map(hookshot).collect();
        let holders = holders(&pieces, &drops);
        assert_eq!(holders.len(), 2 + 4);
        assert!(
            holders
                .iter()
                .all(|h| matches!(h.after, After::Taken(None)))
        );
    }

    #[test]
    fn a_skulltula_keeping_its_token_stays_vanilla() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::SKULLTULAS.checks(&romfs).unwrap();
        let kept = &pieces[..1];
        let mut drops = moved(&pieces);
        drops.remove(&kept[0].hash);
        let holders = holders(&pieces, &drops);
        assert_eq!(holders.len(), 100 - 1);
        assert!(holders.iter().all(|h| h.hash != kept[0].hash));
        let token = Row {
            vanilla: kept[0].item.clone(),
            item: kept[0].item.clone(),
            ..at(actor::ENEMY_SKULLTULA_GOLD, kept[0].hash)
        };
        assert_eq!(flag_at(&token), None);
        // Its placement keeps its vanilla gate, if any.
        let mut doc = checks::load_banc(&romfs, &kept[0].file).unwrap();
        let before = doc.root.clone();
        place(&mut doc.root, &holders).unwrap();
        let find = |root: &Byml| {
            let actors = checks::actors(root).unwrap();
            let a = actors.iter().find(|a| hash_of(a) == Some(kept[0].hash));
            a.map(|a| presence_of(a).map(|p| (p.flag, p.negated)))
        };
        assert_eq!(find(&doc.root), find(&before));
    }

    /// Goron City's Deku Stick pot stands until picked with no twin beside
    /// it: a twin hung the city's load (issue #458).
    #[test]
    fn the_goron_city_stick_pot_stands_alone() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::POTS.checks(&romfs).unwrap();
        let holders = holders(&pieces, &moved(&pieces));
        let pot = pieces.iter().find(|c| c.hash == NO_TWIN[0]).unwrap();
        let mut doc = checks::load_banc(&romfs, &pot.file).unwrap();
        let before = checks::actors(&doc.root).unwrap().len();
        place(&mut doc.root, &holders).unwrap();
        let actors = checks::actors(&doc.root).unwrap();
        let twins = holders.iter().filter(|h| {
            let file = pieces.iter().find(|c| c.hash == h.hash).map(|c| &c.file);
            matches!(h.after, After::Twin(_)) && file == Some(&pot.file)
        });
        assert_eq!(actors.len(), before + twins.count());
        let placed = actors
            .iter()
            .find(|a| hash_of(a) == Some(pot.hash))
            .unwrap();
        let gate = presence_of(placed).unwrap();
        assert_eq!((gate.flag, gate.negated), (got(pot.hash), true));
        assert!(pairs(&holders).iter().all(|(h, _)| *h != pot.hash));
    }

    #[test]
    fn a_skulltula_stands_until_picked_and_a_bean_one_until_planted_too() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = pickups::SKULLTULAS.checks(&romfs).unwrap();
        let holders = holders(&pieces, &moved(&pieces));
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
                true => assert_eq!((gate.flag, gate.negated), (got(h.hash), true)),
                false => assert_eq!((gate.flag, gate.negated), (shown(h.hash), false)),
            }
        }
        let stand_in = "Obj_FakeStandIn_001";
        let exps = exps(&holders, &[(bean.hash, stand_in.into())]);
        assert_eq!(exps.len(), holders.len() + 10);
        let exp = |name: &str| exps.iter().find(|(n, _)| *n == name).unwrap().clone();
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH).unwrap()).unwrap();
        let taken = crate::pickups::taken::flag(stand_in);
        gamedata::add_bools(&mut doc.root, &[flag(bean.hash), taken.clone()]).unwrap();
        let (got, shown) = (exp(&got(bean.hash)), exp(&shown(bean.hash)));
        gamedata::add_exps(&mut doc.root, &[got.clone(), shown.clone()]).unwrap();
        let gd = gamedata::GameData::parse(&doc.to_binary()).unwrap();
        let name_of = |n: &str| gd.name(murmur3(n.as_bytes()));
        let picked_or_taken = format!("{} || {}", name_of(&flag(bean.hash)), name_of(&taken));
        assert_eq!(gd.expr(&got.0).unwrap(), Some(picked_or_taken));
        let expr = format!("{} && !{}", name_of(&soil.flag), name_of(&got.0));
        assert_eq!(gd.expr(&shown.0).unwrap(), Some(expr));
    }

    #[test]
    fn a_holder_stands_until_picked_and_its_vanilla_twin_after() {
        let Some(_) = dev_dump() else { return };
        let (romfs, pieces) = pool();
        let holders = holders(&pieces, &moved(&pieces));
        let rock = pieces.iter().find(|c| ROCKS.holds(c)).unwrap();
        let rock_hash = rock.hash;
        let mut doc = checks::load_banc(&romfs, &rock.file).unwrap();
        let before = checks::actors(&doc.root).unwrap().len();
        // A boulder stays as it is: its seat stands until picked (`seats`).
        let mine = pieces
            .iter()
            .filter(|c| c.file == rock.file && !seats::holds(c))
            .filter(|c| holders.iter().any(|h| h.hash == c.hash))
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
        let flag = got(rock_hash);
        assert_eq!((a.flag.as_str(), a.negated), (flag.as_str(), true));
        assert_eq!((b.flag.as_str(), b.negated), (flag.as_str(), false));
        // Its drop or its drop table, whichever it's placed with.
        let dynamic = |a: &Byml| a.as_map().unwrap()[crate::checks::DYNAMIC].clone();
        assert_eq!(dynamic(held), dynamic(copy));
        let id = |a: &Byml| a.as_map().unwrap()["Phive"].as_map().unwrap()["Placement"].clone();
        assert_eq!(id(copy).as_map().unwrap()["ID"], Byml::U64(twin));
    }

    #[test]
    fn a_pot_gone_once_crushed_stands_until_picked_instead() {
        let Some(_) = dev_dump() else { return };
        let (romfs, pieces) = pool();
        let holders = holders(&pieces, &moved(&pieces));
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
            assert_eq!((gate.flag, gate.negated), (got(hash), true));
        }
    }

    #[test]
    fn a_holder_lists_its_flag_its_shuffled_drop_and_where_it_stands() {
        let Some(_) = dev_dump() else { return };
        let (romfs, pieces) = pool();
        let holders = holders(&pieces, &moved(&pieces));
        let rock = pieces.iter().find(|c| ROCKS.holds(c)).unwrap();
        let mut doc = checks::load_banc(&romfs, &rock.file).unwrap();
        let field = format!(
            "{}.Dynamic.{}=Obj_FakeStandIn_001",
            rock.hash,
            DROP_ACTOR.get()
        );
        place(&mut doc.root, &holders).unwrap();
        checks::set_fields(&mut doc.root, &[field]).unwrap();
        let spots = spots(&doc.root, &holders, &[]).unwrap();
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
        // Its take breaks its twin along with it.
        let rock_holder = holders.iter().find(|h| h.hash == rock.hash).unwrap();
        assert_eq!(After::Twin(mine.twin), rock_holder.after);
        assert_eq!(mine.placed, rock.hash);
        // Twins aren't listed: one spot per holder in the file. A boulder's
        // is its seat's, placed with the seats.
        let here = holders
            .iter()
            .filter(|h| h.after != After::Seated)
            .filter(|h| {
                pieces
                    .iter()
                    .any(|c| c.hash == h.hash && c.file == rock.file)
            })
            .count();
        assert_eq!(spots.len(), here);
    }

    #[test]
    fn each_spot_is_four_entries_the_hook_reads_back_by_index() {
        let spot = |flag| Spot {
            flag,
            name: 7,
            at: [-3, 0, 900],
            placed: u64::MAX - u64::from(flag),
            twin: u64::from(flag) << 40,
        };
        let spots = [spot(1), spot(2)];
        let entries = entries(&spots);
        let keys: Vec<u32> = entries.iter().map(|e| e.0).collect();
        assert_eq!(
            keys,
            (0..8).map(|k| zootr_hook::SPOT_KEY + k).collect::<Vec<_>>()
        );
        for (i, s) in (0..).zip(spots) {
            let word = |w| entries.iter().find(|e| e.0 == Spot::key(i, w)).unwrap().1;
            assert_eq!(Spot::from_words([0, 1, 2, 3].map(word)), s);
        }
    }

    #[test]
    fn rutos_letter_goes_once_picked_not_once_link_has_the_letter() {
        let Some(romfs) = dev_dump() else { return };
        let pieces = RUTOS_LETTER.checks(&romfs).unwrap();
        let holders = holders(&pieces, &moved(&pieces));
        assert_eq!(holders.len(), 1);
        assert_eq!(
            (holders[0].hash, &holders[0].after),
            (LETTER, &After::Deleted)
        );
        let letter = at(actor::OBJ_BOTTLE_LETTER, LETTER);
        assert_eq!(flag_at(&letter), Some(flag(LETTER)));
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
        let spots = spots(&doc.root, &holders, &[]).unwrap();
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
