//! Item checks found in the game's actor placement (Banc) files.
//!
//! Every placed actor with a `Dynamic.Drop__DropActor` param hands out that
//! actor when opened or broken: chests, pots, grass, rocks, hives. The same
//! actor (same `Hash`) can appear in both a `_Child_` and an `_Adult_` file,
//! so one check may span several files.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlDoc, BymlMap, Romfs, files_with_suffix, par_batches};

use crate::known::{Name, actor, field};
use crate::per_dump::PerDump;
use crate::world::{Presence, presence_of};

const BANC_SUFFIX: &str = ".bcett.byml.zs";
pub const DROP_ACTOR: &Name = field::DROP_DROP_ACTOR;
const GET_EVENT: &Name = field::GET_EVENT_NAME;
pub const CHECK_ACTOR: &Name = field::DROP_CHECK_ACTOR;
pub const SUBSTITUTE: &Name = field::DROP_DROP_ACTOR_SUBSTITUTE;
pub const GYAML: &str = "Gyaml";
/// A placement's own params.
pub const DYNAMIC: &str = "Dynamic";
/// Where a placement stands, `[x, y, z]`.
pub const TRANSLATE: &str = "Translate";
/// Each chest family by size ([`Size`] order), the `_NoBake` one apart.
/// The line ammo, Lens of Truth and iron chests have no other size.
const FAMILIES: [[&Name; 3]; 2] = [
    [
        actor::OBJ_T_BOX_A_01,
        actor::OBJ_T_BOX_A_02,
        actor::OBJ_T_BOX_A_03,
    ],
    [
        actor::OBJ_T_BOX_A_01_NO_BAKE,
        actor::OBJ_T_BOX_A_02_NO_BAKE,
        actor::OBJ_T_BOX_A_03_NO_BAKE,
    ],
];
/// The only chest whose params declare `CHECK_ACTOR` and `SUBSTITUTE`. Every
/// other chest ignores them. It's small: there's no big one.
const SUBSTITUTE_CHEST: &Name = actor::OBJ_T_BOX_A_06;
/// The small chest a `SUBSTITUTE_CHEST` is built on.
const PLAIN_CHEST: &Name = actor::OBJ_T_BOX_A_01;
const NO_BAKE: &str = "_NoBake";
/// The prefix every chest's actor name starts with.
pub const CHEST: &str = "Obj_TBox_";

#[derive(Clone)]
pub struct ItemCheck {
    /// romfs-relative Banc file, or a gift's flow.
    pub file: String,
    /// The Banc folder it's placed in, e.g. `KokiriForest`.
    pub area: String,
    /// Placement hash, stable across the child and adult copies of a map.
    pub hash: u64,
    /// The placed actor, e.g. a chest's actor, or a gift's giver.
    pub holder: String,
    /// The item it adds to the pool, e.g. the Kokiri Sword's actor: its drop, or
    /// its substitute when it checks for something else, as the Gerudo
    /// Training Ground's arrow chests do with a Red Rupee stand-in.
    pub item: String,
    /// Event played on opening, when the holder names one.
    pub get_event: Option<String>,
    /// Dropped instead when Link already has `Drop__CheckActor`.
    pub substitute: Option<String>,
    /// The flag the holder needs to exist at all, if any.
    pub presence: Option<Presence>,
    /// Whether a logic module makes the placement appear or watches it.
    pub watched: Watched,
    /// The dungeon room the placement names, if any: the room logic counts
    /// keys for (`key_rooms`).
    pub room: Option<u32>,
}

/// Whether a logic module makes a placement appear or watches it, and so
/// whether the seed can place copies of it (`pickups::steps`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Watched {
    #[default]
    No,
    /// Only by quiet groups: each copy gets duplicates of them, which run
    /// it as the originals run the holder (`pickups::groups`).
    Quiet,
    /// By a group that writes or plays, which the seed can't run twice.
    Loud,
}

/// Every Banc file, as romfs-relative paths.
pub fn banc_files(romfs: &Romfs) -> Result<Vec<String>> {
    files_with_suffix(&romfs.root().join("Banc"), BANC_SUFFIX)?
        .iter()
        .map(|p| romfs.rel(p))
        .collect()
}

/// The area folder a Banc file `rel` sits in, e.g. `KokiriForest`.
pub fn banc_area(rel: &str) -> Option<&str> {
    rel.split('/').nth(1)
}

/// [`banc_area`], or `"?"` for a path with no folder.
pub fn area_of(rel: &str) -> &str {
    banc_area(rel).unwrap_or("?")
}

