//! Finding shapes and reading the pool strings that name things.

use anyhow::{Context, Result};

use super::{SHAPE_MESH_COUNT, SHAPE_NAME};
use crate::reader::Reader;

/// The offset of the shape named `shape`.
pub(super) fn shape_at(data: &[u8], shape: &str) -> Result<usize> {
    shapes(data)
        .find(|&at| shape_name(data, at) == Some(shape.as_bytes()))
        .with_context(|| format!("no shape {shape}"))
}

/// Every shape's name.
pub fn shape_names(data: &[u8]) -> Vec<String> {
    (shapes(data).filter_map(|s| shape_name(data, s)))
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

/// Every shape header's offset.
pub(super) fn shapes(data: &[u8]) -> impl Iterator<Item = usize> + '_ {
    // The furthest field any reader takes is the mesh count byte.
    find_magic(data, b"FSHP", SHAPE_MESH_COUNT + 1, 8)
}

pub(super) fn shape_name(data: &[u8], shape: usize) -> Option<&[u8]> {
    name_at(data, shape + SHAPE_NAME)
}

/// Every offset, a multiple of `align`, where `magic` starts a header
/// that fits in `data`: one that needs `size` bytes from its start,
/// the magic included.
pub(crate) fn find_magic<'a>(
    data: &'a [u8],
    magic: &'a [u8; 4],
    size: usize,
    align: usize,
) -> impl Iterator<Item = usize> + 'a {
    let end = data.len().checked_sub(size).map_or(0, |last| last + 1);
    (0..end)
        .step_by(align)
        .filter(move |&at| data.get(at..at + 4) == Some(magic))
}

/// The pool string the pointer at `at` points to, `None` when the
/// pointer can't be read either.
pub(crate) fn name_at(data: &[u8], at: usize) -> Option<&[u8]> {
    try_name_at(data, at).ok().flatten()
}

/// The pool string the pointer at `at` points to: an error when the
/// pointer can't be read, `None` when it leads to no string.
pub(super) fn try_name_at(data: &[u8], at: usize) -> Result<Option<&[u8]>> {
    Ok(pool_string(data, Reader(data).ptr(at)?))
}

/// A string in the file's pool: its length first.
fn pool_string(data: &[u8], at: usize) -> Option<&[u8]> {
    let len = Reader(data).u16(at).ok()? as usize;
    data.get(at + 2..at + 2 + len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_magic_takes_a_header_that_just_fits() {
        let data = b"....FSHPxyFSHP";
        assert_eq!(find_magic(data, b"FSHP", 6, 1).collect::<Vec<_>>(), [4]);
        assert_eq!(find_magic(data, b"FSHP", 4, 1).collect::<Vec<_>>(), [4, 10]);
        assert_eq!(find_magic(data, b"FSHP", 15, 1).count(), 0);
    }
}
