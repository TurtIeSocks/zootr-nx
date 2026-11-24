//! What a seed with cows writes outside the flows: a clone pack and its
//! rows per plain cow, its event's rows, the placements swapped to it, and
//! the cows' Bools.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, Romfs, pack_path};

use super::{Cow, EVENT_NAME, PLAIN, REPLACE, event_of};
use crate::checks::GYAML;
use crate::known::field;
use crate::rsdb::Events;
use crate::standins::{
    TABLE, Tables, actor_path, component, doc, path_of, raw, with_ref, write_clone,
};

/// Placement fields, as `HASH.PATH=VALUE`: every placement of a plain cow
/// swapped to its clone.
pub fn fields(cows: &[Cow]) -> Vec<String> {
    let plain = cows.iter().filter(|c| c.plain());
    plain
        .flat_map(|c| {
            let name = c.clone_name();
            c.placements
                .iter()
                .map(move |p| format!("{}.{GYAML}={name}", p.hash))
        })
        .collect()
}

/// The Bools the cows' entries ask, one per cow.
pub fn flags(cows: &[Cow]) -> Vec<String> {
    cows.iter().map(Cow::flag).collect()
}

/// A clone pack and its actor rows per plain cow, and its event's rows.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    events: &mut Events,
    cows: &[Cow],
) -> Result<()> {
    let plain: Vec<&Cow> = cows.iter().filter(|c| c.plain()).collect();
    if plain.is_empty() {
        return Ok(());
    }
    let template = ActorPack::load(romfs, PLAIN)?;
    let template_event = event_of(&template)?;
    // A named cow's pack carries the same extra files a clone does, so its
    // rows fit the clone.
    let like = cows
        .iter()
        .find(|c| !c.plain())
        .map_or(&**PLAIN, |c| c.actor());
    for cow in plain {
        clone(builder, &template, &template_event, cow)?;
        tables.copy(&cow.clone_name(), like)?;
        events.copy(&cow.event, &template_event)?;
    }
    Ok(())
}

/// `cow`'s pack: the plain pack, plus its own actor, table and song event
/// param naming `cow.event`, as a named cow's pack has.
fn clone(
    builder: &mut ModBuilder,
    template: &ActorPack,
    template_event: &str,
    cow: &Cow,
) -> Result<()> {
    let name = cow.clone_name();
    let template_actor = actor_path(&template.name);
    let template_table = path_of(template, TABLE)?;
    let table_doc = BymlDoc::parse(&raw(template, &template_table)?)?;
    let template_replace = {
        let table = table_doc.root.as_map()?;
        let r = component(table, REPLACE).context("the plain table has no song event")?;
        pack_path(&r)
    };
    let new_table = format!(
        "{}/{}/{name}.engine__actor__GameParameterTable.bgyml",
        field::GAME_PARAMETER,
        field::GAME_PARAMETER_TABLE
    );
    let new_replace = format!(
        "{}/{}/{name}.game__ocarina__ReplaceEventParam.bgyml",
        field::GAME_PARAMETER,
        field::REPLACE_EVENT_PARAM
    );

    let actor_doc = BymlDoc::parse(&raw(template, &template_actor)?)?;
    let actor = with_ref(actor_doc.root.as_map()?, TABLE, &new_table)?;
    let table = with_ref(table_doc.root.as_map()?, REPLACE, &new_replace)?;
    let replace_doc = BymlDoc::parse(&raw(template, &template_replace)?)?;
    let mut replace = replace_doc.root.as_map()?.clone();
    let old = replace
        .get(&**EVENT_NAME)
        .and_then(|n| n.as_string().ok())
        .context("the plain song event names no event")?
        .to_string();
    let event = renamed_event(&old, template_event, &cow.event)?;
    replace.insert((&**EVENT_NAME).into(), Byml::String(event.into()));

    let files = [
        (actor_path(&name), doc(actor, &actor_doc), template_actor),
        (new_table, doc(table, &table_doc), template_table),
        (new_replace, doc(replace, &replace_doc), template_replace),
    ];
    write_clone(builder, template, &name, files)
}

/// The plain song event's name `old` with its event `from` swapped for `to`.
/// An error if `old` doesn't name `from`: the clone would run the plain cow's
/// event.
fn renamed_event(old: &str, from: &str, to: &str) -> Result<String> {
    let from = format!("/{from}.");
    ensure!(
        old.contains(&from),
        "the plain song event {old} has no {from}"
    );
    Ok(old.replacen(&from, &format!("/{to}."), 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clone_runs_its_own_song_event() {
        let old = "Fake/Path/FakeGiveMilk.game__FakeEvent.bgyml";
        assert_eq!(
            renamed_event(old, "FakeGiveMilk", "FakeCowSong").unwrap(),
            "Fake/Path/FakeCowSong.game__FakeEvent.bgyml"
        );
        assert!(renamed_event("FakeGiveMilk.bgyml", "FakeGiveMilk", "FakeCowSong").is_err());
    }
}
