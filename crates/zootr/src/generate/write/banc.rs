//! The pass over every map: each edit the seed makes to its placed actors.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail, ensure};
use ounce_formats::{BymlDoc, ModBuilder, par_map, threads};
use zootr_hook::Spot;

use super::{BOSS_COPIES, Parts};
use crate::checks::{self, Drop, Watched};
use crate::events::Flows;
use crate::{
    adult_jabu, carpenters, chest_looks, chest_size, enemies, entrances, fortress, gifts,
    goron_pot, jabu_mood, key_tints, picked, pickups, scrubs, shops, twins, water_temple,
};

/// The map pass's inputs, worked out once, and what it gathers map by map.
pub(super) struct Banc<'p, 'a> {
    parts: &'p Parts<'a>,
    all: &'p Flows,
    shelves: &'p shops::Shelves<'a>,
    fields: &'p [String],
    /// Whether each event shows its item off, asked once per event.
    posed: HashMap<String, bool>,
    /// The drops placed as a chest's. A pickup's drop is its field, never
    /// the bare item. A cow's is its flow's grant, the Goron pot's its held
    /// stand-in.
    chest_drops: HashMap<u64, Drop>,
    /// The drops carried by holders whose items moved, to clear.
    carried: &'static [u64],
    /// Each copy of a holder quiet groups make or watch, its steps' and its
    /// picked twin, by the holder: the groups run it too.
    ran: Vec<(u64, u64)>,
    /// A loud holder's copies, which get nodes of their own instead.
    loud: Vec<(u64, u64)>,
    creators: HashMap<u64, Vec<(String, usize)>>,
    /// Each step copy that stands by its module's own gate instead, so it gets
    /// no name in the group and no node (`pickups::nodes::gates`).
    unbound: HashSet<u64>,
    wiring: pickups::nodes::Wiring,
    /// The holders whose tables go, a step's copy's with its holder's.
    tabled: HashSet<u64>,
    /// With the enemies quiet, the ones kept.
    kept: Option<HashSet<u64>>,
    chest_events: HashMap<String, Option<String>>,
    /// Changed maps waiting to be serialized a batch at a time.
    changed_maps: Vec<(String, BymlDoc)>,
    /// The carpenters' bridge actors placed, to check once at the end.
    valley: usize,
    found: HashSet<u64>,
    actors: usize,
    spots: Vec<Spot>,
}

impl<'p, 'a> Banc<'p, 'a> {
    pub(super) fn new(
        parts: &'p Parts<'a>,
        all: &'p Flows,
        shelves: &'p shops::Shelves<'a>,
        fields: &'p [String],
    ) -> Result<Self> {
        let (romfs, pieces, steps) = (parts.romfs, &parts.reads.pieces, &parts.steps);
        let cows = &parts.reads.cows;
        let chest_drops = (parts.drops.iter())
            .filter(|(h, _)| !pieces.iter().any(|p| p.hash == **h))
            .filter(|(h, _)| !cows.iter().any(|c| c.hash() == **h))
            .filter(|(h, _)| **h != goron_pot::HASH)
            .map(|(h, d)| (*h, d.clone()))
            .collect();
        let quiet: HashSet<u64> = (pieces.iter())
            .filter(|p| p.watched == Watched::Quiet)
            .map(|p| p.hash)
            .collect();
        let mut ran = picked::pairs(&parts.picked);
        ran.extend(steps.copies.iter().copied());
        let loud: Vec<(u64, u64)> = (ran.iter().copied())
            .filter(|(of, _)| (pieces.iter()).any(|p| p.hash == *of && p.watched == Watched::Loud))
            .collect();
        ran.retain(|(of, _)| quiet.contains(of));
        let creators = match loud.is_empty() {
            true => HashMap::new(),
            false => pickups::nodes::creators(romfs)?,
        };
        // A copy of a holder that lies until picked, whose module's gate a
        // program can read, stands by that gate: naming it in the module's
        // group or duplicating its node hung the map's load (issue #539).
        let unbound: HashSet<u64> = (steps.copies.iter())
            .filter(|(of, _)| parts.gates.contains_key(of))
            .map(|(_, copy)| *copy)
            .collect();
        let mut tabled = pickups::tabled(pieces);
        // A step's copy is placed with its holder's table, which goes too.
        let copied = steps.copies.iter().filter(|(of, _)| tabled.contains(of));
        tabled.extend(copied.map(|(_, copy)| *copy).collect::<Vec<_>>());
        Ok(Self {
            parts,
            all,
            shelves,
            fields,
            posed: HashMap::new(),
            chest_drops,
            carried: match parts.new_game.fortress_keys.moved() {
                true => &fortress::CARRYING,
                false => &[],
            },
            ran,
            loud,
            creators,
            unbound,
            wiring: pickups::nodes::Wiring::default(),
            tabled,
            kept: match parts.new_game.quiet_enemies {
                true => Some(enemies::kept(romfs, &parts.dealt)?),
                false => None,
            },
            chest_events: HashMap::new(),
            changed_maps: Vec::new(),
            valley: 0,
            found: HashSet::new(),
            actors: 0,
            spots: Vec::new(),
        })
    }

