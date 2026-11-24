//! `probe hurts`: what each kind of hit does to an enemy, from its damage
//! table, with the damage types named. Which of Link's weapons beat it.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use ounce_formats::murmur3;
use ounce_formats::{ActorPack, Byml, Romfs};

use super::arsenal::file;
use crate::items::show;
use crate::known::field;

/// The damage types and elements an attack carries, as `main` names them.
/// Matched to each weapon's own attack file: a bomb's blast is `Bomb`, the
/// slingshot's seed `SlingshotBullet`, a Deku Nut `Body` with the element
/// `DekuNut`, a Light Arrow `Arrow` with the element `Light`. The sword's
/// slashes are `SmallWeapon` and its spin and charged swings `LargeWeapon`.
const NAMES: &[&str] = &[
    "SmallWeapon",
    "LargeWeapon",
    "ThrustWeapon",
    "Hammer",
    "Arrow",
    "SlingshotBullet",
    "Boomerang",
    "Hookshot",
    "Bomb",
    "Body",
    "Fire",
    "Ice",
    "Water",
    "Light",
    "DekuNut",
    "Beam",
    "Shockwave",
    "Spike",
    "Impulse",
    "Shield",
    "Bottle",
    "Durability",
    "Undefined",
];

/// A damage type's name, or its hash when `main` names none we know.
fn named(hash: u32) -> String {
    let hit = NAMES.iter().find(|n| murmur3(n.as_bytes()) == hash);
    hit.map_or_else(|| format!("#{hash:08x}"), |n| (*n).to_owned())
}

fn hash(v: &Byml) -> Option<u32> {
    match v {
        Byml::U32(n) => Some(*n),
        Byml::I32(n) => Some(*n as u32),
        _ => None,
    }
}

/// `v`'s hashes as names joined with `|`, `-` for none.
fn names(v: Option<&Byml>) -> String {
    let all: Vec<String> = (v.and_then(|v| v.as_array().ok()).into_iter().flatten())
        .filter_map(hash)
        .map(named)
        .collect();
    if all.is_empty() {
        "-".into()
    } else {
        all.join("|")
    }
}

/// `actor`'s Life and how each damage type multiplies a hit, lowest first,
/// then its exceptions (a Light Arrow kills a Stalfos outright).
fn hurts(romfs: &Romfs, actor: &str) -> Result<()> {
    let pack = ActorPack::load(romfs, actor).with_context(|| actor.to_owned())?;
    let life = file(&pack, actor, "LifeParameters")
        .and_then(|p| pack.resolve(&p).ok().flatten())
        .and_then(|m| m.get("MaxLife").map(show));
    let Some(path) = file(&pack, actor, "DamageParameters") else {
        println!("{actor}: no damage table");
        return Ok(());
    };
    let map = pack.resolve(&path)?.unwrap_or_default();
    let ext = map.get("ExternalDamageParameters");
    let ext_field = |k: &str| ext.and_then(|e| e.as_map().ok()).and_then(|m| m.get(k));
    let mut rates: Vec<(String, f32)> = match ext_field("DamageRates") {
        Some(Byml::HashMap(h)) => (h.iter())
            .map(|(k, v)| (named(*k), v.as_float().unwrap_or(f32::NAN)))
            .collect(),
        _ => vec![],
    };
    rates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let life = life.unwrap_or_else(|| "?".into());
    println!("{actor}: Life {life}");
    let shown: Vec<String> = rates.iter().map(|(n, r)| format!("{n} {r}")).collect();
    println!("  rates: {}", shown.join(", "));
    let exceptions = ext_field("ExternalDamageExceptionDefines").and_then(|v| v.as_array().ok());
    for ex in exceptions
        .into_iter()
        .flatten()
        .filter_map(|e| e.as_map().ok())
    {
        let text = ex.get("FeatureExplaination").map(show).unwrap_or_default();
        let add = ex
            .get(&**field::RULE_ADD_DAMAGE_AMOUNT)
            .map(show)
            .unwrap_or_default();
        let rate = ex
            .get(&**field::RULE_DAMAGE_RATE)
            .map(show)
            .unwrap_or_default();
        let elements = ex.get("Elements").and_then(|v| v.as_array().ok());
        for el in elements
            .into_iter()
            .flatten()
            .filter_map(|e| e.as_map().ok())
        {
            println!(
                "  except types {} elements {}: add {add}, rate {rate} ({text})",
                names(el.get("DamageTypes")),
                names(el.get("DamageElements")),
            );
        }
    }
    Ok(())
}

/// An enemy's name starts with this.
const ENEMY: &str = "Enemy_";

/// `actors`, then every enemy placed in `map`'s files, each once.
pub(super) fn print(romfs: &Romfs, actors: &[String], map: Option<String>) -> Result<()> {
    let mut all: BTreeSet<String> = actors.iter().cloned().collect();
    if let Some(file) = map {
        let filter = super::Filter {
            gyaml: Some(ENEMY.into()),
            file: Some(file),
            ..Default::default()
        };
        let hits = super::actors::hits(romfs, &filter)?
            .into_iter()
            .map(|h| h.gyaml);
        all.extend(hits.filter(|g| g.starts_with(ENEMY)));
    }
    all.iter().try_for_each(|a| hurts(romfs, a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_types_read_by_name() {
        // The hashes the Stalfos table and the weapons' attack files hold.
        assert_eq!(named(3_237_216_321), "Bomb");
        assert_eq!(named(1_499_870_550), "Arrow");
        assert_eq!(named(2_054_188_199), "SlingshotBullet");
        assert_eq!(named(3_010_012_571), "Hookshot");
        assert_eq!(named(3_346_875_365), "Light");
        assert_eq!(named(7), "#00000007");
    }
}
