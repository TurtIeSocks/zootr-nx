//! Havok binary tagfiles (`TAG0`), the format inside Phive's navmesh and
//! shape files. A tagfile carries its own type definitions, so this reader
//! knows no Havok class: it reads the types, then hands out objects whose
//! fields are found by name.
//!
//! Layout, as read from the game's files: chunks with a big-endian header
//! (top two bits flags, the rest the size including the header) and a tag.
//! `TAG0` holds `SDKV`, `DATA`, `TYPE` and `INDX`. `TYPE` holds the type
//! names (`TST1`), their templates (`TNA1`), field names (`FST1`) and the
//! type bodies (`TBDY`). `INDX` holds the items (`ITEM`): each a type, an
//! offset into `DATA` and a count. A pointer or array in `DATA` stores the
//! index of the item it points at in its first four bytes.

use anyhow::{Context, Result, bail, ensure};

use crate::reader::Reader;

/// A type: its name, the type it extends, its size and its fields.
#[derive(Debug, Clone, Default)]
pub struct Type {
    pub name: String,
    pub parent: usize,
    pub size: usize,
    pub fields: Vec<Field>,
    /// Template arguments: a name starting `t` holds a type index, one
    /// starting `v` a value.
    pub templates: Vec<(String, usize)>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub offset: usize,
    pub ty: usize,
}

/// One block of `DATA`: `count` values of `ty` from `offset`.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub ty: usize,
    pub offset: usize,
    pub count: usize,
}

pub struct Tagfile {
    data: Vec<u8>,
    pub types: Vec<Type>,
    pub items: Vec<Item>,
}

/// A value of a known type at an offset in `DATA`.
#[derive(Clone, Copy)]
pub struct Obj<'a> {
    file: &'a Tagfile,
    pub ty: usize,
    at: usize,
}

/// The chunks directly inside `bytes[from..to]`, as (tag, body).
fn chunks(bytes: &[u8], from: usize, to: usize) -> Result<Vec<(&str, &[u8])>> {
    let mut out = vec![];
    let mut at = from;
    while at + 8 <= to {
        let head = u32::from_be_bytes(bytes[at..at + 4].try_into()?);
        let size = (head & 0x3fff_ffff) as usize;
        let tag = std::str::from_utf8(&bytes[at + 4..at + 8]).context("chunk tag")?;
        ensure!(
            size >= 8 && at + size <= to,
            "chunk {tag} at {at:#x} overruns"
        );
        out.push((tag, &bytes[at + 8..at + size]));
        at += size;
    }
    Ok(out)
}

fn chunk<'a>(list: &[(&str, &'a [u8])], tag: &str) -> Result<&'a [u8]> {
    list.iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, b)| *b)
        .with_context(|| format!("no {tag} chunk"))
}

/// The NUL-separated strings of a `TST1` or `FST1` body.
fn strings(body: &[u8]) -> Vec<String> {
    let mut list: Vec<_> = body
        .split(|&b| b == 0)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    list.pop();
    list
}

/// Havok's packed integers: the top bits of the first byte give the length.
struct Packed<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Packed<'_> {
    fn more(&self) -> bool {
        self.at < self.bytes.len()
    }
    fn next(&mut self) -> Result<usize> {
        let b = |i: usize| -> Result<usize> {
            Ok(*self
                .bytes
                .get(self.at + i)
                .context("packed int past the end")? as usize)
        };
        let first = b(0)?;
        let (len, value) = match first {
            0x00..=0x7f => (1, first),
            0x80..=0xbf => (2, (first & 0x3f) << 8 | b(1)?),
            0xc0..=0xdf => (3, (first & 0x1f) << 16 | b(1)? << 8 | b(2)?),
            _ => (4, (first & 0x07) << 24 | b(1)? << 16 | b(2)? << 8 | b(3)?),
        };
        self.at += len;
        Ok(value)
    }
}

const SUBTYPE: usize = 0x1;
const POINTER: usize = 0x2;
const VERSION: usize = 0x4;
const SIZE: usize = 0x8;
const ABSTRACT: usize = 0x10;
const FIELDS: usize = 0x20;
const INTERFACES: usize = 0x40;

