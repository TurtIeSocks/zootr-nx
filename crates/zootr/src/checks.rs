//! Item checks found in the game's actor placement (Banc) files.
//!
//! Every placed actor with a `Dynamic.Drop__DropActor` param hands out that
//! actor when opened or broken: chests, pots, grass, rocks, hives. The same
//! actor (same `Hash`) can appear in both a `_Child_` and an `_Adult_` file,
//! so one check may span several files.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlDoc, Romfs, files_with_suffix};

use crate::known::{Name, actor, field};
use crate::world::{Presence, presence_of};

const BANC_SUFFIX: &str = ".bcett.byml.zs";
pub const DROP_ACTOR: &Name = field::DROP_DROP_ACTOR;
const GET_EVENT: &Name = field::GET_EVENT_NAME;
pub const CHECK_ACTOR: &Name = field::DROP_CHECK_ACTOR;
pub const SUBSTITUTE: &Name = field::DROP_DROP_ACTOR_SUBSTITUTE;
pub const GYAML: &str = "Gyaml";
/// Where a placement stands, `[x, y, z]`.
pub const TRANSLATE: &str = "Translate";
/// Big chests and the small chest each becomes.
const SHRINK: [(&Name, &Name); 2] = [
    (actor::OBJ_T_BOX_A_02, actor::OBJ_T_BOX_A_01),
    (actor::OBJ_T_BOX_A_02_NO_BAKE, actor::OBJ_T_BOX_A_01_NO_BAKE),
];
/// The only chest whose params declare `CHECK_ACTOR` and `SUBSTITUTE`. Every
/// other chest ignores them. It's small: there's no big one.
const SUBSTITUTE_CHEST: &Name = actor::OBJ_T_BOX_A_06;
/// The small chest a `SUBSTITUTE_CHEST` is built on.
const PLAIN_CHEST: &Name = actor::OBJ_T_BOX_A_01;
const NO_BAKE: &str = "_NoBake";

#[derive(Clone)]
pub struct ItemCheck {
    /// romfs-relative Banc file, or a gift's flow.
    pub file: String,
    /// The Banc folder it's placed in, e.g. `KokiriForest`.
    pub area: String,
    /// Placement hash, stable across the child and adult copies of a map.
    pub hash: u64,
    /// The placed actor, e.g. `Obj_TBox_A_02`, or a gift's giver.
    pub holder: String,
    /// The item it adds to the pool, e.g. `Item_Sword_Kokiri`: its drop, or
    /// its substitute when it checks for something else, as the Gerudo
    /// Training Ground's arrow chests do with a Red Rupee stand-in.
    pub item: String,
    /// Event played on opening, when the holder names one.
    pub get_event: Option<String>,
    /// Dropped instead when Link already has `Drop__CheckActor`.
    pub substitute: Option<String>,
    /// The flag the holder needs to exist at all, if any.
    pub presence: Option<Presence>,
}

/// Every Banc file, as romfs-relative paths.
pub fn banc_files(romfs: &Romfs) -> Result<Vec<String>> {
    files_with_suffix(&romfs.root().join("Banc"), BANC_SUFFIX)?
        .iter()
        .map(|p| romfs.rel(p))
        .collect()
}

pub fn load_banc(romfs: &Romfs, rel: &str) -> Result<BymlDoc> {
    BymlDoc::parse(&romfs.read(rel)?).with_context(|| format!("parsing {rel}"))
}

pub fn actors(root: &Byml) -> Result<&[Byml]> {
    match root.as_map()?.get("Actors") {
        Some(actors) => Ok(actors.as_array()?),
        None => Ok(&[]),
    }
}

fn actors_mut(root: &mut Byml) -> Result<&mut [Byml]> {
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
    let mut out = Vec::new();
    for file in banc_files(romfs)? {
        let doc = load_banc(romfs, &file)?;
        for actor in actors(&doc.root)? {
            let Some(dynamic) = actor.as_map()?.get("Dynamic") else {
                continue;
            };
            let Some(drop) = str_field(dynamic, DROP_ACTOR) else {
                continue;
            };
            let substitute = str_field(dynamic, SUBSTITUTE);
            let item = match (str_field(dynamic, CHECK_ACTOR), substitute) {
                (Some(check), Some(then)) if check != drop => then,
                _ => drop,
            };
            out.push(ItemCheck {
                file: file.clone(),
                area: file.split('/').nth(1).unwrap_or("?").to_owned(),
                hash: hash_of(actor).context("actor without Hash")?,
                holder: str_field(actor, GYAML).unwrap_or_default().to_owned(),
                item: item.to_owned(),
                get_event: str_field(dynamic, GET_EVENT).map(str::to_owned),
                substitute: substitute.map(str::to_owned),
                presence: presence_of(actor),
            });
        }
    }
    Ok(out)
}

