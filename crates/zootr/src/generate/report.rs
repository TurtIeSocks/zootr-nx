//! A seed's spots and the report of what it placed.

use serde::{Deserialize, Serialize};

use crate::names::{Names, pretty};
use crate::shuffle::Placement;
use crate::{checks, gifts, shops};

/// One chest in a seed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Spot {
    #[serde(with = "u64_text")]
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub hash: u64,
    /// The Banc folder the chest is placed in.
    pub area: String,
    pub holder: String,
    pub vanilla: String,
    pub item: String,
    /// Names from the game's text. Loading history fills in readable ids
    /// for seeds from before they were kept.
    #[serde(default)]
    pub area_name: String,
    /// What it held in the vanilla game, plus "(big)" for a big chest.
    #[serde(default)]
    pub chest_name: String,
    #[serde(default)]
    pub item_name: String,
    /// Where it is, e.g. "Mido's House Back Left". Empty for gifts and for
    /// seeds from before it was kept.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub place: String,
    /// The playthrough round that collects it, from 1, for a check logic
    /// placed whose item a rule names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u16>,
    /// On the Way of the Hero.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hero: bool,
    /// The rule at its location, eras joined with "or". Empty where logic
    /// has no location.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub need: String,
    /// The item ids that rule names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub need_items: Vec<String>,
    /// `Some(true)` when only a child can get it by logic, `Some(false)`
    /// only an adult. `None` for both, or when logic can't tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<bool>,
}

fn is_false(b: &bool) -> bool {
    !b
}

impl Spot {
    pub(crate) fn new(p: &Placement, names: &Names) -> Self {
        let area = p.check.area.as_str();
        Self {
            hash: p.check.hash,
            area: area.to_owned(),
            holder: p.check.holder.clone(),
            vanilla: p.check.item.clone(),
            item: p.item.to_owned(),
            area_name: names.area(area),
            chest_name: chest_name(names.item(&p.check.item), &p.check.holder),
            item_name: names.dealt(p.item),
            ..Default::default()
        }
    }

    /// `Area: place -> item`.
    fn line(&self) -> String {
        format!("{}: {} -> {}", self.area_name, self.name(), self.item_name)
    }

    /// The place, else what it held in the vanilla game.
    pub fn name(&self) -> &str {
        match self.place.as_str() {
            "" => &self.chest_name,
            place => place,
        }
    }

    /// Readable ids for names a seed from before they were kept is missing.
    pub fn fill_names(&mut self) {
        if self.area_name.is_empty() {
            self.area_name = pretty(&self.area);
        }
        if self.chest_name.is_empty() {
            self.chest_name = chest_name(pretty(&self.vanilla), &self.holder);
        }
        if self.item_name.is_empty() {
            self.item_name = pretty(&self.item);
        }
    }
}

/// What the check held in the vanilla game, plus "(big)" for a big chest
/// or a gift's giver. A shop slot is its shop and number.
fn chest_name(vanilla: String, holder: &str) -> String {
    if shops::is_holder(holder) {
        return holder.to_owned();
    }
    if gifts::is_giver(holder) {
        return format!("{vanilla} ({holder})");
    }
    match checks::is_big(holder) {
        true => vanilla + " (big)",
        false => vanilla,
    }
}

/// What `write_mod` changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Written {
    /// GameData edits as `name: old -> new`.
    pub defaults: Vec<String>,
    /// Placed actors whose drop changed, child and adult copies counted apart.
    pub actors: usize,
    pub files: usize,
}

/// An item Link gets outside the shuffle, in the round that collects it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RoundEvent {
    pub round: u16,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Report {
    #[serde(with = "u64_text")]
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub seed: u64,
    /// Sorted by area name, then chest name.
    pub spots: Vec<Spot>,
    #[serde(flatten)]
    pub written: Written,
    /// The sources each round of a playthrough collects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<RoundEvent>,
    /// Seeds from before the spots kept their round and hero: the Way of
    /// the Hero as `Area: place -> item`, by area. New seeds leave it empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub way: Vec<String>,
    /// Seeds from before the spots kept their round: what each round
    /// collected. New seeds leave it empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub playthrough: Vec<Vec<String>>,
    /// Checks logic can't show a way into yet, as `Area: place`. They hold
    /// filler.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unprovable: Vec<String>,
    /// What logic starts Link with. Empty for seeds from before it was kept.
    #[serde(default, skip_serializing_if = "Start::is_empty")]
    pub start: Start,
}