impl Tagfile {
    /// Reads a tagfile, or a Phive file wrapping one (its header gives the
    /// tagfile's offset at 0x0c).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let start = if bytes.starts_with(b"Phive") {
            Reader(bytes).u32(0x0c)? as usize
        } else {
            0
        };
        let head = bytes.get(start..start + 4).context("no tagfile")?;
        let end = start + (u32::from_be_bytes(head.try_into()?) & 0x3fff_ffff) as usize;
        ensure!(end <= bytes.len(), "tagfile overruns the file");
        let top = chunks(bytes, start, end)?;
        let tag0 = chunk(&top, "TAG0")?;
        let base = tag0.as_ptr() as usize - bytes.as_ptr() as usize;
        let inner = chunks(bytes, base, base + tag0.len())?;
        let data = chunk(&inner, "DATA")?.to_vec();
        let type_chunk = chunk(&inner, "TYPE")?;
        let type_at = type_chunk.as_ptr() as usize - bytes.as_ptr() as usize;
        let ty = chunks(bytes, type_at, type_at + type_chunk.len())?;
        let index = chunk(&inner, "INDX")?;
        let index_at = index.as_ptr() as usize - bytes.as_ptr() as usize;
        let indx = chunks(bytes, index_at, index_at + index.len())?;
        let types = Self::types(
            &strings(chunk(&ty, "TST1")?),
            &strings(chunk(&ty, "FST1")?),
            chunk(&ty, "TNA1")?,
            chunk(&ty, "TBDY")?,
        )?;
        let items = chunk(&indx, "ITEM")?
            .as_chunks::<12>()
            .0
            .iter()
            .map(|e| {
                let r = Reader(e);
                Ok(Item {
                    ty: (r.u32(0)? & 0x00ff_ffff) as usize,
                    offset: r.u32(4)? as usize,
                    count: r.u32(8)? as usize,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { data, types, items })
    }

    fn types(names: &[String], fields: &[String], tna: &[u8], body: &[u8]) -> Result<Vec<Type>> {
        let name = |i: usize, list: &[String]| -> Result<String> {
            list.get(i)
                .cloned()
                .with_context(|| format!("no string {i}"))
        };
        let mut p = Packed { bytes: tna, at: 0 };
        let count = p.next()?;
        let mut types = vec![Type::default(); count];
        for t in types.iter_mut().skip(1) {
            t.name = name(p.next()?, names)?;
            for _ in 0..p.next()? {
                t.templates.push((name(p.next()?, names)?, p.next()?));
            }
        }
        let mut p = Packed { bytes: body, at: 0 };
        while p.more() {
            let index = p.next()?;
            if index == 0 {
                continue;
            }
            let parent = p.next()?;
            let flags = p.next()?;
            let t = types
                .get_mut(index)
                .with_context(|| format!("body for type {index}"))?;
            t.parent = parent;
            if flags & SUBTYPE != 0 {
                p.next()?;
            }
            if flags & POINTER != 0 {
                p.next()?;
            }
            if flags & VERSION != 0 {
                p.next()?;
            }
            if flags & SIZE != 0 {
                t.size = p.next()?;
                p.next()?; // alignment
            }
            if flags & ABSTRACT != 0 {
                p.next()?;
            }
            if flags & FIELDS != 0 {
                for _ in 0..p.next()? {
                    let name = name(p.next()?, fields)?;
                    p.next()?; // flags
                    let offset = p.next()?;
                    let ty = p.next()?;
                    t.fields.push(Field { name, offset, ty });
                }
            }
            if flags & INTERFACES != 0 {
                for _ in 0..p.next()? {
                    p.next()?;
                    p.next()?;
                }
            }
            ensure!(
                flags & !0x7f == 0,
                "type {index} has unknown flags {flags:#x}"
            );
        }
        Ok(types)
    }

    /// The size of `ty`, taken from the nearest ancestor that gives one.
    pub fn size(&self, mut ty: usize) -> usize {
        while ty != 0 && self.types[ty].size == 0 {
            ty = self.types[ty].parent;
        }
        self.types[ty].size
    }

    /// The `i`th value of item `index`.
    pub fn item(&self, index: usize, i: usize) -> Result<Obj<'_>> {
        let item = self
            .items
            .get(index)
            .with_context(|| format!("no item {index}"))?;
        ensure!(
            i < item.count,
            "item {index} has {} values, not {}",
            item.count,
            i + 1
        );
        Ok(Obj {
            file: self,
            ty: item.ty,
            at: item.offset + i * self.size(item.ty),
        })
    }

    /// The root container's named variants that hold an object: each name
    /// and its object.
    pub fn roots(&self) -> Result<Vec<(String, Obj<'_>)>> {
        let root = self.item(1, 0)?;
        let mut out = vec![];
        for v in root.array("namedVariants")? {
            if let Some(obj) = v.field("variant")?.deref()? {
                out.push((v.field("name")?.string()?, obj));
            }
        }
        Ok(out)
    }
}

