//! SARC archives written back with one file replaced. Names, hashes and
//! order stay as they were, so the SFAT stays sorted, and every file keeps
//! the alignment its old offset had.

use anyhow::{Context, Result, ensure};

use crate::reader::Reader;

/// A file's SFAT node: where it sits, and its data's start and end past
/// the data offset.
struct Node {
    at: usize,
    start: usize,
    end: usize,
}

/// `sarc` with the file at `name` holding `data`. Files stored after it
/// move by as much as keeps each on its old alignment.
pub fn replace_file(sarc: &[u8], name: &str, data: &[u8]) -> Result<Vec<u8>> {
    let r = Reader(sarc);
    ensure!(r.bytes(0, 4)? == b"SARC", "not a SARC archive");
    ensure!(r.u16(6)? == 0xfeff, "big-endian SARC isn't supported");
    let base = r.u32(0x0c)? as usize;
    let sfat = r.u16(4)? as usize;
    ensure!(r.bytes(sfat, 4)? == b"SFAT", "SARC has no SFAT");
    let count = r.u16(sfat + 6)? as usize;
    let first = sfat + r.u16(sfat + 4)? as usize;
    let sfnt = first + count * 16;
    ensure!(r.bytes(sfnt, 4)? == b"SFNT", "SARC has no SFNT");
    let names = sfnt + r.u16(sfnt + 4)? as usize;
    let mut nodes = Vec::new();
    let mut target = None;
    for at in (first..sfnt).step_by(16) {
        let attrs = r.u32(at + 4)? as usize;
        if attrs >> 24 != 0 && cstr(&r, names + (attrs & 0xff_ffff) * 4)? == name.as_bytes() {
            target = Some(at);
        }
        let (start, end) = (r.u32(at + 8)? as usize, r.u32(at + 12)? as usize);
        nodes.push(Node { at, start, end });
    }
    let target = target.with_context(|| format!("SARC has no {name}"))?;
    nodes.sort_by_key(|n| n.start);
    let mut out = r.bytes(0, base)?.to_vec();
    let mut tail = 0;
    for n in nodes {
        let align = 1 << (base + n.start).trailing_zeros();
        out.resize(out.len().next_multiple_of(align), 0);
        let start = out.len() - base;
        match n.at == target {
            true => out.extend(data),
            false => out.extend(r.bytes(base + n.start, n.end.saturating_sub(n.start))?),
        }
        let end = out.len() - base;
        out[n.at + 8..n.at + 12].copy_from_slice(&(start as u32).to_le_bytes());
        out[n.at + 12..n.at + 16].copy_from_slice(&(end as u32).to_le_bytes());
        tail = tail.max(n.end);
    }
    out.extend(sarc.get(base + tail..).context("SARC ends inside a file")?);
    let size = (out.len() as u32).to_le_bytes();
    out[8..12].copy_from_slice(&size);
    Ok(out)
}

/// The null-terminated bytes at `at`.
fn cstr<'a>(r: &Reader<'a>, at: usize) -> Result<&'a [u8]> {
    let rest = r.0.get(at..).context("name past the end")?;
    let len = rest
        .iter()
        .position(|&b| b == 0)
        .context("unterminated name")?;
    Ok(&rest[..len])
}

#[cfg(test)]
mod tests {
    use roead::sarc::{Sarc, SarcWriter};

    use super::*;

    /// Files with sizes that leave gaps, and one aligned wider than the rest.
    fn sample() -> Vec<u8> {
        SarcWriter::new(roead::Endian::Little)
            .with_min_alignment(8)
            .with_file("a.msbt", vec![1; 13])
            .with_file("b.bfres", vec![2; 7])
            .with_file("c.msbt", vec![3; 30])
            .to_binary()
    }

    /// Each file's name and offset in `data`.
    fn offsets(data: &[u8]) -> Vec<(String, usize)> {
        let sarc = Sarc::new(data).unwrap();
        let files = sarc.files().map(|f| {
            let at = f.data().as_ptr() as usize - data.as_ptr() as usize;
            (f.name().unwrap().to_owned(), at)
        });
        files.collect()
    }

    #[test]
    fn an_unchanged_file_writes_back_byte_for_byte() {
        let data = sample();
        let same = Sarc::new(&data)
            .unwrap()
            .get_data("a.msbt")
            .unwrap()
            .to_vec();
        assert_eq!(replace_file(&data, "a.msbt", &same).unwrap(), data);
    }

    #[test]
    fn later_files_move_and_keep_their_alignment() {
        let data = sample();
        let new = replace_file(&data, "a.msbt", &[9; 100]).unwrap();
        let sarc = Sarc::new(&new).unwrap();
        assert_eq!(sarc.get_data("a.msbt").unwrap(), [9; 100]);
        assert_eq!(sarc.get_data("b.bfres").unwrap(), [2; 7]);
        assert_eq!(sarc.get_data("c.msbt").unwrap(), [3; 30]);
        for ((name, old), (_, now)) in offsets(&data).into_iter().zip(offsets(&new)) {
            let align = 1usize << old.trailing_zeros();
            assert_eq!(now % align, 0, "{name} moved off its alignment");
        }
        assert!(replace_file(&data, "d.msbt", &[]).is_err());
    }
}