    /// Make every edit to map `rel`, and keep it to write if it changed.
    pub(super) fn edit(
        &mut self,
        builder: &mut ModBuilder,
        rel: String,
        mut doc: BymlDoc,
    ) -> Result<()> {
        let parts = self.parts;
        let (romfs, new_game) = (parts.romfs, parts.new_game);
        let root = &mut doc.root;
        // Before every edit below, so Jabu's copies in the adult fountain
        // take the same fields and door rewrite as the child's.
        let jabu = adult_jabu::copy(romfs, &rel, root, new_game.adult_jabu)?;
        // First, so a check placed in its twin's map takes its drop below.
        let twinned = twins::place(&rel, root, &parts.twins)? > 0;
        // Before the fields, so each twin keeps its holder's vanilla drop.
        let paired = picked::place(root, &parts.picked)? > 0;
        // Before the step gates and the fields, which give each seat its
        // gate and its pickup.
        let seated = parts.seats.place(root)?;
        // After the twins, so each step is a copy of a holder standing until
        // picked, and before the fields, which give each step its stand-in.
        let stepped = parts.steps.place(root)?;
        let grouped = pickups::groups::copy(root, &self.ran)?
            | self
                .wiring
                .refer(root, &self.loud, &self.creators, &self.unbound)?;
        // Before the fields, which give each step's copy its stand-in.
        let shelved = self.shelves.place(root)?;
        let mut changed = checks::set_fields(root, self.fields)?;
        let copies = &parts.copies;
        self.spots
            .extend(picked::spots(root, &parts.picked, copies)?);
        changed.extend(checks::gone_once(root, &parts.gone)?);
        let leaves = scrubs::leaves(root, &parts.gone)?;
        changed.extend(checks::gone_once(root, &leaves)?);
        if let Some(kept) = &self.kept {
            changed.extend(enemies::hide(root, kept)?);
        }
        // The wall over the temple's mouth, so the entrance is reachable the
        // way its logic rule already says it is (`water_temple`).
        let mouth = water_temple::open(root)?;
        changed.extend(checks::clear_drops(root, self.carried)?);
        changed.extend(checks::clear_tables(root, &self.tabled)?);
        let (all, posed) = (self.all, &mut self.posed);
        let shows_off = |e: &str| {
            *posed
                .entry(e.to_owned())
                .or_insert_with(|| all.shows_off(e))
        };
        changed.extend(checks::set_drops(root, &self.chest_drops, shows_off)?);
        let resize = |size, drop: &str| chest_size::resized(new_game, size, drop);
        let resized = checks::resize_chests(root, resize)? > 0;
        let tinted = key_tints::swap(&rel, root, BOSS_COPIES)? > 0;
        // After resizing: a resized chest runs its new size's flow.
        let levels = &parts.reads.levels;
        let levelled = levels.name(romfs, root, &mut self.chest_events)? > 0;
        // Last: everything above reads a chest by its host's name.
        let looked = chest_looks::swap(&rel, root, new_game)? > 0;
        let bridged = carpenters::banc(&rel, root)?;
        self.valley += bridged;
        let doors = match &parts.reads.doors {
            Some((b, _)) => entrances::rewrite(&rel, root, &new_game.doors, b)? > 0,
            None => false,
        };
        if jabu
            || twinned
            || paired
            || seated
            || stepped
            || grouped
            || shelved
            || resized
            || tinted
            || levelled
            || looked
            || doors
            || mouth
            || bridged > 0
            || !changed.is_empty()
        {
            self.actors += changed.len();
            self.found.extend(changed);
            self.changed_maps.push((rel, doc));
            if self.changed_maps.len() >= 2 * threads() {
                write_maps(builder, &mut self.changed_maps)?;
            }
        }
        Ok(())
    }

    /// Write the maps still waiting and the files the pass gathered, and
    /// check every field and drop found its actor. Returns how many
    /// actors' drops changed, and each holder's spot.
    pub(super) fn finish(mut self, builder: &mut ModBuilder) -> Result<(usize, Vec<Spot>)> {
        let parts = self.parts;
        let (romfs, adult_jabu) = (parts.romfs, parts.new_game.adult_jabu);
        write_maps(builder, &mut self.changed_maps)?;
        carpenters::check_banc(self.valley)?;
        carpenters::write(romfs, builder)?;
        adult_jabu::arrival(romfs, builder, adult_jabu)?;
        adult_jabu::strip_foliage(romfs, builder, adult_jabu)?;
        jabu_mood::write(romfs, builder, adult_jabu)?;
        self.wiring.write(romfs, builder)?;
        for f in self.fields {
            let hash = checks::parse_field(f)?.hash;
            ensure!(
                self.found.contains(&hash),
                "no actor with hash {hash} for {f}"
            );
        }
        // Checks that aren't a placed actor's drop.
        let gift = |h: &u64| {
            gifts::GIFTS.iter().any(|g| g.hash() == *h)
                || scrubs::SCRUBS.iter().any(|s| s.hash() == *h)
                || parts.reads.pieces.iter().any(|p| p.hash == *h)
                || parts.reads.cows.iter().any(|c| c.hash() == *h)
                || *h == goron_pot::HASH
        };
        let found = &self.found;
        if let Some(missing) = parts.drops.keys().find(|h| !found.contains(*h) && !gift(h)) {
            bail!("no actor with hash {missing} has a {}", checks::DROP_ACTOR);
        }
        Ok((self.actors, self.spots))
    }
}

/// Write each of `maps`, serialized on every core, in order, and empty it.
fn write_maps(builder: &mut ModBuilder, maps: &mut Vec<(String, BymlDoc)>) -> Result<()> {
    let bins = par_map(maps, |(_, doc)| doc.to_binary());
    for ((rel, _), bin) in maps.drain(..).zip(bins) {
        builder.write(&rel, &bin)?;
    }
    Ok(())
}
