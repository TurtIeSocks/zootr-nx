//! Writes a seed's drops, new-game edits and flows into a mod.

mod banc;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Result, bail};
use ounce_formats::{Byml, BymlDoc, ModBuilder, Romfs, pack_file};
use zootr_hook::look::LookSpec;

use super::Written;
use crate::ageless::toggles::Enabled;
use crate::bombchu_bags;
use crate::checks::{self, Drop, ItemCheck};
use crate::events::Flows;
use crate::flows::Rewrites;
use crate::game::Release;
use crate::gamedata::{GameData, Op};
use crate::hints::Hint;
use crate::narrowing::maps_move;
use crate::progress::Run;
use crate::progressive::Lines;
use crate::settings::{Bombchus, Bridge};
use crate::settings::{DungeonEntrances, NewGame, TRADE_CHAIN};
use crate::starter_bags;
use crate::world::Era;
use crate::{
    ageless, ammo, blue_warps, bombchus, bombs, bridge, carpenters, chest_levels, cows, cutscenes,
    dampe, day_skulltulas, dig, door, enemies, entrances, foreign, fortress, gamedata, gerudo,
    gifts, goron_pot, held, hints, ice_traps, jabu_mood, jabu_mouth, key_tints, keys, map_copies,
    medallions, navi, notice, patch, picked, pickups, presets, rsdb, scrubs, sheik_crater, shops,
    spawn, standins, starting, steps, text, tracker, trade_chain, trap_chests, twins,
};

/// What both the flows and the mod's files need from the dump for a seed,
/// read once.
pub(super) struct Reads {
    /// The cows, when they give what's dealt.
    cows: Vec<cows::Cow>,
    pieces: Vec<ItemCheck>,
    levels: chest_levels::Levels,
    /// With shuffled doors, each dungeon's front door and the arrivals
    /// that mark it.
    doors: Option<(entrances::Blocks, Vec<entrances::reveal::Arrival>)>,
}

impl Reads {
    pub(super) fn load(
        romfs: &Romfs,
        drops: &HashMap<u64, Drop>,
        new_game: &NewGame,
    ) -> Result<Self> {
        let doors = match new_game.dungeon_entrances {
            DungeonEntrances::Off => None,
            mode => {
                let blocks = entrances::Blocks::read(romfs)?;
                let arrivals = entrances::reveal::plan(&blocks, mode);
                Some((blocks, arrivals))
            }
        };
        Ok(Self {
            cows: match new_game.cows {
                true => cows::find(romfs)?,
                false => Vec::new(),
            },
            pieces: pickups::dealt(romfs, new_game)?,
            levels: levels(romfs, drops, new_game)?,
            doors,
        })
    }
}

/// The flows `new_game` and the gifts in `drops` rewrite, and the GameData
/// edits the skips need.
pub(super) fn flows(
    romfs: &Romfs,
    all: &Flows,
    new_game: &NewGame,
    drops: &HashMap<u64, Drop>,
    reads: &Reads,
) -> Result<(Rewrites, Vec<String>)> {
    let (mut rewrites, flags) = opening(romfs, all, new_game)?;
    deals(romfs, all, &mut rewrites, new_game, drops, reads)?;
    Ok((rewrites, flags))
}

/// [`flows`], reading what it needs.
#[cfg(test)]
pub(super) fn flows_alone(
    romfs: &Romfs,
    all: &Flows,
    new_game: &NewGame,
    drops: &HashMap<u64, Drop>,
) -> Result<(Rewrites, Vec<String>)> {
    let reads = Reads::load(romfs, drops, new_game)?;
    flows(romfs, all, new_game, drops, &reads)
}

