//! Edits to a model file (BFRES v10): vertices edited in place, and a
//! shape's whole mesh replaced by one appended to the memory pool.
//!
//! The children: `codec` packs and unpacks the stored vertex values,
//! `names` finds shapes and reads the pool's strings, and `relocate`
//! moves the pool, blocks and relocation table when the pool grows.

use anyhow::{Context, Result, bail, ensure};

use crate::reader::Reader;

mod codec;
mod names;
mod relocate;

use codec::{half_at, to_half, unorm_at};
pub use codec::{pack, unpack};
pub use names::shape_names;
pub(crate) use names::{find_magic, name_at};
use names::{shape_at, try_name_at};
pub use relocate::pointers;
use relocate::{Pool, grow_blocks, pad, put, relocate};

/// Where the header keeps the file's size (u32).
const FILE_SIZE: usize = 0x1c;
/// The pool's size stays a multiple of this, and each buffer in it starts
/// on a multiple of the smaller.
const POOL_ALIGN: usize = 0x1000;
const BUFFER_ALIGN: usize = 0x100;
/// A shape's name, vertex buffer header, meshes (one per level of
/// detail), bounding boxes and sphere, and how many meshes.
const SHAPE_NAME: usize = 0x08;
const SHAPE_VERTICES: usize = 0x10;
const SHAPE_MESHES: usize = 0x18;
const SHAPE_BOXES: usize = 0x38;
const SHAPE_SPHERE: usize = 0x40;
const SHAPE_MESH_COUNT: usize = 0x5b;
/// A mesh: its submeshes, its index buffer's size, where its indices
/// start in the pool, their format and count, the vertex they count from,
/// and how many submeshes.
const MESH_SIZE: usize = 0x38;
const MESH_SUBMESHES: usize = 0x00;
const MESH_BUFFER_SIZE: usize = 0x18;
const MESH_FACES: usize = 0x20;
const MESH_FORMAT: usize = 0x28;
const MESH_COUNT: usize = 0x2c;
const MESH_FIRST: usize = 0x30;
const MESH_SUBMESH_COUNT: usize = 0x34;
const U16_INDICES: u32 = 1;
/// A vertex buffer header's attributes, their strides, where its data
/// starts in the pool, how many attributes and buffers, and vertices.
const VERTEX_ATTRIBUTES: usize = 0x08;
const VERTEX_SIZES: usize = 0x30;
const VERTEX_STRIDES: usize = 0x38;
const VERTEX_DATA: usize = 0x48;
const VERTEX_ATTRIBUTE_COUNT: usize = 0x4c;
const VERTEX_BUFFER_COUNT: usize = 0x4d;
const VERTEX_COUNT: usize = 0x50;
/// An attribute record's size.
const ATTRIBUTE_SIZE: usize = 16;
/// The attribute formats the game's models use: three and two floats,
/// four and two halves, two 16-bit unorms, and 10:10:10:2 snorms.
const FLOAT3: u32 = 0x1805;
const FLOAT2: u32 = 0x1705;
const HALF4: u32 = 0x1505;
const HALF2: u32 = 0x1205;
const UNORM16X2: u32 = 0x1201;
const SNORM10X3_2: u32 = 0xe02;
/// The attributes edited, by name and the one format each is read in. A
/// normal and a tangent are kept as stored.
const POSITION: (&str, u32) = ("_p0", HALF4);
const UV: (&str, u32) = ("_u0", UNORM16X2);
const NORMAL: (&str, u32) = ("_n0", SNORM10X3_2);
const TANGENT: (&str, u32) = ("_t0", SNORM10X3_2);

/// One vertex of a shape: where it is, its texture coordinates, and its
/// normal and tangent as stored (10:10:10:2, see [`unpack`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub normal: u32,
    pub tangent: u32,
}

/// The vertices of the shape named `shape`, in its buffer's order.
pub fn vertices(data: &[u8], shape: &str) -> Result<Vec<Vertex>> {
    let layout = Layout::find(data, shape)?;
    (0..layout.count).map(|i| layout.read(data, i)).collect()
}

