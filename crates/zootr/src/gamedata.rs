//! GameData: every save flag, keyed by the murmur3 of its name. Names only
//! come back by hashing strings found elsewhere (flows, Banc, enum values).
//!
//! `BoolExp` entries are derived flags, stored as a postfix program over
//! other entries. That's how story state reaches the world: a Presence or a
//! map swap names a BoolExp, and the BoolExp reads quest step enums.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlDoc, KEYED, Romfs, murmur3};
use serde::Serialize;

use crate::known::{Name, field, flag};
use crate::value;
pub use crate::value::Input;

pub const PATH: &str = "GameData/GameDataList.Product.110.byml.zs";
/// The Struct holding whether Link has each item, `IsGet.ITEM`.
pub const IS_GET: &str = "IsGet";
const BOOL_EXP: &Name = flag::BOOL_EXP;
/// The ResetType of a flag that never resets (`NoRespawn`): Gold
/// Skulltulas, heart pieces, placed rupees. `docs/research/tokens-freestanding-pots.md`.
const NO_RESPAWN: i64 = 24;
#[cfg_attr(test, derive(Default))]
pub struct GameData {
    /// Table (`Bool`, `BoolExp`, `Enum`...) and entry, by hash.
    entries: HashMap<u32, (String, Byml)>,
    names: HashMap<u32, String>,
    /// The per-placement flags (Bool64bitKey) that never reset, by
    /// placement hash.
    keys: HashSet<u64>,
    /// From [`labels`](crate::labels), changeable while shared.
    labels: RwLock<HashMap<u32, String>>,
}

/// Each table of the GameData file and how many entries it holds, in the
/// file's order, or with `only` that table's first three entries (or the
/// save layout, for `MetaData`).
pub fn print_tables(romfs: &Romfs, only: Option<&str>) -> Result<()> {
    let doc = BymlDoc::parse(&romfs.read(PATH)?).context(PATH)?;
    let data = doc.root.as_map()?.get("Data").context("no Data")?;
    if only == Some("MetaData") {
        println!("{:?}", doc.root.as_map()?.get("MetaData"));
    }
    for (table, list) in data.as_map()? {
        let list = list.as_array()?;
        match only {
            None => println!("{:>6} {table}", list.len()),
            Some(t) if t == table.as_str() => list.iter().take(3).for_each(|e| println!("{e:?}")),
            Some(_) => {}
        }
    }
    Ok(())
}

/// What GameData says about one value.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Info {
    /// The table: `Bool`, `Enum`, `Int`...
    pub kind: String,
    pub input: Input,
    /// `true` and `false` for a Bool, an Enum's value names in the
    /// definition's order, else empty.
    pub choices: Vec<String>,
    /// What a new game starts with, for a scalar.
    pub default: Option<String>,
}

impl Info {
    /// Refuses `value` for `name` when it isn't one of the choices.
    pub fn check(&self, name: &str, value: &str) -> Result<()> {
        ensure!(
            self.choices.is_empty() || self.choices.iter().any(|c| c == value),
            "{value} isn't a value of {name}, which takes one of: {}",
            self.choices.join(", ")
        );
        Ok(())
    }
}

pub(crate) fn int(v: &Byml) -> Result<i64> {
    Ok(v.as_int::<i64>()?)
}

/// An Enum entry's value names, in order.
fn raw_values(entry: &Byml) -> Option<Vec<String>> {
    let raw = entry
        .as_map()
        .ok()?
        .get(&**field::RAW_VALUES)?
        .as_array()
        .ok()?;
    raw.iter()
        .map(|v| v.as_string().ok().map(|s| s.to_string()))
        .collect()
}