/// The flows the skips, the starting items and the boot rewrite make, and
/// the GameData edits they need.
fn opening(romfs: &Romfs, all: &Flows, new_game: &NewGame) -> Result<(Rewrites, Vec<String>)> {
    // The opening movie and Navi's wake-up play in Kokiri Forest only.
    let skips = cutscenes::Skips {
        intro: new_game.skips.intro || boot(new_game).is_some(),
        ..new_game.skips.clone()
    };
    let (mut rewrites, mut flags) = cutscenes::skip(romfs, all, &skips)?;
    if new_game.quiet_navi {
        navi::quiet(romfs, &mut rewrites)?;
    }
    if new_game.bridge == Bridge::SixMedallions {
        bridge::keep_step(romfs, &mut rewrites)?;
        flags.push(bridge::start());
    }
    if new_game.hints {
        hints::quiet(romfs, &mut rewrites)?;
    }
    if !new_game.magic_with_first_item {
        crate::magic::quiet(romfs, &mut rewrites)?;
    }
    flags.push(starting::navi_guide());
    flags.extend(gifts::egg::hatched(&new_game.on));
    trade_chain::cut_timers(romfs, &mut rewrites)?;
    crate::masks::write(romfs, new_game, &mut rewrites)?;
    // Rauru's gift, the Light Medallion or what was dealt there, comes in
    // the opening with the door open: `gifts::give_all`.
    if new_game.open_door {
        flags.extend(door::open(
            romfs,
            &mut rewrites,
            new_game.age == Era::Adult,
        )?);
    }
    let pouch = crate::inventory::Pouch::load(romfs)?.seeded(new_game);
    let grants = starting::with_gives(new_game.start.grants(&pouch), &new_game.give);
    starting::grant(romfs, &mut rewrites, &grants)?;
    starting::replay(romfs, &mut rewrites, &new_game.progress)?;
    steps::hold(romfs, &mut rewrites, all, &new_game.progress)?;
    steps::dev_branch(romfs, &mut rewrites)?;
    if let Some(spawn) = boot(new_game) {
        let owned = new_game.start.owned();
        flags.extend(spawn::boot(romfs, all, &mut rewrites, &spawn, &owned)?);
    }
    Ok((rewrites, flags))
}

/// Rewrite the flows that give what `drops` dealt, and those the settings
/// change around them.
fn deals(
    romfs: &Romfs,
    all: &Flows,
    rewrites: &mut Rewrites,
    new_game: &NewGame,
    drops: &HashMap<u64, Drop>,
    reads: &Reads,
) -> Result<()> {
    if new_game.on.contains(TRADE_CHAIN) {
        trade_chain::deal(romfs, rewrites, drops)?;
    }
    gifts::give_all(
        romfs,
        all,
        rewrites,
        drops,
        Lines::of(new_game),
        &new_game.on,
    )?;
    reads.levels.give(romfs, all, rewrites)?;
    if new_game.shop_slots > 0 {
        let slots = shops::slots(romfs)?;
        shops::sold::write(romfs, rewrites, &slots, new_game, drops)?;
        shops::rupees::write(romfs, rewrites)?;
    }
    dampe::give(romfs, rewrites, drops)?;
    if new_game.freestanding {
        goron_pot::rename(romfs, rewrites, drops)?;
    }
    gerudo::stand_down(romfs, rewrites, drops)?;
    carpenters::flows(romfs, rewrites)?;
    gerudo::clear_once(romfs, rewrites)?;
    gerudo::rescue_once(romfs, rewrites)?;
    let model = gifts::levels::model(all)?;
    let level = gifts::levels::Level {
        model: &model,
        lines: Lines::of(new_game),
    };
    if new_game.scrubs {
        scrubs::write(romfs, rewrites, drops, new_game.scrub_price, level)?;
    }
    if new_game.cows {
        cows::give(romfs, rewrites, &reads.cows, drops, level)?;
    }
    if new_game.fortress_keys.moved() {
        fortress::give(romfs, rewrites, &reads.pieces, drops, Lines::of(new_game))?;
    }
    picked::delete_letter(romfs, rewrites, &picked::holders(&reads.pieces, drops))?;
    if ice_traps::springs(new_game) {
        ice_traps::flow(romfs, rewrites)?;
        let traps: Vec<&str> = ice_traps::actors(&new_game.foreign).collect();
        ice_traps::one_page(romfs, rewrites, &traps)?;
    }
    // Shuffled doors turn the adult Deku Tree on, so this covers them too.
    if new_game.on.contains(presets::ADULT_DEKU_TREE) {
        blue_warps::keep_age(romfs, rewrites)?;
    }
    if let Some((_, arrivals)) = &reads.doors {
        entrances::reveal::flows(romfs, rewrites, &rsdb::events(romfs)?, arrivals)?;
    }
    Ok(())
}