/// The positions of the shape named `shape`, whatever its other
/// attributes' formats.
pub fn positions(data: &[u8], shape: &str) -> Result<Vec<[f32; 3]>> {
    let r = Reader(data);
    let buffer = Buffer::find(data, shape)?;
    let position = buffer.offset(data, POSITION)?;
    (0..buffer.count)
        .map(|i| {
            let at = buffer.start + buffer.stride * i + position;
            Ok([half_at(&r, at)?, half_at(&r, at + 2)?, half_at(&r, at + 4)?])
        })
        .collect()
}

/// `data` with `edit` run over each vertex of the shape named `shape`.
/// Only the vertices it changes are written back.
pub fn edit_vertices(
    data: &[u8],
    shape: &str,
    mut edit: impl FnMut(&mut Vertex),
) -> Result<Vec<u8>> {
    let layout = Layout::find(data, shape)?;
    let mut out = data.to_vec();
    for i in 0..layout.count {
        let old = layout.read(data, i)?;
        let mut new = old;
        edit(&mut new);
        if new != old {
            layout.write(&mut out, i, &new)?;
        }
    }
    Ok(out)
}

/// One attribute record of a vertex buffer header, at its offset. Its
/// fields are read on demand, so a search reads only the records up to
/// its match.
#[derive(Clone, Copy)]
struct AttrRecord(usize);

impl AttrRecord {
    /// Its name, `None` when its pointer leads to no pool string.
    fn name(self, data: &[u8]) -> Result<Option<&[u8]>> {
        try_name_at(data, self.0)
    }

    fn format(self, data: &[u8]) -> Result<u32> {
        Reader(data).u32(self.0 + 8)
    }

    /// Where it sits in each vertex.
    fn offset(self, data: &[u8]) -> Result<usize> {
        Ok(Reader(data).u16(self.0 + 12)? as usize)
    }

    /// Which buffer holds it.
    fn buffer(self, data: &[u8]) -> Result<usize> {
        Ok(Reader(data).u16(self.0 + 14)? as usize)
    }
}

/// Every attribute record of the vertex buffer header at `v`.
fn attr_records(data: &[u8], v: usize) -> Result<impl Iterator<Item = AttrRecord>> {
    let r = Reader(data);
    let attributes = r.ptr(v + VERTEX_ATTRIBUTES)?;
    let count = r.u8(v + VERTEX_ATTRIBUTE_COUNT)? as usize;
    Ok((0..count).map(move |i| AttrRecord(attributes + ATTRIBUTE_SIZE * i)))
}

/// Every vertex's value of attribute `name` (`_p0`, `_u0`, `_n0`) in the
/// shape named `shape`, whichever of its buffers holds it, as up to four
/// floats (missing ones 0). Reads the formats the game's models use for
/// them: floats, halves, 16-bit unorms and 10:10:10:2 snorms.
pub fn attribute(data: &[u8], shape: &str, name: &str) -> Result<Vec<[f32; 4]>> {
    let r = Reader(data);
    let at = shape_at(data, shape)?;
    let pool = Pool::read(data)?;
    let v = r.ptr(at + SHAPE_VERTICES)?;
    let count = r.offset(v + VERTEX_COUNT)?;
    // An unreadable name reads as no name here, so the error lists the rest.
    let named = |a: AttrRecord| name_at(data, a.0).unwrap_or_default();
    let all: Vec<_> = attr_records(data, v)?.collect();
    let found = all.iter().find(|&&a| named(a) == name.as_bytes());
    let a = *found.with_context(|| {
        let names: Vec<_> = (all.iter())
            .map(|&a| String::from_utf8_lossy(named(a)))
            .collect();
        format!("{shape}: no {name}, only {}", names.join(" "))
    })?;
    let (format, offset, buffer) = (a.format(data)?, a.offset(data)?, a.buffer(data)?);
    // Each buffer follows the one before it in the pool, aligned.
    let (sizes, strides) = (r.ptr(v + VERTEX_SIZES)?, r.ptr(v + VERTEX_STRIDES)?);
    let mut start = pool.start + r.offset(v + VERTEX_DATA)?;
    for k in 0..buffer {
        start = (start + r.offset(sizes + 16 * k)?).next_multiple_of(8);
    }
    let stride = r.offset(strides + 16 * buffer)?;
    (0..count)
        .map(|i| {
            let at = start + stride * i + offset;
            let half = |k: usize| half_at(&r, at + 2 * k);
            let float = |k: usize| r.f32(at + 4 * k);
            let unorm = |k: usize| unorm_at(&r, at + 2 * k);
            Ok(match format {
                FLOAT3 => [float(0)?, float(1)?, float(2)?, 0.0],
                FLOAT2 => [float(0)?, float(1)?, 0.0, 0.0],
                HALF4 => [half(0)?, half(1)?, half(2)?, half(3)?],
                HALF2 => [half(0)?, half(1)?, 0.0, 0.0],
                UNORM16X2 => [unorm(0)?, unorm(1)?, 0.0, 0.0],
                SNORM10X3_2 => unpack(r.u32(at)?),
                other => bail!("{shape}: {name} in format {other:#x}"),
            })
        })
        .collect()
}

