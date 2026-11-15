//! `zootr selftest`: round-trips the dump's files through the format code
//! and reports anything that doesn't survive.

use anyhow::{Context, Result, ensure};
use ounce_formats::{BymlDoc, Flowchart, Restbl, Romfs, frame_dict_id};

use crate::known::flag;
use crate::{checks, flows, gamedata};

/// Recompress `contents` as a mod would and check the game can read it back
/// the way it reads the shipped file: same dictionary, same bytes out.
fn check_repack(romfs: &Romfs, rel: &str, contents: &[u8]) -> Result<()> {
    let packed = romfs.dicts().compress(rel, contents)?;
    let shipped = std::fs::read(romfs.root().join(rel))?;
    let (ours, theirs) = (frame_dict_id(&packed), frame_dict_id(&shipped));
    ensure!(
        ours == theirs,
        "{rel}: written with zstd dictionary {ours}, game ships {theirs}"
    );
    ensure!(
        romfs.dicts().decompress(rel, &packed)? == contents,
        "{rel}: zstd round-trip failed"
    );
    Ok(())
}

pub(super) fn selftest(romfs: &Romfs) -> Result<()> {
    let restbl_rel = romfs.restbl_rel()?;
    let restbl_raw = romfs.read(&restbl_rel)?;
    let restbl = Restbl::parse(&restbl_raw)?;
    ensure!(
        restbl.to_binary() == restbl_raw,
        "RESTBL did not round-trip byte for byte"
    );
    check_repack(romfs, &restbl_rel, &restbl_raw)?;
    println!("ok  RESTBL round-trips byte for byte and repacks ({restbl_rel})");

    let mut files = checks::banc_files(romfs)?;
    files.push(gamedata::PATH.to_owned());
    let (mut identical, mut semantic, mut max_growth) = (0, 0, 0isize);
    for rel in &files {
        let raw = romfs.read(rel)?;
        let doc = BymlDoc::parse(&raw)?;
        let out = doc.to_binary();
        if out == raw {
            identical += 1;
        } else {
            ensure!(
                BymlDoc::parse(&out)?.root == doc.root,
                "{rel}: BYML changed meaning on rewrite"
            );
            semantic += 1;
            max_growth = max_growth.max(out.len() as isize - raw.len() as isize);
        }
        check_repack(romfs, rel, &out)?;
        ensure!(restbl.get(rel).is_some(), "{rel}: no RESTBL entry");
    }
    println!(
        "ok  {} BYML files: {identical} byte-identical, {semantic} equal after re-parse \
         (largest growth {max_growth} B); all repack with the shipped dictionary and \
         have RESTBL entries",
        files.len()
    );

    let names = flows::names(romfs)?;
    let (mut charts, mut events) = (0, 0);
    for name in &names {
        let raw = romfs.read(&flows::rel(name))?;
        let Some(flow) = Flowchart::parse(&raw).with_context(|| format!("parsing {name}"))? else {
            continue;
        };
        ensure!(
            flow.to_bytes() == raw,
            "{name}: flow didn't round-trip byte for byte"
        );
        charts += 1;
        events += flow.events.len();
    }
    println!(
        "ok  {} event flow files: {charts} flowcharts ({events} events, every index in range) \
         round-trip byte for byte, {} timeline-only",
        names.len(),
        names.len() - charts
    );

    let exps = gamedata::GameData::load(romfs)?.check_all()?;
    println!(
        "ok  {exps} {} BoolExps decode to one value each",
        flag::GAME_DATA
    );
    Ok(())
}