/// Where `new_game` boots other than Link's house, as `MAP/SPOT`: the
/// spawn, else the adult start's.
fn boot(new_game: &NewGame) -> Option<String> {
    (new_game.spawn.clone()).or_else(|| (new_game.age == Era::Adult).then(spawn::adult_start))
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
    check_drops(romfs, drops)?;
    let mut builder = ModBuilder::new(romfs, mod_dir)?;
    let all = Flows::load(romfs)?;
    let reads = Reads::load(romfs, drops, new_game)?;
    let (rewrites, flags) = flows(romfs, &all, new_game, drops, &reads)?;
    for (rel, data) in &rewrites {
        for to in crate::flows::copies(romfs, rel) {
            builder.write(&to, data)?;
        }
    }
    let gd = GameData::load(romfs)?;
    let slots = match new_game.shop_slots {
        0 => Vec::new(),
        _ => shops::slots(romfs)?,
    };
    let shelves = shops::Shelves::new(&slots, drops, new_game, &gd)?;
    write_text(romfs, &mut builder, drops, new_game, &shelves, hints)?;
    let mut parts = Parts::new(romfs, drops, new_game, reads, &gd)?;
    // Presets and `--default` edits go last, so they win over a skip's.
    let mut edits = flags;
    edits.extend(new_game.defaults.iter().cloned());
    let mut data = BymlDoc::parse(&romfs.read(gamedata::PATH)?)?;
    let defaults = parts.game_data(&mut data.root, &gd, &slots, &shelves, &edits)?;
    let (fields, look) = parts.assets(&mut builder, &shelves)?;
    let mut maps = banc::Banc::new(&parts, &all, &shelves, &fields)?;
    for (rel, doc) in checks::load_bancs(romfs, checks::banc_files(romfs)?) {
        run.check()?;
        maps.edit(&mut builder, rel, doc?)?;
    }
    let (actors, spots) = maps.finish(&mut builder)?;
    // Last, once every holder's spot is known.
    gamedata::add_u64s(&mut data.root, &picked::entries(&spots))?;
    // Every stand-in is built by now.
    gamedata::add_bools(&mut data.root, &pickups::taken::flags(builder.written()))?;
    builder.write(gamedata::PATH, &data.to_binary())?;
    bombs::write(&mut builder)?;
    dig::write(romfs, &mut builder, drops, new_game)?;
    // After ageless items, which add to the same sound users.
    crate::audio::write(&mut builder, &new_game.audio, new_game.seed)?;
    // Last: a feature copying a texture copies it as the dump has it.
    crate::colors::write(&mut builder, &new_game.colors)?;
    run.check()?;
    let files = builder.finish()?.len();
    run.mark_replaced();
    let keyed: Vec<_> = (parts.rupees.iter())
        .filter_map(|r| ammo::line(ammo::copied(r)?))
        .collect();
    let release = Release::of(romfs.root());
    let table = patch::SeedTable {
        progressive: Lines::of(new_game),
        keyed: &keyed,
        flags: hook_flags(new_game),
        credits: &credits(new_game),
        spots: &spots,
        look: look.as_ref(),
    };
    patch::install(mod_dir, release, &table)?;
    Ok(Written {
        defaults,
        actors,
        files,
        stand_ins: parts.stand_ins,
        release,
    })
}

/// Fail on an item in `drops` the mod can't place: one with no actor that
/// it doesn't write itself.
fn check_drops(romfs: &Romfs, drops: &HashMap<u64, Drop>) -> Result<()> {
    if let Some(item) = drops
        .values()
        .flat_map(|d| {
            [d.item.as_str()]
                .into_iter()
                .chain(d.swap.iter().flat_map(|s| [s.check, s.then]))
        })
        // Key copies, Red Rupee copies, other players' items, ice traps and
        // Bombchu Bags are written with the mod.
        .find(|i| {
            !keys::is_shuffled(i)
                && !foreign::is_copy(i)
                && !crate::known::is_ours(i)
                && ammo::copied(i).is_none()
                && !romfs.root().join(pack_file(i)).is_file()
        })
    {
        bail!("{item} isn't an actor");
    }
    Ok(())
}