/// The finest level of detail's triangles of the shape named `shape`, as
/// indices into its vertices.
pub fn triangles(data: &[u8], shape: &str) -> Result<Vec<usize>> {
    let r = Reader(data);
    let pool = Pool::read(data)?;
    let meshes = Mesh::all(data, shape_at(data, shape)?)?;
    let m = meshes
        .first()
        .with_context(|| format!("{shape}: no mesh"))?;
    (0..m.count)
        .map(|i| Ok(m.first + r.u16(pool.start + m.faces + 2 * i)? as usize))
        .collect()
}

/// One attribute of a shape's vertices: its name (`_p0`, `_u1`, `_i0`),
/// format, where it sits in each vertex, and whether its bytes differ
/// between vertices.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub format: u32,
    pub offset: usize,
    pub varies: bool,
}

/// The attributes of the shape named `shape`, in its buffer's order.
pub fn attributes(data: &[u8], shape: &str) -> Result<Vec<Attribute>> {
    let buffer = Buffer::find(data, shape)?;
    let mut found: Vec<Attribute> = attr_records(data, buffer.vertex)?
        .map(|a| {
            Ok(Attribute {
                name: String::from_utf8_lossy(a.name(data)?.context("an attribute with no name")?)
                    .into(),
                format: a.format(data)?,
                offset: a.offset(data)?,
                varies: false,
            })
        })
        .collect::<Result<_>>()?;
    found.sort_by_key(|a| a.offset);
    let ends: Vec<usize> = (found.iter().skip(1).map(|a| a.offset))
        .chain([buffer.stride])
        .collect();
    let vertex = |i: usize| &data[buffer.start + buffer.stride * i..][..buffer.stride];
    for (a, end) in found.iter_mut().zip(ends) {
        let bytes = |i| &vertex(i)[a.offset..end];
        a.varies = (1..buffer.count).any(|i| bytes(i) != bytes(0));
    }
    Ok(found)
}

/// Each vertex's bone, its `_i0` bytes read little-endian, in its
/// buffer's order: a rigid shape's parts each follow one.
pub fn bones(data: &[u8], shape: &str) -> Result<Vec<u32>> {
    let buffer = Buffer::find(data, shape)?;
    let found = attributes(data, shape)?;
    let at = found
        .iter()
        .position(|a| a.name == "_i0")
        .with_context(|| format!("{shape}: no _i0"))?;
    let start = found[at].offset;
    let end = found
        .get(at + 1)
        .map_or(buffer.stride, |a| a.offset)
        .min(start + 4);
    Ok((0..buffer.count)
        .map(|i| {
            let raw = &data[buffer.start + buffer.stride * i + start..][..end - start];
            raw.iter().rev().fold(0, |n, &b| n << 8 | u32::from(b))
        })
        .collect())
}

