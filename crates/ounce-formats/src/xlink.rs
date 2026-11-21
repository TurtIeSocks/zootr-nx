//! XLink2 files (`.bslnk` sounds, `.belnk` effects), version 0x22 with
//! 64-bit offsets. Each user (an actor's sound or effect set) holds calls
//! the game plays by name: an asset, or a container whose children are
//! calls too. [`copy_call`] gives one user a call another has.
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

// A user's header.
const LOCALS: usize = 0x04;
const NUM_CALL: usize = 0x08;
const NUM_ASSET: usize = 0x0c;
/// Containers of [`RANDOM`] type.
const NUM_RANDOM: usize = 0x10;
const TRIGGERS: usize = 0x30;
const USER_HEADER: usize = 0x38;

// A call.
const CALL: usize = 0x30;
const CALL_NAME: usize = 0x00;
const ASSET: usize = 0x08;
const CALL_FLAGS: usize = 0x0a;
const PARENT: usize = 0x10;
const KEY: usize = 0x18;
const PARAM: usize = 0x20;
const IS_CONTAINER: u16 = 1;

// A container.
const SWITCH: u8 = 0;
const RANDOM: u8 = 2;
const BLEND: u8 = 3;
const BY_PROPERTY: usize = 0x01;
const FIRST: usize = 0x04;
const LAST: usize = 0x08;

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
    let r = Reader(file);
    ensure!(&file[..4] == b"XLNK", "not an XLink2 file");
    let version = r.u32(VERSION_AT)?;
    ensure!(
        version == VERSION,
        "XLink2 version {version:#x}, not {VERSION:#x}"
    );
    let (a, b) = (user(file, from)?, user(file, to)?);
    let named = name_offset(file, name)?;
    ensure!(find(file, &b, name)?.is_none(), "{to} already has {name}");
    let root = find(file, &a, key)?.with_context(|| format!("{from} has no {key}"))?;

    let order = tree(file, &a, root)?;
    let moved = |old: usize| b.count + order.iter().position(|&o| o == old).expect("in the tree");

    let (mut calls, mut params) = (Vec::new(), Vec::new());
    let (mut assets, mut randoms) = (r.u32(b.base + NUM_ASSET)?, 0);
    let param_base = b.triggers - b.containers;
    for (k, &old) in order.iter().enumerate() {
        let from_call = a.calls + old * CALL;
        let mut call = r.bytes(from_call, CALL)?.to_vec();
        let parent = match k {
            0 => {
                put(&mut call, CALL_NAME, &(named as u64).to_le_bytes());
                put(&mut call, KEY, &crc32(name.as_bytes()).to_le_bytes());
                -1
            }
            _ => moved(r.i32(from_call + PARENT)? as usize) as i32,
        };
        put(&mut call, PARENT, &parent.to_le_bytes());
        if r.u16(from_call + CALL_FLAGS)? & IS_CONTAINER != 0 {
            let at = a.containers + r.u64(from_call + PARAM)? as usize;
            let mut param = r.bytes(at, container_size(file, at)?)?.to_vec();
            for field in [FIRST, LAST] {
                put(
                    &mut param,
                    field,
                    &(moved(r.i32(at + field)? as usize) as i32).to_le_bytes(),
                );
            }
            randoms += u32::from(param[0] == RANDOM);
            let offset = (param_base + params.len()) as u64;
            put(&mut call, PARAM, &offset.to_le_bytes());
            params.extend(param);
        } else {
            put(&mut call, ASSET, &(assets as i16).to_le_bytes());
            assets += 1;
        }
        calls.extend(call);
    }

    // The calls by name, which the game searches by halves. The shipped
    // order is kept, ties too, and each new call goes after its equals.
    let total = b.count + order.len();
    let mut names = (0..b.count)
        .map(|i| call_name(file, b.calls + i * CALL))
        .collect::<Result<Vec<_>>>()?;
    for call in calls.chunks(CALL) {
        names.push(name_at(
            file,
            u64::from_le_bytes(call[..8].try_into()?) as usize,
        )?);
    }
    let mut sorted = (0..b.count)
        .map(|k| r.u16(b.ids + 2 * k))
        .collect::<Result<Vec<_>>>()?;
    for new in b.count..total {
        let at = sorted.partition_point(|&i| names[i as usize] <= names[new]);
        sorted.insert(at, new as u16);
    }
    let mut ids: Vec<u8> = sorted.iter().flat_map(|i| i.to_le_bytes()).collect();
    ids.resize(align4(total * 2), 0);

    let mut out = Vec::with_capacity(file.len() + ids.len() + calls.len() + params.len());
    out.extend(&file[..b.ids]);
    out.extend(&ids);
    out.extend(&file[b.calls..b.containers]);
    out.extend(&calls);
    out.extend(&file[b.containers..b.triggers]);
    out.extend(&params);
    out.extend(&file[b.triggers..]);
    let grown = out.len() - file.len();

    let w = |out: &mut Vec<u8>, at: usize, v: u32| put(out, at, &v.to_le_bytes());
    w(&mut out, b.base + NUM_CALL, total as u32);
    w(&mut out, b.base + NUM_ASSET, assets);
    w(
        &mut out,
        b.base + NUM_RANDOM,
        r.u32(b.base + NUM_RANDOM)? + randoms,
    );
    let triggers = r.u64(b.base + TRIGGERS)? + grown as u64;
    put(&mut out, b.base + TRIGGERS, &triggers.to_le_bytes());
    for at in user_offsets(file)?.chain([CONDITIONS, NAMES]) {
        let offset = r.u64(at)? as usize;
        if offset > b.base {
            put(&mut out, at, &((offset + grown) as u64).to_le_bytes());
        }
    }
    let size = out.len() as u32;
    w(&mut out, DATA_SIZE, size);
    Ok(out)
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

/// The call `root` of `user` and every call under it, by index, each
/// container's children together after it.
fn tree(file: &[u8], user: &User, root: usize) -> Result<Vec<usize>> {
    let r = Reader(file);
    let mut order = vec![root];
    let mut i = 0;
    while i < order.len() {
        let call = user.calls + order[i] * CALL;
        if r.u16(call + CALL_FLAGS)? & IS_CONTAINER != 0 {
            let at = user.containers + r.u64(call + PARAM)? as usize;
            let (first, last) = (r.i32(at + FIRST)?, r.i32(at + LAST)?);
            ensure!(
                0 <= first && first <= last && (last as usize) < user.count,
                "a container's children past its user's calls"
            );
            order.extend(first as usize..=last as usize);
        }
        i += 1;
    }
    Ok(order)
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
    let at = Reader(file).u64(NAMES)? as usize + offset;
    let rest = file.get(at..).context("a name past the end")?;
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