/// Write the gifts' and the copies' captions, and `hints` to the gossip
/// stones.
fn write_text(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    drops: &HashMap<u64, Drop>,
    new_game: &NewGame,
    shelves: &shops::Shelves,
    hints: &[Hint],
) -> Result<()> {
    let mut lines = gifts::lines(drops, Lines::of(new_game));
    lines.push(scrubs::lines(drops, Lines::of(new_game)));
    lines.extend(scrubs::sold_out(drops));
    let fortress = new_game.fortress_keys.copies();
    let mut key_lines = key_tints::captions(new_game.keys, BOSS_COPIES, fortress);
    if maps_move(new_game) {
        key_lines.new.extend(map_copies::captions());
    }
    // Copies' own lines before the shelves', so a copy on a shelf takes its
    // caption from its own lines.
    let mut copies = vec![key_lines, foreign::captions(&new_game.foreign)];
    if new_game.shop_slots > 0 {
        copies.push(shops::captions(romfs, shelves)?);
    }
    copies.extend([
        starter_bags::captions(new_game),
        notice::banners(),
        notice::senders(&new_game.players),
    ]);
    text::write(romfs, builder, &lines, &copies, hints)
}

/// What `write_mod`'s passes share: the seed, and what's read and worked
/// out for it once.
struct Parts<'a> {
    romfs: &'a Romfs,
    drops: &'a HashMap<u64, Drop>,
    new_game: &'a NewGame,
    reads: Reads,
    picked: Vec<picked::Holder>,
    steps: pickups::steps::Steps,
    /// Each holder whose step copies stand by their module's own gate, with
    /// the program each waits on (`pickups::nodes::gates`).
    gates: HashMap<u64, Vec<Op>>,
    seats: pickups::seats::Seats,
    /// Every placement copied from a holder's, by the one it copies: each
    /// step's, then each boulder's seat.
    copies: Vec<(u64, u64)>,
    stand_ins: Vec<(u64, String)>,
    /// Every dealt check, a kept item's too: its map's alternative needs it.
    dealt: HashSet<u64>,
    twins: Vec<twins::Twin>,
    /// The scrubs that leave once they've sold.
    gone: Vec<(u64, String)>,
    /// The Red Rupee copies dealt.
    rupees: Vec<String>,
}

/// Each holder whose step copies can stand by its module's own gate instead
/// of a duplicated node (`pickups::nodes::gates`): one that lies until picked,
/// and whose drop no event names and puts in place.
///
/// A thief's call picks which step's key to drop by a Switch on each step's
/// BoolExp, and the flag that lets the key appear is set inside that very
/// call. A step gated on it reads false at the Switch, so her call always
/// drops the holder's own step: a key dealt 10 Arrows would hand a Red Rupee
/// to a Link holding a bow, and one dealt the Hookshot a second Hookshot
/// instead of the Longshot. The four Fortress keys keep their nodes.
fn module_gates(
    romfs: &Romfs,
    gd: &GameData,
    pieces: &[ItemCheck],
    picked: &[picked::Holder],
) -> Result<HashMap<u64, Vec<Op>>> {
    Ok((pickups::nodes::gates(romfs, gd)?)
        .into_iter()
        .filter(|(hash, _)| {
            (picked.iter()).any(|h| {
                h.hash == *hash
                    && matches!(h.after, picked::After::Taken(_))
                    && !(pieces.iter()).any(|c| c.hash == *hash && pickups::is_named(c))
            })
        })
        .collect())
}

