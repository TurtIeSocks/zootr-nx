//! The catalogue of cutscenes: what each one is, how a flow reaches it, and
//! which a set of skips covers.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use ounce_formats::{EventKind, Romfs};

use super::*;
use crate::per_dump::PerDump;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tier {
    /// A flow that changes nothing anywhere it reaches.
    Clean,
    /// No flow, only camera and animation tracks.
    Timeline,
    /// Changes something a skip would lose, and why.
    Effects(String),
}

impl Tier {
    pub fn skippable(&self) -> bool {
        !matches!(self, Tier::Effects(_))
    }
}

#[derive(Clone)]
pub struct Cutscene {
    pub name: String,
    pub area: Option<&'static str>,
    pub tier: Tier,
    /// Flows with a transition to it.
    pub flows: BTreeSet<String>,
    /// Whether a map actor or a scene change starts it.
    pub map: bool,
    /// Whether the save has a played flag for it.
    pub flag: bool,
}

#[derive(Clone)]
pub struct Catalogue(pub Vec<Cutscene>);

impl Catalogue {
    /// Every cutscene, kept per dump: it parses every flow and map.
    pub fn load(romfs: &Romfs) -> Result<Self> {
        static KEPT: PerDump<Catalogue> = PerDump::new();
        KEPT.get(romfs, || Self::read(romfs))
    }

    fn read(romfs: &Romfs) -> Result<Self> {
        let all = Flows::load(romfs)?;
        let mut callers = BTreeMap::<String, BTreeSet<String>>::new();
        // Who reads each played flag: a flow, or the map.
        let mut read = BTreeMap::<String, BTreeSet<String>>::new();
        let mut reads = |by: &str, names: Vec<String>| {
            for n in names {
                read.entry(n).or_default().insert(by.to_owned());
            }
        };
        for (name, flow) in &all.charts {
            for e in &flow.events {
                if let Some(to) = transition(flow, &e.kind) {
                    callers.entry(to.into()).or_default().insert(name.clone());
                }
                if let EventKind::Switch {
                    actor,
                    query,
                    params,
                    ..
                } = &e.kind
                    && flow.actor_of(*actor).queries[*query as usize].contains(&**PLAYED)
                    && let Some((_, v)) = params.iter().find(|(k, _)| k == flow::EVENT_NAME)
                    && let Some(s) = v.as_str()
                {
                    reads(name, vec![s.into()]);
                }
            }
            reads(name, played(flows::strings(flow)));
        }
        let files: BTreeSet<String> = flows::names(romfs)?
            .into_iter()
            .filter(|n| n.starts_with("Dm_"))
            .collect();
        // Started by the map, or by a scene change's `BootEventName`.
        let mut map: BTreeSet<String> = boots(&all);
        for (_, doc) in checks::bancs(romfs)? {
            let doc = doc?;
            let mut s = Vec::new();
            strings(&doc.root, &mut s);
            let mut guarded = Vec::new();
            for a in checks::actors(&doc.root)? {
                if !checks::str_field(a, "Gyaml")
                    .is_some_and(|g| INTRO_GUARDS.iter().any(|n| *n == g))
                {
                    strings(a, &mut guarded);
                }
            }
            reads("map", played(guarded));
            map.extend(
                s.into_iter()
                    .filter(|s| files.contains(*s) || callers.contains_key(*s))
                    .map(String::from),
            );
        }
        let gd = GameData::load(romfs)?;
        let exps = gd.read_by_exps();
        let mut out = Vec::new();
        let flagged = files.iter().filter(|n| gd.table(&played_flag(n)).is_some());
        let names: BTreeSet<&String> = callers.keys().chain(&map).chain(flagged).collect();
        for name in names.into_iter().filter(|n| n.starts_with("Dm_")) {
            let has_flag = gd.table(&played_flag(name)).is_some();
            // A flow that starts a cutscene reading its flag is the "only
            // once" guard, and an NPC reading it picks a line. Anything else
            // would change the world early.
            let starts = callers.get(name);
            let readers: Vec<&String> = read
                .get(name.as_str())
                .into_iter()
                .flatten()
                .filter(|r| starts.is_none_or(|s| !s.contains(*r)))
                .filter(|r| *r != name && !r.starts_with("Npc_"))
                .collect();
            let tier = if let Some(r) = readers.first() {
                Tier::Effects(format!("its played flag is read by {r}"))
            } else if has_flag && exps.contains(&murmur3(played_flag(name).as_bytes())) {
                Tier::Effects(format!("a {} reads its played flag", flag::BOOL_EXP))
            } else {
                match all.effects_where(name, changes) {
                    None => Tier::Timeline,
                    Some(e) => match e.first() {
                        None => Tier::Clean,
                        Some(e) => Tier::Effects(e.to_string()),
                    },
                }
            };
            // A switch case can't end a flow, so a caller with nowhere quiet
            // to end can't drop it.
            let unlinkable = || -> Result<Option<String>> {
                for f in starts.into_iter().flatten() {
                    let only = BTreeSet::from([name.as_str()]);
                    if let Err(e) = relinked(&romfs.read(&flows::rel(f))?, &only) {
                        return Ok(Some(format!("{e} in {f}")));
                    }
                }
                Ok(None)
            };
            let tier = match tier {
                t if t.skippable() => unlinkable()?.map_or(t, Tier::Effects),
                t => t,
            };
            out.push(Cutscene {
                name: name.clone(),
                area: area(name),
                tier,
                flows: callers.get(name).cloned().unwrap_or_default(),
                map: map.contains(name),
                flag: has_flag,
            });
        }
        Ok(Self(out))
    }