/// Whether `holder`, a placed actor's name, is a chest.
pub fn is_chest(holder: &str) -> bool {
    holder.starts_with(CHEST)
}

/// Whether Banc file `rel` is a child map's.
pub fn is_child(rel: &str) -> bool {
    rel.contains("_Child_")
}

pub fn load_banc(romfs: &Romfs, rel: &str) -> Result<BymlDoc> {
    BymlDoc::parse(&romfs.read(rel)?).with_context(|| format!("parsing {rel}"))
}

/// Every Banc file with its path and its doc, read and parsed a batch at a
/// time on every core.
pub fn bancs(romfs: &Romfs) -> Result<impl Iterator<Item = (String, Result<BymlDoc>)> + '_> {
    Ok(load_bancs(romfs, banc_files(romfs)?))
}

/// Each of `files` with its doc, in order, read and parsed a batch at a
/// time on every core. A parsed doc is many times its file, so only a
/// batch is held at once.
pub fn load_bancs(
    romfs: &Romfs,
    files: Vec<String>,
) -> impl Iterator<Item = (String, Result<BymlDoc>)> + '_ {
    par_batches(files, |rel| load_banc(romfs, rel))
}

pub fn actors(root: &Byml) -> Result<&[Byml]> {
    match root.as_map()?.get("Actors") {
        Some(actors) => Ok(actors.as_array()?),
        None => Ok(&[]),
    }
}

pub(crate) fn actors_mut(root: &mut Byml) -> Result<&mut [Byml]> {
    match root.as_mut_map()?.get_mut("Actors") {
        Some(actors) => Ok(actors.as_mut_array()?),
        None => Ok(&mut []),
    }
}

pub fn str_field<'a>(map: &'a Byml, key: &str) -> Option<&'a str> {
    map.as_map()
        .ok()?
        .get(key)?
        .as_string()
        .ok()
        .map(|s| s.as_str())
}

/// Swap every string value `from` in `map` for `to`, keys left as they
/// are. Returns how many.
pub(crate) fn swap_strings(map: &mut BymlMap, from: &str, to: &str) -> usize {
    fn walk(b: &mut Byml, from: &str, to: &str) -> usize {
        match b {
            Byml::String(s) if s.as_str() == from => {
                *s = to.into();
                1
            }
            Byml::Map(m) => m.values_mut().map(|v| walk(v, from, to)).sum(),
            Byml::Array(a) => a.iter_mut().map(|v| walk(v, from, to)).sum(),
            _ => 0,
        }
    }
    map.values_mut().map(|v| walk(v, from, to)).sum()
}

/// A point as `x,y,z`, to a tenth.
pub fn xyz(p: [f32; 3]) -> String {
    format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2])
}

/// The dungeon room `actor` names, if it names one.
pub fn room_of(actor: &Byml) -> Option<u32> {
    let dynamic = actor.as_map().ok()?.get(DYNAMIC)?;
    let room = dynamic
        .as_map()
        .ok()?
        .get(field::DEFAULT_DUNGEON_ROOM_INDEX.get())?;
    room.as_int::<u32>().ok()
}

pub fn hash_of(actor: &Byml) -> Option<u64> {
    actor.as_map().ok()?.get("Hash")?.as_int::<u64>().ok()
}

/// A `[x, y, z]` field, like `Translate`.
pub fn vec3(actor: &Byml, key: &str) -> Option<[f32; 3]> {
    let v = actor.as_map().ok()?.get(key)?.as_array().ok()?;
    let f = |i: usize| v.get(i)?.as_float().ok();
    Some([f(0)?, f(1)?, f(2)?])
}

/// Distance on the ground plane, ignoring height.
pub fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

pub fn scan(romfs: &Romfs) -> Result<Vec<ItemCheck>> {
    Ok(scan_rooms(romfs)?.0)
}

/// [`scan`], and the room each placed scrub names, by its hash: a scrub
/// drops nothing, so its check is built from a table instead. Kept per
/// dump: a seed asks for every check more than once.
fn scan_rooms(romfs: &Romfs) -> Result<(Vec<ItemCheck>, HashMap<u64, u32>)> {
    static KEPT: PerDump<(Vec<ItemCheck>, HashMap<u64, u32>)> = PerDump::new();
    KEPT.get(romfs, || read_rooms(romfs))
}

