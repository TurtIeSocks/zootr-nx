//! Writing RSDB tables (`RSDB/<Table>.Product.111.rstbl.byml.zs`), the
//! game's row databases. Each is a BYML array of rows keyed by `__RowId`.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
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
/// own lookup. `scene::FIRE_TEMPLE_0022`, the jailed Goron's escape, has no flow
/// of its own and runs `scene::FIRE_TEMPLE_0020` from `EntryPoint8`.
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
/// three parts each (folder, name, suffix) sorted by their whole path, the
/// tags in `TagList`. `RankTable` counts, for each tag and each run of 32
/// actors, the actors with that tag up to the end of the run.
pub struct Tags {
    doc: BymlDoc,
    /// Each actor's path, as its three parts.
    paths: Vec<[String; 3]>,
    tags: Vec<String>,
    bits: Vec<u8>,
    added: bool,
}

/// The actors one rank entry covers.
const RANK_RUN: usize = 32;

impl Tags {
    pub fn load(romfs: &Romfs) -> Result<Self> {
        Self::parse(&romfs.read(&path(TAGS))?)
    }

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
        let paths = strings(field::PATH_LIST)?
            .chunks(3)
            .map(|p| [p[0].clone(), p[1].clone(), p[2].clone()])
            .collect();
        let bits = match root.get(field::BIT_TABLE.get()) {
            Some(Byml::BinaryData(b)) => b.clone(),
            _ => anyhow::bail!("no {}", field::BIT_TABLE),
        };
        let tags = strings(field::TAG_LIST)?;
        Ok(Self {
            doc,
            paths,
            tags,
            bits,
            added: false,
        })
    }

    fn row(&self, actor: &str) -> Option<usize> {
        self.paths.iter().position(|p| p[1] == actor)
    }

    fn on(&self, row: usize, tag: usize) -> bool {
        let bit = row * self.tags.len() + tag;
        self.bits[bit / 8] & (1 << (bit % 8)) != 0
    }

    /// `actor`'s tags, or `None` when the table doesn't list it.
    pub fn of(&self, actor: &str) -> Option<Vec<&str>> {
        let row = self.row(actor)?;
        Some(
            (0..self.tags.len())
                .filter(|&t| self.on(row, t))
                .map(|t| self.tags[t].as_str())
                .collect(),
        )
    }

    /// A row for `name` with `like`'s tags, in its sorted place. Nothing
    /// when the table doesn't list `like`: the copy has none either.
    pub fn copy(&mut self, name: &str, like: &str) -> Result<()> {
        let Some(from) = self.row(like) else {
            return Ok(());
        };
        let mut path = self.paths[from].clone();
        path[1] = name.into();
        let whole = |p: &[String; 3]| p.concat();
        let at = self.paths.partition_point(|p| whole(p) < whole(&path));
        ensure!(
            self.paths.get(at).is_none_or(|p| p[1] != name),
            "a tag row {name} already"
        );
        let on: Vec<bool> = (0..self.tags.len()).map(|t| self.on(from, t)).collect();
        let mut rows: Vec<Vec<bool>> = (0..self.paths.len())
            .map(|r| (0..self.tags.len()).map(|t| self.on(r, t)).collect())
            .collect();
        rows.insert(at, on);
        self.paths.insert(at, path);
        self.bits = pack(&rows, self.tags.len());
        self.added = true;
        Ok(())
    }

    /// Each tag's run counts, as `RankTable` holds them.
    fn ranks(&self) -> Vec<u32> {
        let runs = self.paths.len().div_ceil(RANK_RUN);
        let mut out = Vec::with_capacity(runs * self.tags.len());
        for tag in 0..self.tags.len() {
            let mut count = 0;
            for run in 0..runs {
                let rows = run * RANK_RUN..((run + 1) * RANK_RUN).min(self.paths.len());
                count += rows.filter(|&r| self.on(r, tag)).count() as u32;
                out.push(count);
            }
        }
        out
    }

    /// Writes the table if a row was added.
    pub fn write(&self, builder: &mut ModBuilder) -> Result<()> {
        if !self.added {
            return Ok(());
        }
        let mut doc = BymlDoc {
            root: self.doc.root.clone(),
            ..self.doc
        };
        let root = doc.root.as_mut_map()?;
        let paths = self.paths.iter().flatten().map(|p| Byml::String(p.as_str().into()));
        root.insert(field::PATH_LIST.get().into(), Byml::Array(paths.collect()));
        root.insert(field::BIT_TABLE.get().into(), Byml::BinaryData(self.bits.clone()));
        let ranks = self.ranks().iter().flat_map(|r| r.to_le_bytes()).collect();
        root.insert(field::RANK_TABLE.get().into(), Byml::BinaryData(ranks));
        builder.write(&path(TAGS), &doc.to_binary())
    }
}

