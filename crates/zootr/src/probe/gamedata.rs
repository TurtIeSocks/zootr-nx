//! `probe key` and `probe data`: what GameData holds for a key or name.

use anyhow::{Result, ensure};
use ounce_formats::{Romfs, murmur3};

use crate::gamedata::GameData;
use crate::items::show;
use crate::known::flag;

pub(super) fn key(romfs: &Romfs, hash: u64) -> Result<()> {
    let doc = ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH)?)?;
    let keyed = doc.root.as_map()?["Data"].as_map()?[ounce_formats::KEYED].as_array()?;
    let found = keyed.iter().find(|e| {
        let h = e.as_map().ok().and_then(|m| m.get("Hash"));
        h.and_then(|h| crate::gamedata::int(h).ok()) == Some(hash as i64)
    });
    match found {
        Some(e) => println!("{e:?}"),
        None => println!("no key {hash}"),
    }
    Ok(())
}

pub(super) fn data(
    romfs: &Romfs,
    key: &str,
    holders: bool,
    exps: bool,
    member: bool,
) -> Result<()> {
    if exps {
        let survey = crate::flags::Survey::load(romfs, &crate::events::Flows::load(romfs)?)?;
        let gd = &survey.gamedata;
        let mut found: Vec<_> = gd
            .of_kind(flag::BOOL_EXP)
            .map(|h| Ok((gd.bare(h), gd.expr_of(h)?.unwrap_or_default())))
            .collect::<Result<_>>()?;
        found.retain(|(name, e)| name.contains(key) || e.contains(key));
        found.sort();
        found.iter().for_each(|(name, e)| println!("{name}: {e}"));
        return Ok(());
    }
    let gd = GameData::load(romfs)?;
    let hash = match u32::from_str_radix(key.trim_start_matches('#'), 16) {
        Ok(h) if key.len() <= 9 && gd.entry(h).is_some() => h,
        _ => murmur3(key.as_bytes()),
    };
    if holders || member {
        for s in gd.of_kind("Struct") {
            let members = gd.members(s)?.into_iter();
            for (m, v) in members.filter(|&(m, v)| if member { m == hash } else { v == hash }) {
                println!("{} member {} -> {}", gd.name(s), gd.name(m), gd.name(v));
            }
        }
        return Ok(());
    }
    let Some(entry) = gd.entry(hash) else {
        // The plain load only knows enum values. The flows name the rest.
        let gd = crate::recorder::gamedata(romfs)?;
        let like: Vec<_> = gd
            .names()
            .filter(|n| n.to_lowercase().contains(&key.to_lowercase()))
            .collect();
        ensure!(
            !like.is_empty(),
            "{} has no entry named or hashed {key}",
            flag::GAME_DATA
        );
        like.iter().for_each(|n| println!("{n}"));
        return Ok(());
    };
    println!("{} {}", gd.name(hash), gd.kind(hash).unwrap_or_default());
    for (k, v) in entry.as_map()?.iter().filter(|(k, _)| k.as_str() != "Hash") {
        println!("  {k}: {}", show(v));
    }
    if gd.kind(hash) == Some("Struct") {
        for (member, value) in gd.members(hash)? {
            println!("  {} -> {}", gd.name(member), gd.name(value));
        }
    }
    Ok(())
}