fn read_rooms(romfs: &Romfs) -> Result<(Vec<ItemCheck>, HashMap<u64, u32>)> {
    let mut out = Vec::new();
    let mut rooms = HashMap::new();
    for (file, doc) in bancs(romfs)? {
        let doc = doc?;
        for actor in actors(&doc.root)? {
            let Some(dynamic) = actor.as_map()?.get(DYNAMIC) else {
                continue;
            };
            let Some(drop) = str_field(dynamic, DROP_ACTOR) else {
                let scrub = hash_of(actor).filter(|h| crate::scrubs::at(*h).is_some());
                if let (Some(hash), Some(room)) = (scrub, room_of(actor)) {
                    rooms.insert(hash, room);
                }
                continue;
            };
            let substitute = str_field(dynamic, SUBSTITUTE);
            let item = match (str_field(dynamic, CHECK_ACTOR), substitute) {
                (Some(check), Some(then)) if check != drop => then,
                _ => drop,
            };
            out.push(ItemCheck {
                file: file.clone(),
                area: area_of(&file).to_owned(),
                hash: hash_of(actor).context("actor without Hash")?,
                holder: str_field(actor, GYAML).unwrap_or_default().to_owned(),
                item: item.to_owned(),
                get_event: str_field(dynamic, GET_EVENT).map(str::to_owned),
                substitute: substitute.map(str::to_owned),
                presence: presence_of(actor),
                watched: Watched::No,
                room: room_of(actor),
            });
        }
    }
    Ok((out, rooms))
}

/// Every check: placed actors, then gifts, then scrubs.
pub fn all(romfs: &Romfs) -> Result<Vec<ItemCheck>> {
    // Pickups deals the placed holders it knows, like pots, by setting.
    let (mut out, rooms) = scan_rooms(romfs)?;
    out.retain(|c| !crate::pickups::is_pickup(c));
    out.extend(crate::gifts::checks());
    out.extend(crate::scrubs::checks().into_iter().map(|c| ItemCheck {
        room: rooms.get(&c.hash).copied(),
        ..c
    }));
    Ok(out)
}

/// The checks `new_game`'s settings deal.
pub fn dealt(romfs: &Romfs, new_game: &crate::settings::NewGame) -> Result<Vec<ItemCheck>> {
    dealt_of(romfs, all(romfs)?, new_game)
}

/// [`dealt`] from what [`all`] gave, so asking for several settings reads
/// the placed actors once.
pub fn dealt_of(
    romfs: &Romfs,
    mut out: Vec<ItemCheck>,
    new_game: &crate::settings::NewGame,
) -> Result<Vec<ItemCheck>> {
    out.retain(|c| {
        (new_game.scrubs || crate::scrubs::of(c).is_none())
            && crate::gifts::of(c).is_none_or(|g| g.dealt(&new_game.on))
    });
    if new_game.shop_slots > 0 {
        let slots = crate::shops::slots(romfs)?;
        out.extend(crate::shops::checks(&slots, new_game.shop_slots));
    }
    out.retain(|c| !(new_game.fortress_keys.moved() && crate::fortress::is_carried(c)));
    out.extend(crate::pickups::dealt(romfs, new_game)?);
    if new_game.freestanding {
        out.push(crate::goron_pot::check());
    }
    out.extend(crate::cows::dealt(romfs, new_game.cows)?);
    Ok(out)
}

/// What a placed actor drops.
#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    pub item: String,
    pub swap: Option<Swap>,
}

/// Once Link has `check`, drop `then` instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swap {
    pub check: &'static str,
    pub then: &'static str,
}

impl From<String> for Drop {
    fn from(item: String) -> Self {
        Self { item, swap: None }
    }
}

