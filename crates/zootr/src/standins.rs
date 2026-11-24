//! Stand-ins: new actors built from a template's pack that show and give
//! another item. A shop slot's sells it at a price, a pickup's gives it
//! where Link finds it.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_path};
use roead::sarc::SarcWriter;

use crate::known::{Name, actor, field, flag, item};
use crate::medallions;
use crate::settings::{DungeonKeys, NewGame};

mod tables;

pub(crate) use tables::PLAIN_DEAL;
pub use tables::Tables;
use tables::number;
pub(crate) use tables::{insert, row, row_id};

pub(crate) const COMPONENTS: &str = "Components";
pub(crate) const MODEL: &Name = field::MODEL_INFO_REF;
pub(crate) const ANIMATION: &Name = field::ANIMATION_REF;
const NAME: &Name = field::ACTOR_NAME_REF;
/// The animation archives an actor plays from, in its AnimationParam and
/// its `ActorInfo` row alike.
const ANIMATIONS: &Name = field::ANIMATION_RESOURCES;
const ARCHIVE: &Name = field::MODEL_PROJECT_NAME;
/// A shelf stand-in's animations: the spin the goods play when selected,
/// then a pose on the root bones every item model has. The medallions have
/// no animations and the songs none of their own, so on a shelf they stood
/// still, and the bombs' shop pose lifted Bolero well above its spot. The
/// maintainer picked the gold token's pose over each item's own (Mac,
/// 2026-11-15), save for an item a vanilla shop sells in a pose of its
/// own, which keeps it (2026-11-16): the token's laid the Deku Shield
/// flat. A third archive crashed the game as the shop loaded.
const SPIN: &Name = item::COMMON_ANIMATION;
const TOKEN_POSE: &Name = actor::OBJ_TOKEN_GOLD_ANIMATION;
/// An item's own pose archive is its model's name and this.
const OWN_POSE: &str = "_Animation";
/// Shop goods' `ActorInfo` rows start with this.
const GOODS: &str = "Shop_";
pub(crate) const POUCH: &Name = field::POUCH_CONTENT_REF;
pub(crate) const TABLE: &Name = field::GAME_PARAMETER_TABLE_REF;
pub(crate) const RESPAWN: &Name = field::ACTOR_GAME_DATA_SETTING;
pub(crate) const NO_RESPAWN: &str =
    "?Component/ActorGameDataSetting/NoRespawn.game__component__ActorGameDataSetting.bgyml";
pub(crate) const POUCH_ACTOR: &Name = field::POUCH_ACTOR;
pub(crate) const PRICE: &Name = field::PRICE_PARAM;
/// On in every item's pouch content and off in every field drop's
/// (`actor::OBJ_ARROW_FIRE`). A pickup stand-in with it on opened its get window
/// and gave nothing. Off, the Fire Arrow and Hookshot reached the
/// pouch (Mac, 2026-11-15). A shop grants by name, and keeps the item's.
pub(crate) const COUNT_CHECK: &Name = field::IS_COUNT_CHECK_ON_FIRST_GET;
pub(crate) const BUYING: &Name = field::BUYING_PRICE;
/// The template's own, which stays when the item's table replaces the rest.
pub(crate) const PUT_IN_POUCH: &Name = field::ITEM_PUT_IN_POUCH_PARAM;
pub(crate) const PARENT: &str = "$parent";
/// The table parts that make a pickup fall, bounce and land.
pub(crate) const FALLS: [&Name; 2] = [
    field::GAME_PHYSICS_CONTROLLER_PARAM,
    field::DROP_BOUND_PARAM,
];
const CATEGORY: &str = "Category";
pub(crate) const PHYSICS: &Name = field::PHYSICS_REF;
const CONTROLLER_SET: &Name = field::CONTROLLER_SET_PATH;
const SHAPES: &Name = field::SHAPE_NAME_PATH_ARY;
const SHAPE_NAME: &str = "Name";
const SHAPE_FILE: &Name = field::FILE_PATH;
/// The shapes an actor's body rests on, as against its sensors'.
const BODY: &str = "Body";
/// Shape members that aren't points, and stay when the shape is lowered.
const UNMOVED: [&str; 2] = ["Axis", "Tensor"];
const MODEL_MIN: &Name = field::MODEL_AABB_MIN;
const SPHERE: &str = "Sphere";
const CENTRE: &str = "Center";
const RADIUS: &str = "Radius";

