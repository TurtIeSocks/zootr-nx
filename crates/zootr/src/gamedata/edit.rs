//! Edits to the GameData document a seed writes: new Bools, Ints, keys,
//! BoolExps and Struct members.

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{Byml, BymlMap, murmur3};

use super::*;

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
    grow_saves(root, 0, n, 0)?;
    Ok(n)
}

/// [`add_bools`] for the `Int` table, defaulting to 0. Each copies the first
/// Int kept in the save the way Link's rupees and keys are.
pub fn add_ints(root: &mut Byml, names: &[String]) -> Result<()> {
    add(root, "Int", names, is_kept)
}

/// Append an unsaved `UInt64` for each `(hash, value)`, a copy of the
/// table's first unsaved entry. A save never holds one, so it can't carry
/// one seed's values into another, and the save layout stays as it is.
/// Refuses a hash any table already has.
pub fn add_u64s(root: &mut Byml, values: &[(u32, u64)]) -> Result<()> {
    let data = root.as_map()?.get("Data").context("no Data")?.as_map()?;
    let hashes: HashSet<i64> = (data.iter())
        .filter(|(kind, _)| kind.as_str() != KEYED)
        .filter_map(|(_, list)| list.as_array().ok())
        .flatten()
        .filter_map(|e| int(e.as_map().ok()?.get("Hash")?).ok())
        .collect();
    if let Some((hash, _)) = values.iter().find(|(h, _)| hashes.contains(&i64::from(*h))) {
        bail!("{} already has an entry {hash:#010x}", flag::GAME_DATA);
    }
    let entries = table(root, "UInt64")?;
    let unsaved = |e: &&Byml| {
        let file = e
            .as_map()
            .ok()
            .and_then(|m| m.get(&**field::SAVE_FILE_INDEX));
        file.is_some_and(|f| int(f).ok() == Some(-1))
    };
    let template = (entries.iter().find(unsaved))
        .context("the UInt64 table has no unsaved entry to copy")?
        .as_map()?
        .clone();
    for &(hash, value) in values {
        let mut entry = template.clone();
        entry.insert("Hash".into(), typed(template.get("Hash"), hash)?);
        entry.insert(field::DEFAULT_VALUE.to_string().into(), Byml::U64(value));
        entries.push(Byml::Map(entry));
    }
    Ok(())
}

/// [`add_bools`] for the `String64` table, defaulting to empty. Each copies
/// the first String64 the save keeps, as it keeps a bag's size (the Deku
/// Nut slot's current expansion actor).
pub fn add_string64s(root: &mut Byml, names: &[String]) -> Result<()> {
    add(root, STRING64, names, is_kept)
}

/// The GameData table of 64-byte strings.
const STRING64: &str = "String64";

/// The bytes a save keeps for a String64's text, after its table pair.
const STRING64_TEXT: i64 = 64;

/// False, 0 or empty, typed as `like`.
fn zero(like: Option<&Byml>) -> Option<Byml> {
    Some(match like? {
        Byml::Bool(_) => Byml::Bool(false),
        Byml::I32(_) => Byml::I32(0),
        Byml::U32(_) => Byml::U32(0),
        Byml::I64(_) => Byml::I64(0),
        Byml::U64(_) => Byml::U64(0),
        Byml::String(_) => Byml::String("".into()),
        _ => return None,
    })
}

/// The reset type of a value the save keeps.
const KEPT: i64 = 8;

/// Whether the save keeps entry `e` the way it keeps Link's rupees.
fn is_kept(e: &BymlMap) -> bool {
    e.get(&**field::RESET_TYPE_VALUE)
        .is_some_and(|v| int(v).ok() == Some(KEPT))
}

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
            .with_context(|| format!("a {kind}'s default isn't a Bool, an integer or a string"))?;
        entry.insert(field::DEFAULT_VALUE.to_string().into(), value);
        entries.push(Byml::Map(entry));
    }
    let text = match kind == STRING64 {
        true => STRING64_TEXT,
        false => 0,
    };
    grow_saves(root, file, names.len(), text)
}

/// The bytes one entry takes in a save's table: its hash, then its value
/// or where its value is.
const SAVE_PAIR: i64 = 8;