/// Point each actor whose hash is in `drops` at its new item. A moved item
/// loses its event when `shows_off` says the event only poses with the old
/// one. A drop with a swap becomes a `SUBSTITUTE_CHEST`, and one without
/// loses any vanilla swap, its `SUBSTITUTE_CHEST` turning back into the
/// plain chest it's built on. Returns the hashes changed in this document.
pub fn set_drops(
    root: &mut Byml,
    drops: &HashMap<u64, Drop>,
    mut shows_off: impl FnMut(&str) -> bool,
) -> Result<Vec<u64>> {
    let mut changed = Vec::new();
    for actor in actors_mut(root)? {
        let Some(hash) = hash_of(actor) else {
            continue;
        };
        let Some(Drop { item, swap }) = drops.get(&hash) else {
            continue;
        };
        let gyaml = str_field(actor, GYAML).unwrap_or_default();
        let bake = if gyaml.ends_with(NO_BAKE) {
            NO_BAKE
        } else {
            ""
        };
        let guarded = gyaml.starts_with(&**SUBSTITUTE_CHEST);
        let Some(dynamic) = actor.as_mut_map()?.get_mut(DYNAMIC) else {
            continue;
        };
        if str_field(dynamic, DROP_ACTOR).is_none() {
            continue;
        }
        let posed = str_field(dynamic, DROP_ACTOR) != Some(item.as_str())
            && str_field(dynamic, GET_EVENT).is_some_and(&mut shows_off);
        let dynamic = dynamic.as_mut_map()?;
        if posed {
            dynamic.remove(&**GET_EVENT);
        }
        dynamic.insert(DROP_ACTOR.get().into(), Byml::String(item.as_str().into()));
        let chest = match swap {
            Some(Swap { check, then }) => {
                dynamic.insert(CHECK_ACTOR.get().into(), Byml::String((*check).into()));
                dynamic.insert(SUBSTITUTE.get().into(), Byml::String((*then).into()));
                Some(SUBSTITUTE_CHEST)
            }
            None => {
                dynamic.remove(&**CHECK_ACTOR);
                dynamic.remove(&**SUBSTITUTE);
                guarded.then_some(PLAIN_CHEST)
            }
        };
        if let Some(chest) = chest {
            actor
                .as_mut_map()?
                .insert(GYAML.into(), format!("{chest}{bake}").into());
        }
        changed.push(hash);
    }
    Ok(changed)
}

/// Gives `actor` a Presence on `flag`, read lazily, so never mid-open.
/// Negated, it stands only while the flag is unset.
pub(crate) fn gate(actor: &mut Byml, flag: &str, negated: bool) -> Result<()> {
    let map = actor.as_mut_map()?;
    ensure!(!map.contains_key("Presence"), "{flag}: already gated");
    let mut presence = BymlMap::default();
    presence.insert(field::FLAG_NAME.get().into(), Byml::String(flag.into()));
    presence.insert(field::IS_CHECK_LAZY.get().into(), Byml::Bool(true));
    presence.insert(field::IS_NEGATION.get().into(), Byml::Bool(negated));
    map.insert("Presence".into(), Byml::Map(presence));
    Ok(())
}

/// Gate each placement in `gone` on its flag, so it's gone once the flag
/// is set. Returns the hashes changed in this document.
pub fn gone_once(root: &mut Byml, gone: &[(u64, String)]) -> Result<Vec<u64>> {
    let mut changed = Vec::new();
    for actor in actors_mut(root)? {
        let Some(hash) = hash_of(actor) else {
            continue;
        };
        for (_, flag) in gone.iter().filter(|g| g.0 == hash) {
            gate(actor, flag, true)?;
            changed.push(hash);
        }
    }
    Ok(changed)
}

/// Remove the drop of each actor whose hash is in `hashes`, so it drops
/// nothing. Returns the hashes changed in this document.
pub fn clear_drops(root: &mut Byml, hashes: &[u64]) -> Result<Vec<u64>> {
    clear_dynamic(root, |h| hashes.contains(h), DROP_ACTOR)
}

/// Remove the drop table of each actor whose hash is in `hashes`, so it
/// drops only its drop. Returns the hashes changed in this document.
pub fn clear_tables(root: &mut Byml, hashes: &HashSet<u64>) -> Result<Vec<u64>> {
    clear_dynamic(root, |h| hashes.contains(h), field::DROP_DROP_TABLE)
}

/// Remove `key` from the `Dynamic` of each actor whose hash is `of`.
/// Returns the hashes changed in this document.
fn clear_dynamic(root: &mut Byml, of: impl Fn(&u64) -> bool, key: &Name) -> Result<Vec<u64>> {
    let mut changed = Vec::new();
    for actor in actors_mut(root)? {
        let Some(hash) = hash_of(actor).filter(&of) else {
            continue;
        };
        let Some(dynamic) = actor.as_mut_map()?.get_mut(DYNAMIC) else {
            continue;
        };
        if dynamic.as_mut_map()?.remove(&**key).is_some() {
            changed.push(hash);
        }
    }
    Ok(changed)
}

/// A hand-set placement field, `HASH.PATH=VALUE` or `HASH.PATH+=AMOUNT`.
#[derive(Debug, PartialEq)]
pub struct FieldEdit<'a> {
    /// The placement.
    pub hash: u64,
    /// The field path under it, like `Gyaml` or `Dynamic.Drop__DropActor`.
    pub path: Vec<&'a str>,
    pub value: &'a str,
    /// `+=`: raise the number there by `value` instead of setting it.
    pub raise: bool,
}

