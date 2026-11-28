//! The carpenters take part in every step of the Spirit quest.
//!
//! The rescue flows never check the quest step: the only tie between a
//! carpenter and his rescue is the quest table, which lists the four, each
//! with his rescue schedule, at one step alone. At any other step a jailed
//! carpenter has no quest schedule and falls back to his default talk, his
//! thanks after the rescue, with no thief to summon (issue #550). That
//! strands him whenever the quest moves on before he's freed: the card
//! from anywhere, a mode that frees the others.
//!
//! So every step lists the four with the schedules that one step gives
//! them. A freed carpenter is deleted from his cell, so his schedule at a
//! later step never runs, and the valley camp's carpenters are other
//! actors: the vanilla rescue runs as it did.

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, BymlDoc, ModBuilder, Romfs};

use super::CELLS;
use crate::known::{field, flow};
use crate::rsdb;
use crate::standins::row_id;

/// The quest table.
const TABLE: &str = "Challenge";

/// The quest table with the four carpenters in every Spirit step.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let rel = rsdb::path(TABLE);
    let mut doc = BymlDoc::parse(&romfs.read(&rel)?)?;
    every_step(&mut doc.root)?;
    builder.write(&rel, &doc.to_binary())
}

/// Copies the carpenters' participant entries into each step of the Spirit
/// row of `table` that lacks them.
fn every_step(table: &mut Byml) -> Result<()> {
    let steps = steps(table)?;
    // The step that lists them: the game's one.
    let mut theirs = Vec::new();
    for step in steps.iter() {
        theirs = participants(step)?
            .iter()
            .filter(|p| carpenter(p))
            .cloned()
            .collect();
        if !theirs.is_empty() {
            break;
        }
    }
    ensure!(
        theirs.len() == CELLS.len(),
        "the Spirit quest lists {} carpenters at its step, not one each",
        theirs.len()
    );
    for step in steps.iter_mut() {
        let list = participants_mut(step)?;
        if !list.iter().any(carpenter) {
            list.extend(theirs.iter().cloned());
        }
    }
    Ok(())
}

/// The Spirit quest's steps in `table`.
fn steps(table: &mut Byml) -> Result<&mut Vec<Byml>> {
    let row = (table.as_mut_array()?.iter_mut())
        .find(|r| row_id(r).contains(&format!("/{}.", flow::SAGE_OF_SPIRIT)))
        .with_context(|| format!("the quest table has no {}", flow::SAGE_OF_SPIRIT))?;
    let steps = row.as_mut_map()?.get_mut(field::STEPS.get());
    Ok(steps
        .context("the Spirit quest has no steps")?
        .as_mut_array()?)
}

fn participants(step: &Byml) -> Result<&[Byml]> {
    let list = step.as_map()?.get(field::PARTICIPANTS.get());
    Ok(list.context("a step lists no participants")?.as_array()?)
}

fn participants_mut(step: &mut Byml) -> Result<&mut Vec<Byml>> {
    let list = step.as_mut_map()?.get_mut(field::PARTICIPANTS.get());
    Ok(list
        .context("a step lists no participants")?
        .as_mut_array()?)
}

/// Whether participant entry `p` is one of the jailed carpenters. Its
/// actor is named by its parameter file, `Work/Actor/<name>.<suffix>`.
fn carpenter(p: &Byml) -> bool {
    let actor = (p.as_map().ok())
        .and_then(|m| m.get(field::ACTOR_NAME.get()))
        .and_then(|a| a.as_string().ok());
    actor.is_some_and(|a| {
        let name = a.rsplit('/').next().unwrap_or(a).split('.').next();
        CELLS.iter().any(|c| name == Some(c.carpenter.get()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    /// The schedule each carpenter entry gives, by carpenter.
    fn schedules(step: &Byml) -> Vec<String> {
        let mut out: Vec<String> = (participants(step).unwrap().iter())
            .filter(|p| carpenter(p))
            .map(|p| {
                let m = p.as_map().unwrap();
                m[field::SCHEDULE.get()].as_string().unwrap().to_string()
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn every_spirit_step_lists_each_carpenter_once_with_his_rescue() {
        let Some(romfs) = dev_dump() else { return };
        let mut doc = BymlDoc::parse(&romfs.read(&rsdb::path(TABLE)).unwrap()).unwrap();
        let before: Vec<Vec<String>> = steps(&mut doc.root)
            .unwrap()
            .iter()
            .map(schedules)
            .collect();
        let rescue = before.iter().find(|s| !s.is_empty()).unwrap().clone();
        assert_eq!(rescue.len(), CELLS.len());
        assert_eq!(
            before.iter().filter(|s| !s.is_empty()).count(),
            1,
            "the game lists them at one step"
        );
        every_step(&mut doc.root).unwrap();
        let after = steps(&mut doc.root).unwrap();
        assert_eq!(after.len(), before.len());
        for step in after.iter() {
            assert_eq!(schedules(step), rescue);
        }
        // Run twice, nothing doubles.
        every_step(&mut doc.root).unwrap();
        for step in steps(&mut doc.root).unwrap().iter() {
            assert_eq!(schedules(step).len(), CELLS.len());
        }
    }
}