/// What logic counts Link as having before round 1.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Start {
    /// Starting items and what presets hand over, by name.
    pub items: Vec<Held>,
    /// The settings logic's rules read that are on, by label.
    pub settings: Vec<String>,
}

impl Start {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.settings.is_empty()
    }
}

/// An item Link starts with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Held {
    pub item: String,
    pub name: String,
    pub count: u32,
}

impl Report {
    pub fn changed(&self) -> usize {
        self.spots.iter().filter(|s| s.item != s.vanilla).count()
    }

    /// Chests grouped by area, each with what it holds now.
    pub fn spoiler(&self, icons: &[String]) -> String {
        let width = self.spots.iter().map(|s| s.name().chars().count());
        let width = width.max().unwrap_or(0);
        let mut out = format!("seed {}\nicons {}\n", self.seed, icons.join(" · "));
        let mut area = None;
        for s in &self.spots {
            if area != Some(&s.area_name) {
                area = Some(&s.area_name);
                out += &format!("\n{}\n", s.area_name);
            }
            out += &format!("  {:<width$}  -> {}\n", s.name(), s.item_name);
        }
        let way = self.way();
        if !way.is_empty() {
            out += "\nWay of the Hero\n";
            way.iter().for_each(|w| out += &format!("  {w}\n"));
        }
        let playthrough = self.playthrough();
        if !playthrough.is_empty() {
            out += "\nPlaythrough\n";
        }
        for (i, round) in playthrough.iter().enumerate() {
            out += &format!("  Round {}\n", i + 1);
            round.iter().for_each(|got| out += &format!("    {got}\n"));
        }
        if !self.unprovable.is_empty() {
            out += "\nFiller only, until logic can show a way in\n";
            self.unprovable
                .iter()
                .for_each(|u| out += &format!("  {u}\n"));
        }
        out
    }

    /// The Way of the Hero as `Area: place -> item`, by area.
    fn way(&self) -> Vec<String> {
        match self.spots.iter().any(|s| s.hero) {
            true => self
                .spots
                .iter()
                .filter(|s| s.hero)
                .map(Spot::line)
                .collect(),
            false => self.way.clone(),
        }
    }

    /// What each round collects: its checks by area, then its sources.
    fn playthrough(&self) -> Vec<Vec<String>> {
        let spots = self.spots.iter().filter_map(|s| Some((s.round?, s.line())));
        let events = self.events.iter().map(|e| (e.round, e.name.clone()));
        let got: Vec<(u16, String)> = spots.chain(events).collect();
        let Some(last) = got.iter().map(|g| g.0).max() else {
            return self.playthrough.clone();
        };
        let round = |r: u16| got.iter().filter(move |g| g.0 == r).map(|g| g.1.clone());
        (1..=last).map(|r| round(r).collect()).collect()
    }
}

