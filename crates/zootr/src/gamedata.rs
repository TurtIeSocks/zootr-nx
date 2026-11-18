//! GameData: every save flag, keyed by the murmur3 of its name. Names only
//! come back by hashing strings found elsewhere (flows, Banc, enum values).
//!
//! `BoolExp` entries are derived flags, stored as a postfix program over
//! other entries. That's how story state reaches the world: a Presence or a
//! map swap names a BoolExp, and the BoolExp reads quest step enums.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlDoc, BymlMap, KEYED, Romfs, murmur3};
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

/// Append a Bool named each of `names` to the GameData document's `Bool`
/// table, defaulting to false. Each is a copy of the table's first entry
/// with its own hash. The table isn't sorted, so the end will do, and a
/// save from before loads them at their default.
pub fn add_bools(root: &mut Byml, names: &[String]) -> Result<()> {
    add(root, "Bool", names, |_| true)
}

/// Give each new placement in `twins` (a placement's hash, then its
/// copy's) a key like the placement's: reset and area alike. A placement
/// with no key gives its copy none. The table stays sorted by hash, as
/// the game's is. Returns how many it added.
pub fn add_keys(root: &mut Byml, twins: &[(u64, u64)]) -> Result<usize> {
    let entries = table(root, KEYED)?;
    let hash_at = |e: &Byml| -> Option<u64> { e.as_map().ok()?.get("Hash")?.as_int::<u64>().ok() };
    let mut added = Vec::new();
    for &(of, copy) in twins {
        let Some(like) = entries.iter().find(|e| hash_at(e) == Some(of)) else {
            continue;
        };
        let mut entry = like.as_map()?.clone();
        entry.insert("Hash".into(), Byml::U64(copy));
        added.push(Byml::Map(entry));
    }
    let n = added.len();
    for entry in added {
        let hash = hash_at(&entry).context("a key without a hash")?;
        match entries.binary_search_by_key(&Some(hash), &hash_at) {
            Ok(_) => bail!("{} already has a key {hash}", flag::GAME_DATA),
            Err(at) => entries.insert(at, entry),
        }
    }
    // A save keeps each key that's set, 8 bytes each.
    grow_saves(root, 0, n)?;
    Ok(n)
}

/// [`add_bools`] for the `Int` table, defaulting to 0. Each copies the first
/// Int kept in the save the way Link's rupees and keys are.
pub fn add_ints(root: &mut Byml, names: &[String]) -> Result<()> {
    let kept = |e: &BymlMap| {
        e.get(&**field::RESET_TYPE_VALUE)
            .is_some_and(|v| int(v).ok() == Some(KEPT))
    };
    add(root, "Int", names, kept)
}

/// False or 0, typed as `like`.
fn zero(like: Option<&Byml>) -> Option<Byml> {
    Some(match like? {
        Byml::Bool(_) => Byml::Bool(false),
        Byml::I32(_) => Byml::I32(0),
        Byml::U32(_) => Byml::U32(0),
        Byml::I64(_) => Byml::I64(0),
        Byml::U64(_) => Byml::U64(0),
        _ => return None,
    })
}

/// The reset type of a value the save keeps.
const KEPT: i64 = 8;

/// Append an entry to table `kind` named each of `names`, copying the first
/// entry `like` takes, with its value at false or 0.
fn add(
    root: &mut Byml,
    kind: &str,
    names: &[String],
    like: impl Fn(&BymlMap) -> bool,
) -> Result<()> {
    let entries = table(root, kind)?;
    let template = entries
        .iter()
        .filter_map(|e| e.as_map().ok())
        .find(|e| like(e))
        .with_context(|| format!("the {kind} table has no entry to copy"))?
        .clone();
    let file = int(template
        .get(&**field::SAVE_FILE_INDEX)
        .context(field::SAVE_FILE_INDEX)?)?;
    for name in names {
        let hash = murmur3(name.as_bytes());
        let taken = entries.iter().any(|e| has_hash(e, hash));
        ensure!(!taken, "{} already has a {kind} {name}", flag::GAME_DATA);
        let mut entry = template.clone();
        entry.insert("Hash".into(), typed(template.get("Hash"), hash)?);
        let value = zero(template.get(&**field::DEFAULT_VALUE))
            .with_context(|| format!("a {kind}'s default isn't a Bool or an integer"))?;
        entry.insert(field::DEFAULT_VALUE.to_string().into(), value);
        entries.push(Byml::Map(entry));
    }
    grow_saves(root, file, names.len())
}

