//! The tool table: what each obstacle needs, as a rule sentence in the
//! words of `abilities.toml`. The actor pack decides where it says
//! anything, the actor's name where it doesn't.

use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Result;
use ounce_formats::{ActorPack, Byml, BymlMap, Romfs, murmur3};

use crate::checks::{GYAML, str_field};
use crate::items::follow;
use crate::known::{Name, field, flag};

/// Only these actors can be obstacles. `Land_` actors are scenery.
const PREFIXES: &[&str] = &["Obj_", "FldObj_"];

/// Obstacle actors by name fragment, first match wins, with the tool each
/// needs when its pack says nothing. Those tools are guesses. `None` marks
/// actors that look like obstacles but aren't.
static OBSTACLES: LazyLock<Vec<(&str, Option<&str>)>> = LazyLock::new(|| {
    vec![
        // Debris left by broken rocks: a model and nothing else.
        ("Fragment", None),
        // These collide only with bombs or the hookshot.
        ("HitOnlyBomb", None),
        ("Hookshot", None),
        ("HitBomb", Some("explosives")),
        ("CrawlHole", Some("child")),
        ("HookShotTarget", Some("hookshot")),
        (flag::BOMB_FLOWER, Some("goron-bracelet")),
        ("BrokenRock", Some("explosives or hammer")),
        ("BrokenRubble", Some("explosives or hammer")),
        ("CrackWall", Some("explosives or hammer")),
        ("CrackFloor", Some("explosives or hammer")),
        ("LiftRock", Some("gold-gauntlets")),
        ("EyeSwitch", Some("slingshot or bow or boomerang")),
        (flag::GLASS_TRUTH, Some("lens")),
        (flag::SONG_OF_TIME_BLOCK, Some("song-of-time")),
        ("ScarecrowMarker", Some("scarecrow")),
        (flag::RED_ICE, Some("blue-fire")),
        ("Hammer", Some("hammer")),
        (flag::SOLAR_SWITCH, Some("light")),
        ("RupeeToken", Some("silver-rupees")),
        ("DoorChainLock", Some("small-key")),
        (field::SPIDER_WEB, Some("explosives or fire")),
        ("DiggingHoleSoil", Some("digging")),
        // Leaves floating on water, not bean rides: they sit below and far
        // from every bean patch, and in maps that have none. The ride grows
        // from the patch's soil, so a check above a patch says
        // `magic-beans` in its own rule.
        ("MagicBean", None),
    ]
});

/// Damage types an obstacle may need, in the order rules list them, and
/// the ability that deals each.
const DAMAGE: &[(&str, &str)] = &[
    ("Bomb", "explosives"),
    ("Hammer", "hammer"),
    ("SlingshotBullet", "slingshot"),
    ("Arrow", "bow"),
    ("Boomerang", "boomerang"),
    ("Hookshot", "hookshot"),
    ("Fire", "fire"),
];
/// Damage types anyone deals, by rolling or swinging.
const FREE: &[&str] = &["Body", "SmallWeapon", "LargeWeapon", "ThrustWeapon"];
/// Full damage. Actors that take everything take it at 1, so lower rates
/// don't say what an obstacle needs.
const FULL: f32 = 100.0;

/// `LiftGrade`s and who can lift each.
const GRADES: &[(&str, Option<&str>)] = &[
    ("None", None),
    ("Low", Some("goron-bracelet")),
    ("Middle", Some("silver-gauntlets")),
    ("High", Some("gold-gauntlets")),
];

/// The eye switch AI reads this blackboard key, and fires on these damage
/// types, plus `Hookshot` while the key is true.
const HIT_HOOKSHOT: &Name = field::IS_HIT_HOOK_SHOT;
const EYE_SWITCH_HITS: &[&str] = &["SlingshotBullet", "Arrow", "Boomerang"];

/// The damage types an eye switch fires on, with `Hookshot` if it takes it.
fn eye_hits(hookshot: bool) -> Vec<&'static str> {
    let mut hits = EYE_SWITCH_HITS.to_vec();
    hits.extend(hookshot.then_some("Hookshot"));
    hits
}