pub(crate) const ACTOR_INFO: &str = "RSDB/ActorInfo.Product.111.rstbl.byml.zs";
pub(crate) const GAME_ACTOR_INFO: &str = "RSDB/GameActorInfo.Product.111.rstbl.byml.zs";
pub(crate) const POUCH_ACTOR_INFO: &str = "RSDB/PouchActorInfo.Product.111.rstbl.byml.zs";
/// `ActorInfo` fields that follow the model, the actor's name first.
const MODEL_FIELDS: [&Name; 6] = [
    flag::ACTOR_NAME,
    ANIMATIONS,
    field::FMDB_NAME,
    field::MODEL_PROJECT_NAME,
    field::MODEL_AABB_MAX,
    field::MODEL_AABB_MIN,
];
/// `ActorInfo` heaps, sized for the bigger of the template and the item.
const HEAPS: [&Name; 2] = [
    field::INSTANCE_HEAP_SIZE,
    field::PRE_ACTOR_RENDER_INFO_HEAP_SIZE,
];

/// One new actor.
pub struct StandIn<'a> {
    pub name: &'a str,
    /// The pack it's built on: the shelf goods, or a pickup.
    pub template: ActorPack,
    /// The seed's item's pack.
    pub item: ActorPack,
    /// What a shop sells it for. Without one it keeps the item's own table.
    pub price: Option<i32>,
}

/// A pack file's path as a `$parent` names it: `Work/…`, text YAML.
pub(crate) fn work(path: &str) -> String {
    let stem = path.strip_suffix(".bgyml").unwrap_or(path);
    format!("Work/{stem}.gyml")
}

/// An animation list's entry naming `name`.
fn archive(name: &str) -> Byml {
    let mut entry = BymlMap::default();
    entry.insert(ARCHIVE.get().into(), Byml::String(name.into()));
    Byml::Map(entry)
}

/// The pose `item` plays on a shelf: its model's own where some vanilla
/// shop goods play it, else the gold token's. By the model, so a chest's
/// form of the item finds the same pose.
pub(crate) fn shelf_pose(actor: &BymlDoc, item: &str) -> Result<String> {
    let pose = vanilla_pose(actor, item)?;
    Ok(match pose == **TOKEN_POSE && crate::shops::stands(item) {
        true => stand_pose(),
        false => pose,
    })
}

/// The Deku Shield's shelf pose, which stands a flat model up: what a
/// model lying flat in the token's pose plays instead.
pub(crate) fn stand_pose() -> String {
    own_pose(actor::WEAPON_SHIELD_DEKU)
}

/// The pose `item` plays before any flat model stands up: its own if a
/// vanilla shop sells it in one, else the token's.
pub(crate) fn vanilla_pose(actor: &BymlDoc, item: &str) -> Result<String> {
    let Some(model) = model(actor, item)? else {
        return Ok(TOKEN_POSE.to_string());
    };
    let own = own_pose(&model);
    for row in actor.root.as_array()? {
        if !row_id(row).starts_with(GOODS) {
            continue;
        }
        let Some(Byml::Array(archives)) = row.as_map()?.get(ANIMATIONS.get()) else {
            continue;
        };
        let named =
            |a: &Byml| (a.as_map().ok()?.get(ARCHIVE.get())?.as_string().ok()).map(|n| *n == own);
        if archives.iter().any(|a| named(a) == Some(true)) {
            return Ok(own);
        }
    }
    Ok(TOKEN_POSE.to_string())
}

/// The model `item` shows, from its `ActorInfo` row.
pub(crate) fn model(actor: &BymlDoc, item: &str) -> Result<Option<String>> {
    let item_row = row(actor, item)?;
    Ok((item_row.get(ARCHIVE.get()).and_then(|m| m.as_string().ok())).map(|m| m.to_string()))
}

