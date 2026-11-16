//! Edits to a model file (BFRES v10): vertices edited in place, and a
//! shape's whole mesh replaced by one appended to the memory pool.

use anyhow::{Context, Result, bail, ensure};

/// Where the header keeps its first block (u16), the relocation table and
/// the file's size (u32s).
const FIRST_BLOCK: usize = 0x16;
const RELOCATIONS: usize = 0x18;
const FILE_SIZE: usize = 0x1c;
/// Where the header points at the memory pool's info: its size, then
/// where its buffers start.
const POOL_INFO: usize = 0xb0;
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
const U16_INDICES: usize = 1;
/// A vertex buffer header's attributes, their strides, where its data
/// starts in the pool, how many attributes and buffers, and vertices.
const VERTEX_ATTRIBUTES: usize = 0x08;
const VERTEX_SIZES: usize = 0x30;
const VERTEX_STRIDES: usize = 0x38;
const VERTEX_DATA: usize = 0x48;
const VERTEX_ATTRIBUTE_COUNT: usize = 0x4c;
const VERTEX_BUFFER_COUNT: usize = 0x4d;
const VERTEX_COUNT: usize = 0x50;
/// The attributes edited, by name and the one format each is read in:
/// four halves, two 16-bit unorms, and 10:10:10:2 kept as stored.
const POSITION: (&str, u32) = ("_p0", 0x1505);
const UV: (&str, u32) = ("_u0", 0x1201);
const NORMAL: (&str, u32) = ("_n0", 0xe02);
const TANGENT: (&str, u32) = ("_t0", 0xe02);

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
    let buffer = Buffer::find(data, shape)?;
    let position = buffer.offset(data, POSITION)?;
    (0..buffer.count)
        .map(|i| {
            let at = buffer.start + buffer.stride * i + position;
            let half =
                |k: usize| -> Result<f32> { Ok(from_half(u32_at(data, at + 2 * k)? as u16)) };
            Ok([half(0)?, half(1)?, half(2)?])
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

/// One level of detail of a shape: its vertices, and its triangles as
/// indices into them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lod {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// The levels of detail of the shape named `shape`, finest first.
pub fn lods(data: &[u8], shape: &str) -> Result<Vec<Lod>> {
    let layout = Layout::find(data, shape)?;
    let (_, pool, _) = pool(data)?;
    let meshes = Mesh::all(data, shape_at(data, shape)?)?;
    let lod = |m: &Mesh| -> Result<Lod> {
        let indices = (0..m.count)
            .map(|i| Ok(u32_at(data, pool + m.faces + 2 * i)? as u16))
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
    let at = shape_at(data, shape)?;
    let meshes = Mesh::all(data, at)?;
    ensure!(
        meshes.len() == lods.len(),
        "{shape}: {} levels of detail, not {}",
        meshes.len(),
        lods.len()
    );
    let template = layout.template(data)?;
    let (info, pool, size) = pool(data)?;
    let end = pool + size;

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
    let start = size + added.len();
    let vertices: Vec<&Vertex> = lods.iter().flat_map(|l| &l.vertices).collect();
    for v in &vertices {
        let mut raw = template.clone();
        layout.write_at(&mut raw, 0, v)?;
        added.extend(raw);
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

    let v = u64_at(data, at + SHAPE_VERTICES)?;
    put(&mut out, v + VERTEX_DATA, start)?;
    put(&mut out, v + VERTEX_COUNT, vertices.len())?;
    put(
        &mut out,
        u64_at(data, v + VERTEX_SIZES)?,
        vertices.len() * layout.stride,
    )?;
    let mut first = 0;
    for ((mesh, lod), faces) in meshes.iter().zip(lods).zip(faces) {
        let count = lod.indices.len();
        put(&mut out, mesh.at + MESH_FACES, faces)?;
        put(&mut out, mesh.at + MESH_COUNT, count)?;
        put(&mut out, mesh.at + MESH_FIRST, first)?;
        put(
            &mut out,
            u64_at(data, mesh.at + MESH_BUFFER_SIZE)?,
            2 * count,
        )?;
        // Its one submesh: where it starts in the indices, and how many.
        let submesh = u64_at(data, mesh.at + MESH_SUBMESHES)?;
        put(&mut out, submesh, 0)?;
        put(&mut out, submesh + 4, count)?;
        first += lod.vertices.len();
    }
    bound(&mut out, data, at, &meshes, &vertices)?;
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
        let meshes = u64_at(data, shape + SHAPE_MESHES)?;
        let count = *data.get(shape + SHAPE_MESH_COUNT).context("past the end")?;
        (0..count as usize)
            .map(|i| {
                let at = meshes + MESH_SIZE * i;
                ensure!(
                    u32_at(data, at + MESH_FORMAT)? == U16_INDICES,
                    "mesh {i}: not u16 indices"
                );
                let submeshes = u16::from_le_bytes([
                    data[at + MESH_SUBMESH_COUNT],
                    data[at + MESH_SUBMESH_COUNT + 1],
                ]);
                ensure!(submeshes == 1, "mesh {i}: {submeshes} submeshes");
                Ok(Self {
                    at,
                    faces: u32_at(data, at + MESH_FACES)?,
                    count: u32_at(data, at + MESH_COUNT)?,
                    first: u32_at(data, at + MESH_FIRST)?,
                })
            })
            .collect()
    }
}

/// Every bounding box of the shape at `shape` (each mesh's and its
/// submesh's) and its sphere set to hold all of `vertices`, so the
/// game never culls a part of it.
fn bound(
    out: &mut [u8],
    data: &[u8],
    shape: usize,
    meshes: &[Mesh],
    vertices: &[&Vertex],
) -> Result<()> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
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
    let floats = |at: usize, values: &[f32], out: &mut [u8]| {
        for (i, f) in values.iter().enumerate() {
            out[at + 4 * i..at + 4 * i + 4].copy_from_slice(&f.to_le_bytes());
        }
    };
    let boxes = u64_at(data, shape + SHAPE_BOXES)?;
    let box_ = [centre, extent].concat();
    for i in 0..2 * meshes.len() {
        floats(boxes + 24 * i, &box_, out);
    }
    let sphere = u64_at(data, shape + SHAPE_SPHERE)?;
    floats(sphere, &[centre[0], centre[1], centre[2], radius], out);
    Ok(())
}

/// The memory pool's info header, and where the pool starts and how big
/// it is.
fn pool(data: &[u8]) -> Result<(usize, usize, usize)> {
    let info = u64_at(data, POOL_INFO)?;
    Ok((info, u64_at(data, info + 8)?, u32_at(data, info + 4)?))
}

/// Every pointer the relocation table lists: where it is, and where it
/// points.
pub fn pointers(data: &[u8]) -> Result<Vec<(usize, usize)>> {
    let table = Table::find(data)?;
    let mut out = Vec::new();
    for e in 0..table.entries {
        let entry = table.entry(e);
        let structs = u16::from_le_bytes([data[entry + 4], data[entry + 5]]) as usize;
        let (count, padding) = (data[entry + 6] as usize, data[entry + 7] as usize);
        let mut p = u32_at(data, entry)?;
        for _ in 0..structs {
            for _ in 0..count {
                out.push((p, u64_at(data, p)?));
                p += 8;
            }
            p += 8 * padding;
        }
    }
    Ok(out)
}

/// The relocation table: where it is, how many sections and entries.
struct Table {
    at: usize,
    sections: usize,
    entries: usize,
}

impl Table {
    fn find(data: &[u8]) -> Result<Self> {
        let at = u32_at(data, RELOCATIONS)?;
        ensure!(data.get(at..at + 4) == Some(b"_RLT"), "no relocation table");
        let sections = u32_at(data, at + 8)?;
        let mut entries = 0;
        for s in 0..sections {
            let section = at + 0x10 + 0x18 * s;
            entries = entries.max(u32_at(data, section + 16)? + u32_at(data, section + 20)?);
        }
        Ok(Self {
            at,
            sections,
            entries,
        })
    }

    fn section(&self, s: usize) -> usize {
        self.at + 0x10 + 0x18 * s
    }

    fn entry(&self, e: usize) -> usize {
        self.at + 0x10 + 0x18 * self.sections + 8 * e
    }
}

/// Moves the relocation table's sections, entries and pointers for
/// `growth` bytes inserted at `at`. The table follows the pool, so it has
/// moved too: the header learns where.
fn relocate(out: &mut [u8], at: usize, growth: usize) -> Result<()> {
    let old = u32_at(out, RELOCATIONS)?;
    ensure!(old >= at, "the relocation table is before the pool's end");
    put(out, RELOCATIONS, old + growth)?;
    let table = Table::find(out)?;
    put(out, table.at + 4, table.at)?;
    for s in 0..table.sections {
        let section = table.section(s);
        let (offset, size) = (u32_at(out, section + 8)?, u32_at(out, section + 12)?);
        if offset >= at {
            put(out, section + 8, offset + growth)?;
        } else if offset + size >= at {
            put(out, section + 12, size + growth)?;
        }
    }
    for e in 0..table.entries {
        let location = u32_at(out, table.entry(e))?;
        if location >= at {
            put(out, table.entry(e), location + growth)?;
        }
    }
    for (p, to) in pointers(out)? {
        if to >= at {
            out[p..p + 8].copy_from_slice(&((to + growth) as u64).to_le_bytes());
        }
    }
    Ok(())
}

/// Each block whose span holds `at` grown by `growth`, and each link to a
/// block past it lengthened.
fn grow_blocks(out: &mut [u8], at: usize, growth: usize) -> Result<()> {
    let mut block = u16::from_le_bytes([out[FIRST_BLOCK], out[FIRST_BLOCK + 1]]) as usize;
    while block != 0 {
        let (next, size) = (u32_at(out, block + 4)?, u32_at(out, block + 8)?);
        if block < at && block + size >= at {
            put(out, block + 8, size + growth)?;
        }
        if next == 0 {
            break;
        }
        let to = block + next;
        if block < at && to >= at {
            put(out, block + 4, next + growth)?;
        }
        block = if to >= at { to + growth } else { to };
    }
    Ok(())
}

/// `out` padded with zeros to a multiple of `align`.
fn pad(out: &mut Vec<u8>, align: usize) {
    out.resize(out.len().next_multiple_of(align), 0);
}

/// `value` written as a u32 at `at`.
fn put(out: &mut [u8], at: usize, value: usize) -> Result<()> {
    let value = u32::try_from(value).context("past u32")?;
    out.get_mut(at..at + 4)
        .context("past the end")?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

/// The offset of the shape named `shape`.
fn shape_at(data: &[u8], shape: &str) -> Result<usize> {
    shapes(data)
        .find(|&at| shape_name(data, at) == Some(shape.as_bytes()))
        .with_context(|| format!("no shape {shape}"))
}

/// Where a shape's vertices are, and how to find each attribute.
struct Buffer<'a> {
    shape: &'a str,
    start: usize,
    stride: usize,
    count: usize,
    vertex: usize,
    attributes: usize,
}

impl<'a> Buffer<'a> {
    fn find(data: &[u8], shape: &'a str) -> Result<Self> {
        let at = shape_at(data, shape)?;
        let (_, pool, size) = pool(data)?;
        let pool_end = pool + size;
        let v = u64_at(data, at + SHAPE_VERTICES)?;
        ensure!(
            data.get(v + VERTEX_BUFFER_COUNT) == Some(&1),
            "{shape}: not one vertex buffer"
        );
        let buffer = Self {
            shape,
            start: pool + u32_at(data, v + VERTEX_DATA)?,
            stride: u32_at(data, u64_at(data, v + VERTEX_STRIDES)?)?,
            count: u32_at(data, v + VERTEX_COUNT)?,
            vertex: v,
            attributes: u64_at(data, v + VERTEX_ATTRIBUTES)?,
        };
        ensure!(
            buffer.start + buffer.stride * buffer.count <= pool_end.min(data.len()),
            "{shape}: vertices past the pool"
        );
        Ok(buffer)
    }

    /// Where attribute `name` sits in each vertex, if it's in `format`.
    fn offset(&self, data: &[u8], (name, format): (&str, u32)) -> Result<usize> {
        let shape = self.shape;
        for i in 0..data[self.vertex + VERTEX_ATTRIBUTE_COUNT] as usize {
            let a = self.attributes + 16 * i;
            if pool_string(data, u64_at(data, a)?) == Some(name.as_bytes()) {
                let found = u32_at(data, a + 8)? as u32;
                if found != format {
                    bail!("{shape}: {name} in format {found:#x}");
                }
                return Ok(u16::from_le_bytes([data[a + 12], data[a + 13]]) as usize);
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
        let at = self.start + self.stride * i;
        let half = |k: usize| -> Result<f32> {
            Ok(from_half(u32_at(data, at + self.position + 2 * k)? as u16))
        };
        let unorm = |k: usize| -> Result<f32> {
            Ok((u32_at(data, at + self.uv + 2 * k)? & 0xffff) as f32 / 65535.0)
        };
        Ok(Vertex {
            position: [half(0)?, half(1)?, half(2)?],
            uv: [unorm(0)?, unorm(1)?],
            normal: u32_at(data, at + self.normal)? as u32,
            tangent: u32_at(data, at + self.tangent)? as u32,
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
        let mut put = |offset: usize, bytes: &[u8]| {
            out[at + offset..at + offset + bytes.len()].copy_from_slice(bytes)
        };
        for (k, p) in v.position.iter().enumerate() {
            put(self.position + 2 * k, &to_half(*p)?.to_le_bytes());
        }
        for (k, u) in v.uv.iter().enumerate() {
            put(
                self.uv + 2 * k,
                &((u.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes(),
            );
        }
        put(self.normal, &v.normal.to_le_bytes());
        put(self.tangent, &v.tangent.to_le_bytes());
        Ok(())
    }
}

/// A normal or tangent as stored: x, y and z in 10 signed bits each, over
/// 511, and w (a tangent's handedness) in the top 2.
pub fn unpack(packed: u32) -> [f32; 4] {
    let signed = |shift: u32, bits: u32| {
        let v = (packed >> shift) & ((1 << bits) - 1);
        (v as i32) << (32 - bits) >> (32 - bits)
    };
    let xyz = |shift| (signed(shift, 10) as f32 / 511.0).max(-1.0);
    [xyz(0), xyz(10), xyz(20), signed(30, 2) as f32]
}

/// [`unpack`]'s inverse, each value rounded.
pub fn pack(v: [f32; 4]) -> u32 {
    let field = |f: f32, scale: f32, bits: u32| {
        let max = (1 << (bits - 1)) - 1;
        ((f * scale).round() as i32).clamp(-max, max) as u32 & ((1 << bits) - 1)
    };
    field(v[0], 511.0, 10)
        | field(v[1], 511.0, 10) << 10
        | field(v[2], 511.0, 10) << 20
        | field(v[3], 1.0, 2) << 30
}

/// A half float's value.
fn from_half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let (exp, mantissa) = ((h >> 10) & 0x1f, (h & 0x3ff) as f32);
    match exp {
        0 => sign * mantissa * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        e => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(e as i32 - 15),
    }
}

/// `f` as a half float, rounded. Too small for a normal half, it's a
/// subnormal one (steps of 2^-24, rounding up into the smallest normal);
/// too big, an error.
fn to_half(f: f32) -> Result<u16> {
    let bits = f.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    if f.abs() < 2f32.powi(-14) {
        return Ok(sign | (f.abs() * 2f32.powi(24)).round() as u16);
    }
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    ensure!(exp < 31, "{f} is past a half");
    let mantissa = bits & 0x7f_ffff;
    // A carry out of the mantissa rounds up into the exponent, as it should.
    Ok(sign | (((exp as u32) << 10) + (mantissa >> 13) + ((mantissa >> 12) & 1)) as u16)
}

/// Every shape's name.
pub fn shape_names(data: &[u8]) -> Vec<String> {
    (shapes(data).filter_map(|s| shape_name(data, s)))
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

/// Every shape header's offset.
fn shapes(data: &[u8]) -> impl Iterator<Item = usize> + '_ {
    (0..data.len().saturating_sub(SHAPE_VERTICES + 8))
        .step_by(8)
        .filter(|&at| &data[at..at + 4] == b"FSHP")
}

fn shape_name(data: &[u8], shape: usize) -> Option<&[u8]> {
    pool_string(data, u64_at(data, shape + SHAPE_NAME).ok()?)
}

/// A string in the file's pool: its length first.
pub(crate) fn pool_string(data: &[u8], at: usize) -> Option<&[u8]> {
    let len = u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize;
    data.get(at + 2..at + 2 + len)
}

pub(crate) fn u32_at(data: &[u8], at: usize) -> Result<usize> {
    let b = data.get(at..at + 4).context("past the end")?;
    Ok(u32::from_le_bytes(b.try_into()?) as usize)
}

pub(crate) fn u64_at(data: &[u8], at: usize) -> Result<usize> {
    let b = data.get(at..at + 8).context("past the end")?;
    Ok(u64::from_le_bytes(b.try_into()?) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_normals_round_trip() {
        for packed in [0x4000_0000_u32, 0x7fd8_6abc, 0xc19f_8241, 0x5fff_fdff] {
            assert_eq!(pack(unpack(packed)), packed, "{packed:#x}");
        }
        assert_eq!(unpack(pack([1.0, -1.0, 0.0, -1.0])), [1.0, -1.0, 0.0, -1.0]);
    }

    #[test]
    fn halves_round_trip() {
        for h in [
            0x0000, 0x3c00, 0x2283, 0xa283, 0x1c00, 0x7bff, 0x0001, 0x8314,
        ] {
            assert_eq!(to_half(from_half(h)).unwrap(), h, "{h:#x}");
        }
        assert_eq!(from_half(0x3c00), 1.0);
        assert_eq!(to_half(0.013).unwrap(), 0x22a8);
        assert_eq!(to_half(1e-6).unwrap(), 17);
        assert_eq!(to_half(-4.7e-5).unwrap(), 0x8000 | 789);
        assert_eq!(from_half(to_half(6.103e-5).unwrap()), 2f32.powi(-14));
        assert_eq!(to_half(1e-9).unwrap(), 0);
        assert!(to_half(70000.0).is_err());
    }
}
