//! Favorites: setting sets kept by name. The player's own live in the
//! config, and two come built in.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{
    BOMBCHUS, BUNNY_HOOD, Bombchus, CHESTS, DOOR_OF_TIME, DoorOfTime, PROGRESSIVE, QUIET_NAVI,
    SCRUBS, SKIP_ALL, SKIP_ARRIVALS, SKIP_INTRO, Settings, choice_id,
};
use crate::starting;

/// A setting set the player keeps by name: settings only, never a plando,
/// a test setup or flag edits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Favorite {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// When it was saved or last updated, in seconds as History's times.
    pub time: u32,
    pub settings: Settings,
}

/// A favorite that comes with the app: never stored, renamed, updated or
/// deleted.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BuiltIn {
    /// What `--favorite` takes, beside the name.
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    /// Values over the defaults.
    pub settings: Settings,
}

/// The randomizer's usual setup: every setting the defaults had on before
/// they went vanilla, at that value. A settings map saved before then
/// takes these where it says nothing.
pub fn randomizer() -> Settings {
    let start = starting::Start::of(&["master-sword"]);
    let mut settings = on(&[
        CHESTS,
        SCRUBS,
        PROGRESSIVE,
        QUIET_NAVI,
        SKIP_INTRO,
        SKIP_ARRIVALS,
        "menus",
        "clock",
        "no-tutorials",
        "no-pickup-texts",
    ]);
    settings.extend([
        (DOOR_OF_TIME.into(), choice_id(DoorOfTime::Open).into()),
        (BOMBCHUS.into(), choice_id(Bombchus::Free).into()),
        (
            starting::SETTING.into(),
            serde_json::to_value(start).expect("a map"),
        ),
    ]);
    settings
}

/// The Randomizer favorite with `over` laid over it, for a test that needs
/// a shuffle.
#[cfg(test)]
pub fn randomizer_and(over: Settings) -> Settings {
    let mut settings = randomizer();
    settings.extend(over);
    settings
}

/// Nothing shuffled, with the comfort enhancements on.
fn vanilla_plus() -> Settings {
    on(&[
        SKIP_ALL,
        BUNNY_HOOD,
        QUIET_NAVI,
        "menus",
        "no-tutorials",
        "no-pickup-texts",
    ])
}

/// `ids`, each on.
fn on(ids: &[&str]) -> Settings {
    ids.iter().map(|id| ((*id).into(), true.into())).collect()
}

/// The favorites that come with the app, in the order the list shows them.
pub fn built_in() -> Vec<BuiltIn> {
    vec![
        BuiltIn {
            id: "vanilla-plus",
            name: "Vanilla+",
            note: "Nothing shuffled, with comfort enhancements on",
            settings: vanilla_plus(),
        },
        BuiltIn {
            id: "randomizer",
            name: "Randomizer",
            note: "The randomizer's usual setup",
            settings: randomizer(),
        },
    ]
}

/// The settings of the favorite called `name`: a built-in by its id or
/// name, else one of `mine` by name, ignoring case either way.
pub fn find(name: &str, mine: &[Favorite]) -> Result<Settings> {
    let built = built_in();
    let found = (built.iter())
        .find(|b| b.id.eq_ignore_ascii_case(name) || b.name.eq_ignore_ascii_case(name))
        .map(|b| &b.settings)
        .or_else(|| {
            (mine.iter())
                .find(|f| f.name.eq_ignore_ascii_case(name))
                .map(|f| &f.settings)
        });
    match found {
        Some(settings) => Ok(settings.clone()),
        None => {
            let names: Vec<&str> = (built.iter().map(|b| b.id))
                .chain(mine.iter().map(|f| f.name.as_str()))
                .collect();
            bail!("no favorite is called {name}: there's {}", names.join(", "))
        }
    }
}

/// Fails when `mine` can't be kept: a name blank, taken twice, or a
/// built-in's, ignoring case.
pub fn check(mine: &[Favorite]) -> Result<()> {
    let taken: Vec<String> = (built_in().iter())
        .flat_map(|b| [b.id, b.name])
        .map(str::to_lowercase)
        .collect();
    let mut seen = Vec::new();
    for f in mine {
        let name = f.name.trim().to_lowercase();
        if name.is_empty() {
            bail!("a favorite needs a name");
        }
        if taken.contains(&name) || seen.contains(&name) {
            bail!("a favorite is already called {}", f.name.trim());
        }
        seen.push(name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Kind, schema};

    fn mine(name: &str, settings: Settings) -> Favorite {
        Favorite {
            name: name.into(),
            note: String::new(),
            time: 0,
            settings,
        }
    }

    #[test]
    fn every_default_is_vanilla_and_the_randomizer_favorite_differs_from_each() {
        let schema = schema();
        for (id, value) in randomizer() {
            let def = schema.iter().find(|d| d.id == id);
            let def = def.unwrap_or_else(|| panic!("no setting {id}"));
            let default = match &def.kind {
                Kind::Toggle { default } => (*default).into(),
                Kind::Choice { default, .. } => default.as_str().into(),
                Kind::Inventory { default } => serde_json::to_value(default).unwrap(),
            };
            assert_ne!(value, default, "{id}");
        }
        for def in &schema {
            if let Kind::Toggle { default } = def.kind {
                assert!(!default, "{} is on by default", def.id);
            }
        }
    }

    #[test]
    fn every_built_in_value_is_a_setting() {
        let schema = schema();
        for b in built_in() {
            for id in b.settings.keys() {
                assert!(schema.iter().any(|d| &d.id == id), "{}: {id}", b.name);
            }
        }
    }

    #[test]
    fn a_favorite_is_found_by_id_or_name_ignoring_case() {
        let own = Settings::from([(CHESTS.into(), false.into())]);
        let list = [mine("My Run", own.clone())];
        assert_eq!(find("randomizer", &list).unwrap(), randomizer());
        assert_eq!(find("VANILLA+", &list).unwrap(), vanilla_plus());
        assert_eq!(find("my run", &list).unwrap(), own);
        let err = find("nope", &list).unwrap_err().to_string();
        assert!(err.contains("vanilla-plus, randomizer, My Run"), "{err}");
    }

    #[test]
    fn names_are_unique_ignoring_case_and_never_a_built_ins() {
        let s = Settings::new;
        assert!(check(&[mine("A", s()), mine("B", s())]).is_ok());
        assert!(check(&[mine("A", s()), mine(" a ", s())]).is_err());
        assert!(check(&[mine("randomizer", s())]).is_err());
        assert!(check(&[mine("Vanilla+", s())]).is_err());
        assert!(check(&[mine("  ", s())]).is_err());
    }
}
