//! Writes a seed's drops, new-game edits and flows into a mod.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Result, bail, ensure};
use ounce_formats::{BymlDoc, ModBuilder, Romfs, par_map, threads};

use super::Written;
use crate::checks::{self, Drop};
use crate::events::Flows;
use crate::flows::Rewrites;
use crate::hints::Hint;
use crate::narrowing::maps_move;
use crate::progress::Run;
use crate::settings::{AGELESS_WEAPONS, ChestSize, DungeonEntrances, NewGame, TRADE_CHAIN};
use crate::world::Era;
use crate::{
    ageless, ammo, blue_warps, bombchus, bombs, bridge, chest_levels, cows, cutscenes, dampe,
    day_skulltulas, dig, door, enemies, entrances, flags, foreign, fortress, gamedata, gerudo,
    gifts, held, hints, ice_traps, key_tints, keys, map_copies, medallions, navi, notice, patch,
    picked, pickups, presets, rsdb, scrubs, shops, spawn, standins, starting, steps, text, tracker,
    trade_chain, twins,
};

/// The flows `new_game` and the gifts in `drops` rewrite, and the GameData
/// edits the skips need.
pub(super) fn flows(
    romfs: &Romfs,
    all: &Flows,
    new_game: &NewGame,
    drops: &HashMap<u64, Drop>,
) -> Result<(Rewrites, Vec<String>)> {
    let (mut rewrites, mut flags) = cutscenes::skip(romfs, all, &new_game.skips)?;
    if new_game.quiet_navi {
        navi::quiet(romfs, &mut rewrites)?;
    }
    if new_game.bridge_medallions {
        bridge::keep_step(romfs, &mut rewrites)?;
        flags.push(bridge::start());
    }
    if new_game.hints {
        hints::quiet(romfs, &mut rewrites)?;
    }
    flags.push(starting::navi_guide());
    trade_chain::cut_timers(romfs, &mut rewrites)?;
    // Rauru's gift, the Light Medallion or what was dealt there, comes in
    // the opening with the door open: `gifts::give_all`.
    if new_game.open_door {
        flags.extend(door::open(
            romfs,
            &mut rewrites,
            new_game.age == Era::Adult,
        )?);
    }
    let pouch = crate::inventory::Pouch::load(romfs)?;
    let grants = starting::with_gives(new_game.start.grants(&pouch), &new_game.give);
    starting::grant(romfs, &mut rewrites, &grants)?;
    starting::replay(romfs, &mut rewrites, &new_game.progress)?;
    steps::dev_branch(romfs, &mut rewrites)?;
    if let Some(spawn) = &new_game.spawn {
        spawn::warp(romfs, &mut rewrites, spawn)?;
    }
    if new_game.age == Era::Adult {
        spawn::adult(romfs, &mut rewrites)?;
    }
    if new_game.on.contains(TRADE_CHAIN) {
        trade_chain::deal(romfs, &mut rewrites, drops)?;
    }
    gifts::give_all(
        romfs,
        all,
        &mut rewrites,
        drops,
        new_game.progressive,
        &new_game.on,
    )?;
    levels(romfs, drops, new_game)?.give(romfs, all, &mut rewrites)?;
    if new_game.shop_slots > 0 {
        let slots = shops::slots(romfs)?;
        shops::sold::write(romfs, &mut rewrites, &slots, new_game, drops)?;
        shops::rupees::write(romfs, &mut rewrites)?;
    }
    dampe::give(romfs, &mut rewrites, drops)?;
    gerudo::stand_down(romfs, &mut rewrites, drops)?;
    gerudo::clear_once(romfs, &mut rewrites)?;
    let model = gifts::levels::model(all)?;
    let level = gifts::levels::Level {
        model: &model,
        progressive: new_game.progressive,
    };
    if new_game.scrubs {
        scrubs::write(romfs, &mut rewrites, drops, new_game.scrub_price, level)?;
    }
    if new_game.cows {
        cows::give(romfs, &mut rewrites, &cows::find(romfs)?, drops, level)?;
    }
    let pieces = pickups::dealt(romfs, new_game)?;
    if new_game.fortress_keys.moved() {
        fortress::give(romfs, &mut rewrites, &pieces, drops, new_game.progressive)?;
    }
    picked::delete_letter(romfs, &mut rewrites, &picked::holders(&pieces, drops))?;
    if ice_traps::springs(new_game) {
        ice_traps::flow(romfs, &mut rewrites)?;
        let traps: Vec<&str> = ice_traps::actors(&new_game.foreign).collect();
        ice_traps::one_page(romfs, &mut rewrites, &traps)?;
    }
    // Shuffled doors turn the adult Deku Tree on, so this covers them too.
    if new_game.on.contains(presets::ADULT_DEKU_TREE) {
        blue_warps::keep_age(romfs, &mut rewrites)?;
    }
    if new_game.dungeon_entrances != DungeonEntrances::Off {
        entrances::reveal::flows_for(romfs, &mut rewrites, new_game.dungeon_entrances)?;
    }
    Ok((rewrites, flags))
}