/// The model file an actor pack shows: its model info's project and
/// model names, as in `Model/PROJECT.MODEL.bfres.zs`.
pub(crate) fn model_file(pack: &ActorPack) -> Result<Option<(String, String)>> {
    let info = resolved(pack, &path_of(pack, MODEL)?)?;
    let name = |key: &Name| {
        info.get(key.get())
            .and_then(|v| v.as_string().ok())
            .map(|v| v.to_string())
    };
    Ok(name(field::MODEL_PROJECT_NAME).zip(name(field::FMDB_NAME)))
}

/// The pose archive of `model`'s own.
pub(crate) fn own_pose(model: &str) -> String {
    format!("{model}{OWN_POSE}")
}

/// A param file's path, made into the reference that names it.
pub(crate) fn reference(path: &str) -> Byml {
    Byml::String(format!("?{path}").into())
}

/// The string under `key` in a param's `Components`.
pub(crate) fn component(param: &BymlMap, key: &str) -> Option<String> {
    crate::items::component(param, key).map(str::to_owned)
}

pub(crate) fn components(param: &BymlMap) -> Result<BymlMap> {
    Ok(param
        .get(COMPONENTS)
        .map(|c| c.as_map().cloned())
        .transpose()?
        .unwrap_or_default())
}

pub(crate) fn doc(map: BymlMap, like: &BymlDoc) -> BymlDoc {
    BymlDoc {
        root: Byml::Map(map),
        endian: like.endian,
        version: like.version,
    }
}

pub(crate) fn actor_path(pack: &str) -> String {
    format!("Actor/{pack}.engine__actor__ActorParam.bgyml")
}

/// The pack file a component of `pack`'s resolved ActorParam names.
pub(crate) fn path_of(pack: &ActorPack, key: &str) -> Result<String> {
    let param = pack
        .actor_param()?
        .with_context(|| format!("no {}", field::ACTOR_PARAM))?;
    let r = component(&param, key).with_context(|| format!("{} has no {key}", pack.name))?;
    Ok(pack_path(&r))
}

pub(crate) fn resolved(pack: &ActorPack, path: &str) -> Result<BymlMap> {
    pack.resolve(path)?
        .with_context(|| format!("{} has no {path}", pack.name))
}

/// A file of `pack`, borrowed.
fn bytes<'a>(pack: &'a ActorPack, path: &str) -> Result<&'a [u8]> {
    (pack.raw(path)).with_context(|| format!("{} has no {path}", pack.name))
}

pub(crate) fn raw(pack: &ActorPack, path: &str) -> Result<Vec<u8>> {
    Ok(bytes(pack, path)?.to_vec())
}

/// A file of a pack: its path, its bytes, and its bytes edited.
pub(crate) type Edit = (String, Vec<u8>, Vec<u8>);

/// The file of `pack`'s component `at`, run through `edit` with its
/// resolved values and its own.
pub(crate) fn component_edited(
    pack: &ActorPack,
    param: &BymlMap,
    at: &Name,
    edit: impl FnOnce(&BymlMap, &mut BymlMap) -> Result<()>,
) -> Result<Edit> {
    let file =
        component(param, at).with_context(|| format!("{} has no {}", pack.name, at.get()))?;
    let path = pack_path(&file);
    let old = raw(pack, &path)?;
    let mut doc = BymlDoc::parse(&old)?;
    edit(&resolved(pack, &path)?, doc.root.as_mut_map()?)?;
    Ok((path, old, doc.to_binary()))
}

/// A pouch content's `PouchActor`, naming actor `name`.
pub(crate) fn pouch_actor(name: &str) -> Byml {
    Byml::String(format!("Work/Actor/{name}.engine__actor__ActorParam.gyml").into())
}