/// Every check: placed actors, then gifts, then scrubs.
pub fn all(romfs: &Romfs) -> Result<Vec<ItemCheck>> {
    // Pickups deals the placed holders it knows, like pots, by setting.
    let mut out = scan(romfs)?;
    out.retain(|c| !crate::pickups::is_pickup(c));
    out.extend(crate::gifts::checks());
    out.extend(crate::scrubs::checks());
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
    out.retain(|c| !(new_game.fortress_keys && crate::fortress::is_carried(c)));
    out.extend(crate::pickups::dealt(romfs, new_game)?);
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
        let bake = match gyaml.ends_with(NO_BAKE) {
            true => NO_BAKE,
            false => "",
        };
        let guarded = gyaml.starts_with(&**SUBSTITUTE_CHEST);
        let Some(dynamic) = actor.as_mut_map()?.get_mut("Dynamic") else {
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

/// Remove the drop of each actor whose hash is in `hashes`, so it drops
/// nothing. Returns the hashes changed in this document.
pub fn clear_drops(root: &mut Byml, hashes: &[u64]) -> Result<Vec<u64>> {
    let mut changed = Vec::new();
    for actor in actors_mut(root)? {
        let Some(hash) = hash_of(actor).filter(|h| hashes.contains(h)) else {
            continue;
        };
        let Some(dynamic) = actor.as_mut_map()?.get_mut("Dynamic") else {
            continue;
        };
        if dynamic.as_mut_map()?.remove(&**DROP_ACTOR).is_some() {
            changed.push(hash);
        }
    }
    Ok(changed)
}

/// A hand-set placement field, `HASH.PATH=VALUE`: the placement, the field
/// path under it like `Gyaml` or `Dynamic.Drop__DropActor`, and the string.
pub fn field(arg: &str) -> Result<(u64, Vec<&str>, &str, bool)> {
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
    Ok((hash, path, value, raise))
}

/// Set each of `fields` on the actor it names, making any map on its path.
/// `HASH.PATH+=AMOUNT` raises a number already there instead, a number in
/// an array by its index (`Translate.1` is the height). Returns the hashes
/// changed in this document.
pub fn set_fields(root: &mut Byml, fields: &[String]) -> Result<Vec<u64>> {
    let fields = fields
        .iter()
        .map(|f| field(f))
        .collect::<Result<Vec<_>>>()?;
    let mut changed = Vec::new();
    for actor in actors_mut(root)? {
        let Some(hash) = hash_of(actor) else {
            continue;
        };
        for (_, path, value, raise) in fields.iter().filter(|f| f.0 == hash) {
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
            at.as_mut_map()?
                .insert((*last).into(), Byml::String((*value).into()));
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

pub fn is_big(holder: &str) -> bool {
    SHRINK.iter().any(|(big, _)| *big == holder)
}

/// Swap every big chest for a small one, keeping what it holds. Returns how
/// many changed.
pub fn shrink_chests(root: &mut Byml) -> Result<usize> {
    let mut shrunk = 0;
    for actor in actors_mut(root)? {
        let small = str_field(actor, GYAML)
            .and_then(|g| SHRINK.iter().find(|(big, _)| *big == g))
            .map(|(_, small)| *small);
        if let Some(small) = small {
            actor.as_mut_map()?.insert(GYAML.into(), small.get().into());
            shrunk += 1;
        }
    }
    Ok(shrunk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field};

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
            ("Dynamic", dynamic),
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
    fn big_chests_shrink_and_nothing_else_changes() {
        let mut root = Byml::from_iter([(
            "Actors",
            [
                &**actor::OBJ_T_BOX_A_02,
                &**actor::OBJ_T_BOX_A_02_NO_BAKE,
                &**actor::OBJ_T_BOX_A_03,
                // Not a word of the dump's name tables.
                "Obj_Pot",
            ]
            .into_iter()
            .map(|g| Byml::from_iter([(GYAML, g.into())]))
            .collect(),
        )]);
        assert_eq!(shrink_chests(&mut root).unwrap(), 2);
        assert_eq!(
            gyamls(&root),
            [
                actor::OBJ_T_BOX_A_01,
                actor::OBJ_T_BOX_A_01_NO_BAKE,
                actor::OBJ_T_BOX_A_03,
                "Obj_Pot"
            ]
        );
    }

    #[test]
    fn set_fields_writes_top_level_and_nested_fields() {
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
        assert_eq!(
            gyamls(&root),
            [actor::OBJ_SHIELD_HYLIAN, actor::ENEMY_SKULLTULA_GOLD]
        );
        let dynamic = &actors_mut(&mut root).unwrap()[1].as_map().unwrap()["Dynamic"];
        assert_eq!(
            str_field(dynamic, DROP_ACTOR),
            Some(&**actor::OBJ_HEART_PIECE)
        );
    }

    #[test]
    fn field_needs_a_hash_and_a_path() {
        assert!(field("Gyaml=X").is_err());
        assert!(field("1=X").is_err());
        assert_eq!(
            field("1.Dynamic.A=B").unwrap(),
            (1, vec!["Dynamic", "A"], "B", false)
        );
    }

    #[test]
    fn a_cleared_drop_leaves_the_actor_dropping_nothing() {
        let mut root = chest("Item_A", "Flag");
        assert!(clear_drops(&mut root, &[2]).unwrap().is_empty());
        assert_eq!(clear_drops(&mut root, &[1]).unwrap(), [1]);
        let dynamic = &actors(&root).unwrap()[0].as_map().unwrap()["Dynamic"];
        assert_eq!(str_field(dynamic, DROP_ACTOR), None);
        assert_eq!(str_field(dynamic, GET_EVENT), Some("Flag"));
        assert!(clear_drops(&mut root, &[1]).unwrap().is_empty());
    }

    fn event_after(item: &str, event: &str, new: &str) -> Option<String> {
        let mut root = chest(item, event);
        let drops = HashMap::from([(1, Drop::from(new.to_owned()))]);
        assert_eq!(set_drops(&mut root, &drops, |e| e == "Pose").unwrap(), [1]);
        let dynamic = &actors(&root).unwrap()[0].as_map().unwrap()["Dynamic"];
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
        let fields = |swap| {
            let mut root = chest("Item_A", "Flag");
            let drop = Drop {
                item: "Item_B".into(),
                swap,
            };
            set_drops(&mut root, &HashMap::from([(1, drop)]), |_| false).unwrap();
            let actor = &actors(&root).unwrap()[0];
            let dynamic = &actor.as_map().unwrap()["Dynamic"];
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
        let dynamic = &actor.as_map().unwrap()["Dynamic"];
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
                let Some(dynamic) = actor.as_map().unwrap().get("Dynamic") else {
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
