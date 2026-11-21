//! The commands that talk to the running game: watching its GameData,
//! setting a value, giving an item and the dev panel's requests.

use std::fmt::Display;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use ounce_formats::Romfs;
use serde::Serialize;

use super::dev_args::{DevCommand, MemCommand, Switch};
use crate::config::{Config, TestSetup};
use crate::nav::{act, drive, shove};
use crate::notice::Shown;
use crate::play::{self, Input};
use crate::tracker::Flag;
use crate::{gamedata, live, recorder, steps, warp};

/// Whether `hash`'s name contains `grep`, or there's no `grep`.
pub(super) fn named(gd: &gamedata::GameData, grep: &Option<String>, hash: u32) -> bool {
    grep.as_ref()
        .is_none_or(|g| gd.name(hash).contains(g.as_str()))
}

/// `[mm:ss]` since `start`.
pub(super) fn stamp(start: std::time::Instant) -> String {
    live::stamp(start.elapsed().as_secs())
}

/// The game's GameData values, printed as they change.
pub(super) fn live(romfs: &Romfs, grep: Option<String>, all: bool, only_named: bool) -> Result<()> {
    let gd = recorder::gamedata(romfs)?;
    let game = attach(false)?;
    println!("watching, Ctrl-C stops");
    if !all {
        let (n, secs) = (live::BURST, live::WINDOW.as_secs());
        println!("values that change over {n} times in {secs}s are muted, --all shows them");
    }
    let start = std::time::Instant::now();
    let mut poll = live::Poll::new(game)?;
    loop {
        std::thread::sleep(live::TICK);
        let wanted = |h| named(&gd, &grep, h) && (!only_named || gd.known(h));
        for c in poll.next(&gd, all, wanted)?.0 {
            println!("{} {c}", stamp(start));
        }
    }
}

/// The target's running game, on this session's turn. Only a command
/// that `plays` it renews the turn: one that only reads lets it lapse.
fn attach(plays: bool) -> Result<live::Game> {
    let session = crate::turn::session();
    match plays {
        true => crate::turn::hold(&session)?,
        false => crate::turn::sees(&session)?,
    }
    live::Game::attach(&Config::load()?.target()?)
}

/// The target's way of pressing buttons and pushing the stick, on this
/// session's turn.
fn input() -> Result<Box<dyn Input>> {
    crate::turn::hold(&crate::turn::session())?;
    let config = Config::load()?;
    play::input(&config.target()?, config.game.as_deref())
}

/// A report as lines, or as one JSON object with `json`.
pub(super) fn said<T: Display + Serialize>(report: &T, json: bool) -> Result<String> {
    Ok(match json {
        true => serde_json::to_string(report)?,
        false => report.to_string(),
    })
}

/// How often `zootr ap connect` renews its hold on the game.
pub(super) const RENEW: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// `zootr ap connect`: the client core on the live game, its notes
/// printed until the process is stopped.
pub(super) fn ap_connect(
    romfs: &Romfs,
    server: String,
    slot: Option<String>,
    password: Option<String>,
) -> Result<()> {
    use crate::ap::client::{self, Room};
    let gd = recorder::gamedata(romfs)?;
    let mut client = client::installed(romfs, gd.into(), attach(true)?)?;
    let room = Room {
        server,
        slot: slot.unwrap_or(client.slot),
        password,
    };
    // Well inside the queue's 15 min lease. The core stops if a renewal
    // is refused.
    let stop = crate::turn::kept(crate::turn::session(), RENEW);
    let start = std::time::Instant::now();
    client::run(&room, &client.book, &mut client.game, &stop, &mut |note| {
        println!("{} {note}", stamp(start));
    });
    Ok(())
}

pub(super) fn give(romfs: &Romfs, actor: &str, count: i32) -> Result<()> {
    let gd = recorder::gamedata(romfs)?;
    crate::dev::Dev::new(attach(true)?)?.give(&gd, actor, count)?;
    println!("gave {actor} x{count}, open the menu again to see it");
    Ok(())
}

/// GameData as the installed seed wrote it, which names the Bools a seed
/// adds, else the game's own.
/// The installed seed's gamedata, or the game's own when there's no target
/// or the target has no seed installed.
fn installed_gamedata(romfs: &Romfs) -> Result<gamedata::GameData> {
    let installed = Config::load()
        .and_then(|c| Ok(c.target()?.mod_dir.join("romfs").join(gamedata::PATH)))
        .and_then(|file| Ok(std::fs::read(file)?));
    match installed {
        Ok(raw) => gamedata::GameData::parse(&romfs.dicts().decompress(gamedata::PATH, &raw)?),
        Err(_) => gamedata::GameData::load(romfs),
    }
}