/// The bytes one entry takes in a save's table: its hash, then its value
/// or where its value is.
const SAVE_PAIR: i64 = 8;

/// Make room for `n` more entries in save file `file`'s table. The game
/// lays a save out by these sizes, so left alone the longer table runs
/// into the values after it and clobbers the first arrays there.
fn grow_saves(root: &mut Byml, file: i64, n: usize) -> Result<()> {
    let grow = n as i64 * SAVE_PAIR;
    let meta = (root.as_mut_map()?)
        .get_mut(&**field::META_DATA)
        .context(field::META_DATA)?
        .as_mut_map()?;
    for key in [field::ALL_DATA_SAVE_OFFSET, field::ALL_DATA_SAVE_SIZE] {
        bump(meta.get_mut(&**key).context(key)?, grow)?;
    }
    let Ok(file) = usize::try_from(file) else {
        return Ok(());
    };
    for key in [field::SAVE_DATA_OFFSET_POS, field::SAVE_DATA_SIZE] {
        let sizes = meta.get_mut(&**key).context(key)?.as_mut_array()?;
        bump(sizes.get_mut(file).context(key)?, grow)?;
    }
    Ok(())
}

/// Add `by` to integer `v`, keeping its kind.
fn bump(v: &mut Byml, by: i64) -> Result<()> {
    *v = like(v, int(v)? + by)?;
    Ok(())
}

/// Append a BoolExp named each alias that reads its Bool alone, by hash.
/// A Presence names its flag, so this is how one reads a Bool whose name
/// isn't known, like a chest's opened flag.
pub fn add_aliases(root: &mut Byml, aliases: &[(String, u32)]) -> Result<()> {
    let exps = table(root, BOOL_EXP.get())?;
    let first = exps
        .first()
        .with_context(|| format!("the {} table is empty", flag::BOOL_EXP))?
        .as_map()?
        .clone();
    // An instruction pushing an entry, for its integer types.
    let push = exps
        .iter()
        .filter_map(|e| e.as_map().ok()?.get("Values")?.as_array().ok())
        .flatten()
        .filter_map(|i| i.as_array().ok())
        .find(|i| i.len() == 2)
        .context("no BoolExp reads an entry")?
        .to_vec();
    for (name, of) in aliases {
        let hash = murmur3(name.as_bytes());
        let taken = exps.iter().any(|e| has_hash(e, hash));
        ensure!(!taken, "{} already has {name}", flag::GAME_DATA);
        let mut entry = first.clone();
        entry.insert("Hash".into(), typed(first.get("Hash"), hash)?);
        // Opcode 0 pushes a Bool.
        let read = vec![typed(push.first(), 0)?, typed(push.get(1), *of)?];
        entry.insert("Values".into(), Byml::Array(vec![Byml::Array(read)]));
        exps.push(Byml::Map(entry));
    }
    Ok(())
}