/// Apply `drops` (actor hash -> drop) to every Banc file, `new_game` to
/// GameData and the cutscenes, and `hints` to the gossip stones, and write
/// the changed files as a mod. A stop lands before each Banc file, and
/// leaves the mod as it was.
pub fn write_mod(
    romfs: &Romfs,
    mod_dir: &Path,
    drops: &HashMap<u64, Drop>,
    new_game: &NewGame,
    hints: &[Hint],
    run: &Run,
) -> Result<Written> {
    if let Some(item) = drops
        .values()
        .flat_map(|d| {
            [d.item.as_str()]
                .into_iter()
                .chain(d.swap.iter().flat_map(|s| [s.check, s.then]))
        })
        // Key copies, Red Rupee copies, other players' items and ice traps
        // are written with the mod.
        .find(|i| {
            !keys::is_shuffled(i)
                && !foreign::is_copy(i)
                && ammo::copied(i).is_none()
                && !romfs.root().join(flags::actor_pack(i)).is_file()
        })
    {
        bail!("{item} isn't an actor");
    }
    let mut builder = ModBuilder::new(romfs, mod_dir)?;
    let flows = Flows::load(romfs)?;
    let (rewrites, flags) = self::flows(romfs, &flows, new_game, drops)?;
    for (rel, data) in &rewrites {
        for to in crate::flows::copies(romfs, rel) {
            builder.write(&to, data)?;
        }
    }
    let mut lines = gifts::lines(drops, new_game.progressive);
    lines.push(scrubs::lines(drops));
    lines.extend(scrubs::sold_out(drops));
    let gd = gamedata::GameData::load(romfs)?;
    let slots = match new_game.shop_slots {
        0 => Vec::new(),
        _ => shops::slots(romfs)?,
    };
    let shelves = shops::Shelves::new(&slots, drops, new_game, &gd)?;
    let mut copies = match new_game.shop_slots {
        0 => Vec::new(),
        _ => vec![shops::captions(romfs, &shelves)?],
    };
    let key_copies = (
        new_game.keys.copies(),
        BOSS_COPIES,
        new_game.fortress_keys.copies(),
    );
    // First, so a copy on a shelf takes its caption from its own lines.
    copies.insert(0, foreign::captions(&new_game.foreign));
    let mut key_lines = key_tints::captions(new_game.keys, key_copies.1, key_copies.2);
    if maps_move(new_game) {
        key_lines.new.extend(map_copies::captions());
    }
    copies.insert(0, key_lines);
    copies.push(notice::banners());
    copies.push(notice::senders(&new_game.players));
    text::write(romfs, &mut builder, &lines, &copies, hints)?;
    let cows = match new_game.cows {
        true => cows::find(romfs)?,
        false => Vec::new(),
    };
    // Presets and `--default` edits go last, so they win over a skip's.
    let mut edits = flags;
    edits.extend(new_game.defaults.iter().cloned());
    let pieces = pickups::dealt(romfs, new_game)?;
    let picked = picked::holders(&pieces, drops);
    let steps = pickups::steps::Steps::new(&pieces, drops, new_game.progressive, &picked, &gd)?;
    let mut bools = cows::flags(&cows);
    bools.extend(picked::flags(&picked));
    if new_game.shop_slots > 0 {
        bools.extend(shops::sold::flags(&slots, new_game.shop_slots));
        bools.extend(shelves.bools.iter().cloned());
    }
    bools.extend(gifts::flags(&new_game.on));
    bools.extend(dampe::flags(drops));
    if new_game.quiet_enemies {
        bools.push(enemies::FLAG.into());
    }
    if new_game.fortress_keys.moved() {
        bools.extend(fortress::flags());
    }
    bools.extend(entrances::reveal::flags(new_game.dungeon_entrances));
    let gone = match new_game.scrubs {
        true => {
            bools.extend(scrubs::flags(drops));
            scrubs::gone(drops)
        }
        false => Vec::new(),
    };
    let made = key_tints::made(key_copies.0, key_copies.1, key_copies.2);
    let rupees = ammo::copies(drops.values().map(|d| d.item.as_str()));
    let opened = tracker::chest_flags(&gd)?;
    // Every dealt check, a kept item's too: its map's alternative needs it.
    let dealt: HashSet<u64> = (checks::dealt(romfs, new_game)?.iter())
        .map(|c| c.hash)
        .collect();
    let twins = twins::find(romfs, |h| dealt.contains(&h), opened)?;
    let aliases = twins.iter().filter_map(twins::Twin::alias);
    let mut exps: Vec<_> = aliases
        .map(|(name, of)| (name, vec![gamedata::Op::Bool(of)]))
        .collect();
    exps.extend(picked::exps(&picked));
    exps.extend(shelves.exps.iter().cloned());
    exps.extend(steps.exps.iter().cloned());
    let mut data = BymlDoc::parse(&romfs.read(gamedata::PATH)?)?;
    trade_chain::never_counting(&mut data.root)?;
    if new_game.on.contains(TRADE_CHAIN) {
        trade_chain::by_pouch(&mut data.root)?;
    }
    let defaults = presets::apply(&gd, &mut data.root, &edits)?;
    gamedata::add_bools(&mut data.root, &bools)?;
    gamedata::add_ints(&mut data.root, &new_game.ints)?;
    gamedata::add_exps(&mut data.root, &exps)?;
    let mut keyed = picked::pairs(&picked);
    keyed.extend(&steps.copies);
    gamedata::add_keys(&mut data.root, &keyed)?;
    for (copy, template, _) in &made {
        let names = [copy.clone()];
        gamedata::join_structs(&mut data.root, template.get(), &names, &[gamedata::IS_GET])?;
    }
    let names: Vec<String> = new_game.foreign.iter().map(|f| f.actor.clone()).collect();
    if !names.is_empty() {
        gamedata::join_structs(
            &mut data.root,
            keys::SMALL.get(),
            &names,
            &[gamedata::IS_GET],
        )?;
    }
    if new_game.bridge_medallions {
        bridge::need_medallions(&mut data.root)?;
    }
    let mut posed = HashMap::new();
    let mut shows_off = |e: &str| {
        *posed
            .entry(e.to_owned())
            .or_insert_with(|| flows.shows_off(e))
    };
    let mut fields = new_game.fields.clone();
    let mut events = rsdb::Events::load(romfs)?;
    if new_game.scrubs {
        fields.extend(scrubs::fields(drops));
        scrubs::events(drops, &mut events)?;
    }
    fields.extend(cows::fields(&cows));
    let mut tables = standins::Tables::load(romfs)?;
    // Before the stand-ins, so one holding bombchus copies the freed row.
    if new_game.free_bombchus {
        bombchus::write(&mut builder, &mut tables)?;
    }
    if new_game.on.contains(AGELESS_WEAPONS) {
        ageless::write(romfs, &mut builder)?;
    }
    // Before the stand-ins, which copy a song's model as it's shown.
    let shown = new_game.foreign.iter().map(|f| f.shows.as_str());
    let items = drops.values().map(|d| d.item.as_str()).chain(shown);
    medallions::write(romfs, &mut builder, &mut tables, items)?;
    let flags = hook_flags(new_game);
    let bosses = BOSS_COPIES;
    let fortress = new_game.fortress_keys.copies();
    key_tints::write(
        romfs,
        &mut builder,
        &mut tables,
        bosses,
        new_game.keys,
        fortress,
    )?;
    if maps_move(new_game) {
        map_copies::write(romfs, &mut builder, &mut tables)?;
    }
    foreign::write(romfs, &mut builder, &mut tables, &new_game.foreign)?;
    ammo::write(romfs, &mut builder, &mut tables, &rupees)?;
    cows::write(romfs, &mut builder, &mut tables, &mut events, &cows)?;
    let levels = levels(romfs, drops, new_game)?;
    levels.events(&mut events)?;
    let look = if ice_traps::springs(new_game) {
        ice_traps::event(&mut events)?;
        ice_traps::sensor(romfs, &mut builder, &mut tables)?;
        ice_traps::thaw(romfs, &mut builder)?;
        Some(ice_traps::look(romfs, &mut builder)?)
    } else {
        None
    };
    let blocks = match new_game.dungeon_entrances {
        DungeonEntrances::Off => None,
        mode => {
            let mut blocks = entrances::Blocks::read(romfs)?;
            let arrivals = entrances::reveal::plan(&blocks, mode);
            entrances::reveal::events(&arrivals, &mut events)?;
            entrances::reveal::retarget(&mut blocks, &arrivals)?;
            Some(blocks)
        }
    };
    events.write(&mut builder)?;
    if new_game.shop_slots > 0 {
        fields.extend(shops::write(
            romfs,
            &mut builder,
            &mut tables,
            &shelves,
            new_game,
        )?);
    }
    if new_game.day_skulltulas {
        day_skulltulas::write(romfs, &mut builder)?;
    }
    fields.extend(pickups::write(
        romfs,
        &mut builder,
        &mut tables,
        &pieces,
        drops,
        new_game,
    )?);
    if new_game.fortress_keys.moved() {
        fields.extend(fortress::fields(
            romfs,
            &pieces,
            drops,
            new_game.progressive,
        )?);
    }
    steps.write(romfs, &mut builder)?;
    held::write(romfs, &mut builder, &mut tables)?;
    tables.write(&mut builder)?;
    // A pickup's drop is its field above, never the bare item. A cow's is
    // its flow's grant.
    let chest_drops: HashMap<u64, Drop> = drops
        .iter()
        .filter(|(h, _)| !pieces.iter().any(|p| p.hash == **h))
        .filter(|(h, _)| !cows.iter().any(|c| c.hash() == **h))
        .map(|(h, d)| (*h, d.clone()))
        .collect();
    let carried = match new_game.fortress_keys.moved() {
        true => &fortress::CARRYING[..],
        false => &[],
    };
    // Each copy of a holder quiet groups make or watch, its steps' and its
    // picked twin, by the holder: the groups run it too.
    let quiet: HashSet<u64> = (pieces.iter())
        .filter(|p| p.watched == checks::Watched::Quiet)
        .map(|p| p.hash)
        .collect();
    let mut ran = picked::pairs(&picked);
    ran.extend(steps.copies.iter().copied());
    // A loud holder's copies get nodes of their own instead.
    let loud: Vec<(u64, u64)> = (ran.iter().copied())
        .filter(|(of, _)| {
            pieces
                .iter()
                .any(|p| p.hash == *of && p.watched == checks::Watched::Loud)
        })
        .collect();
    ran.retain(|(of, _)| quiet.contains(of));
    let mut wiring = pickups::nodes::Wiring::default();
    let creators = match loud.is_empty() {
        true => HashMap::new(),
        false => pickups::nodes::creators(romfs)?,
    };
    let mut tabled = pickups::tabled(&pieces);
    // A step's copy is placed with its holder's table, which goes too.
    let copied = steps.copies.iter().filter(|(of, _)| tabled.contains(of));
    tabled.extend(copied.map(|(_, copy)| *copy).collect::<Vec<_>>());
    let (mut found, mut actors) = (HashSet::new(), 0);
    let mut spots = Vec::new();
    let mut chest_events = HashMap::new();
    // Changed maps wait here to be serialized a batch at a time.
    let quiet = match new_game.quiet_enemies {
        true => Some(enemies::kept(romfs, &dealt)?),
        false => None,
    };
    let mut changed_maps = Vec::new();
    for (rel, doc) in checks::load_bancs(romfs, checks::banc_files(romfs)?) {
        run.check()?;
        let mut doc = doc?;
        // First, so a check placed in its twin's map takes its drop below.
        let twinned = twins::place(&rel, &mut doc.root, &twins)? > 0;
        // Before the fields, so each twin keeps its holder's vanilla drop.
        let paired = picked::place(&mut doc.root, &picked)? > 0;
        // After the twins, so each step is a copy of a holder standing until
        // picked, and before the fields, which give each step its stand-in.
        let stepped = steps.place(&mut doc.root)?;
        let grouped = pickups::groups::copy(&mut doc.root, &ran)?
            | wiring.refer(&mut doc.root, &loud, &creators)?;
        // Before the fields, which give each step's copy its stand-in.
        let shelved = shelves.place(&mut doc.root)?;
        let mut changed = checks::set_fields(&mut doc.root, &fields)?;
        spots.extend(picked::spots(&doc.root, &picked, &steps.copies)?);
        changed.extend(checks::gone_once(&mut doc.root, &gone)?);
        let leaves = scrubs::leaves(&doc.root, &gone)?;
        changed.extend(checks::gone_once(&mut doc.root, &leaves)?);
        if let Some(kept) = &quiet {
            changed.extend(enemies::hide(&mut doc.root, kept)?);
        }
        changed.extend(checks::clear_drops(&mut doc.root, carried)?);
        changed.extend(checks::clear_tables(&mut doc.root, &tabled)?);
        changed.extend(checks::set_drops(
            &mut doc.root,
            &chest_drops,
            &mut shows_off,
        )?);
        let shrunk =
            new_game.chest_size == ChestSize::Small && checks::shrink_chests(&mut doc.root)? > 0;
        let tinted = key_tints::swap(&rel, &mut doc.root, bosses)? > 0;
        // After shrinking: a shrunk chest runs the small chest's flow.
        let levelled = levels.name(romfs, &mut doc.root, &mut chest_events)? > 0;
        let doors = match &blocks {
            Some(b) => entrances::rewrite(&rel, &mut doc.root, &new_game.doors, b)? > 0,
            None => false,
        };
        if twinned
            || paired
            || stepped
            || grouped
            || shelved
            || shrunk
            || tinted
            || levelled
            || doors
            || !changed.is_empty()
        {
            actors += changed.len();
            found.extend(changed);
            changed_maps.push((rel, doc));
            if changed_maps.len() >= 2 * threads() {
                write_maps(&mut builder, &mut changed_maps)?;
            }
        }
    }
    write_maps(&mut builder, &mut changed_maps)?;
    wiring.write(romfs, &mut builder)?;
    for f in &fields {
        let (hash, ..) = checks::field(f)?;
        ensure!(found.contains(&hash), "no actor with hash {hash} for {f}");
    }
    // Checks that aren't a placed actor's drop.
    let gift = |h: &u64| {
        gifts::GIFTS.iter().any(|g| g.hash() == *h)
            || scrubs::SCRUBS.iter().any(|s| s.hash() == *h)
            || pieces.iter().any(|p| p.hash == *h)
            || cows.iter().any(|c| c.hash() == *h)
    };
    if let Some(missing) = drops.keys().find(|h| !found.contains(*h) && !gift(h)) {
        bail!("no actor with hash {missing} has a {}", checks::DROP_ACTOR);
    }
    // Last, once every holder's spot is known.
    gamedata::add_u64s(&mut data.root, &picked::entries(&spots))?;
    builder.write(gamedata::PATH, &data.to_binary())?;
    bombs::write(&mut builder)?;
    dig::write(romfs, &mut builder, drops, new_game)?;
    run.check()?;
    let files = builder.finish()?.len();
    let credits = credits(new_game);
    let keyed: Vec<_> = (rupees.iter())
        .filter_map(|r| ammo::line(ammo::copied(r)?))
        .collect();
    patch::install(
        mod_dir,
        new_game.progressive,
        &keyed,
        flags,
        &credits,
        &spots,
        look.as_ref(),
    )?;
    Ok(Written {
        defaults,
        actors,
        files,
    })
}

