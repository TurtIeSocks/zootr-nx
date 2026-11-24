//! XLink2 files (`.bslnk` sounds, `.belnk` effects), version 0x22 with
//! 64-bit offsets. Each user (an actor's sound or effect set) holds calls
//! the game plays by name: an asset, or a container whose children are
//! calls too. [`copy_call`] gives one user a call another has, and
//! [`add_case`] a case another's switch has.
//!
//! Every offset a call or container holds is relative to a table the whole
//! file shares (asset params, conditions, property names), or to the
//! user's own tables, so a call tree copies as its entries and container
//! params with the indexes renumbered.

use anyhow::{Context, Result, bail, ensure};

use crate::crc32;
use crate::reader::Reader;

const VERSION: u32 = 0x22;
const DATA_SIZE: usize = 0x04;
const VERSION_AT: usize = 0x08;
const NUM_USER: usize = 0x48;
const CONDITIONS: usize = 0x50;
const NAMES: usize = 0x58;
const USER_HASHES: usize = 0x60;
/// The local property name refs (u64 each): first the names, then the
/// enum names, then the direct values.
const LOCAL_REFS: usize = 0x20;
const NUM_LOCAL_NAMES: usize = 0x28;
const NUM_LOCAL_ENUMS: usize = 0x2c;
const NUM_DIRECT: usize = 0x30;
const LOCAL_REF: usize = 0x08;

// A user's header.
const LOCALS: usize = 0x04;
const NUM_CALL: usize = 0x08;
const NUM_ASSET: usize = 0x0c;
/// Containers of [`RANDOM`] type.
const NUM_RANDOM: usize = 0x10;
/// How many of each trigger table's entries there are, in their order.
const TRIGGER_COUNTS: usize = 0x14;
const TRIGGERS: usize = 0x30;
const USER_HEADER: usize = 0x38;
/// A local property: its name's offset, then four bytes.
const LOCAL: usize = 0x08;

// A user's trigger tables, in order: action slots, actions, action
// triggers, properties, property triggers, always triggers. Each entry's
// size, and where in it a trigger keeps its call, as a byte offset into
// the user's calls.
const TRIGGER_TABLES: [usize; 6] = [0x10, 0x10, 0x28, 0x18, 0x20, 0];
const TRIGGER_CALL: usize = 0x08;
const ACTION_TRIGGERS: usize = 2;
const PROPERTY_TRIGGERS: usize = 4;
const ALWAYS_TRIGGERS: usize = 5;

// A call.
const CALL: usize = 0x30;
const CALL_NAME: usize = 0x00;
const ASSET: usize = 0x08;
const CALL_FLAGS: usize = 0x0a;
const PARENT: usize = 0x10;
const KEY: usize = 0x18;
const PARAM: usize = 0x20;
/// Where in the file's conditions the call's is: what picks it in its
/// container.
const CONDITION: usize = 0x28;
const IS_CONTAINER: u16 = 1;

// A container.
const SWITCH: u8 = 0;
const RANDOM: u8 = 2;
const BLEND: u8 = 3;
const BY_PROPERTY: usize = 0x01;
const FIRST: usize = 0x04;
const LAST: usize = 0x08;
/// The property a switch or blend reads: its name's offset (8 bytes),
/// 4 more, then its index among the file's local property names, or
/// [`GLOBAL`] for a global one.
const PROPERTY: usize = 0x10;
const LOCAL_INDEX: usize = 0x1c;
const GLOBAL: u16 = 0xffff;

/// A user's tables, as offsets into the file.
struct User {
    base: usize,
    calls: usize,
    count: usize,
    ids: usize,
    containers: usize,
    triggers: usize,
}