/// A rendered operand and its top-level operator, which decides whether it
/// needs parens inside a bigger expression.
type Term = (String, Option<&'static str>);

fn pop(stack: &mut Vec<Term>) -> Result<Term> {
    stack
        .pop()
        .with_context(|| format!("{} stack underflow", flag::BOOL_EXP))
}

/// `t` as an operand of `op`. Chains of one operator stay flat, and a
/// comparison only needs parens under `!`.
fn operand((text, top): Term, op: &str) -> String {
    match top {
        Some("cmp") if op != "!" => text,
        Some(t) if t != op => format!("({text})"),
        _ => text,
    }
}

impl GameData {
    pub fn load(romfs: &Romfs) -> Result<Self> {
        Self::parse(&romfs.read(PATH)?)
    }

    /// GameData from its decompressed file.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let doc = BymlDoc::parse(data).context(PATH)?;
        let mut entries = HashMap::new();
        let mut names = HashMap::new();
        let mut keys = HashSet::new();
        for (table, list) in doc
            .root
            .as_map()?
            .get("Data")
            .context("no Data")?
            .as_map()?
        {
            for e in list.as_array()? {
                let hash = int(&e.as_map()?["Hash"])?;
                if table == KEYED {
                    let reset = e.as_map()?.get(&**field::RESET_TYPE_VALUE);
                    if reset.map(int).transpose()? == Some(NO_RESPAWN) {
                        keys.insert(hash as u64);
                    }
                    continue;
                }
                let Ok(hash) = u32::try_from(hash) else {
                    continue;
                };
                if let Some(raw) = e.as_map()?.get(&**field::RAW_VALUES) {
                    for v in raw.as_array()? {
                        let v = v.as_string()?;
                        names.insert(murmur3(v.as_bytes()), v.to_string());
                    }
                }
                entries.insert(hash, (table.to_string(), e.clone()));
            }
        }
        let labels = crate::labels::load(&crate::labels::path().unwrap_or_default());
        Ok(Self {
            entries,
            names,
            keys,
            labels: labels.into(),
        })
    }

    /// Whether placement `hash` has a per-placement flag that never resets.
    /// A pot's resets with its area, and it sets on the break, not when
    /// Link picks up the drop, so it can't say the check is done.
    pub fn has_key(&self, hash: u64) -> bool {
        self.keys.contains(&hash)
    }

    /// It with a key for placement `hash`, as a seed adds one.
    #[cfg(test)]
    pub fn with_key(mut self, hash: u64) -> Self {
        self.keys.insert(hash);
        self
    }

    /// Remember `name` if it hashes to an entry or enum value.
    pub fn learn(&mut self, name: &str) {
        let h = murmur3(name.as_bytes());
        if self.entries.contains_key(&h) {
            self.names.entry(h).or_insert_with(|| name.to_owned());
        }
    }

    /// The name, else the hash with any label.
    pub fn name(&self, hash: u32) -> String {
        let bare = self.bare(hash);
        match self.label(hash) {
            Some(label) => format!("{bare} ({label})"),
            None => bare,
        }
    }

    /// Every entry the game names, in no order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names
            .iter()
            .filter(|(h, _)| self.entries.contains_key(h))
            .map(|(_, n)| n.as_str())
    }

    /// The name, else the hash.
    pub fn bare(&self, hash: u32) -> String {
        self.names
            .get(&hash)
            .cloned()
            .unwrap_or_else(|| format!("#{hash:08x}"))
    }

    /// Our label for a value the game doesn't name.
    pub fn label(&self, hash: u32) -> Option<String> {
        if self.names.contains_key(&hash) {
            return None;
        }
        self.labels.read().unwrap().get(&hash).cloned()
    }

    /// Label `hash` for this session, or drop its label for empty `text`.
    /// [`labels::set`](crate::labels::set) keeps it.
    pub fn set_label(&self, hash: u32, text: &str) {
        let mut labels = self.labels.write().unwrap();
        match text.trim() {
            "" => labels.remove(&hash),
            text => labels.insert(hash, text.into()),
        };
    }

    /// Whether `hash` has a name or a label, rather than a bare hash.
    pub fn known(&self, hash: u32) -> bool {
        self.names.contains_key(&hash) || self.labels.read().unwrap().contains_key(&hash)
    }

    pub fn table(&self, name: &str) -> Option<&str> {
        let (table, _) = self.entries.get(&murmur3(name.as_bytes()))?;
        Some(table)
    }

    /// The table an entry sits in: `Bool`, `Struct`, `Enum`...
    pub fn kind(&self, hash: u32) -> Option<&str> {
        self.entries.get(&hash).map(|(t, _)| t.as_str())
    }

    /// Every entry in one table.
    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = u32> + 'a {
        self.entries
            .iter()
            .filter(move |(_, (t, _))| t == kind)
            .map(|(h, _)| *h)
    }

    /// The value names of the Enum entry `name`, in order.
    pub fn enum_values(&self, name: &str) -> Option<Vec<String>> {
        raw_values(self.entry(murmur3(name.as_bytes()))?)
    }

    /// The type, choices and default of `name`, a GameData name or a `0x`
    /// hash.
    pub fn info(&self, name: &str) -> Result<Info> {
        let hash = crate::live::hash(name)?;
        let (kind, entry) = self
            .entries
            .get(&hash)
            .with_context(|| format!("{} has no entry named or hashed {name}", flag::GAME_DATA))?;
        let map = entry.as_map()?;
        let choices = match kind.as_str() {
            "Bool" => vec!["true".into(), "false".into()],
            "Enum" => raw_values(entry).unwrap_or_default(),
            _ => Vec::new(),
        };
        // A whole array isn't one field; its elements each are.
        let input = match kind.ends_with("Array") {
            true => Input::Read,
            false => value::input(kind),
        };
        let default = map
            .get(&**field::DEFAULT_VALUE)
            .and_then(|d| match (kind.as_str(), d) {
                ("Enum", _) => {
                    let want = i64::from(u32::try_from(int(d).ok()?).ok()?);
                    let at = (map.get("Values")?.as_array().ok()?.iter())
                        .position(|v| int(v).ok().is_some_and(|v| v & 0xffff_ffff == want))?;
                    choices.get(at).cloned()
                }
                (_, Byml::Bool(b)) => Some(b.to_string()),
                (_, Byml::I32(n)) => Some(n.to_string()),
                (_, Byml::U32(n)) => Some(n.to_string()),
                (_, Byml::Float(f)) => Some(f.to_string()),
                (_, Byml::I64(n)) => Some(n.to_string()),
                (_, Byml::U64(n)) => Some(n.to_string()),
                (_, Byml::String(s)) => Some(s.to_string()),
                _ => None,
            });
        Ok(Info {
            kind: kind.clone(),
            input,
            choices,
            default: default.filter(|_| !kind.ends_with("Array") && kind != "Binary"),
        })
    }

    /// The entry for `hash`, if GameData has one.
    pub fn entry(&self, hash: u32) -> Option<&Byml> {
        self.entries.get(&hash).map(|(_, e)| e)
    }

    /// A Struct entry's members, as (name hash, value hash) pairs.
    pub fn members(&self, hash: u32) -> Result<Vec<(u32, u32)>> {
        let e = self
            .entry(hash)
            .with_context(|| format!("no struct {hash:08x}"))?;
        e.as_map()?[&**field::DEFAULT_VALUE]
            .as_array()?
            .iter()
            .map(|m| {
                let m = m.as_map()?;
                Ok((int(&m["Hash"])? as u32, int(&m["Value"])? as u32))
            })
            .collect()
    }

    /// A BoolExp as infix, or `None` for any other kind of entry.
    pub fn expr(&self, name: &str) -> Result<Option<String>> {
        self.expr_of(murmur3(name.as_bytes()))
    }

    /// [`Self::expr`] by hash.
    pub fn expr_of(&self, hash: u32) -> Result<Option<String>> {
        match self.entries.get(&hash) {
            Some((t, e)) if t == BOOL_EXP => {
                Ok(Some(self.render(e).with_context(|| self.bare(hash))?))
            }
            _ => Ok(None),
        }
    }

    /// Opcodes, from the ones whose meaning the flag names give away:
    /// `HaveDefeatedEnemy_X` is `count > 0`, `IsAfter_` and a step uses enum `>`,
    /// `IsCompleted` uses enum `== Complete`.
    pub(crate) fn render(&self, entry: &Byml) -> Result<String> {
        let mut stack: Vec<Term> = Vec::new();
        for ins in entry.as_map()?["Values"].as_array()? {
            let ins = ins.as_array()?;
            let arg = || -> Result<i64> { int(ins.get(1).context("missing operand")?) };
            let name = || -> Result<String> { Ok(self.name(arg()? as u32)) };
            let cmp = |op| -> Result<Term> { Ok((format!("{} {op}", name()?), Some("cmp"))) };
            let term = match int(&ins[0])? {
                0 | 8 | 10 => (name()?, None),
                1 | 15 => cmp("==")?,
                11 | 18 => cmp("<")?,
                12 | 19 => cmp(">")?,
                // Constants complete the comparison pushed just before.
                2 => (format!("{} {}", pop(&mut stack)?.0, name()?), Some("cmp")),
                16 => (format!("{} {}", pop(&mut stack)?.0, arg()?), Some("cmp")),
                5 => (format!("!{}", operand(pop(&mut stack)?, "!")), None),
                op @ (3 | 4) => {
                    let op = if op == 3 { "&&" } else { "||" };
                    let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                    let text = format!("{} {op} {}", operand(a, op), operand(b, op));
                    (text, Some(op))
                }
                op => bail!("unknown {} opcode {op}", flag::BOOL_EXP),
            };
            stack.push(term);
        }
        match stack.pop() {
            Some((text, _)) if stack.is_empty() => Ok(text),
            _ => bail!("{} left {} values", flag::BOOL_EXP, stack.len() + 1),
        }
    }

    /// Whether setting Bool `name` makes BoolExp `exp` true: `exp` is an
    /// `||` chain with `name` as one plain term.
    pub fn implied_by(&self, exp: &str, name: &str) -> Result<bool> {
        let Some(e) = self
            .entries
            .get(&murmur3(exp.as_bytes()))
            .filter(|(t, _)| t == BOOL_EXP)
            .map(|(_, e)| e)
        else {
            return Ok(false);
        };
        let mut ops = Vec::new();
        for ins in e.as_map()?["Values"].as_array()? {
            let ins = ins.as_array()?;
            let arg = ins.get(1).map(int).transpose()?;
            ops.push((int(&ins[0])?, arg));
        }
        let hash = murmur3(name.as_bytes());
        let plain = |i: usize| {
            ops[i].0 == 0
                && ops[i].1.map(|a| a as u32) == Some(hash)
                && ops.get(i + 1).is_none_or(|o| o.0 != 5)
        };
        Ok(ops.iter().all(|o| o.0 != 3) && (0..ops.len()).any(plain))
    }

    /// Every hash a BoolExp reads, plus its constants: enough to tell
    /// whether any derived flag depends on a given one.
    pub fn read_by_exps(&self) -> HashSet<u32> {
        self.entries
            .values()
            .filter(|(t, _)| t == BOOL_EXP)
            .filter_map(|(_, e)| e.as_map().ok()?.get("Values")?.as_array().ok())
            .flatten()
            .filter_map(|ins| int(ins.as_array().ok()?.get(1)?).ok())
            .map(|h| h as u32)
            .collect()
    }

    /// Every entry's hash, in no order.
    pub fn hashes(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.keys().copied()
    }

    /// The entries BoolExp `hash` reads, without its constants, or none
    /// for any other kind of entry.
    pub fn exp_inputs(&self, hash: u32) -> Vec<u32> {
        let Some(values) = (self.entries.get(&hash))
            .filter(|(t, _)| t == BOOL_EXP)
            .and_then(|(_, e)| e.as_map().ok()?.get("Values")?.as_array().ok())
        else {
            return Vec::new();
        };
        let mut inputs: Vec<u32> = values
            .iter()
            .filter_map(|ins| {
                let ins = ins.as_array().ok()?;
                // 2 and 16 complete a comparison with a constant.
                match int(ins.first()?).ok()? {
                    2 | 16 => None,
                    _ => Some(int(ins.get(1)?).ok()? as u32),
                }
            })
            .filter(|h| self.entries.contains_key(h))
            .collect();
        inputs.sort_unstable();
        inputs.dedup();
        inputs
    }

    /// Every BoolExp, rendered, so a bad opcode shows up in selftest.
    pub fn check_all(&self) -> Result<usize> {
        let mut n = 0;
        for (t, e) in self.entries.values() {
            if t == BOOL_EXP {
                self.render(e)?;
                n += 1;
            }
        }
        Ok(n)
    }
}