/// Write each of `maps`, serialized on every core, in order, and empty it.
fn write_maps(builder: &mut ModBuilder, maps: &mut Vec<(String, BymlDoc)>) -> Result<()> {
    let bins = par_map(maps, |(_, doc)| doc.to_binary());
    for ((rel, _), bin) in maps.drain(..).zip(bins) {
        builder.write(&rel, &bin)?;
    }
    Ok(())
}

/// Every seed deals boss key copies. A copy found in a chest that runs a
/// scene of its own is credited by the hook, which every seed carries.
const BOSS_COPIES: bool = true;

/// The items `drops` give through a chest's own copy of its event.
fn levels(
    romfs: &Romfs,
    drops: &HashMap<u64, Drop>,
    new_game: &NewGame,
) -> Result<chest_levels::Levels> {
    chest_levels::Levels::new(romfs, drops, new_game.progressive, sets(new_game))
}

/// What each copy `new_game` deals sets as it's found: its key copies',
/// and with maps and compasses moving theirs.
fn sets(new_game: &NewGame) -> Vec<(String, key_tints::Sets)> {
    let fortress = new_game.fortress_keys.copies();
    let mut sets = key_tints::sets(BOSS_COPIES, new_game.keys, fortress);
    if maps_move(new_game) {
        sets.extend(map_copies::sets());
    }
    sets
}

