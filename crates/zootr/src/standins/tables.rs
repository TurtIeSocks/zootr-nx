//! The RSDB tables a stand-in adds its rows to: the actor's, its game
//! actor's and its pouch's, each keyed by actor name.

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, BymlDoc, BymlMap, ModBuilder, Romfs};

use super::{
    ACTOR_INFO, ANIMATIONS, BUYING, GAME_ACTOR_INFO, HEAPS, MODEL_FIELDS, POUCH_ACTOR_INFO,
};
use crate::known::{field, flag};
use crate::rsdb;

/// The deal of an item the game stores and counts no other way.
pub(crate) const PLAIN_DEAL: &str = "None";

/// The RSDB tables that key a row by actor.
pub struct Tables {
    pub(crate) actor: BymlDoc,
    pub(crate) game: BymlDoc,
    pub(crate) pouch: BymlDoc,
    /// Whether anything changed a row, so `write` has tables to write.
    dirty: bool,
}

pub(crate) fn row_id(row: &Byml) -> &str {
    row.as_map()
        .ok()
        .and_then(|m| m.get(rsdb::ROW_ID))
        .and_then(|v| v.as_string().ok())
        .map_or("", |s| s.as_str())
}

/// Where row `id` sits in `rows`, sorted by id.
fn position(rows: &[Byml], id: &str) -> Result<usize> {
    let at = rows.partition_point(|r| row_id(r) < id);
    let found = rows.get(at).is_some_and(|r| row_id(r) == id);
    found.then_some(at).with_context(|| format!("no row {id}"))
}

pub(crate) fn row(doc: &BymlDoc, id: &str) -> Result<BymlMap> {
    let rows = doc.root.as_array()?;
    Ok(rows[position(rows, id)?].as_map()?.clone())
}

pub(crate) fn insert(doc: &mut BymlDoc, id: &str, mut row: BymlMap) -> Result<()> {
    row.insert(rsdb::ROW_ID.into(), Byml::String(id.into()));
    let rows = doc.root.as_mut_array()?;
    let at = rows.partition_point(|r| row_id(r) < id);
    ensure!(
        rows.get(at).is_none_or(|r| row_id(r) != id),
        "a row {id} already"
    );
    rows.insert(at, Byml::Map(row));
    Ok(())
}

pub(super) fn number(b: &Byml) -> i64 {
    match b {
        Byml::I32(n) => *n as i64,
        Byml::U32(n) => *n as i64,
        Byml::I64(n) => *n,
        Byml::U64(n) => *n as i64,
        _ => 0,
    }
}

/// `row` takes `theirs`'s `fields` and heaps big enough for its model.
fn take_model(row: &mut BymlMap, theirs: &BymlMap, fields: &[impl AsRef<str>]) {
    for field in fields {
        let field = field.as_ref();
        match theirs.get(field) {
            Some(v) => row.insert(field.into(), v.clone()),
            None => row.remove(field),
        };
    }
    for heap in HEAPS {
        if let Some(v) = theirs.get(heap.get())
            && row
                .get(heap.get())
                .is_none_or(|own| number(own) < number(v))
        {
            row.insert(heap.get().into(), v.clone());
        }
    }
}

impl Tables {
    pub fn load(romfs: &Romfs) -> Result<Self> {
        let load = |rel: &str| BymlDoc::parse(&romfs.read(rel)?).with_context(|| rel.to_owned());
        Ok(Self {
            actor: load(ACTOR_INFO)?,
            game: load(GAME_ACTOR_INFO)?,
            pouch: load(POUCH_ACTOR_INFO)?,
            dirty: false,
        })
    }

    /// Rows for `name`: the template's actor facts with the item's model,
    /// and the item's pouch facts, at `price` if it has one.
    pub(super) fn add(
        &mut self,
        name: &str,
        template: &str,
        item: &str,
        price: Option<i32>,
    ) -> Result<()> {
        let mut actor = row(&self.actor, template)?;
        take_model(&mut actor, &row(&self.actor, item)?, &MODEL_FIELDS);
        insert(&mut self.actor, name, actor)?;
        let game = row(&self.game, template)?;
        insert(&mut self.game, name, game)?;
        let mut pouch = row(&self.pouch, item)?;
        if let Some(price) = price {
            pouch.insert(BUYING.get().into(), Byml::I32(price));
        }
        insert(&mut self.pouch, name, pouch)?;
        self.dirty = true;
        Ok(())
    }

    /// `name`'s row shows `model`, a copy of `like`'s model.
    pub(crate) fn show(&mut self, name: &str, like: &str, model: &str) -> Result<()> {
        self.take(name, like, &MODEL_FIELDS[1..])?;
        self.point(name, model)
    }

    /// `name`'s row names `model`, its own fields otherwise kept.
    pub(crate) fn point(&mut self, name: &str, model: &str) -> Result<()> {
        self.point_at(name, model, model)
    }

    /// `name`'s row names `fmdb` in model project `project`.
    pub(crate) fn point_at(&mut self, name: &str, project: &str, fmdb: &str) -> Result<()> {
        let mut actor = row(&self.actor, name)?;
        actor.insert(field::FMDB_NAME.get().into(), Byml::String(fmdb.into()));
        actor.insert(
            field::MODEL_PROJECT_NAME.get().into(),
            Byml::String(project.into()),
        );
        self.replace(name, actor)
    }