/// Point `refs`' `key` at the file `lender`'s ActorParam names there, and
/// copy that file into `writer` when `lender`'s pack holds it.
pub(crate) fn lend(
    writer: &mut SarcWriter,
    refs: &mut BymlMap,
    key: &str,
    lender: &ActorPack,
) -> Result<()> {
    let param = lender
        .actor_param()?
        .with_context(|| format!("no {}", field::ACTOR_PARAM))?;
    let r = component(&param, key).with_context(|| format!("{} has no {key}", lender.name))?;
    if let Some((path, data)) = lender.find(&r) {
        writer.add_file(path, data.to_vec());
    }
    refs.insert(key.into(), Byml::String(r.into()));
    Ok(())
}

/// `param` with its `Components` entry `key` pointing at `path`.
pub(crate) fn with_ref(param: &BymlMap, key: &str, path: &str) -> Result<BymlMap> {
    let mut param = param.clone();
    let mut refs = components(&param)?;
    refs.insert(key.into(), reference(path));
    param.insert(COMPONENTS.into(), Byml::Map(refs));
    Ok(param)
}

/// `name`'s pack: `template`'s with `files` added, each `(path, doc, like)`
/// sized in the RESTBL like the template's file `like`.
pub(crate) fn write_clone(
    builder: &mut ModBuilder,
    template: &ActorPack,
    name: &str,
    files: impl IntoIterator<Item = (String, BymlDoc, String)>,
) -> Result<()> {
    let (pack, added) = cloned(template, files)?;
    for (path, like, like_size, size) in &added {
        builder.add_entry(path, like, *like_size, *size)?;
    }
    builder.add(
        &format!("Pack/Actor/{name}.pack.zs"),
        &pack,
        &format!("Pack/Actor/{}.pack.zs", template.name),
    )
}

/// `template` as actor `name` with `files` over it, built in memory, as
/// [`write_clone`] writes it.
pub(crate) fn clone_pack(
    template: &ActorPack,
    name: &str,
    files: impl IntoIterator<Item = (String, BymlDoc, String)>,
) -> Result<ActorPack> {
    ActorPack::from_bytes(name, cloned(template, files)?.0)
}

/// What a clone is: `template`'s pack with `files` added over it, and
/// each added file's path, the file it's sized like and both sizes.
type Cloned = (Vec<u8>, Vec<(String, String, usize, usize)>);

fn cloned(
    template: &ActorPack,
    files: impl IntoIterator<Item = (String, BymlDoc, String)>,
) -> Result<Cloned> {
    let mut writer = template.writer();
    let mut added = Vec::new();
    for (path, data, like) in files {
        let data = data.to_binary();
        added.push((
            path.clone(),
            like.clone(),
            bytes(template, &like)?.len(),
            data.len(),
        ));
        writer.add_file(path, data);
    }
    Ok((writer.to_binary(), added))
}

/// `item`'s pack, for a holder that shows or stands in for it: a key copy
/// or another player's item the seed builds, or the dump's.
pub(crate) fn item_pack(romfs: &Romfs, item: &str, new_game: &NewGame) -> Result<ActorPack> {
    if let Some(pack) = made_pack(romfs, item, new_game.keys)? {
        return Ok(pack);
    }
    match crate::foreign::pack(romfs, item, &new_game.foreign, new_game.keys)? {
        Some(pack) => Ok(pack),
        None => ActorPack::load(romfs, item),
    }
}

/// `item`'s pack as the seed writes it, if the seed builds it (a key or map
/// copy, a Bombchu Bag, the Magic Meter): the dump doesn't have it.
pub(crate) fn made_pack(romfs: &Romfs, item: &str, keys: DungeonKeys) -> Result<Option<ActorPack>> {
    if let Some(pack) = crate::key_tints::copy_pack(romfs, item, keys)? {
        return Ok(Some(pack));
    }
    if let Some(pack) = crate::map_copies::copy_pack(romfs, item)? {
        return Ok(Some(pack));
    }
    if let Some(pack) = crate::bombchu_bags::copy_pack(romfs, item)? {
        return Ok(Some(pack));
    }
    crate::magic::meter::copy_pack(romfs, item)
}