/// What an actor pack says about getting past its actor, each answer
/// with the data behind it.
#[derive(Clone)]
enum Said {
    Nothing,
    /// Anyone gets past.
    Free(String),
    Rule(String, String),
    /// Whether the hookshot hits it, unless the placement says otherwise.
    EyeSwitch {
        hookshot: bool,
    },
}

pub struct Tools<'a> {
    romfs: &'a Romfs,
    packs: HashMap<String, Said>,
}

impl<'a> Tools<'a> {
    pub fn new(romfs: &'a Romfs) -> Self {
        Self {
            romfs,
            packs: HashMap::new(),
        }
    }

    /// The rule a placed actor needs to get past, `None` if it's no obstacle.
    pub fn of(&mut self, placement: &Byml) -> Result<Option<String>> {
        let gyaml = str_field(placement, GYAML).unwrap_or_default();
        self.rule(gyaml, dynamic_bool(placement, HIT_HOOKSHOT))
    }

    /// The rule an actor needs as its pack says, before any placement's
    /// own params, `None` if it's no obstacle.
    pub fn of_actor(&mut self, gyaml: &str) -> Result<Option<String>> {
        self.rule(gyaml, None)
    }

    /// `hookshot` is the placement's say on whether an eye switch takes it.
    fn rule(&mut self, gyaml: &str, hookshot: Option<bool>) -> Result<Option<String>> {
        let Some(guess) = guess(gyaml) else {
            return Ok(None);
        };
        Ok(match self.said(gyaml)? {
            Said::Nothing => Some(guess.to_owned()),
            Said::Free(_) => None,
            Said::Rule(rule, _) => Some(rule),
            Said::EyeSwitch { hookshot: default } => any_of(&eye_hits(hookshot.unwrap_or(default))),
        })
    }

    /// What an actor's pack says about it, before any placement's params:
    /// the rule (`None` when anyone gets past), why, and whether the rule
    /// is a guess from its name. `None` when it's no obstacle at all.
    pub fn explain(&mut self, gyaml: &str) -> Result<Option<(Option<String>, String, bool)>> {
        let Some(guess) = guess(gyaml) else {
            return Ok(None);
        };
        Ok(Some(match self.said(gyaml)? {
            Said::Nothing => (
                Some(guess.to_owned()),
                "its name; its pack says nothing".into(),
                true,
            ),
            Said::Free(why) => (None, why, false),
            Said::Rule(rule, why) => (Some(rule), why, false),
            Said::EyeSwitch { hookshot } => {
                let hits = eye_hits(hookshot);
                let why = format!("eye switch: hit by {}", hits.join(", "));
                (any_of(&hits), why, false)
            }
        }))
    }

    /// Whether an obstacle's tool comes from its name, not its pack.
    pub fn guessed(&mut self, gyaml: &str) -> Result<bool> {
        Ok(guess(gyaml).is_some() && matches!(self.said(gyaml)?, Said::Nothing))
    }

    /// What the actor's pack says, or else what the first actor it holds
    /// says: a bomb flower holds its fruit.
    fn said(&mut self, gyaml: &str) -> Result<Said> {
        if let Some(said) = self.packs.get(gyaml) {
            return Ok(said.clone());
        }
        // Saying nothing meanwhile ends a cycle of actors holding each other.
        self.packs.insert(gyaml.to_owned(), Said::Nothing);
        let said = self.read_pack(gyaml);
        match &said {
            Ok(said) => self.packs.insert(gyaml.to_owned(), said.clone()),
            // Not cached, so the next ask meets the error again.
            Err(_) => self.packs.remove(gyaml),
        };
        said
    }

    /// [`Self::said`], uncached.
    fn read_pack(&mut self, gyaml: &str) -> Result<Said> {
        let pack = ActorPack::load(self.romfs, gyaml)?;
        let Some(param) = pack.actor_param()? else {
            return Ok(Said::Nothing);
        };
        let mut said = read(&pack, &param)?;
        if matches!(said, Said::Nothing) {
            for actor in held(&pack, &param)? {
                said = self.said(&actor)?;
                if !matches!(said, Said::Nothing) {
                    break;
                }
            }
        }
        Ok(said)
    }
}

