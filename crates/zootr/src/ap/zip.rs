//! Reads the files of a zip as Archipelago writes them: stored or
//! deflated, no zip64, found through the central directory.

use std::collections::HashMap;
use std::io::Read;

use anyhow::{Context, Result, bail, ensure};

const END: u32 = 0x0605_4b50;
const ENTRY: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

fn u16_at(data: &[u8], at: usize) -> Result<usize> {
    let b = data.get(at..at + 2).context("the zip ends early")?;
    Ok(u16::from_le_bytes([b[0], b[1]]).into())
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    let b = data.get(at..at + 4).context("the zip ends early")?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Every file in the zip, by name.
pub fn files(data: &[u8]) -> Result<HashMap<String, Vec<u8>>> {
    // The end record sits in the last 22 bytes, before a comment of up to 64k.
    let first = data.len().saturating_sub(22 + 0xffff);
    let end = (first..=data.len().saturating_sub(22))
        .rev()
        .find(|&at| u32_at(data, at).is_ok_and(|s| s == END))
        .context("not a zip")?;
    let count = u16_at(data, end + 10)?;
    let mut at = u32_at(data, end + 16)? as usize;
    let mut out = HashMap::new();
    for _ in 0..count {
        ensure!(u32_at(data, at)? == ENTRY, "a broken zip directory");
        let method = u16_at(data, at + 10)?;
        let packed = u32_at(data, at + 20)? as usize;
        let size = u32_at(data, at + 24)? as usize;
        let (name_len, extra, comment) = (
            u16_at(data, at + 28)?,
            u16_at(data, at + 30)?,
            u16_at(data, at + 32)?,
        );
        let local = u32_at(data, at + 42)? as usize;
        let name = data
            .get(at + 46..at + 46 + name_len)
            .context("the zip ends early")?;
        let name = String::from_utf8(name.to_vec()).context("a zip file name isn't UTF-8")?;
        at += 46 + name_len + extra + comment;

        ensure!(
            u32_at(data, local)? == LOCAL,
            "a broken zip entry for {name}"
        );
        let start = local + 30 + u16_at(data, local + 26)? + u16_at(data, local + 28)?;
        let raw = data
            .get(start..start + packed)
            .context("the zip ends early")?;
        let file = match method {
            0 => raw.to_vec(),
            8 => {
                let mut file = Vec::with_capacity(size);
                flate2::read::DeflateDecoder::new(raw).read_to_end(&mut file)?;
                file
            }
            m => bail!("{name} is packed with method {m}, which isn't read"),
        };
        ensure!(file.len() == size, "{name} unpacked to the wrong size");
        out.insert(name, file);
    }
    Ok(out)
}