/// A new file, sized in the RESTBL like a file of a pack it copies.
struct NewFile<'a> {
    path: String,
    data: Vec<u8>,
    like_pack: &'a ActorPack,
    like: String,
}

impl StandIn<'_> {
    /// Whether it's a pickup, not a shop's: it has no price.
    fn is_pickup(&self) -> bool {
        self.price.is_none()
    }

    pub fn build(&self, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
        let name = self.name;
        let (template, item) = (&self.template, &self.item);
        let template_actor = actor_path(&template.name);
        let like = BymlDoc::parse(&raw(template, &template_actor)?)?;
        let given = item
            .actor_param()?
            .with_context(|| format!("the item has no {}", field::ACTOR_PARAM))?;
        let mut new_files = Vec::new();

        // What it gives: the item's own pouch content, naming the item.
        let item_pouch = path_of(item, POUCH)?;
        let mut pouch = resolved(item, &item_pouch)?;
        pouch
            .entry(POUCH_ACTOR.get().into())
            .or_insert_with(|| pouch_actor(&item.name));
        if self.is_pickup() {
            pouch.insert(COUNT_CHECK.get().into(), Byml::Bool(false));
            // Every take plays its get, so the pickup event graph marks it
            // ([`crate::pickups::taken`]), whatever Link already has.
            let many = field::PLAY_GET_EVENT_MANY_TIMES.get();
            pouch.insert(many.into(), Byml::Bool(true));
        }
        let new_pouch = format!(
            "Component/{}/{name}.game__component__PouchContentParam.bgyml",
            field::POUCH_CONTENT_PARAM
        );

        // The actor: the template's, showing and giving the item, once.
        let mut actor = like.root.as_map()?.clone();
        let mut refs = components(&actor)?;
        for key in [MODEL, ANIMATION, NAME] {
            if let Some(r) = component(&given, key) {
                refs.insert(key.get().into(), Byml::String(r.into()));
            }
        }
        refs.insert(POUCH.get().into(), reference(&new_pouch));
        refs.insert(RESPAWN.get().into(), Byml::String(NO_RESPAWN.into()));
        let shelf = if self.is_pickup() {
            None
        } else {
            Some(self.animate(name, tables, &like, &mut refs, &mut new_files)?)
        };
        if self.is_pickup() && self.falls()? {
            self.lift(name, tables, &mut refs, &mut new_files)?;
        }
        let new_table = self.table(name, &like, &mut new_files)?;
        refs.insert(TABLE.get().into(), reference(&new_table));
        actor.insert(COMPONENTS.into(), Byml::Map(refs));
        new_files.push(NewFile {
            path: actor_path(name),
            data: doc(actor, &like).to_binary(),
            like_pack: template,
            like: template_actor.clone(),
        });
        new_files.push(NewFile {
            path: new_pouch,
            data: doc(pouch, &like).to_binary(),
            like_pack: item,
            like: item_pouch.clone(),
        });

        // The template's files, but its actor and pouch content, then the
        // item's, but its actors and pouch content.
        let mut writer = template.writer();
        writer.remove_file(template_actor.as_str());
        writer.remove_file(path_of(template, POUCH)?.as_str());
        let shown = medallions::model_info(item)?;
        for file in item.files() {
            let skip = file.starts_with("Actor/") || file == item_pouch;
            if !skip && writer.get_file(file.as_str()).is_none() {
                let data = match &shown {
                    Some((path, info)) if *path == file => info.clone(),
                    _ => raw(item, &file)?,
                };
                writer.add_file(file.clone(), data);
            }
        }
        for f in new_files {
            let like_size = bytes(f.like_pack, &f.like)?.len();
            builder.add_entry(&f.path, &f.like, like_size, f.data.len())?;
            writer.add_file(f.path, f.data);
        }
        let pack = format!("Pack/Actor/{name}.pack.zs");
        builder.add(
            &pack,
            &writer.to_binary(),
            &format!("Pack/Actor/{}.pack.zs", template.name),
        )?;

        tables.add(name, &template.name, &item.name, self.price)?;
        match shelf {
            Some(animations) => tables.animate(name, animations),
            None => Ok(()),
        }
    }

    /// Its animations on a shelf. Returns them for its row.
    fn animate<'a>(
        &'a self,
        name: &str,
        tables: &Tables,
        like: &BymlDoc,
        refs: &mut BymlMap,
        new_files: &mut Vec<NewFile<'a>>,
    ) -> Result<Byml> {
        let template = &self.template;
        let pose = shelf_pose(&tables.actor, &self.item.name)?;
        let animations = Byml::Array(vec![archive(SPIN), archive(&pose)]);
        let template_param = path_of(template, ANIMATION)?;
        let mut param = resolved(template, &template_param)?;
        param.insert(ANIMATIONS.get().into(), animations.clone());
        let path = format!(
            "Component/{}/{name}.engine__component__AnimationParam.bgyml",
            field::ANIMATION_PARAM
        );
        refs.insert(ANIMATION.get().into(), reference(&path));
        new_files.push(NewFile {
            path,
            data: doc(param, like).to_binary(),
            like_pack: template,
            like: template_param,
        });
        Ok(animations)
    }

    /// Whether the template falls and lands, as drops do.
    fn falls(&self) -> Result<bool> {
        let own = resolved(&self.template, &path_of(&self.template, TABLE)?)?;
        Ok(FALLS.iter().all(|&key| component(&own, key).is_some()))
    }

    /// Its body, lowered so the item's model rests its base on the floor:
    /// the template's body sits under its own model, and a token's,
    /// medallion's or Hookshot's model is centred on its origin, so on the
    /// template's body they lay half in the floor (maintainer, 2026-11-14).
    fn lift<'a>(
        &'a self,
        name: &str,
        tables: &Tables,
        refs: &mut BymlMap,
        new_files: &mut Vec<NewFile<'a>>,
    ) -> Result<()> {
        let template = &self.template;
        let Some(base) = model_floor(&row(&tables.actor, &self.item.name)?) else {
            return Ok(());
        };
        let physics = path_of(template, PHYSICS)?;
        let set = resolved(template, &physics)?;
        let set = pack_path(
            set.get(CONTROLLER_SET.get())
                .context("no controller set")?
                .as_string()?,
        );
        let bodies = bodies(&BymlDoc::parse(&raw(template, &set)?)?.root)?;
        let mut bottom = f32::INFINITY;
        for (_, _, path) in &bodies {
            let shape = BymlDoc::parse(&raw(template, path)?)?;
            bottom = bottom.min(shape_bottom(shape.root.as_map()?).context("no sphere")?);
        }
        let by = bottom - base;
        if by <= 0.0 {
            return Ok(());
        }
        let mut shapes = Vec::new();
        for (_, shape, path) in &bodies {
            shapes.push(edited(template, path, &format!("{name}_{shape}"), |s| {
                lower(s, by);
                Ok(())
            })?);
        }
        let set = edited(template, &set, name, |set| {
            let entries = set
                .get_mut(SHAPES.get())
                .context("no shapes")?
                .as_mut_array()?;
            for ((at, _, _), lowered) in bodies.iter().zip(&shapes) {
                let entry = entries[*at].as_mut_map()?;
                entry.insert(
                    SHAPE_FILE.get().into(),
                    Byml::String(work(&lowered.path).into()),
                );
            }
            Ok(())
        })?;
        let physics = edited(template, &physics, name, |p| {
            p.insert(
                CONTROLLER_SET.get().into(),
                Byml::String(work(&set.path).into()),
            );
            Ok(())
        })?;
        refs.insert(PHYSICS.get().into(), reference(&physics.path));
        new_files.extend(shapes);
        new_files.extend([set, physics]);
        Ok(())
    }

    /// Its table, over the item's table, its parent, as the game's own
    /// shelves inherit theirs. A shop's keeps the shelf's pouching and a
    /// price of its own. A pickup's keeps how the template falls: songs and
    /// tokens have no table, and a stand-in that kept only the item's fell
    /// through the floor. A shop's that kept the shelf's falling crashed the
    /// game as the shop loaded, and so does a table written flat, its
    /// parents' parts copied in (Mac, 2026-11-14). Returns its path.
    fn table<'a>(
        &'a self,
        name: &str,
        like: &BymlDoc,
        new_files: &mut Vec<NewFile<'a>>,
    ) -> Result<String> {
        let (template, item) = (&self.template, &self.item);
        let template_table = path_of(template, TABLE)?;
        let own = resolved(template, &template_table)?;
        let mut parts = BymlMap::default();
        let kept: &[&Name] = if self.is_pickup() {
            &FALLS
        } else {
            &[PUT_IN_POUCH]
        };
        for &key in kept {
            if let Some(part) = component(&own, key) {
                parts.insert(key.get().into(), Byml::String(part.into()));
            }
        }
        if let Some(price) = self.price {
            let template_price =
                pack_path(&component(&own, PRICE).context("the template's table has no price")?);
            let mut price_param = resolved(template, &template_price)?;
            price_param.insert(BUYING.get().into(), Byml::I32(price));
            let new_price = format!(
                "{}/{}/{name}.game__pouchcontent__PriceParam.bgyml",
                field::GAME_PARAMETER,
                field::PRICE_PARAM
            );
            parts.insert(PRICE.get().into(), reference(&new_price));
            new_files.push(NewFile {
                path: new_price,
                data: doc(price_param, like).to_binary(),
                like_pack: template,
                like: template_price,
            });
        }
        let mut table = BymlMap::default();
        let given = item
            .actor_param()?
            .with_context(|| format!("the item has no {}", field::ACTOR_PARAM))?;
        match component(&given, TABLE) {
            Some(theirs) => {
                let parent = work(&pack_path(&theirs));
                table.insert(PARENT.into(), Byml::String(parent.into()));
            }
            // Rupees, bundles and songs have none: the template's category
            // stands alone.
            None => {
                let category = own
                    .get(CATEGORY)
                    .context("the template's table has no category")?;
                table.insert(CATEGORY.into(), category.clone());
            }
        }
        table.insert(COMPONENTS.into(), Byml::Map(parts));
        let new_table = format!(
            "{}/{}/{name}.engine__actor__GameParameterTable.bgyml",
            field::GAME_PARAMETER,
            field::GAME_PARAMETER_TABLE
        );
        new_files.push(NewFile {
            path: new_table.clone(),
            data: doc(table, like).to_binary(),
            like_pack: template,
            like: template_table,
        });
        Ok(new_table)
    }
}

