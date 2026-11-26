//! Each check's name for players, written by hand in `logic/names/`: the
//! app, the spoiler and Archipelago all show it. Archipelago YAMLs name
//! checks by it, so a name a release shipped never changes
//! (`ap::released`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use anyhow::{Context, Result, ensure};

/// Every names file, by name. A test keeps this in step with `logic/names/`.
const FILES: &[(&str, &str)] = &[
    (
        "death-mountain-trail.toml",
        include_str!("../logic/names/death-mountain-trail.toml"),
    ),
    (
        "graveyard.toml",
        include_str!("../logic/names/graveyard.toml"),
    ),
    (
        "hyrule-castle.toml",
        include_str!("../logic/names/hyrule-castle.toml"),
    ),
    (
        "hyrule-field.toml",
        include_str!("../logic/names/hyrule-field.toml"),
    ),
    (
        "kakariko-village.toml",
        include_str!("../logic/names/kakariko-village.toml"),
    ),
    (
        "kokiri-forest.toml",
        include_str!("../logic/names/kokiri-forest.toml"),
    ),
    (
        "lost-woods.toml",
        include_str!("../logic/names/lost-woods.toml"),
    ),
    ("market.toml", include_str!("../logic/names/market.toml")),
    (
        "outside-ganons-castle.toml",
        include_str!("../logic/names/outside-ganons-castle.toml"),
    ),
    (
        "sacred-forest-meadow.toml",
        include_str!("../logic/names/sacred-forest-meadow.toml"),
    ),
    (
        "water-temple.toml",
        include_str!("../logic/names/water-temple.toml"),
    ),
];

/// Each area's prefixes, in OoTR's style: dungeons spelled out, overworld
/// areas by OoTR's abbreviations, apostrophes dropped. Keyed by the area's
/// Archipelago name (`ap::names::area`). The remake's Hyrule Field map
/// holds Lake Hylia and Lon Lon Ranch too, so its checks take whichever
/// of the three they stand in. Kokiri Forest's map holds the bridge to the
/// Lost Woods, where OoTR's `LW Gift from Saria` stands.
const PREFIXES: &[(&str, &[&str])] = &[
    ("Bottom of the Well", &["Bottom of the Well"]),
    ("Death Mountain Crater", &["DMC"]),
    ("Death Mountain Trail", &["DMT"]),
    ("Deku Tree", &["Deku Tree"]),
    ("Desert Colossus", &["Colossus"]),
    ("Dodongo's Cavern", &["Dodongos Cavern"]),
    ("Dream World", &["Dream World"]),
    ("Fire Temple", &["Fire Temple"]),
    ("Forest Temple", &["Forest Temple"]),
    ("Ganon's Castle", &["Ganons Castle"]),
    ("Gerudo Training Ground", &["Gerudo Training Ground"]),
    ("Gerudo Valley", &["GV"]),
    ("Gerudo's Fortress", &["GF"]),
    ("Goron City", &["GC"]),
    ("Graveyard", &["Graveyard"]),
    ("Haunted Wasteland", &["Wasteland"]),
    ("Hyrule Castle", &["HC"]),
    ("Hyrule Field", &["HF", "LH", "LLR"]),
    ("Ice Cavern", &["Ice Cavern"]),
    ("Jabu-Jabu's Belly", &["Jabu Jabus Belly"]),
    ("Kakariko Village", &["Kak"]),
    ("Kokiri Forest", &["KF", "LW"]),
    ("Lost Woods", &["LW"]),
    ("Market", &["Market"]),
    ("Outside Ganon's Castle", &["OGC"]),
    ("Sacred Forest Meadow", &["SFM"]),
    ("Shadow Temple", &["Shadow Temple"]),
    ("Spirit Temple", &["Spirit Temple"]),
    ("Water Temple", &["Water Temple"]),
    ("Zora's Domain", &["ZD"]),
    ("Zora's Fountain", &["ZF"]),
    ("Zora's River", &["ZR"]),
];

/// `hash`'s name, if its region has been named.
pub fn of(hash: u64) -> Option<&'static str> {
    static NAMES: OnceLock<BTreeMap<u64, String>> = OnceLock::new();
    // The tests parse every file, so this never fails in a build.
    let names = NAMES.get_or_init(|| parse(FILES).expect("logic/names"));
    names.get(&hash).map(String::as_str)
}

/// The prefixes a name in `area` (its Archipelago name) may start with.
pub fn prefixes(area: &str) -> Option<&'static [&'static str]> {
    PREFIXES.iter().find(|(a, _)| *a == area).map(|(_, p)| *p)
}

/// Each file's `hash = "name"` lines, checked: no check named twice, no
/// name used twice.
fn parse(files: &[(&str, &str)]) -> Result<BTreeMap<u64, String>> {
    let mut out = BTreeMap::new();
    for (file, text) in files {
        let table: BTreeMap<String, String> =
            toml::from_str(text).with_context(|| format!("names/{file}"))?;
        for (key, name) in table {
            let hash: u64 = (key.parse()).with_context(|| format!("names/{file}: {key}"))?;
            ensure!(
                out.insert(hash, name).is_none(),
                "names/{file}: check {hash} is named twice"
            );
        }
    }
    let mut seen = BTreeSet::new();
    for name in out.values() {
        ensure!(seen.insert(name), "two checks are named {name}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_files_parse() {
        parse(FILES).unwrap();
    }

    #[test]
    fn a_check_named_twice_is_refused() {
        let files = [
            ("a.toml", "1 = \"KF Fake Chest\""),
            ("b.toml", "1 = \"KF Other Fake Chest\""),
        ];
        assert!(parse(&files).is_err());
    }

    #[test]
    fn a_name_used_twice_is_refused() {
        let files = [("a.toml", "1 = \"KF Fake Chest\"\n2 = \"KF Fake Chest\"")];
        assert!(parse(&files).is_err());
    }

    #[test]
    fn every_names_file_is_built_in() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/logic/names");
        let on_disk: BTreeSet<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| !f.starts_with('.'))
            .collect();
        let built: BTreeSet<String> = FILES.iter().map(|f| f.0.to_owned()).collect();
        assert_eq!(on_disk, built);
    }

    /// Every name is a check the export deals, starting with its area's
    /// prefix.
    #[test]
    fn every_name_is_a_dealt_check_under_its_area_s_prefix() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let names = parse(FILES).unwrap();
        if names.is_empty() {
            return;
        }
        let checks = crate::ap::export::checks(&romfs).unwrap();
        let areas: BTreeMap<u64, &str> = (checks.iter())
            .map(|c| (c.hash, crate::ap::names::area(&c.area).unwrap()))
            .collect();
        let mut wrong = Vec::new();
        for (hash, name) in &names {
            match areas.get(hash) {
                None => wrong.push(format!("{hash} ({name}) isn't a dealt check")),
                Some(area) => {
                    let prefixes = prefixes(area).unwrap_or_else(|| panic!("no prefix for {area}"));
                    if !prefixes.iter().any(|p| name.starts_with(&format!("{p} "))) {
                        wrong.push(format!(
                            "{name} doesn't start with {}",
                            prefixes.join(" or ")
                        ));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