/// One level of detail of a shape: its vertices, and its triangles as
/// indices into them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lod {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// The levels of detail of the shape named `shape`, finest first.
pub fn lods(data: &[u8], shape: &str) -> Result<Vec<Lod>> {
    let r = Reader(data);
    let layout = Layout::find(data, shape)?;
    let pool = Pool::read(data)?;
    let meshes = Mesh::all(data, shape_at(data, shape)?)?;
    let lod = |m: &Mesh| -> Result<Lod> {
        let indices = (0..m.count)
            .map(|i| r.u16(pool.start + m.faces + 2 * i))
            .collect::<Result<Vec<u16>>>()?;
        let count = indices.iter().max().map_or(0, |&i| i as usize + 1);
        let vertices = (m.first..m.first + count)
            .map(|i| layout.read(data, i))
            .collect::<Result<_>>()?;
        Ok(Lod { vertices, indices })
    };
    meshes.iter().map(lod).collect()
}

/// `data` with the shape named `shape` drawing `lods` instead, one for
/// each of its levels of detail. The new buffers are appended to the
/// memory pool, so the header keeps its offsets; what follows the pool
/// moves down, and every pointer to it with it. A new vertex's attributes
/// beyond [`Vertex`]'s (its bone) are the shape's, which every vertex
/// must share.
pub fn replace_mesh(data: &[u8], shape: &str, lods: &[Lod]) -> Result<Vec<u8>> {
    let layout = Layout::find(data, shape)?;
    let template = layout.template(data)?;
    replace(data, shape, &layout, lods, |_, _| Ok(template.clone()))
}

/// [`replace_mesh`] for a shape whose vertices don't all share their other
/// attributes (several parts, each on its own bone): new vertex `j` of
/// level `i` takes every attribute beyond [`Vertex`]'s from the shape's
/// vertex `like[i][j]`, counted through its whole buffer.
pub fn replace_mesh_like(
    data: &[u8],
    shape: &str,
    lods: &[Lod],
    like: &[Vec<usize>],
) -> Result<Vec<u8>> {
    let layout = Layout::find(data, shape)?;
    ensure!(
        like.len() == lods.len()
            && like
                .iter()
                .zip(lods)
                .all(|(l, lod)| l.len() == lod.vertices.len()),
        "{shape}: a like for each new vertex"
    );
    replace(data, shape, &layout, lods, |i, j| {
        let k = like[i][j];
        ensure!(k < layout.count, "{shape}: no vertex {k}");
        Ok(data[layout.start + layout.stride * k..][..layout.stride].to_vec())
    })
}

