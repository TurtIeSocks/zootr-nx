//! Edits to a model file (BFRES v10) that keep every offset in place.

use anyhow::{Context, Result, bail, ensure};

/// Where the header points at the memory pool's info: its size, then
/// where its buffers start.
const POOL_INFO: usize = 0xb0;
/// A shape's name and vertex buffer header.
const SHAPE_NAME: usize = 0x08;
const SHAPE_VERTICES: usize = 0x10;
/// A vertex buffer header's attributes, their strides, where its data
/// starts in the pool, how many attributes and buffers, and vertices.
const VERTEX_ATTRIBUTES: usize = 0x08;
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
/// normal and tangent as stored.
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
        let at = shapes(data)
            .find(|&at| shape_name(data, at) == Some(shape.as_bytes()))
            .with_context(|| format!("no shape {shape}"))?;
        let info = u64_at(data, POOL_INFO)?;
        let pool = u64_at(data, info + 8)?;
        let pool_end = pool + u32_at(data, info + 4)?;
        let v = u64_at(data, at + SHAPE_VERTICES)?;
        ensure!(
            data.get(v + VERTEX_BUFFER_COUNT) == Some(&1),
            "{shape}: not one vertex buffer"
        );
        let attributes = u64_at(data, v + VERTEX_ATTRIBUTES)?;
        let offset = |(name, format): (&str, u32)| -> Result<usize> {
            for i in 0..data[v + VERTEX_ATTRIBUTE_COUNT] as usize {
                let a = attributes + 16 * i;
                if shape_string(data, u64_at(data, a)?) == Some(name.as_bytes()) {
                    let found = u32_at(data, a + 8)? as u32;
                    if found != format {
                        bail!("{shape}: {name} in format {found:#x}");
                    }
                    return Ok(u16::from_le_bytes([data[a + 12], data[a + 13]]) as usize);
                }
            }
            bail!("{shape}: no {name}")
        };
        let layout = Self {
            start: pool + u32_at(data, v + VERTEX_DATA)?,
            stride: u32_at(data, u64_at(data, v + VERTEX_STRIDES)?)?,
            count: u32_at(data, v + VERTEX_COUNT)?,
            position: offset(POSITION)?,
            uv: offset(UV)?,
            normal: offset(NORMAL)?,
            tangent: offset(TANGENT)?,
        };
        ensure!(
            layout.start + layout.stride * layout.count <= pool_end.min(data.len()),
            "{shape}: vertices past the pool"
        );
        Ok(layout)
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

    fn write(&self, out: &mut [u8], i: usize, v: &Vertex) -> Result<()> {
        let at = self.start + self.stride * i;
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

/// `f` as a half float, rounded: zero or a normal half only.
fn to_half(f: f32) -> Result<u16> {
    if f == 0.0 {
        return Ok(0);
    }
    let bits = f.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    ensure!((1..31).contains(&exp), "{f} is no normal half");
    let mantissa = bits & 0x7f_ffff;
    // A carry out of the mantissa rounds up into the exponent, as it should.
    Ok(sign | (((exp as u32) << 10) + (mantissa >> 13) + ((mantissa >> 12) & 1)) as u16)
}

/// Every shape header's offset.
fn shapes(data: &[u8]) -> impl Iterator<Item = usize> + '_ {
    (0..data.len().saturating_sub(SHAPE_VERTICES + 8))
        .step_by(8)
        .filter(|&at| &data[at..at + 4] == b"FSHP")
}

fn shape_name(data: &[u8], shape: usize) -> Option<&[u8]> {
    shape_string(data, u64_at(data, shape + SHAPE_NAME).ok()?)
}

/// A string in the file's pool: its length first.
fn shape_string(data: &[u8], at: usize) -> Option<&[u8]> {
    let len = u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize;
    data.get(at + 2..at + 2 + len)
}

fn u32_at(data: &[u8], at: usize) -> Result<usize> {
    let b = data.get(at..at + 4).context("past the end")?;
    Ok(u32::from_le_bytes(b.try_into()?) as usize)
}

fn u64_at(data: &[u8], at: usize) -> Result<usize> {
    let b = data.get(at..at + 8).context("past the end")?;
    Ok(u64::from_le_bytes(b.try_into()?) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_round_trip() {
        for h in [0x0000, 0x3c00, 0x2283, 0xa283, 0x1c00, 0x7bff] {
            assert_eq!(to_half(from_half(h)).unwrap(), h, "{h:#x}");
        }
        assert_eq!(from_half(0x3c00), 1.0);
        assert_eq!(to_half(0.013).unwrap(), 0x22a8);
        assert!(to_half(1e-6).is_err());
    }
}