pub(super) fn set(romfs: &Romfs, name: &str, value: &str) -> Result<()> {
    use live::Memory;
    let info = installed_gamedata(romfs)?.info(name)?;
    if value == "?" {
        if !info.choices.is_empty() {
            println!("{name} takes one of: {}", info.choices.join(", "));
        }
        if let Some(default) = &info.default {
            println!("default: {default}");
        }
    } else {
        info.check(name, value)?;
    }
    let hash = live::hash(name)?;
    let game = attach(value != "?")?;
    let (kind, at, len) = live::locate(&game, game.global(), hash)?;
    let show = |label: &str| -> Result<()> {
        let both = game.read(at, len * 2)?;
        println!("{label}: {:02x?} saved {:02x?}", &both[..len], &both[len..]);
        Ok(())
    };
    println!("{name} is a {kind} at {at:#x}");
    show("was")?;
    if value != "?" {
        game.set(hash, value)?;
        show("now")?;
    }
    Ok(())
}

pub(super) fn dev(romfs: &Romfs, game_path: &Path, cmd: DevCommand) -> Result<()> {
    use crate::dev::{self, Dev, Place, Request};
    let reads = matches!(
        cmd,
        DevCommand::State
            | DevCommand::Around { .. }
            | DevCommand::Scene { .. }
            | DevCommand::Go { plan: true, .. }
            | DevCommand::Checked { .. }
            | DevCommand::Nearby { .. }
    );
    let game = Dev::new(attach(!reads)?)?;
    let said = match cmd {
        DevCommand::State => {
            let s = game.state()?;
            let age = if s.child { "child" } else { "adult" };
            let busy = [(s.loading, ", loading"), (s.event, ", event playing")];
            let busy: String = busy.iter().filter(|b| b.0).map(|b| b.1).collect();
            format!("{} as {age}{busy}", s.map)
        }
        DevCommand::Teleport { place, near, side } => {
            let place = Place::parse(&place)?;
            dev::known(&place, &dev::places(romfs)?)?;
            let at = (near.as_deref())
                .map(|actor| warp::spot(romfs, &place.map, actor, side))
                .transpose()?;
            let went = game.go(&place)?;
            match near.zip(at) {
                Some((actor, at)) => {
                    game.land(&place.map)?;
                    let warped = game.warp(at)?;
                    format!("{went}, then {warped} beside {actor}")
                }
                None => went.into(),
            }
        }
        DevCommand::Warp { actor, side } => warp::beside(&game, romfs, &actor, side)?,
        DevCommand::Around { radius, all, json } => {
            said(&crate::around::read(&game, romfs, radius, all)?, json)?
        }
        DevCommand::Scene { radius, json } => said(
            &crate::nav::scene::read(&game, romfs, game_path, radius)?,
            json,
        )?,
        DevCommand::Go {
            hash,
            plan: true,
            json,
            ..
        } => said(
            &crate::nav::route::read(&game, romfs, game_path, hash)?,
            json,
        )?,
        DevCommand::Go {
            hash,
            use_it,
            warp,
            budget,
            json,
            ..
        } => {
            let how = drive::How {
                then_use: use_it,
                warp,
                budget: Duration::from_secs(budget),
            };
            said(
                &drive::go(&game, &mut input()?, romfs, game_path, hash, &how)?,
                json,
            )?
        }
        DevCommand::Move { hour, secs, json } => said(
            &shove::moved(&game, &mut input()?, romfs, game_path, hour, secs)?,
            json,
        )?,
        DevCommand::Use { hash, json } => said(
            &act::used(&game, &mut input()?, romfs, game_path, hash)?,
            json,
        )?,
        DevCommand::Switches { on, off } => {
            if off || !on.is_empty() {
                game.set_switches(dev::Switches {
                    health: on.contains(&Switch::Health),
                    magic: on.contains(&Switch::Magic),
                    resources: on.contains(&Switch::Resources),
                })?;
            }
            let s = game.switches()?;
            let held = [
                ("health", s.health),
                ("magic", s.magic),
                ("resources", s.resources),
            ];
            let on: Vec<_> = held.iter().filter(|(_, on)| *on).map(|(n, _)| *n).collect();
            match on.is_empty() {
                true => "no switches on".into(),
                false => format!("on: {}", on.join(", ")),
            }
        }
        DevCommand::Notice {
            item,
            from,
            count,
            mixed,
        } => {
            let shown = match (item, count) {
                (Some(item), _) => Shown::item(&item, from),
                (None, count) => Shown::summary((!mixed).then_some(from), count.unwrap_or(1)),
            };
            game.notice(&shown)?;
            "shown".into()
        }
        DevCommand::Kill => {
            game.kill()?;
            "sent".into()
        }
        DevCommand::Slay {
            target,
            within,
            json,
            ..
        } => match target {
            Some(t) => said(&game.slay(romfs, &dev::Target::parse(&t), within)?, json)?,
            None => said(&game.foes(within)?, json)?,
        },
        DevCommand::Nearby { grep, limit, json } => {
            let mut near = game.nearby()?;
            if let Some(text) = grep {
                near.0.retain(|n| n.name.contains(&text));
            }
            near.0.truncate(limit);
            said(&near, json)?
        }
        DevCommand::Hide { hashes, show } => {
            let found = game.hide(&hashes, !show)?;
            let missing: Vec<String> = (hashes.iter())
                .filter(|h| !found.contains(h))
                .map(u64::to_string)
                .collect();
            let done = if show { "showed" } else { "hid" };
            let mut lines = vec![format!("{done} {} of {}", found.len(), hashes.len())];
            if !missing.is_empty() {
                lines.push(format!("not live: {}", missing.join(" ")));
            }
            lines.join("\n")
        }
        DevCommand::Freeze => match game.freeze()? {
            0 => "sent".into(),
            n => format!("sent, after {n} still waiting"),
        },
        DevCommand::Age => {
            let (place, done) = game.switch_age(&dev::places(romfs)?)?;
            format!("{done}, to {place}")
        }
        DevCommand::Time { hour } => game.ask(&Request::Time(hour))?.into(),
        DevCommand::Step { challenge, step } => {
            steps::check(&recorder::gamedata(romfs)?, &challenge, &step)?;
            let done = game.ask(&Request::Step(challenge.clone(), step.clone()))?;
            format!("{done} {challenge}={step}")
        }
        DevCommand::Play { event } => {
            format!("{} {event}", game.ask(&Request::Play(event.clone()))?)
        }
        DevCommand::Apply { name } => {
            let setup = saved_test(&name)?;
            let near = warp::near(romfs, &setup, || Ok(game.state()?.map))?;
            let applied = game.apply(&recorder::gamedata(romfs)?, &setup, near)?;
            let held = applied
                .held
                .iter()
                .map(|g| format!("{g} held before Apply"));
            applied
                .done
                .into_iter()
                .chain(held)
                .collect::<Vec<_>>()
                .join("\n")
        }
        DevCommand::Checked { hash } => {
            let gd = installed_gamedata(romfs)?;
            match crate::ledger::flag(romfs, &gd, hash)? {
                None => "no flag".into(),
                // A pickup's Bool exists only in a seed that shuffles it.
                Some(Flag::Bool(h)) if gd.entry(h).is_none() => "no flag in this seed".into(),
                Some(Flag::Bool(h)) => (game.game().get(&gd, h)? == "true").to_string(),
                Some(Flag::Key(k)) => {
                    let g = game.game();
                    live::keys(g, g.global())?.contains(&k).to_string()
                }
            }
        }
        DevCommand::Goals { name } => {
            let setup = saved_test(&name)?;
            let goals = game.goals(&recorder::gamedata(romfs)?, &setup);
            let lines = goals.iter().map(|g| {
                let now = g.now.as_deref().or(g.error.as_deref()).unwrap_or("");
                format!(
                    "{} {}={} ({now})",
                    if g.met { "✓" } else { "✗" },
                    g.name,
                    g.want
                )
            });
            lines
                .chain((!setup.expect.is_empty()).then(|| format!("expect: {}", setup.expect)))
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    println!("{said}");
    Ok(())
}

pub(super) fn saved_test(name: &str) -> Result<TestSetup> {
    (Config::load()?.saved_tests.remove(name))
        .with_context(|| format!("no saved test setup is named {name}"))
}

/// How far a search reads at once, so a big region never stalls the game.
const CHUNK: usize = 1 << 20;

pub(super) fn mem(cmd: MemCommand) -> Result<()> {
    use live::Memory;
    let game = attach(matches!(cmd, MemCommand::Write { .. }))?;
    let main = game.global() - u64::from(crate::patch::OFFSETS.gamedata);
    let address = |text: &str| address(text, main);
    match cmd {
        MemCommand::Actors { prefix } => {
            let wanted = |name: &str| {
                prefix.is_empty() || prefix.iter().any(|p| name.starts_with(p.as_str()))
            };
            for actor in live::actors::actors(&game, main)? {
                if !wanted(&actor.name) {
                    continue;
                }
                let [x, y, z] = actor.pos;
                print!("{:#x} {} ({x:.2}, {y:.2}, {z:.2})", actor.at, actor.name);
                if let Some(life) = actor.life(&game, main)? {
                    print!(" life {}/{} at {:#x}", life.left, life.most, life.left_at());
                }
                println!();
            }
        }
        MemCommand::Keys => {
            for key in live::keys(&game, game.global())? {
                println!("{key}");
            }
        }
        MemCommand::Regions { min } => {
            for (start, size, prot) in game.regions()? {
                if size >= min << 20 {
                    let end = start + size as u64;
                    println!("{start:#x}..{end:#x} {:>8} KB prot {prot}", size >> 10);
                }
            }
        }
        MemCommand::Find {
            needle,
            writable,
            limit,
            from,
            to,
        } => {
            let needle = bytes(&needle)?;
            anyhow::ensure!(!needle.is_empty(), "nothing to find");
            let wanted = if writable { 3 } else { 1 };
            let from = from.as_deref().map(address).transpose()?.unwrap_or(0);
            let to = to.as_deref().map(address).transpose()?.unwrap_or(u64::MAX);
            let mut hits = 0;
            for (start, size, prot) in game.regions()? {
                if prot & wanted != wanted {
                    continue;
                }
                let end = (start + size as u64).min(to);
                let mut at = start.max(from);
                while at < end && hits < limit {
                    let len = CHUNK.min((end - at) as usize);
                    // Overlap chunks so a match across a seam is still seen.
                    let len = (len + needle.len() - 1).min((end - at) as usize);
                    let Ok(data) = game.read(at, len) else { break };
                    for i in memchr::memmem::find_iter(&data, &needle) {
                        println!("{:#x} (region {start:#x}, prot {prot})", at + i as u64);
                        hits += 1;
                        if hits == limit {
                            break;
                        }
                    }
                    at += CHUNK as u64;
                }
            }
            println!("{hits} found");
        }
        MemCommand::Read { at, len } => {
            let at = address(&at)?;
            for (i, row) in game.read(at, len)?.chunks(16).enumerate() {
                let hex: Vec<_> = row.iter().map(|b| format!("{b:02x}")).collect();
                let text: String = (row.iter())
                    .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
                    .collect();
                println!("{:#x}  {:<47}  {text}", at + 16 * i as u64, hex.join(" "));
            }
        }
        MemCommand::Write { at, bytes: hex } => {
            let (at, data) = (address(&at)?, bytes(&hex)?);
            println!("was {:02x?}", game.read(at, data.len())?);
            game.write(at, &data)?;
            println!("now {:02x?}", game.read(at, data.len())?);
        }
    }
    Ok(())
}

/// A 0x address, or `main+0x…` for an offset into `main`, which loads at
/// `main`.
fn address(text: &str, main: u64) -> Result<u64> {
    let (base, offset) = match text.strip_prefix("main+") {
        Some(offset) => (main, offset),
        None => (0, text),
    };
    let digits = offset.trim_start_matches("0x");
    let n =
        u64::from_str_radix(digits, 16).with_context(|| format!("{text} isn't a hex address"))?;
    Ok(base + n)
}

/// Hex bytes, or `s:` and text.
fn bytes(text: &str) -> Result<Vec<u8>> {
    if let Some(s) = text.strip_prefix("s:") {
        return Ok(s.as_bytes().to_vec());
    }
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    anyhow::ensure!(
        digits.len().is_multiple_of(2),
        "{text} has an odd number of hex digits"
    );
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).context("not hex"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game;
    use crate::known::flag;

    #[test]
    fn set_refuses_a_bad_enum_or_bool_before_looking_for_the_game() {
        let Some(romfs) = game::dev_dump() else {
            return;
        };
        for (name, value, valid) in [
            (
                crate::steps::saved(crate::known::flow::GOTO_HYRULE_CASTLE).as_str(),
                "Step1",
                format!("{}, Ready, Step01", flag::NOT_READY).as_str(),
            ),
            (flag::HYRULE_FIELD_ARRIVE_ENTRANCE, "yes", "true, false"),
        ] {
            let said = set(&romfs, name, value).unwrap_err().to_string();
            assert!(
                said.contains(format!("{value} isn't a value of {name}").as_str()),
                "{said}"
            );
            assert!(said.contains(valid), "{said}");
        }
    }

    #[test]
    fn a_needle_is_hex_or_text() {
        assert_eq!(bytes("0f 1e").unwrap(), [0x0f, 0x1e]);
        assert_eq!(bytes("0f1e").unwrap(), [0x0f, 0x1e]);
        assert_eq!(bytes("s:Ab").unwrap(), b"Ab");
        assert!(bytes("0f1").is_err());
        assert_eq!(address("0x7f21d600e0", 1).unwrap(), 0x007f_21d6_00e0);
        assert_eq!(address("main+0x10", 0x1000).unwrap(), 0x1010);
        assert!(address("main+zz", 0x1000).is_err());
    }
}