/// [`sets`] as the hook's table names it, then each ice trap's freeze.
fn credits(new_game: &NewGame) -> Vec<(String, zootr_hook::Credit)> {
    let mut credits = key_tints::as_credits(sets(new_game));
    credits.extend(ice_traps::credits(&new_game.foreign));
    credits
}

/// The settings the hook acts on, as the table's flag bits. Every seed
/// lets Link pick what the trade slot shows.
fn hook_flags(new_game: &NewGame) -> u32 {
    [
        (new_game.free_bombchus, zootr_hook::FREE_BOMBCHUS),
        (new_game.bunny_hood, zootr_hook::BUNNY_HOOD),
        (ice_traps::springs(new_game), zootr_hook::ICE_TRAPS),
        (true, zootr_hook::TRADE_CYCLE),
    ]
    .into_iter()
    .filter_map(|(on, bit)| on.then_some(bit))
    .fold(0, |all, bit| all | bit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zootr_hook::{BUNNY_HOOD, FREE_BOMBCHUS, TRADE_CYCLE};

    #[test]
    fn each_hook_setting_sets_its_own_bit() {
        assert_eq!(FREE_BOMBCHUS & BUNNY_HOOD & TRADE_CYCLE, 0);
        let flags = |free_bombchus, bunny_hood| {
            hook_flags(&NewGame {
                free_bombchus,
                bunny_hood,
                ..Default::default()
            })
        };
        assert_eq!(flags(false, false), TRADE_CYCLE);
        assert_eq!(flags(true, false), FREE_BOMBCHUS | TRADE_CYCLE);
        assert_eq!(flags(false, true), BUNNY_HOOD | TRADE_CYCLE);
        assert_eq!(flags(true, true), FREE_BOMBCHUS | BUNNY_HOOD | TRADE_CYCLE);
    }
}