    pub(super) fn get(&self, name: &str) -> Option<&Cutscene> {
        self.0.iter().find(|c| c.name == name)
    }

    /// The cutscenes `skips` covers. Named ones are skipped whatever their
    /// tier: the intro is playtested, Saria's warp only poses Link, and the
    /// owl is meant to lose his rides.
    pub fn picked(&self, skips: &Skips) -> Vec<&Cutscene> {
        let mut named: Vec<&Name> = Vec::new();
        if skips.intro {
            named.extend(INTRO);
        }
        if skips.saria {
            named.extend(SARIA);
        }
        if skips.owl {
            named.extend(OWL);
        }
        if skips.areas.contains(ZELDA_AREA) {
            named.extend(ZELDA);
        }
        if skips.areas.contains(IMPA_AREA) {
            named.extend(IMPA);
        }
        let area = |c: &&Cutscene| c.area.is_some_and(|a| skips.areas.contains(a));
        let mut out: Vec<&Cutscene> = self
            .0
            .iter()
            .filter(|c| (c.tier.skippable() || POSED.iter().any(|n| **n == c.name)) && area(c))
            .chain(named.iter().filter_map(|n| self.get(n)))
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out.dedup_by(|a, b| a.name == b.name);
        out
    }
}

impl Catalogue {
    /// The scenes with effects that `skips` cuts instead of relinking: the
    /// ones in its areas the rule in `cut` takes, and the plan for each.
    pub(crate) fn cut(&self, skips: &Skips, all: &Flows) -> Vec<(&Cutscene, cut::Plan)> {
        let area = |c: &&Cutscene| c.area.is_some_and(|a| skips.areas.contains(a));
        // The arrival skip has its own way with the ranch's intro.
        let arrival = |c: &&Cutscene| skips.arrivals && arrivals::requested(&c.name);
        self.0
            .iter()
            .filter(|c| !c.tier.skippable())
            .filter(area)
            .filter(|c| !arrival(c))
            .filter_map(|c| match cut::judge(all, c) {
                cut::Verdict::Cut(plan) => Some((c, plan)),
                cut::Verdict::Kept(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cutscenes::tests::every;
    use crate::known::scene;

    fn names_of<'a>(picked: &[&'a Cutscene]) -> BTreeSet<&'a str> {
        picked.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn only_harmless_cutscenes_are_skipped_and_nothing_runs_them() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = Catalogue::load(&romfs).unwrap();
        for n in INTRO.iter().chain(&OWL).chain(&SARIA) {
            assert!(catalogue.get(n).is_some(), "{n} isn't a cutscene");
        }
        let picked = catalogue.picked(&every());
        let named: BTreeSet<&str> = INTRO
            .iter()
            .chain(&OWL)
            .chain(&SARIA)
            .chain(&POSED)
            .chain(&ZELDA)
            .chain(&IMPA)
            .map(|n| n.get())
            .collect();
        assert!(named.iter().all(|n| catalogue.get(n).is_some()));
        assert!(POSED.iter().all(|n| names_of(&picked).contains(n.get())));
        for c in &picked {
            assert!(
                c.tier.skippable() || named.contains(c.name.as_str()),
                "{}",
                c.name
            );
        }
        // Zelda's and Impa's chains are swapped for flags, not relinked.
        let mut names = names_of(&picked);
        names.retain(|n| !ZELDA.iter().any(|z| *z == *n) && !IMPA.iter().any(|z| *z == *n));
        for flow in picked
            .iter()
            .flat_map(|c| &c.flows)
            .collect::<BTreeSet<_>>()
        {
            let data = romfs.read(&flows::rel(flow)).unwrap();
            let after = relinked(&data, &names).unwrap();
            let after = Flowchart::parse(&after).unwrap().unwrap();
            for (i, e) in (0u16..).zip(&after.events) {
                if transition(&after, &e.kind).is_some_and(|t| names.contains(t)) {
                    let relinked = |n: u16| {
                        let kind = &after.events[n as usize].kind;
                        names.contains(transition(&after, kind).unwrap_or(""))
                    };
                    let reached = (after.links_to(i).into_iter())
                        .any(|l| !matches!(l, Link::Next(n) if relinked(n)));
                    assert!(!reached, "{flow} still runs {}", e.name);
                }
            }
        }
    }

    /// The audit's class A: the scenes whose effects all copy. The rule in
    /// `cut` is computed from the flows, and a change to it shows as a diff
    /// against these names.
    const CLASS_A: [&Name; 111] = [
        scene::AWAKE_FIRE_GET,
        scene::AWAKE_FOREST_GET,
        scene::AWAKE_SHADOW,
        scene::AWAKE_SHADOW_GET,
        scene::AWAKE_SPIRIT,
        scene::AWAKE_SPIRIT_GET,
        scene::AWAKE_WATER,
        scene::AWAKE_WATER_GET,
        scene::DEATH_MT_CRATER_0000,
        scene::DEATH_MT_CRATER_0000_FROM_TOP,
        scene::DEATH_MT_CRATER_0010,
        scene::DEATH_MT_TRAIL_0000,
        scene::DEATH_MT_TRAIL_0025,
        scene::DEATH_MT_TRAIL_0027,
        scene::DEATH_MT_TRAIL_0035,
        scene::DEKU_0000,
        scene::DEKU_0003,
        scene::DESERT_COLOSSUS_0000,
        scene::DESERT_COLOSSUS_0025,
        scene::DESERT_COLOSSUS_0030,
        scene::DODONGO_CAVE_0000,
        scene::DODONGO_CAVE_0010,
        scene::FIRE_TEMPLE_0010,
        scene::FOREST_TEMPLE_0000,
        scene::GANON_CASTLE_0000,
        scene::GERUDO_0000,
        scene::GERUDO_0030,
        scene::GERUDO_VALLEY_0000,
        scene::GORON_0000,
        scene::GORON_0020,
        scene::GORON_0025,
        scene::GORON_0030,
        scene::GRAVEYARD_0000,
        scene::HRL_CASTLE_0000,
        scene::HRL_CASTLE_0005,
        scene::HRL_CASTLE_0020,
        scene::HRL_CASTLE_0030,
        scene::HRL_CASTLE_0035,
        scene::HRL_CASTLE_0050,
        scene::HRL_CASTLE_0070,
        scene::HRL_FIELD_0010,
        scene::HRL_FIELD_0020,
        scene::HRL_FIELD_0030,
        scene::HRL_FIELD_0040,
        scene::HRL_FIELD_0050,
        scene::HRL_FIELD_0080,
        scene::INSIDE_GANON_CASTLE_0005,
        scene::INSIDE_GANON_CASTLE_0006,
        scene::INSIDE_GANON_CASTLE_0007,
        scene::INSIDE_GANON_CASTLE_0010,
        scene::INSIDE_GANON_CASTLE_0020,
        scene::INSIDE_GANON_CASTLE_0030,
        scene::INSIDE_GANON_CASTLE_0040,
        scene::INSIDE_GANON_CASTLE_0050,
        scene::INSIDE_GANON_CASTLE_0070,
        scene::INSIDE_GANON_CASTLE_0080,
        scene::INSIDE_GANON_CASTLE_0100,
        scene::INSIDE_GANON_CASTLE_0150,
        scene::INSIDE_JABU_JABU_0000,
        scene::INSIDE_JABU_JABU_0030,
        scene::KAKARIKO_0000,
        scene::KAKARIKO_0010,
        scene::KAKARIKO_0050,
        scene::KAKARIKO_0060,
        scene::KOKIRI_0045,
        scene::KOKIRI_0060,
        scene::KOKIRI_0070,
        scene::KOKIRI_0075,
        scene::KOKIRI_0145,
        scene::KOKIRI_0150,
        scene::KOKIRI_0155,
        scene::KOKIRI_0170,
        scene::KOKIRI_0175,
        scene::LAKE_HRL_0000,
        scene::LAKE_HRL_0003,
        scene::LAKE_HRL_0005,
        scene::LAKE_HRL_0011,
        scene::LAKE_HRL_0035,
        scene::LON_LON_0010,
        scene::LOST_WOOD_0000,
        scene::LOST_WOOD_0010,
        scene::MELODY_GET_EPONA,
        scene::MELODY_GET_FIRE,
        scene::MELODY_GET_FOREST,
        scene::MELODY_GET_SARIA,
        scene::MELODY_GET_SHADOW,
        scene::MELODY_GET_SHINE,
        scene::MELODY_GET_SPILIT,
        scene::MELODY_GET_STORM,
        scene::MELODY_GET_WATER,
        scene::MELODY_GET_ZELDA,
        scene::SAGE_ROOM_0005,
        scene::SAGE_ROOM_0010,
        scene::SHADOW_TEMPLE_0000,
        scene::SPIRIT_TEMPLE_0000,
        scene::TIME_TEMPLE_0000,
        scene::TIME_TEMPLE_0055,
        scene::TIME_TEMPLE_0070,
        scene::TIME_TEMPLE_0075,
        scene::TIME_TEMPLE_0086,
        scene::TIME_TEMPLE_0087,
        scene::TIME_TEMPLE_0130,
        scene::WATER_TEMPLE_0015,
        scene::ZORA_0000,
        scene::ZORA_0010,
        scene::ZORA_FOUNTAIN_0000,
        scene::ZORA_FOUNTAIN_0030,
        scene::ZORA_FOUNTAIN_0040,
        scene::ZORA_RIVER_0000,
        scene::ZORA_RIVER_0005,
        scene::ZORA_RIVER_0010,
    ];

    /// Class A scenes the rule leaves out, where the audit read them as
    /// starting from a flow. Each starts from the map, calls the horse, or
    /// calls a flow its caller's event doesn't bind, so a transition to swap
    /// or an effect to copy is missing.
    const LEFT_OUT: [&Name; 9] = [
        // The map starts it, and it moves a quest step.
        scene::FOREST_TEMPLE_0000,
        // The map starts it, and it sets a flag that is its effect.
        scene::LAKE_HRL_0005,
        // Malon's call brings Epona with `EventTriggerCallHorseAround`.
        scene::MELODY_GET_EPONA,
        // Sheik's Bolero, Minuet and Serenade call `AllSage`, which the
        // events that play them don't bind.
        scene::MELODY_GET_FIRE,
        scene::MELODY_GET_FOREST,
        scene::MELODY_GET_WATER,
        // A flow plays it, and so does the map.
        scene::MELODY_GET_SARIA,
        // The map starts it, and it grants.
        scene::MELODY_GET_SPILIT,
        // The map starts it, and it sets the temple's gates and wind.
        scene::SHADOW_TEMPLE_0000,
    ];

    /// Scenes the audit read as timelines, as no flow has their name. Each
    /// one's `EventSetting` row runs another scene's flow, and the rule cuts
    /// them with what that entry point sets: a rescued Goron's actor delete
    /// flag, or Ganon's Castle's flag that Link has seen a barrier.
    const SHARED_FLOW: [&Name; 12] = [
        scene::FIRE_TEMPLE_0014,
        scene::FIRE_TEMPLE_0015,
        scene::FIRE_TEMPLE_0016,
        scene::FIRE_TEMPLE_0017,
        scene::FIRE_TEMPLE_0018,
        scene::FIRE_TEMPLE_0019,
        scene::FIRE_TEMPLE_0020,
        scene::FIRE_TEMPLE_0021,
        scene::INSIDE_GANON_CASTLE_0200,
        scene::INSIDE_GANON_CASTLE_0201,
        scene::INSIDE_GANON_CASTLE_0202,
        scene::INSIDE_GANON_CASTLE_0203,
    ];

    fn effects_tier(catalogue: &Catalogue) -> impl Iterator<Item = &Cutscene> {
        catalogue.0.iter().filter(|c| !c.tier.skippable())
    }

    #[test]
    fn the_rule_cuts_the_audits_class_a_but_for_what_it_leaves_out_or_skips_already() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let catalogue = Catalogue::load(&romfs).unwrap();
        let all = Flows::load(&romfs).unwrap();
        let cut: BTreeSet<&str> = effects_tier(&catalogue)
            .filter(|c| matches!(cut::judge(&all, c), cut::Verdict::Cut(_)))
            .map(|c| c.name.as_str())
            .collect();
        let expected: BTreeSet<&str> = CLASS_A
            .into_iter()
            .chain(SHARED_FLOW)
            .map(|n| n.get())
            .filter(|n| !owned(n) && !LEFT_OUT.iter().any(|l| l == n))
            .collect();
        assert_eq!(cut, expected);
        // Every name the tables use is a scene.
        for n in CLASS_A
            .iter()
            .chain(&LEFT_OUT)
            .copied()
            .chain(cut::KEPT.iter().map(|k| k.0))
        {
            assert!(catalogue.get(n).is_some(), "{n}");
        }
        // The ones with a skip of their own are not cut twice.
        assert!(cut.iter().all(|n| !owned(n)));
        // Darunia's dance and gift go, his prompt and his wrong-tune scene stay.
        for n in [scene::GORON_0025, scene::GORON_0030] {
            assert!(cut.contains(&**n), "{n}");
        }
        assert!(!cut.contains(&**scene::GORON_0028));
    }
}
