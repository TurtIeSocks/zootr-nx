//! Stand-ins: new actors built from a template's pack that show and give
//! another item. A shop slot's sells it at a price, a pickup's gives it
//! where Link finds it.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_path};

use crate::known::{Name, actor, field, flag, item};
use crate::{medallions, rsdb};

pub(crate) const COMPONENTS: &str = "Components";
pub(crate) const MODEL: &Name = field::MODEL_INFO_REF;
pub(crate) const ANIMATION: &Name = field::ANIMATION_REF;
const NAME: &Name = field::ACTOR_NAME_REF;
/// The animation archives an actor plays from, in its AnimationParam and
/// its `ActorInfo` row alike.
const ANIMATIONS: &Name = field::ANIMATION_RESOURCES;
const ARCHIVE: &Name = field::MODEL_PROJECT_NAME;
/// Every shelf stand-in's animations: the spin the goods play when
/// selected, and the gold token's pose on the `Root` and `Skl_Root` every
/// item model has. The medallions have no animations and the songs none of
/// their own, so on a shelf they stood still, and the bombs' `Pose_Shop`
/// lifted Bolero well above its spot. The maintainer picked the token's
/// pose over each item's own (Mac, 2026-11-15). Both, a third archive,
/// crashed the game as the shop loaded.
const SHELF_ANIMATIONS: [&Name; 2] = [item::COMMON_ANIMATION, actor::OBJ_TOKEN_GOLD_ANIMATION];
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

/// A param file's path, made into the reference that names it.
pub(crate) fn reference(path: &str) -> Byml {
    Byml::String(format!("?{path}").into())
}

/// The string under `key` in a param's `Components`.
pub(crate) fn component(param: &BymlMap, key: &str) -> Option<String> {
    let components = param.get(COMPONENTS)?.as_map().ok()?;
    Some(components.get(key)?.as_string().ok()?.to_string())
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

pub(crate) fn raw(pack: &ActorPack, path: &str) -> Result<Vec<u8>> {
    Ok(pack
        .raw(path)
        .with_context(|| format!("{} has no {path}", pack.name))?
        .to_vec())
}

/// A new file, sized in the RESTBL like a file of a pack it copies.
struct NewFile<'a> {
    path: String,
    data: Vec<u8>,
    like_pack: &'a ActorPack,
    like: String,
}

impl StandIn<'_> {
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
        pouch.entry(POUCH_ACTOR.get().into()).or_insert_with(|| {
            Byml::String(format!("Work/Actor/{}.engine__actor__ActorParam.gyml", item.name).into())
        });
        if self.price.is_none() {
            pouch.insert(COUNT_CHECK.get().into(), Byml::Bool(false));
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
        let shelf = match self.price {
            Some(_) => Some(self.animate(name, &like, &mut refs, &mut new_files)?),
            None => None,
        };
        if self.price.is_none() && self.falls()? {
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
        for f in &new_files {
            let like_size = raw(f.like_pack, &f.like)?.len();
            builder.add_entry(&f.path, &f.like, like_size, f.data.len())?;
            writer.add_file(f.path.clone(), f.data.clone());
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
        like: &BymlDoc,
        refs: &mut BymlMap,
        new_files: &mut Vec<NewFile<'a>>,
    ) -> Result<Byml> {
        let template = &self.template;
        let animations = Byml::Array(SHELF_ANIMATIONS.map(|a| archive(a)).to_vec());
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
        let kept: &[&Name] = match self.price {
            Some(_) => &[PUT_IN_POUCH],
            None => &FALLS,
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
        match path_of(item, TABLE) {
            Ok(theirs) => table.insert(PARENT.into(), Byml::String(work(&theirs).into())),
            // Rupees, bundles and songs have none: the template's category
            // stands alone.
            Err(_) => {
                let category = own
                    .get(CATEGORY)
                    .context("the template's table has no category");
                table.insert(CATEGORY.into(), category?.clone())
            }
        };
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
fn renamed(path: &str, name: &str) -> String {
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

/// How far under its origin an `ActorInfo` row's model reaches.
/// How far a placed stand-in rises to sit where its goods sat: how much
/// further below its origin the item's model reaches than the goods'. The
/// gold token's is centred, a shelf's bomb bundle sits on its base.
pub fn rise(tables: &Tables, goods: &str, item: &str) -> Result<f32> {
    let floor =
        |name| Ok::<_, anyhow::Error>(model_floor(&row(&tables.actor, name)?).unwrap_or(0.0));
    Ok((floor(goods)? - floor(item)?).max(0.0))
}

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

/// The RSDB tables that key a row by actor.
pub struct Tables {
    pub(crate) actor: BymlDoc,
    pub(crate) game: BymlDoc,
    pub(crate) pouch: BymlDoc,
    added: usize,
}

pub(crate) fn row_id(row: &Byml) -> &str {
    row.as_map()
        .ok()
        .and_then(|m| m.get(rsdb::ROW_ID))
        .and_then(|v| v.as_string().ok())
        .map_or("", |s| s.as_str())
}

pub(crate) fn row(doc: &BymlDoc, id: &str) -> Result<BymlMap> {
    let rows = doc.root.as_array()?;
    let at = rows.partition_point(|r| row_id(r) < id);
    let found = rows.get(at).filter(|r| row_id(r) == id);
    Ok(found
        .with_context(|| format!("no row {id}"))?
        .as_map()?
        .clone())
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

fn number(b: &Byml) -> i64 {
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
            added: 0,
        })
    }

    /// Rows for `name`: the template's actor facts with the item's model,
    /// and the item's pouch facts, at `price` if it has one.
    fn add(&mut self, name: &str, template: &str, item: &str, price: Option<i32>) -> Result<()> {
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
        self.added += 1;
        Ok(())
    }

    /// `name`'s row shows `model`, a copy of `like`'s model.
    pub(crate) fn show(&mut self, name: &str, like: &str, model: &str) -> Result<()> {
        self.take(name, like, &MODEL_FIELDS[1..])?;
        let mut actor = row(&self.actor, name)?;
        for field in [field::FMDB_NAME, field::MODEL_PROJECT_NAME] {
            actor.insert(field.get().into(), Byml::String(model.into()));
        }
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
    fn animate(&mut self, name: &str, animations: Byml) -> Result<()> {
        let mut actor = row(&self.actor, name)?;
        actor.insert(ANIMATIONS.get().into(), animations);
        self.replace(name, actor)
    }

    fn replace(&mut self, name: &str, actor: BymlMap) -> Result<()> {
        let rows = self.actor.root.as_mut_array()?;
        let at = rows.partition_point(|r| row_id(r) < name);
        rows[at] = Byml::Map(actor);
        self.added += 1;
        Ok(())
    }

    /// Rows for `name`, an actor like `like` in every way but its name.
    pub(crate) fn copy(&mut self, name: &str, like: &str) -> Result<()> {
        let (actor, game) = (row(&self.actor, like)?, row(&self.game, like)?);
        insert(&mut self.actor, name, actor)?;
        insert(&mut self.game, name, game)?;
        self.added += 1;
        Ok(())
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
        self.added += changed;
        Ok(changed)
    }

    /// Writes the tables if anything changed a row.
    pub fn write(&self, builder: &mut ModBuilder) -> Result<()> {
        if self.added == 0 {
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
    use crate::game::dev_dump;
    use crate::known::{actor, field};

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
