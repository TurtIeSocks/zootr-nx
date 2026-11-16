//! Writes a seed's drops, new-game edits and flows into a mod.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Result, bail, ensure};
use ounce_formats::{BymlDoc, ModBuilder, Romfs};

use super::Written;
use crate::checks::{self, Drop};
use crate::events::Flows;
use crate::flows::Rewrites;
use crate::hints::Hint;
use crate::settings::{ChestSize, NewGame};
use crate::{
    bombchus, bombs, bridge, cows, cutscenes, dampe, dig, door, flags, fortress, gamedata, gerudo,
    gifts, hints, medallions, navi, patch, pickups, presets, rsdb, scrubs, shops, spawn, standins,
    starting, steps, text,
};

/// The flows `new_game` and the gifts in `drops` rewrite, and the GameData
/// edits the skips need.
pub(super) fn flows(
    romfs: &Romfs,
    all: &Flows,
    new_game: &NewGame,
    drops: &HashMap<u64, Drop>,
) -> Result<(Rewrites, Vec<String>)> {
    let (mut rewrites, mut flags) = cutscenes::skip(romfs, &new_game.skips)?;
    if new_game.quiet_navi {
        navi::quiet(romfs, &mut rewrites)?;
    }
    if new_game.bridge_medallions {
        bridge::keep_step(romfs, &mut rewrites)?;
        flags.push(bridge::START.into());
    }
    if new_game.hints {
        hints::quiet(romfs, &mut rewrites)?;
    }
    flags.push(starting::navi_guide());
    let mut give = new_game.give.clone();
    if new_game.open_door {
        flags.extend(door::open(romfs, &mut rewrites)?);
        give.push(door::MEDALLION.to_string());
    }
    starting::grant(romfs, &mut rewrites, &new_game.start, &give)?;
    starting::replay(romfs, &mut rewrites, &new_game.progress)?;
    if new_game.dev_panel {
        steps::dev_branch(romfs, &mut rewrites)?;
    }
    if let Some(spawn) = &new_game.spawn {
        spawn::warp(romfs, &mut rewrites, spawn)?;
    }
    gifts::give_all(romfs, all, &mut rewrites, drops, new_game.progressive)?;
    dampe::give(romfs, &mut rewrites, drops)?;
    gerudo::stand_down(romfs, &mut rewrites, drops)?;
    if new_game.scrubs {
        scrubs::write(romfs, &mut rewrites, drops, new_game.scrub_price)?;
    }
    if new_game.cows {
        cows::give(romfs, &mut rewrites, &cows::find(romfs)?, drops)?;
    }
    if new_game.fortress_keys {
        let pieces = pickups::dealt(romfs, new_game)?;
        fortress::give(romfs, &mut rewrites, &pieces, drops)?;
    }
    Ok((rewrites, flags))
}