impl<'a> Parts<'a> {
    fn new(
        romfs: &'a Romfs,
        drops: &'a HashMap<u64, Drop>,
        new_game: &'a NewGame,
        reads: Reads,
        gd: &GameData,
    ) -> Result<Self> {
        let pieces = &reads.pieces;
        let picked = picked::holders(pieces, drops);
        let gates = module_gates(romfs, gd, pieces, &picked)?;
        let steps =
            pickups::steps::Steps::new(pieces, drops, Lines::of(new_game), &picked, gd, &gates)?;
        let seats = pickups::seats::Seats::new(pieces, drops, Lines::of(new_game));
        let copies = [steps.copies.as_slice(), &seats.pairs].concat();
        let stand_ins = pickups::stand_ins(pieces, drops, Lines::of(new_game));
        let dealt: HashSet<u64> = (checks::dealt(romfs, new_game)?.iter())
            .map(|c| c.hash)
            .collect();
        let twins = twins::find(romfs, |h| dealt.contains(&h), tracker::chest_flags(gd)?)?;
        let gone = match new_game.scrubs {
            true => scrubs::gone(drops),
            false => Vec::new(),
        };
        Ok(Self {
            romfs,
            drops,
            new_game,
            reads,
            picked,
            steps,
            gates,
            seats,
            copies,
            stand_ins,
            dealt,
            twins,
            gone,
            rupees: ammo::copies(drops.values().map(|d| d.item.as_str())),
        })
    }

    /// The Bools the seed adds to GameData.
    fn bools(&self, slots: &[shops::Slot], shelves: &shops::Shelves) -> Vec<String> {
        let new_game = self.new_game;
        let mut bools = cows::flags(&self.reads.cows);
        bools.extend(picked::flags(&self.picked));
        if new_game.shop_slots > 0 {
            bools.extend(shops::sold::flags(slots, new_game.shop_slots));
            bools.extend(shelves.bools.iter().cloned());
        }
        bools.extend(gifts::flags(&new_game.on));
        bools.extend(dampe::flags(self.drops));
        if new_game.quiet_enemies {
            bools.push(enemies::FLAG.into());
        }
        if new_game.fortress_keys.moved() {
            bools.extend(fortress::flags());
        }
        bools.extend(entrances::reveal::flags(new_game.dungeon_entrances));
        if new_game.scrubs {
            bools.extend(scrubs::flags(self.drops));
        }
        bools
    }

    /// The BoolExps the seed adds to GameData.
    fn exps(&self, gd: &GameData, shelves: &shops::Shelves) -> Result<Vec<(String, Vec<Op>)>> {
        let aliases = self.twins.iter().filter_map(twins::Twin::alias);
        let mut exps: Vec<_> = aliases
            .map(|(name, of)| (name, vec![Op::Bool(of)]))
            .collect();
        exps.extend(picked::exps(&self.picked, &self.stand_ins));
        exps.extend(shelves.exps.iter().cloned());
        exps.extend(self.steps.exps.iter().cloned());
        exps.extend(jabu_mood::exps(gd, self.new_game.adult_jabu)?);
        exps.extend(carpenters::exps(gd)?);
        Ok(exps)
    }

    /// Edit GameData's `root` for the seed, with `edits` (presets and
    /// defaults) last. Returns the edits as `name: old -> new`.
    fn game_data(
        &self,
        root: &mut Byml,
        gd: &GameData,
        slots: &[shops::Slot],
        shelves: &shops::Shelves,
        edits: &[String],
    ) -> Result<Vec<String>> {
        let new_game = self.new_game;
        trade_chain::never_counting(root)?;
        if new_game.on.contains(TRADE_CHAIN) {
            trade_chain::by_pouch(root)?;
        }
        let defaults = presets::apply(gd, root, edits)?;
        gamedata::add_bools(root, &self.bools(slots, shelves))?;
        gamedata::add_ints(root, &new_game.ints)?;
        gamedata::add_exps(root, &self.exps(gd, shelves)?)?;
        let mut keyed = picked::pairs(&self.picked);
        keyed.extend(&self.copies);
        keyed.extend(trap_chests::keyed(self.drops));
        gamedata::add_keys(root, &keyed)?;
        trap_chests::game_data(root, self.drops)?;
        if new_game.breakables_respawn {
            gamedata::respawn_on_load(root)?;
        }
        let fortress = new_game.fortress_keys.copies();
        for (copy, template, _) in &key_tints::made(new_game.keys.copies(), BOSS_COPIES, fortress) {
            let names = [copy.clone()];
            gamedata::join_structs(root, template.get(), &names, &[gamedata::IS_GET])?;
        }
        if new_game.bombchus == Bombchus::Bag {
            bombchu_bags::game_data(root)?;
        }
        crate::magic::meter::game_data(root)?;
        let names: Vec<String> = new_game.foreign.iter().map(|f| f.actor.clone()).collect();
        if !names.is_empty() {
            gamedata::join_structs(root, keys::SMALL.get(), &names, &[gamedata::IS_GET])?;
        }
        if new_game.bridge == Bridge::SixMedallions {
            bridge::need_medallions(root)?;
        }
        Ok(defaults)
    }