/// `file` with the call `key` of user `from`, and every call under it,
/// copied into user `to` and named `name` there. `name` must already be in
/// the file's names, and `to` must not have it.
pub fn copy_call(file: &[u8], from: &str, key: &str, to: &str, name: &str) -> Result<Vec<u8>> {
    let (a, b) = (user(file, from)?, user(file, to)?);
    ensure!(find(file, &b, name)?.is_none(), "{to} already has {name}");
    let root = find(file, &a, key)?.with_context(|| format!("{from} has no {key}"))?;
    let named = name_offset(file, name)? as u64;
    let condition = Reader(file).u64(a.calls + root * CALL + CONDITION)?;
    let place = Place {
        at: b.count,
        parent: -1,
        name: named,
        key: crc32(name.as_bytes()),
        condition,
    };
    graft(file, &a, root, &b, place)
}

/// `file` with `case`, a case of a switch of user `from`, added as the
/// last case of `to`'s switch of the same name, playing the tree of
/// `from`'s call `body`. The new case takes `case`'s name and condition,
/// so `body` may be a call under it, picked out of a switch `to` can't
/// read. Both switches must read the same property, and every switch in
/// the tree one `to` has: `to` must not already have `case`.
pub fn add_case(file: &[u8], from: &str, case: &str, body: &str, to: &str) -> Result<Vec<u8>> {
    let r = Reader(file);
    let (a, b) = (user(file, from)?, user(file, to)?);
    ensure!(find(file, &b, case)?.is_none(), "{to} already has {case}");
    let at = find(file, &a, case)?.with_context(|| format!("{from} has no {case}"))?;
    let root = find(file, &a, body)?.with_context(|| format!("{from} has no {body}"))?;
    let parent = usize::try_from(r.i32(a.calls + at * CALL + PARENT)?)
        .with_context(|| format!("{case} is no switch's case"))?;
    let switch = std::str::from_utf8(call_name(file, a.calls + parent * CALL)?)?;
    let theirs = switch_param(file, &a, parent)?;
    let own = find(file, &b, switch)?.with_context(|| format!("{to} has no {switch}"))?;
    let mine = switch_param(file, &b, own)?;
    ensure!(
        r.bytes(theirs + PROPERTY, 0x10)? == r.bytes(mine + PROPERTY, 0x10)?,
        "{from}'s and {to}'s {switch} read different properties"
    );
    let locals = local_names(file, &b)?;
    for i in tree(file, &a, root)? {
        if let Some(name) = local_property(file, &a, i)? {
            ensure!(
                locals.contains(&name),
                "{body} reads {}, which {to} lacks",
                String::from_utf8_lossy(name)
            );
        }
    }
    let from_case = a.calls + at * CALL;
    let place = Place {
        at: r.i32(mine + LAST)? as usize + 1,
        parent: own as i32,
        name: r.u64(from_case + CALL_NAME)?,
        key: r.u32(from_case + KEY)?,
        condition: r.u64(from_case + CONDITION)?,
    };
    graft(file, &a, root, &b, place)
}

/// Where a grafted tree's root goes in its new user, and what it's called
/// there: its index, its parent's (-1 for none), its name's offset, its
/// key and its condition. Its parent, if any, is a switch it becomes the
/// last case of.
struct Place {
    at: usize,
    parent: i32,
    name: u64,
    key: u32,
    condition: u64,
}

