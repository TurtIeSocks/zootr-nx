//! Our own copies of the game's actors: the prefix their names share, the
//! files a copy is written from, the lines its get reads, and what it sets
//! as it's found. Key colours, map copies, ammo, bags and the rest build on
//! these.

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlDoc, ModBuilder, murmur3};
use zootr_hook::Credit;

use crate::known::field;
use crate::standins::{
    MODEL, POUCH, POUCH_ACTOR, Tables, actor_path, clone_pack, doc, path_of, pouch_actor, raw,
    renamed, resolved, with_ref, write_clone,
};

/// How every item actor the seed makes begins, the dump's prefix and ours.
pub(crate) const MADE: &str = "Item_Rando_";

/// `template`'s pack as an own copy named `item` showing `model`, as
/// [`copy`] writes it.
pub(crate) fn own_pack(template: &ActorPack, item: &str, model: &str) -> Result<ActorPack> {
    let files = copied(template, item, model)?;
    clone_pack(template, item, files)
}

/// The lines a copy's get reads, each under the copy's name.
pub(crate) const TEXT_PARTS: [&str; 3] = ["_Name", "_GetMessage", "_GetCaption"];

/// The label of `item`'s name line, the first of its [`TEXT_PARTS`].
pub(crate) fn label(item: &str) -> String {
    format!("{item}{}", TEXT_PARTS[0])
}

/// The text file holding those lines.
pub(crate) const POUCH_TEXT: &str = "ActorMsg/PouchContent";

/// What a copy sets as it's found: a Bool true, an Int to a value, or one
/// more on an Int.
#[derive(Clone, Debug, PartialEq)]
pub enum Sets {
    Bool(String),
    Int(String, i32),
    Add(String, i32),
}

/// `sets` as the hook's table names it.
pub fn as_credits(sets: Vec<(String, Sets)>) -> Vec<(String, Credit)> {
    let hash = |name: &str| murmur3(name.as_bytes());
    let credit = |s: Sets| match s {
        Sets::Bool(name) => Credit::Bool(hash(&name)),
        Sets::Int(name, value) => Credit::Int(hash(&name), value),
        Sets::Add(name, n) => Credit::Add(hash(&name), n),
    };
    (sets.into_iter())
        .map(|(item, s)| (item, credit(s)))
        .collect()
}

/// `template` copied as `name`, showing `model`, with its rows. An `own`
/// copy answers to its own name (a ring, boss key, map or compass copy the
/// hook credits), any other to the template's, as the game's own key.
pub(crate) fn copy(
    builder: &mut ModBuilder,
    tables: &mut Tables,
    template: &ActorPack,
    name: &str,
    model: &str,
    own: bool,
) -> Result<()> {
    let files = match own {
        true => copied(template, name, model)?,
        false => pointed(template, name, model)?.into(),
    };
    write_clone(builder, template, name, files)?;
    tables.copy_item(name, &template.name)?;
    if own {
        tables.name_itself(name)?;
    }
    tables.show(name, &template.name, model)
}

/// `name`'s ActorParam and ModelInfo as `template`'s, the ModelInfo a file
/// of its own naming `model`, since packs sharing a file share whichever
/// loaded first. Each is `(path, doc, the template file it copies)`.
pub(crate) fn pointed(
    template: &ActorPack,
    name: &str,
    model: &str,
) -> Result<[(String, BymlDoc, String); 2]> {
    pointed_at(template, name, model, model)
}

/// [`pointed`], naming `fmdb` in model project `project`.
pub(crate) fn pointed_at(
    template: &ActorPack,
    name: &str,
    project: &str,
    fmdb: &str,
) -> Result<[(String, BymlDoc, String); 2]> {
    let actor_like = actor_path(&template.name);
    let info_like = path_of(template, MODEL)?;
    let info_path = renamed(&info_like, name);
    let actor_doc = BymlDoc::parse(&raw(template, &actor_like)?)?;
    let actor = with_ref(actor_doc.root.as_map()?, MODEL, &info_path)?;
    let info_doc = BymlDoc::parse(&raw(template, &info_like)?)?;
    let mut info = info_doc.root.as_map()?.clone();
    info.insert(field::FMDB_NAME.get().into(), Byml::String(fmdb.into()));
    info.insert(
        field::MODEL_PROJECT_NAME.get().into(),
        Byml::String(project.into()),
    );
    Ok([
        (actor_path(name), doc(actor, &actor_doc), actor_like),
        (info_path, doc(info, &info_doc), info_like),
    ])
}

/// A copy's files: `template`'s actor showing `model`, as [`pointed`], and
/// its own pouch content naming the copy. Left naming the template, the
/// pouch would hand over the game's key from any holder that resolves it.
pub(crate) fn copied(
    template: &ActorPack,
    name: &str,
    model: &str,
) -> Result<Vec<(String, BymlDoc, String)>> {
    let [(actor_at, actor, actor_like), info] = pointed(template, name, model)?;
    let pouch_like = path_of(template, POUCH)?;
    let pouch_at = renamed(&pouch_like, name);
    let pouch_doc = BymlDoc::parse(&raw(template, &pouch_like)?)?;
    let mut pouch = resolved(template, &pouch_like)?;
    pouch.insert(POUCH_ACTOR.get().into(), pouch_actor(name));
    let actor = doc(with_ref(actor.root.as_map()?, POUCH, &pouch_at)?, &actor);
    Ok(vec![
        (actor_at, actor, actor_like),
        info,
        (pouch_at, doc(pouch, &pouch_doc), pouch_like),
    ])
}