    /// Write the seed's events, stand-ins and other files outside the maps.
    /// Returns the fields the maps' actors take, and the ice traps' look.
    fn assets(
        &mut self,
        builder: &mut ModBuilder,
        shelves: &shops::Shelves,
    ) -> Result<(Vec<String>, Option<LookSpec>)> {
        let (romfs, drops, new_game) = (self.romfs, self.drops, self.new_game);
        let mut fields = new_game.fields.clone();
        let mut events = rsdb::Events::load(romfs)?;
        steps::dev_event(&mut events)?;
        if new_game.scrubs {
            fields.extend(scrubs::fields(drops));
            scrubs::events(drops, &mut events)?;
        }
        fields.extend(cows::fields(&self.reads.cows));
        let mut tables = standins::Tables::load(romfs)?;
        self.models(builder, &mut tables)?;
        cows::write(romfs, builder, &mut tables, &mut events, &self.reads.cows)?;
        self.reads.levels.events(&mut events)?;
        let look = if ice_traps::springs(new_game) {
            ice_traps::event(&mut events)?;
            ice_traps::sensor(romfs, builder, &mut tables)?;
            ice_traps::thaw(builder)?;
            Some(ice_traps::look(romfs, builder)?)
        } else {
            None
        };
        if let Some((blocks, arrivals)) = &mut self.reads.doors {
            entrances::reveal::events(arrivals, &mut events)?;
            entrances::reveal::retarget(blocks, arrivals)?;
        }
        events.write(builder)?;
        if new_game.shop_slots > 0 {
            fields.extend(shops::write(
                romfs,
                builder,
                &mut tables,
                shelves,
                new_game,
            )?);
        }
        if new_game.day_skulltulas {
            day_skulltulas::write(romfs, builder)?;
        }
        sheik_crater::write(romfs, builder)?;
        if new_game.on.contains(presets::JABU_OPEN) {
            jabu_mouth::write(romfs, builder)?;
        }
        let pieces = &self.reads.pieces;
        fields.extend(pickups::write(
            romfs,
            builder,
            &mut tables,
            pieces,
            drops,
            new_game,
        )?);
        if new_game.fortress_keys.moved() {
            let lines = Lines::of(new_game);
            fields.extend(fortress::fields(romfs, pieces, drops, lines)?);
        }
        self.steps.write(romfs, builder)?;
        held::write(romfs, builder, &mut tables)?;
        if new_game.freestanding {
            goron_pot::write(romfs, builder, &mut tables, drops, new_game)?;
        }
        let boots = Enabled::of(&new_game.on).boots();
        if !boots.is_empty() {
            ageless::gear::shortcut(&mut tables, &boots)?;
        }
        tables.write(builder)?;
        Ok((fields, look))
    }