impl<'a> Obj<'a> {
    fn r(&self) -> Reader<'a> {
        Reader(&self.file.data)
    }

    pub fn type_name(&self) -> &'a str {
        &self.file.types[self.ty].name
    }

    /// The field `name` of this object or of a type it extends.
    pub fn field(&self, name: &str) -> Result<Obj<'a>> {
        let mut ty = self.ty;
        while ty != 0 {
            if let Some(f) = self.file.types[ty].fields.iter().find(|f| f.name == name) {
                return Ok(Obj {
                    file: self.file,
                    ty: f.ty,
                    at: self.at + f.offset,
                });
            }
            ty = self.file.types[ty].parent;
        }
        bail!("{} has no field {name}", self.type_name())
    }

    /// The object a pointer or variant points at, by item index, or `None`
    /// for a null one.
    pub fn deref(&self) -> Result<Option<Obj<'a>>> {
        match self.r().u32(self.at)? as usize {
            0 => Ok(None),
            index => self.file.item(index, 0).map(Some),
        }
    }

    /// The values of the array field `name`. An `hkArray` stores the
    /// index of its item, and the item's count is its length: the size word
    /// is left for the game to fill in.
    pub fn array(&self, name: &str) -> Result<Vec<Obj<'a>>> {
        let f = self.field(name)?;
        let index = f.r().u32(f.at)? as usize;
        if index == 0 {
            return Ok(vec![]);
        }
        let len = self.file.items.get(index).map_or(0, |i| i.count);
        (0..len).map(|i| self.file.item(index, i)).collect()
    }

    /// An `hkStringPtr`: the item it points at, as text.
    pub fn string(&self) -> Result<String> {
        let index = self.r().u32(self.at)? as usize;
        if index == 0 {
            return Ok(String::new());
        }
        let item = self.file.items[index];
        let bytes = Reader(&self.file.data).bytes(item.offset, item.count)?;
        Ok(String::from_utf8_lossy(bytes).trim_end_matches('\0').into())
    }

    pub fn u32(&self) -> Result<u32> {
        self.r().u32(self.at)
    }

    pub fn i32(&self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn u16(&self) -> Result<u16> {
        self.r().u16(self.at)
    }

    /// `n` floats from here, like an `hkVector4`'s x, y, z.
    pub fn floats<const N: usize>(&self) -> Result<[f32; N]> {
        let mut out = [0.0; N];
        for (i, v) in out.iter_mut().enumerate() {
            *v = self.r().f32(self.at + 4 * i)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &str, body: &[u8], leaf: bool) -> Vec<u8> {
        let head = (body.len() + 8) as u32 | if leaf { 0x4000_0000 } else { 0 };
        [&head.to_be_bytes()[..], tag.as_bytes(), body].concat()
    }

    fn strs(list: &[&str]) -> Vec<u8> {
        list.iter()
            .flat_map(|s| [s.as_bytes(), b"\0"].concat())
            .collect()
    }

    fn item(ty: u32, kind: u32, offset: u32, count: u32) -> Vec<u8> {
        [ty | kind << 24, offset, count]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect()
    }

    /// Types: 1 Root { points: hkArray @0, label: hkStringPtr @16 },
    /// 2 hkArray, 3 Point { x: float @0, n: int @4 }, 4 float, 5 int,
    /// 6 char, 7 hkStringPtr. Root holds two points and the label "hi".
    fn sample() -> Vec<u8> {
        let names = strs(&[
            "Root",
            "hkArray",
            "Point",
            "float",
            "int",
            "char",
            "hkStringPtr",
        ]);
        let fields = strs(&["points", "label", "x", "n"]);
        let tna: Vec<u8> = [vec![8], (0..7).flat_map(|i| [i, 0]).collect()].concat();
        let body = [
            vec![1, 0, 0x28, 24, 8, 2, 0, 0, 0, 2, 1, 0, 16, 7],
            vec![2, 0, 0x08, 16, 8],
            vec![3, 0, 0x28, 8, 4, 2, 2, 0, 0, 4, 3, 0, 4, 5],
            vec![
                4, 0, 0x08, 4, 4, 5, 0, 0x08, 4, 4, 6, 0, 0x08, 1, 1, 7, 0, 0x08, 8, 8,
            ],
        ]
        .concat();
        let mut data = vec![0u8; 48];
        data[0..4].copy_from_slice(&2u32.to_le_bytes());
        data[16..20].copy_from_slice(&3u32.to_le_bytes());
        data[24..28].copy_from_slice(&1.5f32.to_le_bytes());
        data[28..32].copy_from_slice(&7u32.to_le_bytes());
        data[32..36].copy_from_slice(&(-2.0f32).to_le_bytes());
        data[36..40].copy_from_slice(&9u32.to_le_bytes());
        data[40..43].copy_from_slice(b"hi\0");
        let types = [
            chunk("TST1", &names, true),
            chunk("TNA1", &tna, true),
            chunk("FST1", &fields, true),
            chunk("TBDY", &body, true),
        ]
        .concat();
        let items = [
            item(0, 0, 0, 0),
            item(1, 0x10, 0, 1),
            item(3, 0x20, 24, 2),
            item(6, 0x20, 40, 3),
        ];
        let tag0 = [
            chunk("SDKV", b"20250200", true),
            chunk("DATA", &data, true),
            chunk("TYPE", &types, false),
            chunk("INDX", &chunk("ITEM", &items.concat(), true), false),
        ]
        .concat();
        chunk("TAG0", &tag0, false)
    }

    #[test]
    fn reads_fields_arrays_and_strings_by_name() {
        let file = Tagfile::parse(&sample()).unwrap();
        let root = file.item(1, 0).unwrap();
        assert_eq!(root.type_name(), "Root");
        let points = root.array("points").unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].field("x").unwrap().floats::<1>().unwrap(), [-2.0]);
        assert_eq!(points[0].field("n").unwrap().i32().unwrap(), 7);
        assert_eq!(root.field("label").unwrap().string().unwrap(), "hi");
        assert!(root.field("nope").is_err());
    }

    #[test]
    fn finds_the_tagfile_inside_a_phive_header() {
        let tag = sample();
        let mut phive = b"Phive\0\x01\x00\xff\xfe\x01\x01".to_vec();
        phive.extend(0x14u32.to_le_bytes());
        phive.extend((tag.len() as u32).to_le_bytes());
        phive.extend(&tag);
        phive.extend(b"trailing section");
        let file = Tagfile::parse(&phive).unwrap();
        assert_eq!(file.items.len(), 4);
    }

    #[test]
    fn packed_ints_take_their_length_from_the_first_byte() {
        let bytes = [0x7f, 0xbf, 0xff, 0xc1, 0x23, 0x45, 0xe1, 0x23, 0x45, 0x67];
        let mut p = Packed {
            bytes: &bytes,
            at: 0,
        };
        let got: Vec<_> = (0..4).map(|_| p.next().unwrap()).collect();
        assert_eq!(got, [0x7f, 0x3fff, 0x12345, 0x1234567]);
        assert!(!p.more());
    }

    #[test]
    fn refuses_other_files() {
        assert!(Tagfile::parse(b"YB\x07\x00").is_err());
    }
}