/// Make room for `n` more entries in save file `file`'s table, each with
/// `text` bytes of value after the table (a String64's text). The game
/// lays a save out by these sizes, so left alone the longer table runs
/// into the values after it and clobbers the first arrays there.
fn grow_saves(root: &mut Byml, file: i64, n: usize, text: i64) -> Result<()> {
    let table = n as i64 * SAVE_PAIR;
    let size = table + n as i64 * text;
    let meta = (root.as_mut_map()?)
        .get_mut(&**field::META_DATA)
        .context(field::META_DATA)?
        .as_mut_map()?;
    for (key, by) in [
        (field::ALL_DATA_SAVE_OFFSET, table),
        (field::ALL_DATA_SAVE_SIZE, size),
    ] {
        bump(meta.get_mut(&**key).context(key)?, by)?;
    }
    let Ok(file) = usize::try_from(file) else {
        return Ok(());
    };
    for (key, by) in [
        (field::SAVE_DATA_OFFSET_POS, table),
        (field::SAVE_DATA_SIZE, size),
    ] {
        let sizes = meta.get_mut(&**key).context(key)?.as_mut_array()?;
        bump(sizes.get_mut(file).context(key)?, by)?;
    }
    Ok(())
}

/// Add `by` to integer `v`, keeping its kind.
fn bump(v: &mut Byml, by: i64) -> Result<()> {
    *v = like(v, int(v)? + by)?;
    Ok(())
}

/// One instruction of a BoolExp program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Push a Bool, by hash.
    Bool(u32),
    /// Push another BoolExp, by hash.
    Exp(u32),
    Not,
    And,
    Or,
}

impl Op {
    /// Push `name`'s entry, a Bool or a BoolExp in `gd`.
    pub fn read(gd: &GameData, name: &str) -> Result<Self> {
        let hash = murmur3(name.as_bytes());
        match gd.kind(hash) {
            Some("Bool") => Ok(Self::Bool(hash)),
            Some(k) if k == BOOL_EXP.get() => Ok(Self::Exp(hash)),
            other => bail!("{name} is {other:?}, not a Bool or a {}", flag::BOOL_EXP),
        }
    }
}

/// Append a BoolExp named each of `exps`, running its program. A Presence
/// names one flag, so this is how one reads a Bool whose name isn't known
/// (a chest's opened flag), or two Bools at once.
pub fn add_exps(root: &mut Byml, exps: &[(String, Vec<Op>)]) -> Result<()> {
    let table = table(root, BOOL_EXP.get())?;
    let first = table
        .first()
        .with_context(|| format!("the {} table is empty", flag::BOOL_EXP))?
        .as_map()?
        .clone();
    // An instruction pushing an entry, for its integer types.
    let template = (table.iter())
        .filter_map(|e| e.as_map().ok()?.get("Values")?.as_array().ok())
        .flatten()
        .filter_map(|i| i.as_array().ok())
        .find(|i| i.len() == 2)
        .context("no BoolExp reads an entry")?
        .to_vec();
    let reads = |code: i64, of: u32| -> Result<Byml> {
        let arg = typed(template.get(1), of)?;
        Ok(Byml::Array(vec![like(&template[0], code)?, arg]))
    };
    for (name, program) in exps {
        let hash = murmur3(name.as_bytes());
        let taken = table.iter().any(|e| has_hash(e, hash));
        ensure!(!taken, "{} already has {name}", flag::GAME_DATA);
        let values = program.iter().map(|op| match *op {
            Op::Bool(of) => reads(op::BOOL, of),
            Op::Exp(of) => reads(op::BOOL_EXP, of),
            Op::Not => instruction(&template, op::NOT, None),
            Op::And => instruction(&template, op::AND, None),
            Op::Or => instruction(&template, op::OR, None),
        });
        let mut entry = first.clone();
        entry.insert("Hash".into(), typed(first.get("Hash"), hash)?);
        entry.insert("Values".into(), Byml::Array(values.collect::<Result<_>>()?));
        table.push(Byml::Map(entry));
    }
    Ok(())
}