    /// `name`'s row takes `like`'s `fields`, and heaps big enough for both.
    pub(crate) fn take(
        &mut self,
        name: &str,
        like: &str,
        fields: &[impl AsRef<str>],
    ) -> Result<()> {
        let mut actor = row(&self.actor, name)?;
        take_model(&mut actor, &row(&self.actor, like)?, fields);
        self.replace(name, actor)
    }

    /// `name`'s row with room for `like`'s heaps besides its own: an
    /// actor taking on another's parts as well as its model.
    pub(crate) fn add_heaps(&mut self, name: &str, like: &str) -> Result<()> {
        let mut actor = row(&self.actor, name)?;
        let theirs = row(&self.actor, like)?;
        for heap in HEAPS {
            let sum = number(actor.get(heap.get()).unwrap_or(&Byml::I32(0)))
                + number(theirs.get(heap.get()).unwrap_or(&Byml::I32(0)));
            let value = match actor.get(heap.get()) {
                Some(Byml::U32(_)) => Byml::U32(sum.try_into()?),
                _ => Byml::I32(sum.try_into()?),
            };
            actor.insert(heap.get().into(), value);
        }
        self.replace(name, actor)
    }

    /// `name`'s row plays `animations`.
    pub(super) fn animate(&mut self, name: &str, animations: Byml) -> Result<()> {
        let mut actor = row(&self.actor, name)?;
        actor.insert(ANIMATIONS.get().into(), animations);
        self.replace(name, actor)
    }

    pub(super) fn replace(&mut self, name: &str, actor: BymlMap) -> Result<()> {
        let rows = self.actor.root.as_mut_array()?;
        let at = position(rows, name)?;
        rows[at] = Byml::Map(actor);
        self.dirty = true;
        Ok(())
    }

    /// Rows for `name`, an actor like `like` in every way but its name.
    pub(crate) fn copy(&mut self, name: &str, like: &str) -> Result<()> {
        let (actor, game) = (row(&self.actor, like)?, row(&self.game, like)?);
        insert(&mut self.actor, name, actor)?;
        insert(&mut self.game, name, game)?;
        self.dirty = true;
        Ok(())
    }

    /// Rows for `name`, an item like `like` in every way but its name: its
    /// pouch row too, so it counts as `like` does once Link has it.
    pub(crate) fn copy_item(&mut self, name: &str, like: &str) -> Result<()> {
        self.copy(name, like)?;
        self.copy_pouch(name, like)
    }

    /// `name`'s actor row answers to its own name. A copied row answers to
    /// its source's, and the game asks that field who an actor is.
    pub(crate) fn name_itself(&mut self, name: &str) -> Result<()> {
        let rows = self.actor.root.as_mut_array()?;
        let at = position(rows, name)?;
        let row = rows[at].as_mut_map()?;
        row.insert(flag::ACTOR_NAME.get().into(), Byml::String(name.into()));
        self.dirty = true;
        Ok(())
    }

    /// `name`'s pouch row, `like`'s.
    pub(crate) fn copy_pouch(&mut self, name: &str, like: &str) -> Result<()> {
        let pouch = row(&self.pouch, like)?;
        insert(&mut self.pouch, name, pouch)?;
        self.dirty = true;
        Ok(())
    }

    /// The pouch rows of `names` take the deal of an item the game stores
    /// and counts no other way. Returns how many it found.
    pub(crate) fn plain_deal(&mut self, names: &[String]) -> Result<usize> {
        self.edit_pouch(|row| {
            let id = row.get(rsdb::ROW_ID).and_then(|v| v.as_string().ok());
            let hit = id.is_some_and(|id| names.iter().any(|n| n == id));
            if hit {
                row.insert(field::POUCH_SPECIAL_DEAL.get().into(), PLAIN_DEAL.into());
            }
            hit
        })
    }

    /// Hand `edit` every row of the pouch table, written with the rest.
    /// `edit` returns whether it changed the row. Returns how many changed.
    pub(crate) fn edit_pouch(
        &mut self,
        mut edit: impl FnMut(&mut BymlMap) -> bool,
    ) -> Result<usize> {
        let mut changed = 0;
        for row in self.pouch.root.as_mut_array()? {
            changed += edit(row.as_mut_map()?) as usize;
        }
        self.dirty |= changed > 0;
        Ok(changed)
    }

    /// Writes the tables if anything changed a row.
    pub fn write(&self, builder: &mut ModBuilder) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        for (rel, doc) in [
            (ACTOR_INFO, &self.actor),
            (GAME_ACTOR_INFO, &self.game),
            (POUCH_ACTOR_INFO, &self.pouch),
        ] {
            builder.write(rel, &doc.to_binary())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(ids: &[&str]) -> Vec<Byml> {
        let row = |id: &str| {
            let mut row = BymlMap::default();
            row.insert(rsdb::ROW_ID.into(), Byml::String(id.into()));
            Byml::Map(row)
        };
        ids.iter().map(|id| row(id)).collect()
    }

    #[test]
    fn a_missing_row_has_no_position_to_replace() {
        let rows = rows(&["FakeA", "FakeC"]);
        assert_eq!(position(&rows, "FakeA").unwrap(), 0);
        assert_eq!(position(&rows, "FakeC").unwrap(), 1);
        // Between rows and past the end, where `replace` once overwrote
        // FakeC or indexed out of bounds.
        assert!(position(&rows, "FakeB").is_err());
        assert!(position(&rows, "FakeD").is_err());
    }
}
