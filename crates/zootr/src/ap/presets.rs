//! The option sets rooms and bench runs generate with, from
//! `ap/bench/presets.toml`, and the player file each slot gets.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

/// What each slot starts with: the test seeds' items, by AP name.
const START: [&str; 5] = [
    "Kokiri Sword",
    "Deku Shield",
    "Progressive Slingshot",
    "Progressive Bomb Bag",
    "Progressive Ocarina",
];

const PRESETS: &str = include_str!("../../../../ap/bench/presets.toml");

/// Each preset's options, by setting id.
pub fn presets() -> Result<BTreeMap<String, BTreeMap<String, toml::Value>>> {
    toml::from_str(PRESETS).context("ap/bench/presets.toml")
}

/// `name`'s options, as the slot YAML's lines under the game.
pub(super) fn options(name: &str) -> Result<Vec<String>> {
    let presets = presets()?;
    let preset =
        (presets.get(name)).with_context(|| format!("no preset {name}: {}", names(&presets)))?;
    preset
        .iter()
        .map(|(id, v)| {
            let value = match v {
                toml::Value::Boolean(b) => b.to_string(),
                toml::Value::String(s) => s.clone(),
                _ => bail!("{name}.{id}: a toggle or a choice"),
            };
            Ok(format!("{}: {value}", id.replace('-', "_")))
        })
        .collect()
}

fn names(presets: &BTreeMap<String, BTreeMap<String, toml::Value>>) -> String {
    presets.keys().cloned().collect::<Vec<_>>().join(", ")
}

/// One slot's player file.
pub(super) fn player_yaml(slot: &str, options: &[String]) -> String {
    let mut out = format!("name: {slot}\ngame: {}\n{}:\n", super::GAME, super::GAME);
    for line in options {
        out += &format!("  {line}\n");
    }
    out += "  start_inventory_from_pool:\n";
    for item in START {
        out += &format!("    {item}: 1\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exported options: id, kind, and a choice's values.
    fn exported() -> Vec<(String, Option<Vec<String>>)> {
        let text = include_str!("../../../../ap/znx/data/options.json");
        let json: Vec<serde_json::Value> = serde_json::from_str(text).unwrap();
        json.iter()
            .map(|e| {
                let values = e["options"].as_array().map(|o| {
                    o.iter()
                        .map(|v| v["id"].as_str().unwrap().to_owned())
                        .collect()
                });
                (e["id"].as_str().unwrap().to_owned(), values)
            })
            .collect()
    }

    #[test]
    fn each_preset_names_only_options_the_world_offers_at_values_it_takes() {
        let exported = exported();
        for (name, preset) in presets().unwrap() {
            for (id, value) in preset {
                let (_, values) = (exported.iter().find(|(e, _)| *e == id))
                    .unwrap_or_else(|| panic!("{name}.{id} isn't an AP option"));
                match (values, value) {
                    (None, toml::Value::Boolean(_)) => {}
                    (Some(vs), toml::Value::String(v)) => {
                        assert!(vs.contains(&v), "{name}.{id} = {v}: not one of {vs:?}")
                    }
                    (_, v) => panic!("{name}.{id} = {v}: the wrong kind"),
                }
            }
        }
    }

    #[test]
    fn all_on_names_every_option_the_world_offers() {
        let all = presets().unwrap().remove("all-on").unwrap();
        for (id, _) in exported() {
            assert!(all.contains_key(&id), "all-on leaves out {id}");
        }
    }

    #[test]
    fn a_player_file_holds_the_preset_and_the_test_start() {
        let yaml = player_yaml("Link", &options("defaults").unwrap());
        assert!(yaml.starts_with("name: Link\ngame: Ocarina of Time (znx)\n"));
        assert!(yaml.contains("\n  open_forest: true\n"));
        assert!(yaml.contains("\n  start_inventory_from_pool:\n    Kokiri Sword: 1\n"));
    }

    #[test]
    fn an_unknown_preset_names_the_ones_there_are() {
        let err = options("everything").unwrap_err().to_string();
        assert_eq!(err, "no preset everything: all-on, defaults");
    }
}