mod edit;
pub use edit::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::flag;

    #[test]
    fn knows_each_placements_own_flag() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        // A Kokiri Forest Gold Skulltula and heart piece. A rock's key
        // resets with its area, and a chest's opened flag is no key.
        assert!(gd.has_key(7116603238806634901));
        assert!(gd.has_key(12406896586250888098));
        assert!(!gd.has_key(6429280567576879719));
        assert!(!gd.has_key(0x316d8a20));
    }

    #[test]
    fn a_label_shows_next_to_the_hash() {
        let gd = GameData {
            labels: crate::labels::built_in().into(),
            ..Default::default()
        };
        assert!(gd.name(0xc66e58f2).starts_with("#c66e58f2 (rupees"));
        assert!(gd.known(0xc66e58f2));
        assert_eq!(gd.name(1), "#00000001");
        assert!(!gd.known(1));
        gd.set_label(1, "one");
        assert_eq!(gd.name(1), "#00000001 (one)");
        gd.set_label(1, " ");
        assert!(!gd.known(1));
    }

    fn dump() -> Option<GameData> {
        Some(GameData::load(&crate::game::dev_dump()?).unwrap())
    }

    #[test]
    fn a_bool_offers_true_and_false() {
        let Some(gd) = dump() else { return };
        let info = gd.info(flag::HYRULE_FIELD_ARRIVE_ENTRANCE).unwrap();
        assert_eq!(info.kind, "Bool");
        assert_eq!(info.input, Input::Choice);
        assert_eq!(info.choices, ["true", "false"]);
        assert_eq!(info.default.as_deref(), Some("false"));
    }

    #[test]
    fn an_enum_offers_its_values_in_order() {
        let Some(gd) = dump() else { return };
        let info = gd
            .info(&crate::steps::saved(crate::known::flow::GOTO_HYRULE_CASTLE))
            .unwrap();
        assert_eq!(info.kind, "Enum");
        assert_eq!(info.input, Input::Choice);
        assert_eq!(
            info.choices[..4],
            [flag::NOT_READY, "Ready", "Step01", "Step02"]
        );
        assert_eq!(info.choices.last().map(String::as_str), Some("Complete"));
        assert_eq!(info.default.as_deref(), Some(&**flag::NOT_READY));
        // The same entry by its hash.
        assert_eq!(gd.info("0xdb89c1a9").unwrap(), info);
    }

    #[test]
    fn an_int_has_no_choices_but_a_default() {
        let Some(gd) = dump() else { return };
        let info = gd.info(flag::PLAYER_STATUS_CURRENT_RUPEE).unwrap();
        assert_eq!(info.kind, "Int");
        assert_eq!(info.input, Input::Number);
        assert!(info.choices.is_empty());
        assert_eq!(info.default.as_deref(), Some("0"));
        info.check(flag::PLAYER_STATUS_CURRENT_RUPEE, "17").unwrap();
    }

    #[test]
    fn a_value_outside_the_choices_is_refused_naming_them() {
        let Some(gd) = dump() else { return };
        let enums = gd
            .info(&crate::steps::saved(crate::known::flow::GOTO_HYRULE_CASTLE))
            .unwrap();
        enums
            .check(
                &crate::steps::saved(crate::known::flow::GOTO_HYRULE_CASTLE),
                "Step01",
            )
            .unwrap();
        let said = enums
            .check(
                &crate::steps::saved(crate::known::flow::GOTO_HYRULE_CASTLE),
                "Step1",
            )
            .unwrap_err()
            .to_string();
        assert!(
            said.starts_with(&format!(
                "Step1 isn't a value of {}",
                crate::steps::saved(crate::known::flow::GOTO_HYRULE_CASTLE)
            )),
            "{said}"
        );
        assert!(
            said.contains(&format!("{}, Ready, Step01", flag::NOT_READY)),
            "{said}"
        );
        let bools = gd.info(flag::HYRULE_FIELD_ARRIVE_ENTRANCE).unwrap();
        let said = bools
            .check(flag::HYRULE_FIELD_ARRIVE_ENTRANCE, "yes")
            .unwrap_err()
            .to_string();
        assert!(said.ends_with("one of: true, false"), "{said}");
        assert!(gd.info("NoSuchName").is_err());
    }
}