/// Make each of `names` a member of every Struct `like` is a member of
/// (`IsGet` and others), each member's value a new false Bool. The game
/// finds an item's entries through these member lists, and an item with
/// none can't be taken from the floor. A value is named `S.NAME` after
/// the one `S.LIKE` names `like`'s, for each `S` in `prefixes`, else after
/// the Struct's hash. Only Bool values are copied: a Struct whose value
/// for `like` is a count (the bomb bags are in two) is left alone.
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
        if !bools.contains(&value) {
            continue;
        }
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

/// The Struct of pouch counts, a member per item naming a Struct of its
/// saved count. The game's pouch count reads it, and the hook asks that
/// count whether Link holds each level of a line: an item with none reads
/// as never held, so every Bombchu Bag chest gave the Small.
pub const POUCH_COUNTS: u32 = 0x3da094e3;

/// An item of our own, `name`, saved as `like` is: its `IsGet` and the
/// rest of `like`'s Bool entries, and its pouch count.
pub fn copy_item(root: &mut Byml, like: &str, name: &str) -> Result<()> {
    join_structs(root, like, &[name.to_owned()], &[IS_GET])?;
    copy_member(root, POUCH_COUNTS, like, name, &format!("{name}.Count"))
}

/// Give Struct `of` a member `name` like its member `like`: the member's
/// value a copy of `like`'s entry, named `value`, kept in the save the same
/// way. The Struct of ammo caps holds one saved Int per pouch slot, and a
/// slot without one keeps no cap.
pub fn copy_member(root: &mut Byml, of: u32, like: &str, name: &str, value: &str) -> Result<()> {
    let members = table(root, "Struct")?
        .iter_mut()
        .find(|s| has_hash(s, of))
        .with_context(|| format!("no Struct {of:#010x}"))?
        .as_mut_map()?
        .get_mut(field::DEFAULT_VALUE.get())
        .context("a Struct with no members")?
        .as_mut_array()?;
    let mut member = (members.iter())
        .find(|m| has_hash(m, murmur3(like.as_bytes())))
        .with_context(|| format!("{like} isn't in Struct {of:#010x}"))?
        .as_map()?
        .clone();
    let was = int(&member["Value"])? as u32;
    member.insert(
        "Hash".into(),
        typed(member.get("Hash"), murmur3(name.as_bytes()))?,
    );
    let hash = murmur3(value.as_bytes());
    member.insert("Value".into(), typed(member.get("Value"), hash)?);
    members.push(Byml::Map(member));
    copy_entry(root, was, value)
}

/// Copy entry `was` as `name`. A Struct's copy points at copies of its
/// members' entries, each named `name.` and its member's hash, so no two
/// items share a count. Only a saved entry takes room in the save.
fn copy_entry(root: &mut Byml, was: u32, name: &str) -> Result<()> {
    let hash = murmur3(name.as_bytes());
    let data = root
        .as_mut_map()?
        .get_mut("Data")
        .context("no Data")?
        .as_mut_map()?;
    let (kind, entries) = (data.iter_mut())
        .filter(|(kind, _)| kind.as_str() != KEYED)
        .find_map(|(kind, list)| {
            let list = list.as_mut_array().ok()?;
            list.iter()
                .any(|e| has_hash(e, was))
                .then(|| (kind.to_string(), list))
        })
        .with_context(|| format!("no entry {was:#010x}"))?;
    let mut entry = (entries.iter().find(|e| has_hash(e, was)))
        .context("the entry went missing")?
        .as_map()?
        .clone();
    ensure!(
        !entries.iter().any(|e| has_hash(e, hash)),
        "{} already has {name}",
        flag::GAME_DATA
    );
    entry.insert("Hash".into(), typed(entry.get("Hash"), hash)?);
    let mut inner = Vec::new();
    if kind == "Struct" {
        let members = (entry.get_mut(field::DEFAULT_VALUE.get()))
            .context("a Struct with no members")?
            .as_mut_array()?;
        for m in members.iter_mut() {
            let m = m.as_mut_map()?;
            let key = int(&m["Hash"])? as u32;
            let child = format!("{name}.{key:08x}");
            inner.push((int(&m["Value"])? as u32, child.clone()));
            m.insert(
                "Value".into(),
                typed(m.get("Value"), murmur3(child.as_bytes()))?,
            );
        }
    }
    let file = int(entry
        .get(&**field::SAVE_FILE_INDEX)
        .context(field::SAVE_FILE_INDEX)?)?;
    entries.push(Byml::Map(entry));
    if file >= 0 {
        let text = if kind == STRING64 { STRING64_TEXT } else { 0 };
        grow_saves(root, file, 1, text)?;
    }
    for (was, child) in inner {
        copy_entry(root, was, &child)?;
    }
    Ok(())
}