    /// Write the item models the seed changes or adds, and their rows in
    /// `tables`.
    fn models(&self, builder: &mut ModBuilder, tables: &mut standins::Tables) -> Result<()> {
        let (romfs, drops, new_game) = (self.romfs, self.drops, self.new_game);
        // Before the stand-ins, so one holding bombchus copies the freed row,
        // and one holding nuts or sticks the row that keeps its bag.
        bombchus::write(builder, tables, new_game.bombchus)?;
        if new_game.bombchus == Bombchus::Bag {
            bombchu_bags::write(romfs, builder, tables)?;
        }
        starter_bags::write(romfs, new_game, builder, tables)?;
        // Before the stand-ins, so one copying a magic item copies it apart.
        crate::magic::write(romfs, builder, tables, !new_game.magic_with_first_item)?;
        Enabled::of(&new_game.on).write(romfs, builder, tables, &new_game.colors)?;
        // Before the stand-ins, which copy a song's model as it's shown.
        let shown = new_game.foreign.iter().map(|f| f.shows.as_str());
        let items = drops.values().map(|d| d.item.as_str()).chain(shown);
        medallions::write(romfs, builder, tables, items)?;
        let fortress = new_game.fortress_keys.copies();
        key_tints::write(romfs, builder, tables, BOSS_COPIES, new_game.keys, fortress)?;
        if maps_move(new_game) {
            map_copies::write(romfs, builder, tables)?;
        }
        // One tag table for both: each adds its copies' rows.
        let mut tags = crate::rsdb::Tags::load(romfs)?;
        crate::marks::write(romfs, builder, tables, &mut tags)?;
        crate::chest_looks::write(romfs, builder, tables, &mut tags)?;
        tags.write(builder)?;
        foreign::write(romfs, builder, tables, &new_game.foreign, new_game.keys)?;
        ammo::write(romfs, builder, tables, &self.rupees)
    }
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
    chest_levels::Levels::new(romfs, drops, Lines::of(new_game), sets(new_game))
}

/// What each copy `new_game` deals sets as it's found: its key copies',
/// and with maps and compasses moving theirs.
fn sets(new_game: &NewGame) -> Vec<(String, crate::copies::Sets)> {
    let fortress = new_game.fortress_keys.copies();
    let mut sets = key_tints::sets(BOSS_COPIES, new_game.keys, fortress);
    if maps_move(new_game) {
        sets.extend(map_copies::sets());
    }
    sets
}

/// [`sets`] as the hook's table names it. Traps need none: the hook
/// freezes Link for any of them by name.
fn credits(new_game: &NewGame) -> Vec<(String, zootr_hook::Credit)> {
    crate::copies::as_credits(sets(new_game))
}