/// Apply `drops` (actor hash -> drop) to every Banc file, `new_game` to
/// GameData and the cutscenes, and `hints` to the gossip stones, and write
/// the changed files as a mod.
pub fn write_mod(
    romfs: &Romfs,
    mod_dir: &Path,
    drops: &HashMap<u64, Drop>,
    new_game: &NewGame,
    hints: &[Hint],
) -> Result<Written> {
    if let Some(item) = drops
        .values()
        .flat_map(|d| {
            [d.item.as_str()]
                .into_iter()
                .chain(d.swap.iter().flat_map(|s| [s.check, s.then]))
        })
        .find(|i| !romfs.root().join(flags::actor_pack(i)).is_file())
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
    let mut lines = gifts::lines(drops);
    lines.push(scrubs::lines(drops));
    let copies = match new_game.shop_slots {
        0 => Vec::new(),
        count => vec![shops::captions(romfs, &shops::slots(romfs)?, count, drops)?],
    };
    text::write(romfs, &mut builder, &lines, &copies, hints)?;
    let cows = match new_game.cows {
        true => cows::find(romfs)?,
        false => Vec::new(),
    };
    // Presets and `--default` edits go last, so they win over a skip's.
    let mut edits = flags;
    edits.extend(new_game.defaults.iter().cloned());
    let mut defaults = Vec::new();
    let mut bools = cows::flags(&cows);
    bools.extend(gifts::flags(&new_game.on));
    bools.extend(dampe::flags(drops));
    if !edits.is_empty() || !bools.is_empty() {
        let mut doc = BymlDoc::parse(&romfs.read(gamedata::PATH)?)?;
        let gd = gamedata::GameData::load(romfs)?;
        defaults = presets::apply(&gd, &mut doc.root, &edits)?;
        gamedata::add_bools(&mut doc.root, &bools)?;
        if new_game.bridge_medallions {
            bridge::need_medallions(&mut doc.root)?;
        }
        builder.write(gamedata::PATH, &doc.to_binary())?;
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
    // Before the stand-ins, which copy a song's model as it's shown.
    let items = drops.values().map(|d| d.item.as_str());
    medallions::write(romfs, &mut builder, &mut tables, items)?;
    cows::write(romfs, &mut builder, &mut tables, &mut events, &cows)?;
    events.write(&mut builder)?;
    if new_game.shop_slots > 0 {
        let slots = shops::slots(romfs)?;
        let (count, seed) = (new_game.shop_slots, new_game.seed);
        fields.extend(shops::write(
            romfs,
            &mut builder,
            &mut tables,
            &slots,
            count,
            drops,
            seed,
        )?);
    }
    let pieces = pickups::dealt(romfs, new_game)?;
    fields.extend(pickups::write(
        romfs,
        &mut builder,
        &mut tables,
        &pieces,
        drops,
    )?);
    tables.write(&mut builder)?;
    // A pickup's drop is its field above, never the bare item. A cow's is
    // its flow's grant.
    let chest_drops: HashMap<u64, Drop> = drops
        .iter()
        .filter(|(h, _)| !pieces.iter().any(|p| p.hash == **h))
        .filter(|(h, _)| !cows.iter().any(|c| c.hash() == **h))
        .map(|(h, d)| (*h, d.clone()))
        .collect();
    let carried = match new_game.fortress_keys {
        true => &fortress::CARRYING[..],
        false => &[],
    };
    let (mut found, mut actors) = (HashSet::new(), 0);
    for rel in checks::banc_files(romfs)? {
        let mut doc = checks::load_banc(romfs, &rel)?;
        let mut changed = checks::set_fields(&mut doc.root, &fields)?;
        changed.extend(checks::clear_drops(&mut doc.root, carried)?);
        changed.extend(checks::set_drops(
            &mut doc.root,
            &chest_drops,
            &mut shows_off,
        )?);
        let shrunk =
            new_game.chest_size == ChestSize::Small && checks::shrink_chests(&mut doc.root)? > 0;
        if shrunk || !changed.is_empty() {
            builder.write(&rel, &doc.to_binary())?;
            actors += changed.len();
            found.extend(changed);
        }
    }
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
    bombs::write(&mut builder)?;
    dig::write(romfs, &mut builder, drops)?;
    let files = builder.finish()?.len();
    patch::install(
        mod_dir,
        new_game.progressive,
        new_game.dev_panel,
        hook_flags(new_game),
    )?;
    Ok(Written {
        defaults,
        actors,
        files,
    })
}

/// The settings the hook acts on, as the table's flag bits.
fn hook_flags(new_game: &NewGame) -> u32 {
    [
        (new_game.free_bombchus, zootr_hook::FREE_BOMBCHUS),
        (new_game.bunny_hood, zootr_hook::BUNNY_HOOD),
    ]
    .into_iter()
    .filter_map(|(on, bit)| on.then_some(bit))
    .fold(0, |all, bit| all | bit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zootr_hook::{BUNNY_HOOD, FREE_BOMBCHUS};

    #[test]
    fn each_hook_setting_sets_its_own_bit() {
        assert_eq!(FREE_BOMBCHUS & BUNNY_HOOD, 0);
        let flags = |free_bombchus, bunny_hood| {
            hook_flags(&NewGame {
                free_bombchus,
                bunny_hood,
                ..Default::default()
            })
        };
        assert_eq!(flags(false, false), 0);
        assert_eq!(flags(true, false), FREE_BOMBCHUS);
        assert_eq!(flags(false, true), BUNNY_HOOD);
        assert_eq!(flags(true, true), FREE_BOMBCHUS | BUNNY_HOOD);
    }
}