/// `file` with the call `root` of user `a`, and every call under it,
/// copied into user `b`: the root at `place`, the rest after `b`'s calls.
/// Every index `b` keeps of a call from the root's place on moves up one:
/// parents, children, its calls by name, and its triggers' calls.
fn graft(file: &[u8], a: &User, root: usize, b: &User, place: Place) -> Result<Vec<u8>> {
    let r = Reader(file);
    ensure!(r.bytes(0, 4)? == b"XLNK", "not an XLink2 file");
    let version = r.u32(VERSION_AT)?;
    ensure!(
        version == VERSION,
        "XLink2 version {version:#x}, not {VERSION:#x}"
    );
    let insert_at = place.at;
    ensure!(insert_at <= b.count, "a place past {}'s calls", b.count);
    let order = tree(file, a, root)?;
    let (own, own_params) = renumber_own(file, b, &place)?;
    let tree = copy_tree(file, a, b, &order, &place)?;

    let mut calls = own[..insert_at * CALL].to_vec();
    calls.extend(&tree.root);
    calls.extend(&own[insert_at * CALL..]);
    calls.extend(&tree.rest);
    // Every shipped user numbers its assets in call order: kept so.
    let mut assets = 0u32;
    for call in calls.as_chunks_mut::<CALL>().0 {
        if u16::from_le_bytes([call[CALL_FLAGS], call[CALL_FLAGS + 1]]) & IS_CONTAINER == 0 {
            put(call, ASSET, &(assets as i16).to_le_bytes());
            assets += 1;
        }
    }
    let ids = sorted_ids(file, b, &calls, insert_at)?;

    let mut out = Vec::with_capacity(file.len() + ids.len() + calls.len() + tree.params.len());
    out.extend(&file[..b.ids]);
    out.extend(&ids);
    out.extend(&calls);
    out.extend(&own_params);
    out.extend(&tree.params);
    out.extend(&file[b.triggers..]);
    let counts = Counts {
        calls: calls.len() / CALL,
        assets,
        randoms: tree.randoms,
    };
    patch_header(file, b, &mut out, insert_at, counts)?;
    Ok(out)
}

/// `i`, the index of a call, once a call is put at `insert_at`.
fn shift(i: usize, insert_at: usize) -> usize {
    if i >= insert_at { i + 1 } else { i }
}

/// `b`'s own calls and container params, with every index of a call from
/// `place` on moved up one, and `place`'s switch, if any, ending at it.
fn renumber_own(file: &[u8], b: &User, place: &Place) -> Result<(Vec<u8>, Vec<u8>)> {
    let r = Reader(file);
    let insert_at = place.at;
    let mut own = r.bytes(b.calls, b.count * CALL)?.to_vec();
    let mut own_params = r.bytes(b.containers, b.triggers - b.containers)?.to_vec();
    let mut renumbered = std::collections::HashSet::new();
    for i in 0..b.count {
        let call = i * CALL;
        let parent = i32_in(&own, call + PARENT);
        if parent >= 0 {
            let parent = shift(parent as usize, insert_at) as i32;
            put(&mut own, call + PARENT, &parent.to_le_bytes());
        }
        if u16::from_le_bytes([own[call + CALL_FLAGS], own[call + CALL_FLAGS + 1]]) & IS_CONTAINER
            == 0
        {
            continue;
        }
        let param = u64::from_le_bytes(own[call + PARAM..call + PARAM + 8].try_into()?) as usize;
        if !renumbered.insert(param) {
            continue;
        }
        for field in [FIRST, LAST] {
            let child = shift(i32_in(&own_params, param + field) as usize, insert_at) as i32;
            put(&mut own_params, param + field, &child.to_le_bytes());
        }
        if place.parent == i as i32 {
            ensure!(
                i32_in(&own_params, param + LAST) as usize + 1 == insert_at,
                "a case not after its switch's last"
            );
            put(
                &mut own_params,
                param + LAST,
                &(insert_at as i32).to_le_bytes(),
            );
        }
    }
    Ok((own, own_params))
}

/// A call tree copied for its new user: the root's call, the rest's
/// calls, their container params, and how many of those are random.
struct Copied {
    root: Vec<u8>,
    rest: Vec<u8>,
    params: Vec<u8>,
    randoms: u32,
}