/// Make each of `names` a member of every Struct `like` is a member of
/// (`IsGet` and others), each member's value a new false Bool. The game
/// finds an item's entries through these member lists, and an item with
/// none can't be taken from the floor. A value is named `S.NAME` after
/// the one `S.LIKE` names `like`'s, for each `S` in `prefixes`, else after
/// the Struct's hash. Only Bool values are copied.
pub fn join_structs(
    root: &mut Byml,
    like: &str,
    names: &[String],
    prefixes: &[&str],
) -> Result<()> {
    let like_hash = murmur3(like.as_bytes());
    let bools: HashSet<u32> = table(root, "Bool")?
        .iter()
        .filter_map(|e| Some(int(e.as_map().ok()?.get("Hash")?).ok()? as u32))
        .collect();
    let mut values = Vec::new();
    for s in table(root, "Struct")?.iter_mut() {
        let s = s.as_mut_map()?;
        let struct_hash = int(s.get("Hash").context("a Struct with no hash")?)? as u32;
        let members = s
            .get_mut(field::DEFAULT_VALUE.get())
            .context("a Struct with no members")?
            .as_mut_array()?;
        let Some(member) = members.iter().find(|m| has_hash(m, like_hash)) else {
            continue;
        };
        let member = member.as_map()?.clone();
        let value = int(&member["Value"])? as u32;
        ensure!(
            bools.contains(&value),
            "{like}'s value in Struct #{struct_hash:08x} isn't a Bool"
        );
        let prefix = prefixes
            .iter()
            .find(|p| murmur3(format!("{p}.{like}").as_bytes()) == value)
            .map_or_else(|| format!("#{struct_hash:08x}"), |p| p.to_string());
        for name in names {
            let value = format!("{prefix}.{name}");
            let mut m = member.clone();
            m.insert(
                "Hash".into(),
                typed(member.get("Hash"), murmur3(name.as_bytes()))?,
            );
            m.insert(
                "Value".into(),
                typed(member.get("Value"), murmur3(value.as_bytes()))?,
            );
            members.push(Byml::Map(m));
            values.push(value);
        }
    }
    ensure!(
        !values.is_empty() || names.is_empty(),
        "{like} is in no Struct"
    );
    add_bools(root, &values)
}

/// BoolExp opcodes, as [`GameData::render`] reads them.
pub(crate) mod op {
    pub const INT_EQUAL: i64 = 15;
    pub const INT_GREATER: i64 = 19;
    pub const INT_CONSTANT: i64 = 16;
    pub const AND: i64 = 3;
    pub const OR: i64 = 4;
    pub const NOT: i64 = 5;
}

/// BoolExp `name`'s program, a postfix list of instructions, to edit.
pub(crate) fn program<'a>(root: &'a mut Byml, name: &str) -> Result<&'a mut Vec<Byml>> {
    let hash = murmur3(name.as_bytes());
    Ok(table(root, BOOL_EXP.get())?
        .iter_mut()
        .find(|e| has_hash(e, hash))
        .with_context(|| format!("no {} {name}", flag::BOOL_EXP))?
        .as_mut_map()?
        .get_mut("Values")
        .with_context(|| format!("{name} has no Values"))?
        .as_mut_array()?)
}

/// Instruction `code`, with `operand` if any, in the integer types of
/// `template`, an instruction of the same program that has an operand.
pub(crate) fn instruction(template: &[Byml], code: i64, operand: Option<i64>) -> Result<Byml> {
    let [op, arg] = template else {
        bail!("a template instruction takes an operand")
    };
    let mut out = vec![like(op, code)?];
    out.extend(operand.map(|n| like(arg, n)).transpose()?);
    Ok(Byml::Array(out))
}

/// Make BoolExp `name` always false, as `e && !e` of its own program, so
/// it still reads only what it read before.
pub fn never(root: &mut Byml, name: &str) -> Result<()> {
    let values = program(root, name)?;
    let template = (values.iter())
        .filter_map(|v| v.as_array().ok())
        .find(|i| i.len() == 2)
        .with_context(|| format!("{name} reads nothing"))?
        .to_vec();
    let e = values.clone();
    values.extend(e);
    values.push(instruction(&template, op::NOT, None)?);
    values.push(instruction(&template, op::AND, None)?);
    Ok(())
}

/// `n` as the same BYML integer kind as `template`.
fn like(template: &Byml, n: i64) -> Result<Byml> {
    Ok(match template {
        Byml::I32(_) => Byml::I32(i32::try_from(n)?),
        Byml::U32(_) => Byml::U32(u32::try_from(n)?),
        Byml::I64(_) => Byml::I64(n),
        Byml::U64(_) => Byml::U64(u64::try_from(n)?),
        other => bail!("{other:?} isn't an integer"),
    })
}