/// u64s as strings: JavaScript numbers lose precision past 2^53.
pub(super) mod u64_text {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(n: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(n)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        String::deserialize(d)?.parse().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::actor;

    fn report() -> Report {
        Report {
            seed: u64::MAX,
            events: vec![],
            way: vec![],
            playthrough: vec![],
            unprovable: vec![],
            start: Default::default(),
            spots: vec![Spot {
                hash: u64::MAX - 1,
                area: "Area".into(),
                holder: actor::OBJ_T_BOX_A_02.to_string(),
                vanilla: "Item_A".into(),
                item: "Item_B".into(),
                ..Default::default()
            }],
            written: Written {
                defaults: vec![],
                actors: 2,
                files: 3,
            },
        }
    }

    #[test]
    fn big_numbers_survive_json_as_strings() {
        let json = serde_json::to_value(report()).unwrap();
        assert_eq!(json["seed"], u64::MAX.to_string());
        assert_eq!(json["files"], 3);
        assert_eq!(serde_json::from_value::<Report>(json).unwrap(), report());
    }

    #[test]
    fn spoiler_groups_chests_by_area() {
        let spot = |area: &str, chest: &str, item: &str| Spot {
            area_name: area.into(),
            chest_name: chest.into(),
            item_name: item.into(),
            ..Default::default()
        };
        let report = Report {
            seed: 7,
            spots: vec![
                spot("Field", "Sword", "Bow"),
                Spot {
                    place: "North Bow (big)".into(),
                    ..spot("Field", "Bow (big)", "Sword")
                },
                spot("Lake", "Map", "Map"),
            ],
            ..report()
        };
        assert_eq!(
            report.spoiler(&["Sun".into(), "Moon".into()]),
            "seed 7\nicons Sun · Moon\n\nField\n  Sword            -> Bow\n  North Bow (big)  -> Sword\n\nLake\n  Map              -> Map\n"
        );
    }

    #[test]
    fn the_way_of_the_hero_and_the_rounds_come_from_the_spots() {
        let spot = |chest: &str, item: &str, round: Option<u16>, hero: bool| Spot {
            area_name: "Field".into(),
            chest_name: chest.into(),
            item_name: item.into(),
            round,
            hero,
            ..Default::default()
        };
        let report = Report {
            seed: 7,
            spots: vec![
                spot("Bow", "Sword", Some(2), false),
                spot("Map", "Rupee", None, false),
                spot("Sword", "Bow", Some(1), true),
            ],
            events: vec![
                RoundEvent {
                    round: 2,
                    name: "Beat Gohma".into(),
                },
                RoundEvent {
                    round: 1,
                    name: "Deku Shield".into(),
                },
            ],
            ..report()
        };
        let text = report.spoiler(&[]);
        let tail = &text[text.find("\nWay").unwrap()..];
        assert_eq!(
            tail,
            "\nWay of the Hero\n  Field: Sword -> Bow\n\nPlaythrough\n  Round 1\n    Field: Sword -> Bow\n    Deku Shield\n  Round 2\n    Field: Bow -> Sword\n    Beat Gohma\n"
        );
    }

    #[test]
    fn the_logic_fields_survive_json() {
        let report = Report {
            spots: vec![Spot {
                round: Some(3),
                hero: true,
                need: "Item_A or child".into(),
                need_items: vec!["Item_A".into()],
                ..report().spots[0].clone()
            }],
            events: vec![RoundEvent {
                round: 1,
                name: "Deku Shield".into(),
            }],
            ..report()
        };
        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("way").is_none() && json.get("playthrough").is_none());
        assert_eq!(serde_json::from_value::<Report>(json).unwrap(), report);
    }

    #[test]
    fn an_old_entry_keeps_its_way_and_playthrough_lines() {
        let json = r#"{"seed":"7","spots":[],"defaults":[],"actors":0,"files":0,
            "way":["Field: Sword -> Bow"],"playthrough":[["Field: Sword -> Bow"]]}"#;
        let report: Report = serde_json::from_str(json).unwrap();
        assert!(report.events.is_empty());
        assert_eq!(
            report.spoiler(&[]),
            "seed 7\nicons \n\nWay of the Hero\n  Field: Sword -> Bow\n\nPlaythrough\n  Round 1\n    Field: Sword -> Bow\n"
        );
    }

    #[test]
    fn spots_saved_before_names_get_readable_ids() {
        needs_names!();
        let json = &format!(
            r#"{{"hash":"1","area":"OpenField","holder":"{}","vanilla":"{}","item":"Item_A"}}"#,
            actor::OBJ_T_BOX_A_02,
            actor::T_BOX_RUPEE_BLUE
        );
        let mut spot: Spot = serde_json::from_str(json).unwrap();
        spot.fill_names();
        let names = [spot.area_name, spot.chest_name, spot.item_name];
        assert_eq!(names, ["Open Field", "Rupee Blue (big)", "A"]);
    }
}