/// The calls `order` of user `a`, a tree, copied for user `b`: the root
/// named and placed at `place`, the rest after `b`'s calls, their params
/// after `b`'s.
fn copy_tree(file: &[u8], a: &User, b: &User, order: &[usize], place: &Place) -> Result<Copied> {
    let r = Reader(file);
    let moved = |old: usize| match order.iter().position(|&o| o == old).expect("in the tree") {
        0 => place.at,
        k => b.count + k,
    };
    let mut copied = Copied {
        root: Vec::new(),
        rest: Vec::new(),
        params: Vec::new(),
        randoms: 0,
    };
    let param_base = b.triggers - b.containers;
    for (k, &old) in order.iter().enumerate() {
        let from_call = a.calls + old * CALL;
        let mut call = r.bytes(from_call, CALL)?.to_vec();
        let parent = match k {
            0 => {
                put(&mut call, CALL_NAME, &place.name.to_le_bytes());
                put(&mut call, KEY, &place.key.to_le_bytes());
                put(&mut call, CONDITION, &place.condition.to_le_bytes());
                place.parent
            }
            _ => moved(r.i32(from_call + PARENT)? as usize) as i32,
        };
        put(&mut call, PARENT, &parent.to_le_bytes());
        if r.u16(from_call + CALL_FLAGS)? & IS_CONTAINER != 0 {
            let param_at = container_at(file, a, from_call)?;
            let mut param = r.bytes(param_at, container_size(file, param_at)?)?.to_vec();
            for field in [FIRST, LAST] {
                let child = moved(r.i32(param_at + field)? as usize) as i32;
                put(&mut param, field, &child.to_le_bytes());
            }
            copied.randoms += u32::from(param[0] == RANDOM);
            let offset = (param_base + copied.params.len()) as u64;
            put(&mut call, PARAM, &offset.to_le_bytes());
            copied.params.extend(param);
        }
        match k {
            0 => copied.root = call,
            _ => copied.rest.extend(call),
        }
    }
    Ok(copied)
}