/// The guessed tool for an obstacle actor, `None` if it's no obstacle.
fn guess(gyaml: &str) -> Option<&'static str> {
    if !PREFIXES.iter().any(|p| gyaml.starts_with(p)) {
        return None;
    }
    OBSTACLES.iter().find(|(frag, _)| gyaml.contains(frag))?.1
}

/// Eye switch, then lift grade, then what breaks it.
fn read(pack: &ActorPack, param: &BymlMap) -> Result<Said> {
    if let Some(hookshot) = blackboard_bool(pack, param, HIT_HOOKSHOT)? {
        return Ok(Said::EyeSwitch { hookshot });
    }
    if let Some(grade) = follow(pack, param, field::LIFTABLE_REF)?
        .and_then(|lift| lift.get(&**field::LIFT_GRADE)?.as_int::<u32>().ok())
    {
        return Ok(match GRADES.iter().find(|(g, _)| hash(g) == grade) {
            Some((g, None)) => Said::Free(format!("lift grade {g}")),
            Some((g, Some(tool))) => Said::Rule((*tool).to_owned(), format!("lift grade {g}")),
            None => Said::Nothing,
        });
    }
    let mut breakers = full_damage(pack, param)?;
    let burn = burns(pack, param)?;
    if burn {
        breakers.push("Fire");
    }
    let why = || {
        let mut why = format!("full damage from {}", breakers.join(", "));
        if burn {
            why += " (its material burns)";
        }
        why
    };
    if breakers.iter().any(|t| FREE.contains(t)) {
        return Ok(Said::Free(why()));
    }
    Ok(any_of(&breakers).map_or(Said::Nothing, |rule| Said::Rule(rule, why())))
}

fn hash(name: &str) -> u32 {
    murmur3(name.as_bytes())
}

/// The abilities that deal `types`, joined with `or`. `None` if none do.
fn any_of(types: &[&str]) -> Option<String> {
    let tools: Vec<_> = DAMAGE
        .iter()
        .filter(|(t, _)| types.contains(t))
        .map(|d| d.1)
        .collect();
    (!tools.is_empty()).then(|| tools.join(" or "))
}

/// Damage types the actor takes in full, from its damage table.
fn full_damage(pack: &ActorPack, param: &BymlMap) -> Result<Vec<&'static str>> {
    let Some(life) = follow(pack, param, field::LIFE_REF)? else {
        return Ok(vec![]);
    };
    let Some(damage) = string(&life, field::DAMAGE_PARAMETERS) else {
        return Ok(vec![]);
    };
    let Some(damage) = pack.resolve(damage)? else {
        return Ok(vec![]);
    };
    let Some(rates) = damage
        .get(&**field::EXTERNAL_DAMAGE_PARAMETERS)
        .and_then(|e| {
            e.as_map()
                .ok()?
                .get(&**field::DAMAGE_RATES)?
                .as_hash_map()
                .ok()
        })
    else {
        return Ok(vec![]);
    };
    let known = DAMAGE.iter().map(|d| d.0).chain(FREE.iter().copied());
    Ok(known
        .filter(|t| {
            let rate = rates.get(&hash(t)).and_then(|r| r.as_float().ok());
            rate.is_some_and(|r| r >= FULL)
        })
        .collect())
}

/// The actors this one holds, by name.
fn held(pack: &ActorPack, param: &BymlMap) -> Result<Vec<String>> {
    let Some(holder) = follow(pack, param, field::ACTOR_HOLDER_REF)? else {
        return Ok(vec![]);
    };
    let settings = holder
        .get(&**field::ACTOR_HOLD_SETTINGS)
        .and_then(|s| s.as_map().ok());
    Ok(settings
        .into_iter()
        .flatten()
        .filter_map(|(_, s)| string(s.as_map().ok()?, "Actor"))
        .filter_map(|a| a.strip_prefix("Work/Actor/")?.split('.').next())
        .map(str::to_owned)
        .collect())
}