/// Whether entry `e`'s hash is `hash`.
fn has_hash(e: &Byml, hash: u32) -> bool {
    let h = e.as_map().ok().and_then(|m| m.get("Hash")).map(int);
    h.is_some_and(|h| h.is_ok_and(|h| h == i64::from(hash)))
}

/// `Data`'s table `kind`.
fn table<'a>(root: &'a mut Byml, kind: &str) -> Result<&'a mut Vec<Byml>> {
    Ok(root
        .as_mut_map()?
        .get_mut("Data")
        .context("no Data")?
        .as_mut_map()?
        .get_mut(kind)
        .with_context(|| format!("no {kind} table"))?
        .as_mut_array()?)
}

/// `hash` as the same integer type as `like`.
fn typed(like: Option<&Byml>, hash: u32) -> Result<Byml> {
    Ok(match like {
        Some(Byml::U32(_)) => Byml::U32(hash),
        Some(Byml::I32(_)) => Byml::I32(hash as i32),
        Some(Byml::U64(_)) => Byml::U64(hash.into()),
        Some(Byml::I64(_)) => Byml::I64(hash.into()),
        other => bail!("a hash is {other:?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{field, flag};

    #[test]
    fn a_twin_gets_a_key_like_its_holders_in_order() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let size = |root: &Byml| {
            let meta = root.as_map().unwrap()[&**field::META_DATA]
                .as_map()
                .unwrap();
            int(&meta[&**field::ALL_DATA_SAVE_SIZE]).unwrap()
        };
        let was = size(&doc.root);
        // A rock's key resets with its area. A made-up placement has none.
        let (rock, fake) = (6429280567576879719, 3);
        let twins = [(rock, 1 << 62), (fake, 5)];
        assert_eq!(add_keys(&mut doc.root, &twins).unwrap(), 1);
        assert_eq!(size(&doc.root), was + SAVE_PAIR);
        let keys = table(&mut doc.root, KEYED).unwrap();
        let hashes: Vec<u64> = keys
            .iter()
            .map(|e| e.as_map().unwrap()["Hash"].as_int::<u64>().unwrap())
            .collect();
        assert!(hashes.is_sorted());
        let entry = |h: u64| keys[hashes.iter().position(|&x| x == h).unwrap()].clone();
        let (mut held, copy) = (entry(rock).as_map().unwrap().clone(), entry(1 << 62));
        held.insert("Hash".into(), Byml::U64(1 << 62));
        assert_eq!(Byml::Map(held), copy);
        assert!(!hashes.contains(&5));
        assert!(add_keys(&mut doc.root, &twins).is_err());
    }

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
    fn an_alias_reads_its_bool_alone() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let alias = ("ZootrOpened.1".to_string(), 0xc66e58f2);
        add_aliases(&mut doc.root, std::slice::from_ref(&alias)).unwrap();
        let gd = GameData::parse(&doc.to_binary()).unwrap();
        assert_eq!(gd.expr(&alias.0).unwrap(), Some(gd.name(alias.1)));
        assert!(gd.implied_by(&alias.0, &gd.name(alias.1)).is_ok());
        assert!(add_aliases(&mut doc.root, &[alias]).is_err(), "added twice");
    }

    #[test]
    fn a_joined_item_is_in_every_struct_its_like_is_with_bools_of_its_own() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let like = crate::known::item::DUNGEON_SMALL_KEY.get();
        let name = "Item_A".to_string();
        join_structs(&mut doc.root, like, std::slice::from_ref(&name), &[IS_GET]).unwrap();
        let members = |root: &mut Byml, item: &str| {
            let hash = murmur3(item.as_bytes());
            let structs = table(root, "Struct").unwrap();
            let mut found = Vec::new();
            for s in structs.iter() {
                let s = s.as_map().unwrap();
                for m in s[field::DEFAULT_VALUE.get()].as_array().unwrap() {
                    if has_hash(m, hash) {
                        found.push(int(&m.as_map().unwrap()["Value"]).unwrap() as u32);
                    }
                }
            }
            found
        };
        let theirs = members(&mut doc.root, like);
        let ours = members(&mut doc.root, &name);
        assert_eq!(theirs.len(), ours.len());
        assert!(ours.contains(&murmur3(format!("{IS_GET}.{name}").as_bytes())));
        let bools = table(&mut doc.root, "Bool").unwrap();
        assert!(ours.iter().all(|v| bools.iter().any(|b| has_hash(b, *v))));
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

    #[test]
    fn an_added_int_is_kept_in_the_save_at_0_under_its_own_hash() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let name = "Rando_FakeCount".to_owned();
        add_ints(&mut doc.root, std::slice::from_ref(&name)).unwrap();
        let back = BymlDoc::parse(&doc.to_binary()).unwrap();
        let ints = back.root.as_map().unwrap()["Data"].as_map().unwrap()["Int"]
            .as_array()
            .unwrap();
        let entry = ints.last().unwrap().as_map().unwrap();
        assert_eq!(
            int(&entry["Hash"]).unwrap(),
            i64::from(murmur3(name.as_bytes()))
        );
        assert_eq!(int(&entry[&**field::DEFAULT_VALUE]).unwrap(), 0);
        assert_eq!(int(&entry[&**field::RESET_TYPE_VALUE]).unwrap(), KEPT);
        assert!(add_ints(&mut doc.root, &[name]).is_err());
    }

    #[test]
    fn added_bools_default_to_false_under_their_own_hash() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let count = |root: &Byml| {
            root.as_map().unwrap()["Data"].as_map().unwrap()["Bool"]
                .as_array()
                .unwrap()
                .len()
        };
        let before = count(&doc.root);
        let names = ["Rando_Test_00".to_owned(), "Rando_Test_01".to_owned()];
        add_bools(&mut doc.root, &names).unwrap();
        assert_eq!(count(&doc.root), before + 2);
        let bools = doc.root.as_map().unwrap()["Data"].as_map().unwrap()["Bool"]
            .as_array()
            .unwrap();
        for (name, entry) in names.iter().zip(&bools[before..]) {
            let entry = entry.as_map().unwrap();
            let hash = int(&entry["Hash"]).unwrap();
            assert_eq!(hash, i64::from(murmur3(name.as_bytes())));
            assert_eq!(entry[&**field::DEFAULT_VALUE], Byml::Bool(false));
        }
        assert!(add_bools(&mut doc.root, &names[..1]).is_err());
        let back = BymlDoc::parse(&doc.to_binary()).unwrap();
        assert_eq!(count(&back.root), before + 2);
    }

    /// The save's layout sizes, from `MetaData`: progress.sav's table end
    /// and size, then the same for the file of every value.
    fn layout(root: &Byml) -> [i64; 4] {
        let meta = root.as_map().unwrap()[&**field::META_DATA]
            .as_map()
            .unwrap();
        let first = |key: &Name| int(&meta[&**key].as_array().unwrap()[0]).unwrap();
        let one = |key: &Name| int(&meta[&**key]).unwrap();
        [
            first(field::SAVE_DATA_OFFSET_POS),
            first(field::SAVE_DATA_SIZE),
            one(field::ALL_DATA_SAVE_OFFSET),
            one(field::ALL_DATA_SAVE_SIZE),
        ]
    }

    #[test]
    fn added_entries_grow_the_save_table_so_it_ends_before_the_values() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let before = layout(&doc.root);
        let bools = ["FakeBool_00".to_owned(), "FakeBool_01".to_owned()];
        add_bools(&mut doc.root, &bools).unwrap();
        add_ints(&mut doc.root, &["FakeInt_00".to_owned()]).unwrap();
        let back = BymlDoc::parse(&doc.to_binary()).unwrap();
        assert_eq!(layout(&back.root), before.map(|n| n + 3 * SAVE_PAIR));
    }
}