/// [`replace_mesh`], new vertex `j` of level `i` starting from `rest(i, j)`.
fn replace(
    data: &[u8],
    shape: &str,
    layout: &Layout,
    lods: &[Lod],
    rest: impl Fn(usize, usize) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let r = Reader(data);
    let at = shape_at(data, shape)?;
    let meshes = Mesh::all(data, at)?;
    ensure!(
        meshes.len() == lods.len(),
        "{shape}: {} levels of detail, not {}",
        meshes.len(),
        lods.len()
    );
    let vertices: Vec<&Vertex> = lods.iter().flat_map(|l| &l.vertices).collect();
    let bounds = Bounds::of(&vertices).with_context(|| format!("{shape}: no vertices"))?;
    let Pool { info, start, size } = Pool::read(data)?;
    let end = start + size;

    let mut added = Vec::new();
    let mut faces = Vec::new();
    for lod in lods {
        ensure!(
            lod.vertices.len() <= 1 << 16,
            "{shape}: too many vertices for u16 indices"
        );
        ensure!(
            lod.indices
                .iter()
                .all(|&i| (i as usize) < lod.vertices.len()),
            "{shape}: an index past its vertices"
        );
        pad(&mut added, BUFFER_ALIGN);
        faces.push(size + added.len());
        added.extend(lod.indices.iter().flat_map(|i| i.to_le_bytes()));
    }
    pad(&mut added, BUFFER_ALIGN);
    let vertex_data = size + added.len();
    for (i, lod) in lods.iter().enumerate() {
        for (j, v) in lod.vertices.iter().enumerate() {
            let mut raw = rest(i, j)?;
            layout.write_at(&mut raw, 0, v)?;
            added.extend(raw);
        }
    }
    pad(&mut added, POOL_ALIGN);
    let growth = added.len();

    let mut out = Vec::with_capacity(data.len() + growth);
    out.extend_from_slice(&data[..end]);
    out.extend(added);
    out.extend_from_slice(&data[end..]);
    put(&mut out, info + 4, size + growth)?;
    let len = out.len();
    put(&mut out, FILE_SIZE, len)?;
    relocate(&mut out, end, growth)?;
    grow_blocks(&mut out, end, growth)?;

    let v = r.ptr(at + SHAPE_VERTICES)?;
    put(&mut out, v + VERTEX_DATA, vertex_data)?;
    put(&mut out, v + VERTEX_COUNT, vertices.len())?;
    put(
        &mut out,
        r.ptr(v + VERTEX_SIZES)?,
        vertices.len() * layout.stride,
    )?;
    let mut first = 0;
    for ((mesh, lod), faces) in meshes.iter().zip(lods).zip(faces) {
        let count = lod.indices.len();
        put(&mut out, mesh.at + MESH_FACES, faces)?;
        put(&mut out, mesh.at + MESH_COUNT, count)?;
        put(&mut out, mesh.at + MESH_FIRST, first)?;
        put(&mut out, r.ptr(mesh.at + MESH_BUFFER_SIZE)?, 2 * count)?;
        // Its one submesh: where it starts in the indices, and how many.
        let submesh = r.ptr(mesh.at + MESH_SUBMESHES)?;
        put(&mut out, submesh, 0)?;
        put(&mut out, submesh + 4, count)?;
        first += lod.vertices.len();
    }
    bounds.write(&mut out, data, at, meshes.len())?;
    Ok(out)
}

/// One mesh header of a shape, as read.
struct Mesh {
    at: usize,
    faces: usize,
    count: usize,
    first: usize,
}

impl Mesh {
    /// A shape's meshes, each with one submesh of u16 indices.
    fn all(data: &[u8], shape: usize) -> Result<Vec<Self>> {
        let r = Reader(data);
        let meshes = r.ptr(shape + SHAPE_MESHES)?;
        (0..r.u8(shape + SHAPE_MESH_COUNT)? as usize)
            .map(|i| {
                let at = meshes + MESH_SIZE * i;
                ensure!(
                    r.u32(at + MESH_FORMAT)? == U16_INDICES,
                    "mesh {i}: not u16 indices"
                );
                let submeshes = r.u16(at + MESH_SUBMESH_COUNT)?;
                ensure!(submeshes == 1, "mesh {i}: {submeshes} submeshes");
                Ok(Self {
                    at,
                    faces: r.offset(at + MESH_FACES)?,
                    count: r.offset(at + MESH_COUNT)?,
                    first: r.offset(at + MESH_FIRST)?,
                })
            })
            .collect()
    }
}

/// A box and a sphere holding every vertex of a shape.
#[derive(Debug, PartialEq)]
struct Bounds {
    centre: [f32; 3],
    extent: [f32; 3],
    radius: f32,
}

impl Bounds {
    /// `None` without a vertex to hold.
    fn of(vertices: &[&Vertex]) -> Option<Self> {
        let first = vertices.first()?.position;
        let (mut lo, mut hi) = (first, first);
        for v in vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v.position[k]);
                hi[k] = hi[k].max(v.position[k]);
            }
        }
        let centre: [f32; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
        let extent: [f32; 3] = std::array::from_fn(|k| (hi[k] - lo[k]) / 2.0);
        let radius = vertices
            .iter()
            .map(|v| {
                (0..3)
                    .map(|k| (v.position[k] - centre[k]).powi(2))
                    .sum::<f32>()
                    .sqrt()
            })
            .fold(0.0, f32::max);
        Some(Self {
            centre,
            extent,
            radius,
        })
    }

    /// Every bounding box of the shape at `shape` (each of its `meshes`'
    /// and its submesh's) and its sphere set to these, so the game never
    /// culls a part of it.
    fn write(&self, out: &mut [u8], data: &[u8], shape: usize, meshes: usize) -> Result<()> {
        let r = Reader(data);
        let floats = |at: usize, values: &[f32], out: &mut [u8]| {
            for (i, f) in values.iter().enumerate() {
                out[at + 4 * i..at + 4 * i + 4].copy_from_slice(&f.to_le_bytes());
            }
        };
        let boxes = r.ptr(shape + SHAPE_BOXES)?;
        let box_ = [self.centre, self.extent].concat();
        for i in 0..2 * meshes {
            floats(boxes + 24 * i, &box_, out);
        }
        let sphere = r.ptr(shape + SHAPE_SPHERE)?;
        let [x, y, z] = self.centre;
        floats(sphere, &[x, y, z, self.radius], out);
        Ok(())
    }
}