/// `pack`'s file at `path`, edited, as a new file named for `name`.
fn edited<'a>(
    pack: &'a ActorPack,
    path: &str,
    name: &str,
    edit: impl FnOnce(&mut BymlMap) -> Result<()>,
) -> Result<NewFile<'a>> {
    let like = BymlDoc::parse(&raw(pack, path)?)?;
    let mut map = like.root.as_map()?.clone();
    edit(&mut map)?;
    Ok(NewFile {
        path: renamed(path, name),
        data: doc(map, &like).to_binary(),
        like_pack: pack,
        like: path.to_owned(),
    })
}

/// `path` with its file named `name`, keeping its folder and type.
pub(crate) fn renamed(path: &str, name: &str) -> String {
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    let kind = file.split_once('.').map_or("", |(_, kind)| kind);
    format!("{dir}/{name}.{kind}")
}

fn float(b: &Byml) -> Option<f32> {
    match b {
        Byml::Float(f) => Some(*f),
        Byml::Double(f) => Some(*f as f32),
        Byml::I32(_) | Byml::U32(_) | Byml::I64(_) | Byml::U64(_) => Some(number(b) as f32),
        _ => None,
    }
}

/// A controller set's body shapes: where each sits in its list, its name
/// and its file.
fn bodies(set: &Byml) -> Result<Vec<(usize, String, String)>> {
    let entries = set
        .as_map()?
        .get(SHAPES.get())
        .context("no shapes")?
        .as_array()?;
    let mut out = Vec::new();
    for (at, entry) in entries.iter().enumerate() {
        let entry = entry.as_map()?;
        let shape = entry.get(SHAPE_NAME).context("a shape without a name")?;
        let shape = shape.as_string()?.to_string();
        if shape.starts_with(BODY) {
            let file = entry.get(SHAPE_FILE.get()).context("no shape file")?;
            out.push((at, shape, pack_path(file.as_string()?)));
        }
    }
    ensure!(!out.is_empty(), "no body shape");
    Ok(out)
}