/// Whether fire destroys the actor: its material burns, and burning kills it.
fn burns(pack: &ActorPack, param: &BymlMap) -> Result<bool> {
    let Some(chemical) = follow(pack, param, field::CHEMICAL_REF)? else {
        return Ok(false);
    };
    let objects = chemical.get("Object").and_then(|o| o.as_array().ok());
    for object in objects.into_iter().flatten() {
        let Ok(object) = object.as_map() else {
            continue;
        };
        let dies = object
            .get(&**field::REQUEST_DEATH_IF_BURNED)
            .and_then(|b| b.as_bool().ok());
        let Some(material) =
            string(object, field::CHEMICAL_MATERIAL).filter(|_| dies == Some(true))
        else {
            continue;
        };
        let material = pack.resolve(material)?;
        if material.and_then(|m| m.get(&**field::IS_BURN)?.as_bool().ok()) == Some(true) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The initial value of a bool on the actor's blackboard, if it has one.
fn blackboard_bool(pack: &ActorPack, param: &BymlMap, key: &str) -> Result<Option<bool>> {
    let array = field::BLACKBOARD_PARAM_BOOL_ARRAY;
    let value = blackboard(pack, param, array, field::INIT_VAL, key)?;
    Ok(value.and_then(|v| v.as_bool().ok()))
}

/// The initial value of a string on actor `gyaml`'s blackboard, if it has
/// one.
pub(crate) fn blackboard_string(romfs: &Romfs, gyaml: &str, key: &str) -> Result<Option<String>> {
    let pack = ActorPack::load(romfs, gyaml)?;
    let Some(param) = pack.actor_param()? else {
        return Ok(None);
    };
    let array = field::BLACKBOARD_PARAM_STRING_ARRAY;
    let value = blackboard(&pack, &param, array, field::INIT_VAL_CONVERTED, key)?;
    Ok(value.and_then(|v| v.as_string().ok().map(|s| s.to_string())))
}

/// The initial value `init` of `key` in the `array` of the actor's
/// blackboard tables, if it has one. The first table naming it wins: an
/// actor's own table comes before the ones it shares (a big chest's before
/// the small chest's).
fn blackboard(
    pack: &ActorPack,
    param: &BymlMap,
    array: &Name,
    init: &Name,
    key: &str,
) -> Result<Option<Byml>> {
    let Some(info) = follow(pack, param, field::BLACKBOARD_REF)? else {
        return Ok(None);
    };
    let tables = info
        .get(&**field::BLACKBOARD_PARAM_TABLES)
        .and_then(|t| t.as_array().ok());
    for table in tables
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_string().ok())
    {
        let Some(table) = pack.resolve(table)? else {
            continue;
        };
        let entries = table.get(&**array).and_then(|b| b.as_array().ok());
        for entry in entries
            .into_iter()
            .flatten()
            .filter_map(|e| e.as_map().ok())
        {
            if string(entry, "BBKey") == Some(key) {
                return Ok(entry.get(&**init).cloned());
            }
        }
    }
    Ok(None)
}

/// A bool the placement sets in its `Dynamic` params.
fn dynamic_bool(placement: &Byml, key: &str) -> Option<bool> {
    let dynamic = placement.as_map().ok()?.get("Dynamic")?.as_map().ok()?;
    dynamic.get(key)?.as_bool().ok()
}

fn string<'m>(map: &'m BymlMap, key: &str) -> Option<&'m str> {
    map.get(key)?.as_string().ok().map(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{GYAML, actors, banc_files, hash_of, load_banc};
    use crate::game::dev_dump;
    use crate::known::{actor, flag};

    fn named(gyaml: &str) -> Byml {
        Byml::Map(
            [(GYAML.into(), Byml::String(gyaml.into()))]
                .into_iter()
                .collect(),
        )
    }

    fn placed(romfs: &Romfs, area: &str, hash: u64) -> Byml {
        for rel in banc_files(romfs).unwrap() {
            if !rel.contains(area) {
                continue;
            }
            let doc = load_banc(romfs, &rel).unwrap();
            if let Some(a) = actors(&doc.root)
                .unwrap()
                .iter()
                .find(|a| hash_of(a) == Some(hash))
            {
                return a.clone();
            }
        }
        panic!("no placement {hash} in {area}")
    }

    #[test]
    fn crack_walls_need_explosives_or_hammer() {
        let Some(romfs) = dev_dump() else { return };
        let mut tools = Tools::new(&romfs);
        for wall in [
            actor::OBJ_SHADOW_TEMPLE_CRACK_WALL_A_02,
            actor::OBJ_DODONGO_CAVE_CRACK_FLOOR_A_01,
            actor::OBJ_GANON_CASTLE_BROKEN_RUBBLE_A_01,
        ] {
            let rule = tools.of(&named(wall)).unwrap();
            assert_eq!(rule.as_deref(), Some("explosives or hammer"), "{wall}");
            assert!(!tools.guessed(wall).unwrap(), "{wall}");
        }
    }

    #[test]
    fn rocks_need_the_strength_their_grade_says() {
        let Some(romfs) = dev_dump() else { return };
        let mut tools = Tools::new(&romfs);
        for (rock, rule) in [
            (actor::OBJ_LIFT_ROCK_A_01, None),
            (actor::OBJ_LIFT_ROCK_B_01, Some("silver-gauntlets")),
            (actor::OBJ_LIFT_ROCK_C_01, Some("gold-gauntlets")),
            (actor::OBJ_BOMB_FLOWER_A_03, Some("goron-bracelet")),
        ] {
            assert_eq!(tools.of(&named(rock)).unwrap().as_deref(), rule, "{rock}");
            assert!(!tools.guessed(rock).unwrap(), "{rock}");
        }
    }

    #[test]
    fn eye_switches_take_the_hookshot_where_the_placement_allows() {
        let Some(romfs) = dev_dump() else { return };
        let mut tools = Tools::new(&romfs);
        let no_hookshot = placed(&romfs, flag::FOREST_TEMPLE, 7569020732507755909);
        let default = placed(&romfs, flag::FOREST_TEMPLE, 18179844149316132602);
        assert_eq!(
            tools.of(&no_hookshot).unwrap().as_deref(),
            Some("slingshot or bow or boomerang")
        );
        assert_eq!(
            tools.of(&default).unwrap().as_deref(),
            Some("slingshot or bow or boomerang or hookshot")
        );
    }

    #[test]
    fn webs_burn_or_blow_up() {
        let Some(romfs) = dev_dump() else { return };
        let mut tools = Tools::new(&romfs);
        let web = actor::OBJ_INSIDE_THE_DEKU_TREE_SPIDER_WEB_A_02;
        let rule = tools.of(&named(web)).unwrap();
        assert_eq!(rule.as_deref(), Some("explosives or fire"));
        assert!(!tools.guessed(web).unwrap());
    }

    #[test]
    fn air_walls_and_scenery_are_not_obstacles() {
        let Some(romfs) = dev_dump() else { return };
        let mut tools = Tools::new(&romfs);
        for actor in [
            actor::FLD_OBJ_AIR_WALL_HIT_ONLY_BOMB,
            actor::FLD_OBJ_AIR_WALL_HOOKSHOT,
            actor::OBJ_COMMON_FRAGMENT_BROKEN_ROCK_A_00,
            actor::LAND_DEKU_SPIDER_WEB_PROP_A_05,
            actor::OBJ_PLANT_MAGIC_BEAN_LEAF_A_02,
        ] {
            assert_eq!(tools.of(&named(actor)).unwrap(), None, "{actor}");
        }
    }

    #[test]
    fn unread_obstacles_keep_their_guess() {
        let Some(romfs) = dev_dump() else { return };
        let mut tools = Tools::new(&romfs);
        let ice = actor::OBJ_RED_ICE_A_01;
        assert_eq!(tools.of(&named(ice)).unwrap().as_deref(), Some("blue-fire"));
        assert!(tools.guessed(ice).unwrap());
    }
}