/// The new `calls` of user `b` by name, which the game searches by halves,
/// padded to four bytes. The shipped order is kept, ties too, and each new
/// call goes after its equals.
fn sorted_ids(file: &[u8], b: &User, calls: &[u8], insert_at: usize) -> Result<Vec<u8>> {
    let r = Reader(file);
    let total = calls.len() / CALL;
    let names = (0..total)
        .map(|i| {
            name_at(
                file,
                u64::from_le_bytes(calls[i * CALL..][..8].try_into()?) as usize,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut sorted = (0..b.count)
        .map(|k| Ok(shift(r.u16(b.ids + 2 * k)? as usize, insert_at) as u16))
        .collect::<Result<Vec<_>>>()?;
    for new in std::iter::once(insert_at).chain(b.count + 1..total) {
        let place = sorted.partition_point(|&i| names[i as usize] <= names[new]);
        sorted.insert(place, new as u16);
    }
    let mut ids: Vec<u8> = sorted.iter().flat_map(|i| i.to_le_bytes()).collect();
    ids.resize(align4(total * 2), 0);
    Ok(ids)
}

/// Counts written into user `b`'s header after a graft.
struct Counts {
    calls: usize,
    assets: u32,
    randoms: u32,
}

/// Bring `out`, `file` with user `b` grown, up to date: `b`'s counts and
/// trigger offset, its triggers' calls from `insert_at` on, every offset
/// past `b`, and the file's size.
fn patch_header(
    file: &[u8],
    b: &User,
    out: &mut Vec<u8>,
    insert_at: usize,
    counts: Counts,
) -> Result<()> {
    let r = Reader(file);
    let grown = out.len() - file.len();
    let w = |out: &mut Vec<u8>, at: usize, v: u32| put(out, at, &v.to_le_bytes());
    w(out, b.base + NUM_CALL, counts.calls as u32);
    w(out, b.base + NUM_ASSET, counts.assets);
    let randoms = r.u32(b.base + NUM_RANDOM)? + counts.randoms;
    w(out, b.base + NUM_RANDOM, randoms);
    let triggers = r.u64(b.base + TRIGGERS)? + grown as u64;
    put(out, b.base + TRIGGERS, &triggers.to_le_bytes());
    if insert_at < b.count {
        for pos in trigger_calls(file, b)? {
            let call = shift(trigger_call(file, b, pos)?, insert_at);
            put(out, pos + grown, &((call * CALL) as u64).to_le_bytes());
        }
    }
    for header_at in user_offsets(file)?.chain([CONDITIONS, NAMES]) {
        let offset = r.u64(header_at)? as usize;
        if offset > b.base {
            put(out, header_at, &((offset + grown) as u64).to_le_bytes());
        }
    }
    let size = out.len() as u32;
    w(out, DATA_SIZE, size);
    Ok(())
}

/// A user's call, as [`calls`] reads it.
pub struct Call {
    /// Where it is in the file.
    pub at: usize,
    /// Where its params are: an asset's record from [`asset_params`] on, a
    /// container's from its user's container params on.
    pub param: u32,
    /// Whether it's a container, whose children [`call_children`] gives.
    pub container: bool,
}

/// `user`'s calls, in their order.
pub fn calls(file: &[u8], user_name: &str) -> Result<Vec<Call>> {
    let (r, u) = (Reader(file), user(file, user_name)?);
    (0..u.count)
        .map(|i| {
            let at = u.calls + i * CALL;
            Ok(Call {
                at,
                param: r.u32(at + PARAM)?,
                container: r.u16(at + CALL_FLAGS)? & IS_CONTAINER != 0,
            })
        })
        .collect()
}

/// The calls right under `user`'s call `i`, by index.
pub fn call_children(file: &[u8], user_name: &str, i: usize) -> Result<std::ops::Range<usize>> {
    children(file, &user(file, user_name)?, i)
}

/// Where the asset param records start, in the file: after the param
/// definitions, which give their own size.
pub fn asset_params(file: &[u8]) -> Result<usize> {
    let r = Reader(file);
    let defines = offsets_at(file)? + 8 * r.u32(NUM_USER)? as usize;
    Ok(defines + r.offset(defines)?)
}

/// The direct values, as raw words: what an asset param's field indexes,
/// a float for a size. They follow the local property names and enum
/// names.
pub fn direct_values(file: &[u8]) -> Result<Vec<u32>> {
    let r = Reader(file);
    let refs = r.offset(NUM_LOCAL_NAMES)? + r.offset(NUM_LOCAL_ENUMS)?;
    let at = r.ptr(LOCAL_REFS)? + LOCAL_REF * refs;
    (0..r.offset(NUM_DIRECT)?)
        .map(|i| r.u32(at + 4 * i))
        .collect()
}

/// The names of `user`'s calls, in their order.
pub fn call_names<'a>(file: &'a [u8], user_name: &str) -> Result<Vec<&'a str>> {
    let u = user(file, user_name)?;
    (0..u.count)
        .map(|i| Ok(std::str::from_utf8(call_name(file, u.calls + i * CALL)?)?))
        .collect()
}

/// The names of the call `key` of `user` and every call under it, each
/// container's children together after it.
pub fn tree_names<'a>(file: &'a [u8], user_name: &str, key: &str) -> Result<Vec<&'a str>> {
    let u = user(file, user_name)?;
    let root = find(file, &u, key)?.with_context(|| format!("{user_name} has no {key}"))?;
    (tree(file, &u, root)?.into_iter())
        .map(|i| Ok(std::str::from_utf8(call_name(file, u.calls + i * CALL)?)?))
        .collect()
}

/// The name of the call each of `user`'s action and property triggers
/// plays, in their order.
pub fn trigger_names<'a>(file: &'a [u8], user_name: &str) -> Result<Vec<&'a str>> {
    let u = user(file, user_name)?;
    (trigger_calls(file, &u)?.into_iter())
        .map(|at| {
            let call = trigger_call(file, &u, at)?;
            Ok(std::str::from_utf8(call_name(
                file,
                u.calls + call * CALL,
            )?)?)
        })
        .collect()
}

/// The names of the calls right under `user`'s container `key`, in order.
pub fn child_names<'a>(file: &'a [u8], user_name: &str, key: &str) -> Result<Vec<&'a str>> {
    let u = user(file, user_name)?;
    let at = find(file, &u, key)?.with_context(|| format!("{user_name} has no {key}"))?;
    (children(file, &u, at)?)
        .map(|i| Ok(std::str::from_utf8(call_name(file, u.calls + i * CALL)?)?))
        .collect()
}