/// The settings the hook acts on, as the table's flag bits. Every seed
/// lets Link pick what the trade slot shows.
fn hook_flags(new_game: &NewGame) -> u32 {
    [
        (
            new_game.bombchus == Bombchus::Free,
            zootr_hook::FREE_BOMBCHUS,
        ),
        (new_game.bombchus == Bombchus::Bag, zootr_hook::BOMBCHU_BAG),
        (new_game.shuffle_nut_bag, zootr_hook::SHUFFLED_NUT_BAG),
        (new_game.shuffle_stick_bag, zootr_hook::SHUFFLED_STICK_BAG),
        (new_game.bunny_hood, zootr_hook::BUNNY_HOOD),
        (ice_traps::springs(new_game), zootr_hook::ICE_TRAPS),
        (true, zootr_hook::MENU_CYCLE),
        (Enabled::of(&new_game.on).gear(), zootr_hook::CHILD_GEAR),
        (
            new_game.on.contains(crate::settings::ONE_TAP_SONGS),
            zootr_hook::ONE_TAP_SONGS,
        ),
    ]
    .into_iter()
    .filter_map(|(on, bit)| on.then_some(bit))
    .fold(0, |all, bit| all | bit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;
    use crate::pickups::{BOSS_HEARTS, FORTRESS_KEYS};
    use zootr_hook::{BUNNY_HOOD, FREE_BOMBCHUS, MENU_CYCLE};

    /// A thief dealt ammo keeps its node, and a boss heart dealt the same
    /// doesn't: her call names each step's key and picks one by its BoolExp,
    /// and the flag that lets the key appear is set inside that call.
    #[test]
    fn a_called_holder_keeps_the_module_gate_out() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let arrows = actor::T_BOX_ARROW_BUNDLE_SMALL.get();
        let heart = 17948245526767051747;
        let keys: Vec<ItemCheck> = FORTRESS_KEYS.checks(&romfs).unwrap();
        let mut pieces: Vec<ItemCheck> = keys.clone();
        pieces.extend(
            BOSS_HEARTS
                .checks(&romfs)
                .unwrap()
                .into_iter()
                .filter(|c| c.hash == heart),
        );
        let drops: HashMap<u64, Drop> = (pieces.iter())
            .map(|c| (c.hash, Drop::from(arrows.to_owned())))
            .collect();
        let picked = picked::holders(&pieces, &drops);
        let gates = module_gates(&romfs, &gd, &pieces, &picked).unwrap();
        assert!(
            gates.contains_key(&heart),
            "a heart dealt ammo stands on Gohma's own flag"
        );
        for key in &keys {
            assert!(
                !gates.contains_key(&key.hash),
                "{} waits on her call",
                key.hash
            );
        }
    }

    #[test]
    fn an_adult_start_holds_the_master_sword_only_from_the_start_inventory() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let all = Flows::load(&romfs).unwrap();
        let sword = actor::WEAPON_SWORD_MASTER.get();
        let opening = |start: serde_json::Value| {
            let settings = crate::settings::Settings::from([
                (crate::settings::STARTING_AGE.into(), "adult".into()),
                (starting::SETTING.into(), start),
            ]);
            let new_game = NewGame::from_settings(&settings, &[]).unwrap();
            let (rewrites, _) = flows_alone(&romfs, &all, &new_game, &HashMap::new()).unwrap();
            let rendered = |name: &crate::known::Name| {
                let flow = ounce_formats::Flowchart::parse(&rewrites[&crate::flows::rel(name)]);
                crate::flows::render(&flow.unwrap().unwrap())
            };
            (
                rendered(starting::OPENING),
                rendered(crate::known::flow::MARKET_CHILD_FIELD_CHILD_TO_ADULT),
            )
        };
        // Without it, the opening never names it: the pedestal's swap
        // gives it on his first pull back to adult.
        let (without, swap) = opening(serde_json::json!({}));
        assert!(!without.contains(sword));
        assert!(swap.contains(sword));
        let (with, _) = opening(serde_json::json!({ "master-sword": 1 }));
        assert!(with.contains(sword));
    }

    #[test]
    fn each_hook_setting_sets_its_own_bit() {
        assert_eq!(FREE_BOMBCHUS & BUNNY_HOOD & MENU_CYCLE, 0);
        let flags = |free: bool, bunny_hood| {
            hook_flags(&NewGame {
                bombchus: [Bombchus::BombBag, Bombchus::Free][usize::from(free)],
                bunny_hood,
                ..Default::default()
            })
        };
        assert_eq!(flags(false, false), MENU_CYCLE);
        assert_eq!(flags(true, false), FREE_BOMBCHUS | MENU_CYCLE);
        assert_eq!(flags(false, true), BUNNY_HOOD | MENU_CYCLE);
        assert_eq!(flags(true, true), FREE_BOMBCHUS | BUNNY_HOOD | MENU_CYCLE);
        let plain = hook_flags(&NewGame::default());
        for (nuts, sticks, want) in [
            (false, false, 0),
            (true, false, zootr_hook::SHUFFLED_NUT_BAG),
            (false, true, zootr_hook::SHUFFLED_STICK_BAG),
            (
                true,
                true,
                zootr_hook::SHUFFLED_NUT_BAG | zootr_hook::SHUFFLED_STICK_BAG,
            ),
        ] {
            let game = NewGame {
                shuffle_nut_bag: nuts,
                shuffle_stick_bag: sticks,
                ..Default::default()
            };
            assert_eq!(hook_flags(&game), plain | want);
        }
        assert_eq!(plain & zootr_hook::CHILD_GEAR, 0);
        let with = |id: &str| {
            hook_flags(&NewGame {
                on: [id.to_string()].into_iter().collect(),
                ..Default::default()
            })
        };
        // Any boots or tunic, and nothing else.
        for toggle in &crate::ageless::toggles::TOGGLES {
            let gear = Enabled::of(&[toggle.id.to_string()].into_iter().collect()).gear();
            let want = [plain, plain | zootr_hook::CHILD_GEAR][usize::from(gear)];
            assert_eq!(with(toggle.id), want, "{}", toggle.id);
        }
        assert_eq!(with("both-ages-iron-boots"), plain | zootr_hook::CHILD_GEAR);
        assert_eq!(with("both-ages-hammer"), plain);
        let songs = crate::settings::ONE_TAP_SONGS;
        assert_eq!(plain & zootr_hook::ONE_TAP_SONGS, 0);
        assert_eq!(with(songs), plain | zootr_hook::ONE_TAP_SONGS);
    }
}