/// `data` with the shape named `shape` drawing nothing: every mesh and
/// submesh of it zero indices long. Its vertices and bones stay.
pub fn hide_shape(data: &[u8], shape: &str) -> Result<Vec<u8>> {
    let r = Reader(data);
    let at = shape_at(data, shape)?;
    let mut out = data.to_vec();
    let meshes = r.ptr(at + SHAPE_MESHES)?;
    for i in 0..r.u8(at + SHAPE_MESH_COUNT)? as usize {
        let m = meshes + MESH_SIZE * i;
        put(&mut out, m + MESH_COUNT, 0)?;
        let sub = r.ptr(m + MESH_SUBMESHES)?;
        for j in 0..r.u16(m + MESH_SUBMESH_COUNT)? as usize {
            put(&mut out, sub + 8 * j + 4, 0)?;
        }
    }
    Ok(out)
}

/// Where a shape's vertices are, and how to find each attribute.
struct Buffer<'a> {
    shape: &'a str,
    start: usize,
    stride: usize,
    count: usize,
    /// The vertex buffer header.
    vertex: usize,
}

impl<'a> Buffer<'a> {
    fn find(data: &[u8], shape: &'a str) -> Result<Self> {
        let r = Reader(data);
        let at = shape_at(data, shape)?;
        let pool = Pool::read(data)?;
        let v = r.ptr(at + SHAPE_VERTICES)?;
        ensure!(
            r.u8(v + VERTEX_BUFFER_COUNT)? == 1,
            "{shape}: not one vertex buffer"
        );
        let buffer = Self {
            shape,
            start: pool.start + r.offset(v + VERTEX_DATA)?,
            stride: r.offset(r.ptr(v + VERTEX_STRIDES)?)?,
            count: r.offset(v + VERTEX_COUNT)?,
            vertex: v,
        };
        ensure!(
            buffer.start + buffer.stride * buffer.count <= (pool.start + pool.size).min(data.len()),
            "{shape}: vertices past the pool"
        );
        Ok(buffer)
    }

    /// Where attribute `name` sits in each vertex, if it's in `format`.
    fn offset(&self, data: &[u8], (name, format): (&str, u32)) -> Result<usize> {
        let shape = self.shape;
        for a in attr_records(data, self.vertex)? {
            if a.name(data)? == Some(name.as_bytes()) {
                let found = a.format(data)?;
                ensure!(found == format, "{shape}: {name} in format {found:#x}");
                return a.offset(data);
            }
        }
        bail!("{shape}: no {name}")
    }
}

/// Where a shape's vertices and each edited attribute are.
struct Layout {
    start: usize,
    stride: usize,
    count: usize,
    position: usize,
    uv: usize,
    normal: usize,
    tangent: usize,
}

impl Layout {
    fn find(data: &[u8], shape: &str) -> Result<Self> {
        let buffer = Buffer::find(data, shape)?;
        let offset = |attribute| buffer.offset(data, attribute);
        Ok(Self {
            start: buffer.start,
            stride: buffer.stride,
            count: buffer.count,
            position: offset(POSITION)?,
            uv: offset(UV)?,
            normal: offset(NORMAL)?,
            tangent: offset(TANGENT)?,
        })
    }

