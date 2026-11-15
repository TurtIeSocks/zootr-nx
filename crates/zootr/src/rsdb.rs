//! Writing RSDB tables (`RSDB/<Table>.Product.111.rstbl.byml.zs`), the
//! game's row databases. Each is a BYML array of rows keyed by `__RowId`.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use ounce_formats::{Byml, BymlDoc, ModBuilder, Romfs};

use crate::known::{Name, field};
use crate::standins::{insert, row};

/// Names each event's flow and entry point.
pub const EVENTS: &Name = field::EVENT_SETTING;
/// What the game knows of each event ahead of starting it: the flows it
/// calls and the events it leads to.
// Not a word of the dump's name tables.
pub const ANALYZED: &str = "GameAnalyzedEventInfo";
const ENTRY: &Name = field::ENTRY_POINT_NAME;
const FLOW_FILE: &Name = field::FLOWCHART_FILE_NAME;
/// Every table row's id field.
pub const ROW_ID: &str = "__RowId";

/// The romfs path of a table.
pub fn path(table: &str) -> String {
    format!("RSDB/{table}.Product.111.rstbl.byml.zs")
}

/// What an event name runs: its flow, and the entry point when the row
/// names one.
pub type Runs = (String, Option<String>);

/// Every event the game can start by name, and what it runs: the game's
/// own lookup. `Dm_FireTemple_0022`, the jailed Goron's escape, has no flow
/// of its own and runs `Dm_FireTemple_0020` from `EntryPoint8`.
pub fn events(romfs: &Romfs) -> Result<BTreeMap<String, Runs>> {
    let doc = BymlDoc::parse(&romfs.read(&path(EVENTS))?)?;
    let field = |r: &Byml, key: &str| -> Option<String> {
        let s = r.as_map().ok()?.get(key)?.as_string().ok()?;
        (!s.is_empty()).then(|| s.to_string())
    };
    let rows = doc.root.as_array()?.iter().filter_map(|r| {
        let flow = flow_name(&field(r, FLOW_FILE)?).to_owned();
        Some((field(r, ROW_ID)?, (flow, field(r, ENTRY))))
    });
    Ok(rows.collect())
}

/// The flow a `FlowchartFileName` names: `Work/Event/EventFlow/X.evfc` is
/// `X`.
pub fn flow_name(file: &str) -> &str {
    let stem = file.rsplit('/').next().unwrap_or(file);
    stem.split('.').next().unwrap_or(stem)
}

/// The `Tag` table: a bit per actor and tag, the actors in `PathList` as
/// three parts each (folder, name, suffix), the tags in `TagList`.
pub struct Tags {
    actors: Vec<String>,
    tags: Vec<String>,
    bits: Vec<u8>,
}

impl Tags {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let doc = BymlDoc::parse(data)?;
        let root = doc.root.as_map()?;
        let strings = |key: &str| -> Result<Vec<String>> {
            let list = root.get(key).with_context(|| format!("no {key}"))?;
            list.as_array()?
                .iter()
                .map(|v| Ok(v.as_string()?.to_string()))
                .collect()
        };
        let paths = strings(field::PATH_LIST)?;
        let actors = paths.chunks(3).map(|p| p[1].clone()).collect();
        let bits = match root.get(field::BIT_TABLE.get()) {
            Some(Byml::BinaryData(b)) => b.clone(),
            _ => anyhow::bail!("no {}", field::BIT_TABLE),
        };
        Ok(Self {
            actors,
            tags: strings(field::TAG_LIST)?,
            bits,
        })
    }

    /// `actor`'s tags, or `None` when the table doesn't list it.
    pub fn of(&self, actor: &str) -> Option<Vec<&str>> {
        let row = self.actors.iter().position(|a| a == actor)?;
        let on = |tag: usize| {
            let bit = row * self.tags.len() + tag;
            self.bits[bit / 8] & (1 << (bit % 8)) != 0
        };
        Some(
            (0..self.tags.len())
                .filter(|&t| on(t))
                .map(|t| self.tags[t].as_str())
                .collect(),
        )
    }
}

/// The event tables. The game starts an event only when both have its row,
/// so a placement naming any other event does nothing when it's talked to.
pub struct Events {
    pub(crate) setting: BymlDoc,
    pub(crate) analyzed: BymlDoc,
    added: usize,
}

impl Events {
    pub fn load(romfs: &Romfs) -> Result<Self> {
        let load = |table: &str| {
            let rel = path(table);
            BymlDoc::parse(&romfs.read(&rel)?).with_context(|| rel)
        };
        Ok(Self {
            setting: load(EVENTS)?,
            analyzed: load(ANALYZED)?,
            added: 0,
        })
    }

    /// Rows for event `name`, which runs the entry point of the same name
    /// and is like `like` in every other way.
    pub fn copy(&mut self, name: &str, like: &str) -> Result<()> {
        let mut setting = row(&self.setting, like)?;
        setting.insert(ENTRY.get().into(), Byml::String(name.into()));
        insert(&mut self.setting, name, setting)?;
        let analyzed = row(&self.analyzed, like)?;
        insert(&mut self.analyzed, name, analyzed)?;
        self.added += 1;
        Ok(())
    }

    /// Writes the tables if any event was added.
    pub fn write(&self, builder: &mut ModBuilder) -> Result<()> {
        if self.added == 0 {
            return Ok(());
        }
        builder.write(&path(EVENTS), &self.setting.to_binary())?;
        builder.write(&path(ANALYZED), &self.analyzed.to_binary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field};

    #[test]
    fn the_tag_table_reads_each_actors_tags() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let tags = Tags::parse(&romfs.read(&path("Tag")).unwrap()).unwrap();
        assert_eq!(
            tags.of(actor::OBJ_ARROW_FIRE),
            Some(vec![actor::OBJ_FIRE_ARROW.get()])
        );
        assert_eq!(
            tags.of(actor::WEAPON_SWORD_BIGGORON),
            Some(vec![field::BIGGORON_SWORD.get()])
        );
        assert_eq!(tags.of("Obj_Rando_000"), None);
    }
}