/// BoolExp opcodes, as [`GameData::render`] reads them.
pub(crate) mod op {
    pub const BOOL: i64 = 0;
    pub const BOOL_EXP: i64 = 10;
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
    use crate::known::field;
    use ounce_formats::BymlDoc;

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
    fn unsaved_u64s_keep_their_values_and_the_save_layout() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let meta = |root: &Byml| root.as_map().unwrap()[&**field::META_DATA].clone();
        let was = meta(&doc.root);
        let values = [(0xfa4e_0001, u64::MAX), (0xfa4e_0002, 7)];
        add_u64s(&mut doc.root, &values).unwrap();
        assert_eq!(meta(&doc.root), was);
        let entries = table(&mut doc.root, "UInt64").unwrap();
        for (hash, value) in values {
            let e = entries
                .iter()
                .find(|e| has_hash(e, hash))
                .unwrap()
                .as_map()
                .unwrap();
            assert_eq!(e[&**field::DEFAULT_VALUE], Byml::U64(value));
            assert_eq!(int(&e[&**field::SAVE_FILE_INDEX]).unwrap(), -1);
        }
        assert!(
            add_u64s(&mut doc.root, &values[..1]).is_err(),
            "added twice"
        );
    }

    #[test]
    fn an_exp_reads_its_bools() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let read = |name: &str| (name.to_string(), murmur3(name.as_bytes()));
        let (opened, shut) = (read("ZootrFakeOpened"), read("ZootrFakeShut"));
        let alias = ("ZootrOpened.1".to_string(), opened.1);
        let both = ("ZootrBoth.1".to_string(), shut.1);
        let exps = [
            (alias.0.clone(), vec![Op::Bool(alias.1)]),
            (
                both.0.clone(),
                vec![Op::Bool(alias.1), Op::Bool(both.1), Op::Not, Op::And],
            ),
        ];
        add_exps(&mut doc.root, &exps).unwrap();
        let gd = GameData::parse(&doc.to_binary()).unwrap();
        assert_eq!(gd.expr(&alias.0).unwrap(), Some(gd.name(alias.1)));
        assert!(gd.implied_by(&alias.0, &opened.0).unwrap());
        let (a, b) = (gd.name(alias.1), gd.name(both.1));
        assert_eq!(gd.expr(&both.0).unwrap(), Some(format!("{a} && !{b}")));
        // Only an `||` chain is implied by one of its terms, never an `&&`.
        assert!(!gd.implied_by(&both.0, &opened.0).unwrap());
        assert!(!gd.implied_by(&both.0, &shut.0).unwrap());
        assert!(add_exps(&mut doc.root, &exps[..1]).is_err(), "added twice");
    }

    #[test]
    fn an_exp_reads_other_exps_and_ors() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let (bool_name, exp_name) = (
            flag::IS_GET_ITEM_DIN_FIRE.get(),
            flag::SAGE_OF_FOREST_IS_COMPLETED_EXP.get(),
        );
        let program = vec![
            Op::read(&gd, bool_name).unwrap(),
            Op::read(&gd, exp_name).unwrap(),
            Op::Not,
            Op::Or,
        ];
        assert_eq!(program[1], Op::Exp(murmur3(exp_name.as_bytes())));
        let name = "ZootrFake.Exp".to_string();
        add_exps(&mut doc.root, &[(name.clone(), program)]).unwrap();
        let mut ours = GameData::parse(&doc.to_binary()).unwrap();
        ours.learn(bool_name);
        ours.learn(exp_name);
        assert_eq!(
            ours.expr(&name).unwrap(),
            Some(format!("{bool_name} || !{exp_name}"))
        );
    }

    #[test]
    fn a_copied_member_has_a_saved_entry_of_its_own_like_its_likes() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let caps = 0xf80f8077;
        let before = GameData::parse(&doc.to_binary()).unwrap();
        let size = |root: &Byml| {
            int(&root.as_map().unwrap()[field::META_DATA.get()]
                .as_map()
                .unwrap()[field::ALL_DATA_SAVE_SIZE.get()])
            .unwrap()
        };
        let was = size(&doc.root);
        copy_member(&mut doc.root, caps, "DekuNut", "FakeAmmo", "FakeAmmo.Cap").unwrap();
        assert_eq!(size(&doc.root), was + SAVE_PAIR);
        let after = GameData::parse(&doc.to_binary()).unwrap();
        let members: HashMap<u32, u32> = after.members(caps).unwrap().into_iter().collect();
        let nut = members[&murmur3(b"DekuNut")];
        assert_eq!(members[&murmur3(b"FakeAmmo")], murmur3(b"FakeAmmo.Cap"));
        let info = |gd: &GameData, hash: u32| gd.entry(hash).unwrap().as_map().unwrap().clone();
        let (ours, theirs) = (info(&after, murmur3(b"FakeAmmo.Cap")), info(&after, nut));
        for key in [field::SAVE_FILE_INDEX.get(), field::RESET_TYPE_VALUE.get()] {
            assert_eq!(ours[key], theirs[key]);
        }
        assert_eq!(after.kind(murmur3(b"FakeAmmo.Cap")), before.kind(nut));
    }

    #[test]
    fn a_copied_struct_member_points_at_saved_entries_of_its_own() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let (counts, like) = (POUCH_COUNTS, crate::known::item::BOMB_BAG_SMALL.get());
        let size = |root: &Byml| {
            int(&root.as_map().unwrap()[field::META_DATA.get()]
                .as_map()
                .unwrap()[field::ALL_DATA_SAVE_SIZE.get()])
            .unwrap()
        };
        let was = size(&doc.root);
        copy_member(&mut doc.root, counts, like, "FakeBag", "FakeBag.Count").unwrap();
        // The Struct isn't saved, the Int it names is.
        assert_eq!(size(&doc.root), was + SAVE_PAIR);
        let gd = GameData::parse(&doc.to_binary()).unwrap();
        let of = |name: &[u8]| {
            let members: HashMap<u32, u32> = gd.members(counts).unwrap().into_iter().collect();
            let inner = members[&murmur3(name)];
            assert_eq!(gd.kind(inner), Some("Struct"));
            gd.members(inner).unwrap()
        };
        let (ours, theirs) = (of(b"FakeBag"), of(like.as_bytes()));
        assert_eq!(ours.len(), theirs.len());
        for ((key, mine), (their_key, its)) in ours.into_iter().zip(theirs) {
            assert_eq!(key, their_key);
            assert_ne!(mine, its);
            assert_eq!(gd.kind(mine), gd.kind(its));
            let info = |hash| gd.entry(hash).unwrap().as_map().unwrap().clone();
            assert_eq!(
                info(mine)[field::SAVE_FILE_INDEX.get()],
                info(its)[field::SAVE_FILE_INDEX.get()]
            );
        }
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

    #[test]
    fn a_string_grows_the_table_by_its_pair_and_the_save_by_its_text_too() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut doc = BymlDoc::parse(&romfs.read(PATH).unwrap()).unwrap();
        let [table, size, all_table, all_size] = layout(&doc.root);
        add_string64s(&mut doc.root, &["FakeString_00".to_owned()]).unwrap();
        let back = BymlDoc::parse(&doc.to_binary()).unwrap();
        let (pair, whole) = (SAVE_PAIR, SAVE_PAIR + STRING64_TEXT);
        let grown = [
            table + pair,
            size + whole,
            all_table + pair,
            all_size + whole,
        ];
        assert_eq!(layout(&back.root), grown);
        let gd = GameData::parse(&back.to_binary()).unwrap();
        assert_eq!(gd.kind(murmur3(b"FakeString_00")), Some(STRING64));
    }
}