    fn read(&self, data: &[u8], i: usize) -> Result<Vertex> {
        let r = Reader(data);
        let at = self.start + self.stride * i;
        let half = |k: usize| half_at(&r, at + self.position + 2 * k);
        let unorm = |k: usize| unorm_at(&r, at + self.uv + 2 * k);
        Ok(Vertex {
            position: [half(0)?, half(1)?, half(2)?],
            uv: [unorm(0)?, unorm(1)?],
            normal: r.u32(at + self.normal)?,
            tangent: r.u32(at + self.tangent)?,
        })
    }

    /// The bytes every vertex shares outside the edited attributes, as one
    /// vertex: what a new vertex starts from.
    fn template(&self, data: &[u8]) -> Result<Vec<u8>> {
        let edited = [
            (self.position, 6),
            (self.uv, 4),
            (self.normal, 4),
            (self.tangent, 4),
        ];
        let shared = |raw: &[u8]| -> Vec<u8> {
            let mut raw = raw.to_vec();
            for (offset, len) in edited {
                raw[offset..offset + len].fill(0);
            }
            raw
        };
        let vertex =
            |i: usize| data.get(self.start + self.stride * i..self.start + self.stride * (i + 1));
        let first = vertex(0).context("no vertices")?;
        let want = shared(first);
        for i in 1..self.count {
            ensure!(
                shared(vertex(i).context("past the end")?) == want,
                "vertex {i} differs from the first outside the edited attributes"
            );
        }
        Ok(first.to_vec())
    }

    fn write(&self, out: &mut [u8], i: usize, v: &Vertex) -> Result<()> {
        self.write_at(out, self.start + self.stride * i, v)
    }

    /// `v`'s edited attributes written into the vertex at byte `at`.
    fn write_at(&self, out: &mut [u8], at: usize, v: &Vertex) -> Result<()> {
        let mut set = |offset: usize, bytes: &[u8]| {
            out[at + offset..at + offset + bytes.len()].copy_from_slice(bytes)
        };
        for (k, p) in v.position.iter().enumerate() {
            set(self.position + 2 * k, &to_half(*p)?.to_le_bytes());
        }
        for (k, u) in v.uv.iter().enumerate() {
            set(
                self.uv + 2 * k,
                &((u.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes(),
            );
        }
        set(self.normal, &v.normal.to_le_bytes());
        set(self.tangent, &v.tangent.to_le_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_hold_every_vertex_and_need_one() {
        let at = |position| Vertex {
            position,
            uv: [0.0; 2],
            normal: 0,
            tangent: 0,
        };
        assert_eq!(Bounds::of(&[]), None);
        let (a, b) = (at([0.0, -2.0, 1.0]), at([4.0, 2.0, 1.0]));
        assert_eq!(
            Bounds::of(&[&a, &b]),
            Some(Bounds {
                centre: [2.0, 0.0, 1.0],
                extent: [2.0, 2.0, 0.0],
                radius: 8f32.sqrt(),
            })
        );
    }

    #[test]
    fn an_attribute_search_reads_records_up_to_its_match() {
        // A vertex buffer header at 0 with two records, the second past
        // the end: the first, `_p0`, is found; any other name hits the
        // unreadable second and says so.
        let mut data = vec![0; 0x70];
        data[VERTEX_ATTRIBUTES..][..8].copy_from_slice(&0x60u64.to_le_bytes());
        data[VERTEX_ATTRIBUTE_COUNT] = 2;
        data[0x58..0x5d].copy_from_slice(b"\x03\0_p0");
        data[0x60..0x68].copy_from_slice(&0x58u64.to_le_bytes());
        data[0x68..0x6c].copy_from_slice(&FLOAT3.to_le_bytes());
        data[0x6c] = 4;
        let buffer = Buffer {
            shape: "s",
            start: 0,
            stride: 0,
            count: 0,
            vertex: 0,
        };
        assert_eq!(buffer.offset(&data, ("_p0", FLOAT3)).unwrap(), 4);
        let err = buffer.offset(&data, ("_n0", FLOAT3)).unwrap_err();
        assert!(err.to_string().contains("past the end"), "{err}");
    }
}