/// Parse a hand-set placement field.
pub fn parse_field(arg: &str) -> Result<FieldEdit<'_>> {
    let (left, value) = arg
        .split_once('=')
        .context("fields look like HASH.PATH=VALUE")?;
    let (left, raise) = match left.strip_suffix('+') {
        Some(left) => (left, true),
        None => (left, false),
    };
    let mut path = left.split('.');
    let hash = path.next().unwrap_or_default();
    let hash = hash
        .parse()
        .with_context(|| format!("{hash} isn't a placement hash"))?;
    let path: Vec<&str> = path.collect();
    ensure!(!path.is_empty(), "{arg} names no field after the hash");
    Ok(FieldEdit {
        hash,
        path,
        value,
        raise,
    })
}

/// Set each of `fields` on the actor it names, making any map on its path.
/// A number in an array goes by its index (`Translate.1` is the height).
/// `HASH.PATH+=AMOUNT` raises a number already there instead. Returns the
/// hashes changed in this document.
pub fn set_fields(root: &mut Byml, fields: &[String]) -> Result<Vec<u64>> {
    let fields = fields
        .iter()
        .map(|f| parse_field(f))
        .collect::<Result<Vec<_>>>()?;
    let mut changed = Vec::new();
    for actor in actors_mut(root)? {
        let Some(hash) = hash_of(actor) else {
            continue;
        };
        for FieldEdit {
            path, value, raise, ..
        } in fields.iter().filter(|f| f.hash == hash)
        {
            if *raise {
                let by = value
                    .parse()
                    .with_context(|| format!("{value} isn't a number"))?;
                raised(actor, path, by).with_context(|| format!("{hash}.{}", path.join(".")))?;
                changed.push(hash);
                continue;
            }
            let (last, maps) = path.split_last().context("no field")?;
            let mut at = &mut *actor;
            for key in maps {
                at = at
                    .as_mut_map()?
                    .entry((*key).into())
                    .or_insert_with(|| Byml::Map(Default::default()));
            }
            match at {
                Byml::Array(items) => {
                    let slot = items.get_mut(last.parse::<usize>()?);
                    let slot = slot.with_context(|| format!("{hash} has no {}", path.join(".")))?;
                    let number = value.parse();
                    *slot = Byml::Float(number.with_context(|| format!("{value} isn't a number"))?);
                }
                map => {
                    map.as_mut_map()?
                        .insert((*last).into(), Byml::String((*value).into()));
                }
            }
            changed.push(hash);
        }
    }
    Ok(changed)
}

/// Raise the number at `path` in `at` by `by`.
fn raised(mut at: &mut Byml, path: &[&str], by: f32) -> Result<()> {
    for key in path {
        at = match at {
            Byml::Array(items) => items.get_mut(key.parse::<usize>()?),
            map => map.as_mut_map()?.get_mut(*key),
        }
        .with_context(|| format!("no {key}"))?;
    }
    match at {
        Byml::Float(f) => *f += by,
        other => bail!("{other:?} isn't a float"),
    }
    Ok(())
}

/// A chest's size, as its actor shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Small,
    Big,
    /// The boss key chest: the big chest's body with a model of its own.
    Boss,
}

const SIZES: [Size; 3] = [Size::Small, Size::Big, Size::Boss];

/// Chest `gyaml`'s family and size, when it comes in more than one size.
fn family_of(gyaml: &str) -> Option<(usize, Size)> {
    FAMILIES.iter().enumerate().find_map(|(f, family)| {
        let at = family.iter().position(|g| *g == gyaml)?;
        Some((f, SIZES[at]))
    })
}

/// Chest `gyaml`'s size, when it comes in more than one size.
pub fn size_of(gyaml: &str) -> Option<Size> {
    family_of(gyaml).map(|(_, size)| size)
}

pub fn is_big(holder: &str) -> bool {
    size_of(holder) == Some(Size::Big)
}

/// Swap each chest that comes in more than one size for the size `to`
/// gives from its own size and its drop, keeping what it holds. Returns how
/// many changed.
pub fn resize_chests(root: &mut Byml, to: impl Fn(Size, &str) -> Size) -> Result<usize> {
    let mut resized = 0;
    for actor in actors_mut(root)? {
        let Some((family, size)) = str_field(actor, GYAML).and_then(family_of) else {
            continue;
        };
        let dynamic = actor.as_map()?.get(DYNAMIC);
        let Some(drop) = dynamic.and_then(|d| str_field(d, DROP_ACTOR)) else {
            continue;
        };
        let new = to(size, drop);
        if new != size {
            let gyaml = FAMILIES[family][new as usize];
            actor.as_mut_map()?.insert(GYAML.into(), gyaml.get().into());
            resized += 1;
        }
    }
    Ok(resized)
}