/// How far over its origin a shape's lowest sphere reaches down to.
fn shape_bottom(shape: &BymlMap) -> Option<f32> {
    let spheres = shape.get(SPHERE)?.as_array().ok()?;
    spheres
        .iter()
        .filter_map(|s| {
            let s = s.as_map().ok()?;
            let y = float(s.get(CENTRE)?.as_map().ok()?.get("Y")?)?;
            Some(y - float(s.get(RADIUS)?)?)
        })
        .reduce(f32::min)
}

/// How far above its origin `item`'s model starts, by its `ActorInfo`
/// row: the raw model for some items, posed in its own pose for others.
/// 0 when the row has no bounds.
pub(crate) fn table_floor(actor: &BymlDoc, item: &str) -> Result<f32> {
    Ok(model_floor(&row(actor, item)?).unwrap_or(0.0))
}

/// How far above its origin an `ActorInfo` row's model starts.
fn model_floor(row: &BymlMap) -> Option<f32> {
    float(row.get(MODEL_MIN.get())?.as_map().ok()?.get("Y")?)
}

/// Every point in a shape, `by` lower.
fn lower(shape: &mut BymlMap, by: f32) {
    let point = shape.contains_key("X") && shape.contains_key("Z");
    for (key, value) in shape.iter_mut() {
        match value {
            _ if point && key == "Y" => {
                if let Some(y) = float(value) {
                    *value = Byml::Float(y - by);
                }
            }
            Byml::Map(m) if !UNMOVED.contains(&key.as_str()) => lower(m, by),
            Byml::Array(a) => a.iter_mut().for_each(|v| {
                if let Byml::Map(m) = v {
                    lower(m, by)
                }
            }),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{actor, field, item};

    #[test]
    fn a_shield_keeps_its_own_shelf_pose_and_a_medallion_takes_the_tokens() {
        let Some(romfs) = dev_dump() else { return };
        let tables = Tables::load(&romfs).unwrap();
        let pose = |i: &Name| shelf_pose(&tables.actor, i).unwrap();
        let shield = format!("{}{OWN_POSE}", actor::WEAPON_SHIELD_DEKU);
        assert_eq!(pose(actor::WEAPON_SHIELD_DEKU), shield);
        assert_eq!(pose(actor::T_BOX_SHIELD_DEKU), shield);
        assert_eq!(pose(item::MEDAL_FIRE), TOKEN_POSE.get());
    }

    #[test]
    fn every_pool_item_can_stand_in() {
        let Some(romfs) = dev_dump() else { return };
        let tables = Tables::load(&romfs).unwrap();
        let mut items: Vec<String> = crate::checks::all(&romfs)
            .unwrap()
            .into_iter()
            .map(|c| c.item)
            .collect();
        items.extend([
            actor::T_BOX_RUPEE_BLUE.to_string(),
            actor::T_BOX_RUPEE_RED.to_string(),
        ]);
        items.sort();
        items.dedup();
        let mut unable = Vec::new();
        for item in &items {
            let ok = (|| -> Result<()> {
                let pack = ActorPack::load(&romfs, item)?;
                let param = pack
                    .actor_param()?
                    .with_context(|| format!("no {}", field::ACTOR_PARAM))?;
                component(&param, MODEL).context("no model")?;
                resolved(&pack, &path_of(&pack, POUCH)?)?;
                row(&tables.actor, item)?;
                row(&tables.pouch, item)?;
                Ok(())
            })();
            if let Err(e) = ok {
                unable.push(format!("{item}: {e:#}"));
            }
        }
        assert!(unable.is_empty(), "{unable:#?}");
    }
}