/// The call `root` of `user` and every call under it, by index, each
/// container's children together after it.
fn tree(file: &[u8], user: &User, root: usize) -> Result<Vec<usize>> {
    let mut order = vec![root];
    let mut i = 0;
    while i < order.len() {
        order.extend(children(file, user, order[i])?);
        i += 1;
    }
    Ok(order)
}

/// The calls right under `user`'s call `i`: none unless it's a container.
fn children(file: &[u8], user: &User, i: usize) -> Result<std::ops::Range<usize>> {
    let r = Reader(file);
    let call = user.calls + i * CALL;
    if r.u16(call + CALL_FLAGS)? & IS_CONTAINER == 0 {
        return Ok(0..0);
    }
    let at = container_at(file, user, call)?;
    let (first, last) = (r.i32(at + FIRST)?, r.i32(at + LAST)?);
    ensure!(
        0 <= first && first <= last && (last as usize) < user.count,
        "a container's children past its user's calls"
    );
    Ok(first as usize..last as usize + 1)
}

fn user(file: &[u8], name: &str) -> Result<User> {
    let r = Reader(file);
    let count = r.u32(NUM_USER)? as usize;
    let hash = crc32(name.as_bytes());
    let hashes = (0..count).map(|i| r.u32(USER_HASHES + 4 * i));
    let index = (hashes.collect::<Result<Vec<_>>>()?.binary_search(&hash))
        .ok()
        .with_context(|| format!("no XLink2 user {name}"))?;
    let base = r.u64(offsets_at(file)? + 8 * index)? as usize;
    let defines = offsets_at(file)? + 8 * count;
    let user_params = r.u32(defines + 4)? as usize;
    let locals = r.u16(base + LOCALS)? as usize;
    let calls_count = r.u32(base + NUM_CALL)? as usize;
    let ids = base + USER_HEADER + 8 * locals + 4 * user_params;
    let calls = align4(ids + 2 * calls_count);
    Ok(User {
        base,
        calls,
        count: calls_count,
        ids,
        containers: calls + CALL * calls_count,
        triggers: base + r.u64(base + TRIGGERS)? as usize,
    })
}

/// Where each of `user`'s triggers keeps its call, in the file. Always
/// triggers aren't read yet, so a user with any is refused.
fn trigger_calls(file: &[u8], user: &User) -> Result<Vec<usize>> {
    let r = Reader(file);
    let counts = (0..TRIGGER_TABLES.len())
        .map(|t| Ok(r.u32(user.base + TRIGGER_COUNTS + 4 * t)? as usize))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        counts[ALWAYS_TRIGGERS] == 0,
        "always triggers aren't read yet"
    );
    let mut at = user.triggers;
    let mut calls = Vec::new();
    for (t, (&size, &count)) in TRIGGER_TABLES.iter().zip(&counts).enumerate() {
        if t == ACTION_TRIGGERS || t == PROPERTY_TRIGGERS {
            calls.extend((0..count).map(|k| at + k * size + TRIGGER_CALL));
        }
        at += size * count;
    }
    Ok(calls)
}

/// The index of the call a trigger of `user` keeps at `at`.
fn trigger_call(file: &[u8], user: &User, at: usize) -> Result<usize> {
    let call = Reader(file).u64(at)? as usize;
    ensure!(
        call.is_multiple_of(CALL) && call / CALL < user.count,
        "a trigger's call past its user's"
    );
    Ok(call / CALL)
}

/// The params of `user`'s call `i`, a switch.
fn switch_param(file: &[u8], user: &User, i: usize) -> Result<usize> {
    let r = Reader(file);
    let call = user.calls + i * CALL;
    ensure!(
        r.u16(call + CALL_FLAGS)? & IS_CONTAINER != 0,
        "not a switch"
    );
    let at = container_at(file, user, call)?;
    ensure!(r.u8(at)? == SWITCH, "not a switch");
    Ok(at)
}