/// All small's resize: every big chest small, the boss key chests kept.
pub fn shrunk(size: Size, _drop: &str) -> Size {
    match size {
        Size::Big => Size::Small,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field, item};

    fn chest(item: &str, event: &str) -> Byml {
        chest_in(
            actor::OBJ_T_BOX_A_02_NO_BAKE,
            [(DROP_ACTOR, item), (GET_EVENT, event)],
        )
    }

    fn chest_in<const N: usize>(gyaml: &str, dynamic: [(&str, &str); N]) -> Byml {
        let dynamic = Byml::from_iter(dynamic.map(|(k, v)| (k, v.into())));
        let actor = Byml::from_iter([
            ("Hash", Byml::from(1u64)),
            (GYAML, gyaml.into()),
            (DYNAMIC, dynamic),
        ]);
        Byml::from_iter([("Actors", Byml::from(vec![actor]))])
    }

    fn gyamls(root: &Byml) -> Vec<&str> {
        actors(root)
            .unwrap()
            .iter()
            .filter_map(|a| str_field(a, GYAML))
            .collect()
    }

    #[test]
    fn a_banc_path_names_its_area_folder() {
        let rel = "Banc/FakeArea/FakeMap_Child_Static.bcett.byml.zs";
        assert_eq!(banc_area(rel), Some("FakeArea"));
        assert_eq!(area_of(rel), "FakeArea");
        assert!(is_child(rel));
        assert_eq!(banc_area("loose"), None);
        assert_eq!(area_of("loose"), "?");
    }

    /// Actors of each `gyaml`, every one dropping `drop`.
    fn placed(gyamls: &[&str], drop: &str) -> Byml {
        let actor = |g: &str| {
            let dynamic = Byml::from_iter([(DROP_ACTOR.get(), Byml::String(drop.into()))]);
            Byml::from_iter([(GYAML, g.into()), (DYNAMIC, dynamic)])
        };
        Byml::from_iter([("Actors", gyamls.iter().map(|g| actor(g)).collect())])
    }

    #[test]
    fn all_small_shrinks_big_chests_and_nothing_else() {
        needs_names!();
        let mut root = placed(
            &[
                &**actor::OBJ_T_BOX_A_02,
                &**actor::OBJ_T_BOX_A_02_NO_BAKE,
                &**actor::OBJ_T_BOX_A_03,
                // Not a word of the dump's name tables.
                "Thing_A",
            ],
            actor::OBJ_HEART_PIECE.get(),
        );
        assert_eq!(resize_chests(&mut root, shrunk).unwrap(), 2);
        assert_eq!(
            gyamls(&root),
            [
                actor::OBJ_T_BOX_A_01,
                actor::OBJ_T_BOX_A_01_NO_BAKE,
                actor::OBJ_T_BOX_A_03,
                "Thing_A"
            ]
        );
    }

    #[test]
    fn a_chest_takes_any_size_of_its_family_and_keeps_its_bake() {
        needs_names!();
        let chests = [
            actor::OBJ_T_BOX_A_01,
            actor::OBJ_T_BOX_A_02_NO_BAKE,
            actor::OBJ_T_BOX_A_03,
            actor::OBJ_T_BOX_A_06,
            actor::OBJ_T_BOX_A_01_MEMO11,
        ];
        let sized = |size: Size| {
            let chests = chests.map(|g| g.get());
            let mut root = placed(&chests, actor::OBJ_HEART_PIECE.get());
            resize_chests(&mut root, |_, _| size).unwrap();
            gyamls(&root)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let kept = [actor::OBJ_T_BOX_A_06, actor::OBJ_T_BOX_A_01_MEMO11].map(|g| g.to_string());
        let want = |s: &Name, n: &Name, b: &Name| {
            [
                s.to_string(),
                n.to_string(),
                b.to_string(),
                kept[0].clone(),
                kept[1].clone(),
            ]
        };
        use actor::*;
        assert_eq!(
            sized(Size::Small),
            want(OBJ_T_BOX_A_01, OBJ_T_BOX_A_01_NO_BAKE, OBJ_T_BOX_A_01)
        );
        assert_eq!(
            sized(Size::Big),
            want(OBJ_T_BOX_A_02, OBJ_T_BOX_A_02_NO_BAKE, OBJ_T_BOX_A_02)
        );
        assert_eq!(
            sized(Size::Boss),
            want(OBJ_T_BOX_A_03, OBJ_T_BOX_A_03_NO_BAKE, OBJ_T_BOX_A_03)
        );
    }

    #[test]
    fn a_chest_is_sized_by_its_drop_and_one_without_a_drop_stays() {
        needs_names!();
        let mut root = placed(&[actor::OBJ_T_BOX_A_01.get()], item::HOOKSHOT.get());
        let gyaml = Byml::from_iter([(GYAML, actor::OBJ_T_BOX_A_01.to_string().into())]);
        root.as_mut_map()
            .unwrap()
            .get_mut("Actors")
            .unwrap()
            .as_mut_array()
            .unwrap()
            .push(gyaml);
        let big_for_hookshot = |_, drop: &str| match drop == item::HOOKSHOT.get() {
            true => Size::Big,
            false => Size::Small,
        };
        assert_eq!(resize_chests(&mut root, big_for_hookshot).unwrap(), 1);
        assert_eq!(
            gyamls(&root),
            [actor::OBJ_T_BOX_A_02, actor::OBJ_T_BOX_A_01]
        );
    }

    #[test]
    fn set_fields_writes_top_level_and_nested_fields() {
        needs_names!();
        let mut root = Byml::from_iter([(
            "Actors",
            Byml::Array(vec![
                Byml::from_iter([
                    ("Hash", Byml::from(1u64)),
                    (GYAML, actor::OBJ_HEART_PIECE.to_string().into()),
                ]),
                Byml::from_iter([
                    ("Hash", Byml::from(2u64)),
                    (GYAML, actor::ENEMY_SKULLTULA_GOLD.to_string().into()),
                ]),
            ]),
        )]);
        let fields = [
            format!("1.Gyaml={}", actor::OBJ_SHIELD_HYLIAN),
            format!(
                "2.Dynamic.{}={}",
                field::DROP_DROP_ACTOR,
                actor::OBJ_HEART_PIECE
            ),
        ];
        assert_eq!(set_fields(&mut root, &fields).unwrap(), [1, 2]);
        let raise = ["1.Translate.1+=0.25".to_owned()];
        assert!(set_fields(&mut root, &raise).is_err(), "1 has no Translate");
        actors_mut(&mut root).unwrap()[0]
            .as_mut_map()
            .unwrap()
            .insert(
                TRANSLATE.into(),
                Byml::Array(vec![1.0f32.into(), 2.0f32.into(), 3.0f32.into()]),
            );
        assert_eq!(set_fields(&mut root, &raise).unwrap(), [1]);
        assert_eq!(
            vec3(&actors_mut(&mut root).unwrap()[0], TRANSLATE),
            Some([1.0, 2.25, 3.0])
        );
        let put = ["1.Translate.0=-4.5".to_owned()];
        assert_eq!(set_fields(&mut root, &put).unwrap(), [1]);
        assert_eq!(
            vec3(&actors_mut(&mut root).unwrap()[0], TRANSLATE),
            Some([-4.5, 2.25, 3.0])
        );
        assert_eq!(
            gyamls(&root),
            [actor::OBJ_SHIELD_HYLIAN, actor::ENEMY_SKULLTULA_GOLD]
        );
        let dynamic = &actors_mut(&mut root).unwrap()[1].as_map().unwrap()[DYNAMIC];
        assert_eq!(
            str_field(dynamic, DROP_ACTOR),
            Some(&**actor::OBJ_HEART_PIECE)
        );
    }

    #[test]
    fn field_needs_a_hash_and_a_path() {
        assert!(parse_field("Gyaml=X").is_err());
        assert!(parse_field("1=X").is_err());
        assert_eq!(
            parse_field("1.Dynamic.A=B").unwrap(),
            FieldEdit {
                hash: 1,
                path: vec![DYNAMIC, "A"],
                value: "B",
                raise: false,
            }
        );
    }

    #[test]
    fn a_cleared_drop_leaves_the_actor_dropping_nothing() {
        let mut root = chest("Item_A", "Flag");
        assert!(clear_drops(&mut root, &[2]).unwrap().is_empty());
        assert_eq!(clear_drops(&mut root, &[1]).unwrap(), [1]);
        let dynamic = &actors(&root).unwrap()[0].as_map().unwrap()[DYNAMIC];
        assert_eq!(str_field(dynamic, DROP_ACTOR), None);
        assert_eq!(str_field(dynamic, GET_EVENT), Some("Flag"));
        assert!(clear_drops(&mut root, &[1]).unwrap().is_empty());
    }

    fn event_after(item: &str, event: &str, new: &str) -> Option<String> {
        let mut root = chest(item, event);
        let drops = HashMap::from([(1, Drop::from(new.to_owned()))]);
        assert_eq!(set_drops(&mut root, &drops, |e| e == "Pose").unwrap(), [1]);
        let dynamic = &actors(&root).unwrap()[0].as_map().unwrap()[DYNAMIC];
        assert_eq!(str_field(dynamic, DROP_ACTOR), Some(new));
        str_field(dynamic, GET_EVENT).map(str::to_owned)
    }

    #[test]
    fn a_moved_item_loses_an_event_that_only_poses_with_the_old_one() {
        assert_eq!(event_after("Sword", "Pose", "Bomb"), None);
        assert_eq!(
            event_after("Sword", "Pose", "Sword").as_deref(),
            Some("Pose")
        );
        assert_eq!(
            event_after("Sword", "Flag", "Bomb").as_deref(),
            Some("Flag")
        );
    }

    #[test]
    fn a_drop_with_a_swap_names_its_check_and_substitute_in_a_chest_that_reads_them() {
        needs_names!();
        let fields = |swap| {
            let mut root = chest("Item_A", "Flag");
            let drop = Drop {
                item: "Item_B".into(),
                swap,
            };
            set_drops(&mut root, &HashMap::from([(1, drop)]), |_| false).unwrap();
            let actor = &actors(&root).unwrap()[0];
            let dynamic = &actor.as_map().unwrap()[DYNAMIC];
            [
                (actor, GYAML),
                (dynamic, CHECK_ACTOR),
                (dynamic, SUBSTITUTE),
            ]
            .map(|(m, k)| str_field(m, k).map(str::to_owned))
        };
        assert_eq!(
            fields(None),
            [Some(actor::OBJ_T_BOX_A_02_NO_BAKE.to_string()), None, None]
        );
        let swap = Swap {
            check: "Item_D",
            then: "Item_C",
        };
        assert_eq!(
            fields(Some(swap)),
            [
                Some(actor::OBJ_T_BOX_A_06_NO_BAKE.to_string()),
                Some("Item_D".into()),
                Some("Item_C".into())
            ]
        );
    }

    #[test]
    fn a_drop_without_a_swap_clears_a_vanilla_one() {
        needs_names!();
        let mut root = chest_in(
            actor::OBJ_T_BOX_A_06_NO_BAKE,
            [
                (DROP_ACTOR, "Item_A"),
                (CHECK_ACTOR, "Item_A"),
                (SUBSTITUTE, "Item_B"),
            ],
        );
        let drops = HashMap::from([(1, Drop::from("Item_C".to_owned()))]);
        set_drops(&mut root, &drops, |_| false).unwrap();
        let actor = &actors(&root).unwrap()[0];
        let dynamic = &actor.as_map().unwrap()[DYNAMIC];
        assert_eq!(gyamls(&root), [actor::OBJ_T_BOX_A_01_NO_BAKE]);
        assert_eq!(
            [DROP_ACTOR, CHECK_ACTOR, SUBSTITUTE].map(|k| str_field(dynamic, k)),
            [Some("Item_C"), None, None]
        );
    }

    #[test]
    fn a_chest_guarding_its_drop_pools_the_substitute() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let checks = scan(&romfs).unwrap();
        let gerudo = checks.iter().find(|c| c.hash == 11683433230229591151);
        assert_eq!(gerudo.unwrap().item, actor::T_BOX_ARROW_BUNDLE_SMALL);
        let shield = checks.iter().find(|c| c.hash == 874970148260328653);
        assert_eq!(shield.unwrap().item, actor::T_BOX_SHIELD_DEKU);
    }

    #[test]
    fn vanilla_substitute_chests_name_their_check_actor() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut subs = 0;
        for rel in banc_files(&romfs).unwrap() {
            let doc = load_banc(&romfs, &rel).unwrap();
            for actor in actors(&doc.root).unwrap() {
                let Some(dynamic) = actor.as_map().unwrap().get(DYNAMIC) else {
                    continue;
                };
                if str_field(dynamic, SUBSTITUTE).is_some() {
                    assert!(str_field(dynamic, CHECK_ACTOR).is_some(), "{rel}");
                    let chest = str_field(actor, GYAML).unwrap();
                    assert!(chest.starts_with(&**SUBSTITUTE_CHEST), "{rel}: {chest}");
                    subs += 1;
                }
            }
        }
        assert!(subs > 0);
    }
}