/// The table of each actor's tags.
// Not a word of the dump's name tables.
const TAGS: &str = "Tag";

/// `rows` of `width` bits each, packed low bit first.
fn pack(rows: &[Vec<bool>], width: usize) -> Vec<u8> {
    let mut bits = vec![0u8; (rows.len() * width).div_ceil(8)];
    for (r, row) in rows.iter().enumerate() {
        for (t, &on) in row.iter().enumerate() {
            if on {
                let bit = r * width + t;
                bits[bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    bits
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
        let tags = Tags::load(&romfs).unwrap();
        assert_eq!(
            tags.of(actor::OBJ_ARROW_FIRE),
            Some(vec![actor::OBJ_FIRE_ARROW.get()])
        );
        assert_eq!(
            tags.of(actor::WEAPON_SWORD_BIGGORON),
            Some(vec![field::BIGGORON_SWORD.get()])
        );
        assert_eq!(tags.of("Obj_R_000"), None);
    }

    fn shipped_ranks(tags: &Tags) -> Vec<u32> {
        let root = tags.doc.root.as_map().unwrap();
        let Some(Byml::BinaryData(raw)) = root.get(field::RANK_TABLE.get()) else {
            panic!("no ranks");
        };
        raw.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect()
    }

    #[test]
    fn ranks_count_each_tags_actors_a_run_at_a_time_as_the_game_s_do() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let tags = Tags::load(&romfs).unwrap();
        assert_eq!(tags.ranks(), shipped_ranks(&tags));
        // Rebuilt from rows, the bits are the game's too.
        let width = tags.tags.len();
        let rows: Vec<Vec<bool>> = (0..tags.paths.len())
            .map(|r| (0..width).map(|t| tags.on(r, t)).collect())
            .collect();
        assert_eq!(pack(&rows, width), tags.bits);
    }

    #[test]
    fn a_copy_has_its_hosts_tags_in_its_sorted_place() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut tags = Tags::load(&romfs).unwrap();
        let (host, copy) = (actor::OBJ_POT_A_01.get(), "Obj_FakePot_A_01");
        let before = tags.paths.len();
        tags.copy(copy, host).unwrap();
        assert_eq!(tags.paths.len(), before + 1);
        assert_eq!(tags.of(copy), tags.of(host));
        assert!(tags.of(copy).unwrap().len() > 1);
        let whole: Vec<String> = tags.paths.iter().map(|p| p.concat()).collect();
        assert!(whole.is_sorted());
        // Every other actor keeps its tags.
        assert_eq!(
            tags.of(actor::OBJ_ARROW_FIRE),
            Some(vec![actor::OBJ_FIRE_ARROW.get()])
        );
        assert!(tags.copy(copy, host).is_err());
        // A host the table doesn't list leaves the copy without a row.
        tags.copy("Obj_FakeR_001", "Obj_R_000").unwrap();
        assert_eq!(tags.of("Obj_FakeR_001"), None);
    }
}
