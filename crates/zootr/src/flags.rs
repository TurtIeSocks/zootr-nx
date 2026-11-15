//! What each Presence flag means: its GameData kind, the expression behind
//! it when it's a BoolExp, and which flows set it when it's a plain Bool.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use ounce_formats::{Byml, EventKind, Flowchart, Romfs, files_with_suffix, murmur3};

use crate::checks::{actors, banc_files, load_banc};
use crate::events::Flows;
use crate::flows;
use crate::gamedata::GameData;
use crate::known::{field, flag};
use crate::world::presence_of;

const SETTER: &str = "EventTriggerSetGameData";
const ACTORS: (&str, &str) = ("Pack/Actor", ".pack.zs");

/// romfs path of an actor's pack, e.g. `Pack/Actor/Item_Hookshot.pack.zs`.
pub fn actor_pack(actor: &str) -> String {
    format!("{}/{actor}{}", ACTORS.0, ACTORS.1)
}

#[derive(Default)]
pub struct Usage {
    /// Actors that exist while the flag is set.
    pub when_set: usize,
    /// Actors that exist while it's unset.
    pub when_unset: usize,
    pub areas: BTreeSet<String>,
}

/// The GameData names `chart`'s actions set.
pub fn set_in(chart: &Flowchart) -> impl Iterator<Item = &str> {
    chart.events.iter().filter_map(|e| {
        let EventKind::Action {
            actor,
            action,
            params,
            ..
        } = &e.kind
        else {
            return None;
        };
        if !chart.actor_of(*actor).actions[*action as usize].starts_with(SETTER) {
            return None;
        }
        let (_, name) = params.iter().find(|(k, _)| k == field::GAME_DATA_NAME)?;
        name.as_str()
    })
}

pub struct Survey {
    pub gamedata: GameData,
    pub presence: BTreeMap<String, Usage>,
    /// Flows that set each flag.
    pub setters: BTreeMap<String, BTreeSet<String>>,
}

fn learn_all(gd: &mut GameData, v: &Byml) {
    match v {
        Byml::String(s) => gd.learn(s),
        Byml::Array(a) => a.iter().for_each(|v| learn_all(gd, v)),
        Byml::Map(m) => m.values().for_each(|v| learn_all(gd, v)),
        _ => {}
    }
}

fn learn(gd: &mut GameData, s: &str) {
    gd.learn(s);
    // Quest step enums are named after their challenge.
    gd.learn(&format!("Step_{s}"));
}

impl Survey {
    pub fn load(romfs: &Romfs, flows: &Flows) -> Result<Self> {
        let mut gamedata = GameData::load(romfs)?;
        let mut presence = BTreeMap::<String, Usage>::new();
        for file in banc_files(romfs)? {
            let doc = load_banc(romfs, &file)?;
            let area = file.split('/').nth(1).unwrap_or("?").to_owned();
            for actor in actors(&doc.root)? {
                learn_all(&mut gamedata, actor);
                if let Some(p) = presence_of(actor) {
                    let u = presence.entry(p.flag).or_default();
                    *if p.negated {
                        &mut u.when_unset
                    } else {
                        &mut u.when_set
                    } += 1;
                    u.areas.insert(area.clone());
                }
            }
        }
        // Flags the engine sets itself, named after an actor or a flow.
        for path in files_with_suffix(&romfs.root().join(ACTORS.0), ACTORS.1)? {
            let rel = romfs.rel(&path)?;
            let actor = rel.rsplit('/').next().unwrap_or(&rel);
            gamedata.learn(&format!("IsGet.{}", actor.trim_end_matches(ACTORS.1)));
        }
        let mut setters = BTreeMap::<String, BTreeSet<String>>::new();
        for (name, chart) in &flows.0 {
            gamedata.learn(&format!("{}.{name}", flag::HAVE_PLAYED_EVENT));
            flows::strings(chart)
                .into_iter()
                .for_each(|s| learn(&mut gamedata, s));
            for flag in set_in(chart) {
                setters
                    .entry(flag.to_owned())
                    .or_default()
                    .insert(name.clone());
            }
        }
        Ok(Self {
            gamedata,
            presence,
            setters,
        })
    }

    /// One line on what `flag` is: its expression, its setters, or that
    /// nothing in the data sets it.
    pub fn describe(&self, flag: &str) -> Result<String> {
        let table = (self.gamedata.table(flag))
            .map_or_else(|| format!("not in {}", flag::GAME_DATA), str::to_owned);
        if let Some(expr) = self.gamedata.expr(flag)? {
            return Ok(format!("{table}: {expr}"));
        }
        let set = match self.setters.get(flag) {
            Some(s) => format!(
                "{table}, set by {}",
                s.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
            None => format!("{table}, set outside flows"),
        };
        let read = self
            .gamedata
            .read_by_exps()
            .contains(&murmur3(flag.as_bytes()));
        Ok(if read {
            format!("{set}, read by a {}", flag::BOOL_EXP)
        } else {
            set
        })
    }
}