/// The names of `user`'s local properties.
fn local_names<'a>(file: &'a [u8], user: &User) -> Result<Vec<&'a [u8]>> {
    let r = Reader(file);
    (0..r.u16(user.base + LOCALS)? as usize)
        .map(|k| name_at(file, r.u32(user.base + USER_HEADER + LOCAL * k)? as usize))
        .collect()
}

/// The local property `user`'s call `i` reads, if it's a switch or a
/// blend by property and the property isn't global.
fn local_property<'a>(file: &'a [u8], user: &User, i: usize) -> Result<Option<&'a [u8]>> {
    let r = Reader(file);
    let call = user.calls + i * CALL;
    if r.u16(call + CALL_FLAGS)? & IS_CONTAINER == 0 {
        return Ok(None);
    }
    let at = container_at(file, user, call)?;
    let reads = match r.u8(at)? {
        SWITCH => true,
        BLEND => r.u8(at + BY_PROPERTY)? != 0,
        _ => false,
    };
    if !reads || r.u16(at + LOCAL_INDEX)? == GLOBAL {
        return Ok(None);
    }
    Ok(Some(name_at(file, r.u64(at + PROPERTY)? as usize)?))
}

/// Where the params of `user`'s container call at `call` are, in the file.
fn container_at(file: &[u8], user: &User, call: usize) -> Result<usize> {
    let param = Reader(file).ptr(call + PARAM)?;
    (user.containers.checked_add(param)).context("a container's params past the end")
}

fn i32_in(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(d[at..at + 4].try_into().expect("four bytes"))
}

/// Where the users' offsets start, after their hashes.
fn offsets_at(file: &[u8]) -> Result<usize> {
    let count = Reader(file).u32(NUM_USER)? as usize;
    Ok((USER_HASHES + 4 * count).next_multiple_of(8))
}

/// Where each user's offset is kept.
fn user_offsets(file: &[u8]) -> Result<impl Iterator<Item = usize>> {
    let (at, count) = (offsets_at(file)?, Reader(file).u32(NUM_USER)? as usize);
    Ok((0..count).map(move |i| at + 8 * i))
}

fn find(file: &[u8], user: &User, name: &str) -> Result<Option<usize>> {
    for i in 0..user.count {
        if call_name(file, user.calls + i * CALL)? == name.as_bytes() {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

fn call_name(file: &[u8], call: usize) -> Result<&[u8]> {
    name_at(file, Reader(file).u64(call + CALL_NAME)? as usize)
}

fn name_at(file: &[u8], offset: usize) -> Result<&[u8]> {
    let at = Reader(file).ptr(NAMES)?.checked_add(offset);
    let rest = at
        .and_then(|at| file.get(at..))
        .context("a name past the end")?;
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .context("an unended name")?;
    Ok(&rest[..end])
}

/// `name`'s offset in the file's names.
fn name_offset(file: &[u8], name: &str) -> Result<usize> {
    let names = Reader(file).u64(NAMES)? as usize;
    let table = file.get(names..).context("names past the end")?;
    let at = table
        .split(|&b| b == 0)
        .scan(0, |start, s| {
            let here = *start;
            *start += s.len() + 1;
            Some((here, s))
        })
        .find(|(_, s)| *s == name.as_bytes())
        .with_context(|| format!("{name} is not among the XLink2 names"))?;
    Ok(at.0)
}

fn container_size(file: &[u8], at: usize) -> Result<usize> {
    let r = Reader(file);
    Ok(match r.u8(at)? {
        SWITCH => 0x20,
        BLEND if r.u8(at + BY_PROPERTY)? != 0 => 0x20,
        1..=4 => 0x10,
        kind => bail!("container type {kind} is not copied yet"),
    })
}

fn put(d: &mut [u8], at: usize, bytes: &[u8]) {
    d[at..at + bytes.len()].copy_from_slice(bytes);
}

fn align4(n: usize) -> usize {
    n.next_multiple_of(4)
}
