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
            // Deflate packs at most 1032 to one.
            8 => read_capped(
                flate2::read::DeflateDecoder::new(raw),
                size,
                raw.len().saturating_mul(1032),
            )?,
            m => bail!("{name} is packed with method {m}, which isn't read"),
        };
        ensure!(file.len() == size, "{name} unpacked to the wrong size");
        out.insert(name, file);
    }
    Ok(out)
}

/// Read `from` up to one byte past `size`. The sizes come from the file,
/// so a lying one stops there, which is enough to call it wrong, instead
/// of filling memory. `most` bounds what's reserved up front.
fn read_capped(from: impl Read, size: usize, most: usize) -> Result<Vec<u8>> {
    let mut file = Vec::with_capacity(size.min(most));
    from.take(size as u64 + 1).read_to_end(&mut file)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// A zip of one file named `a`, as `method` packs `packed`, claiming
    /// `size` bytes unpacked.
    fn one_file(method: u16, packed: &[u8], size: u32) -> Vec<u8> {
        let le16 = |v: u16| v.to_le_bytes();
        let le32 = |v: u32| v.to_le_bytes();
        let len = u32::try_from(packed.len()).unwrap();
        let mut zip = Vec::new();
        zip.extend(le32(LOCAL));
        zip.extend([0; 4]);
        zip.extend(le16(method));
        zip.extend([0; 8]);
        zip.extend(le32(len));
        zip.extend(le32(size));
        zip.extend(le16(1));
        zip.extend(le16(0));
        zip.push(b'a');
        zip.extend(packed);
        let directory = u32::try_from(zip.len()).unwrap();
        zip.extend(le32(ENTRY));
        zip.extend([0; 6]);
        zip.extend(le16(method));
        zip.extend([0; 8]);
        zip.extend(le32(len));
        zip.extend(le32(size));
        zip.extend(le16(1));
        zip.extend([0; 12]);
        zip.extend(le32(0));
        zip.push(b'a');
        let directory_len = u32::try_from(zip.len()).unwrap() - directory;
        zip.extend(le32(END));
        zip.extend([0; 4]);
        zip.extend(le16(1));
        zip.extend(le16(1));
        zip.extend(le32(directory_len));
        zip.extend(le32(directory));
        zip.extend(le16(0));
        zip
    }

    fn deflate(data: &[u8]) -> Vec<u8> {
        let mut enc =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn a_stored_and_a_deflated_file_read_back() {
        let stored = files(&one_file(0, b"hi", 2)).unwrap();
        assert_eq!(stored["a"], b"hi");
        let data = [7u8; 300];
        let deflated = files(&one_file(8, &deflate(&data), 300)).unwrap();
        assert_eq!(deflated["a"], data);
    }

    #[test]
    fn a_zip_pointing_past_its_end_is_refused() {
        let zip = one_file(0, b"hi", 2);
        let directory = zip.len() - 22 - 47;
        let past = u32::try_from(zip.len()).unwrap().to_le_bytes();
        // The directory, a packed size and a local header, each past the end.
        for at in [zip.len() - 6, directory + 20, directory + 42] {
            let mut bad = zip.clone();
            bad[at..at + 4].copy_from_slice(&past);
            let message = format!("{:#}", files(&bad).unwrap_err());
            assert!(message.contains("ends early"), "field at {at}: {message}");
        }
        // Cut anywhere, the end record goes with it.
        for len in 0..zip.len() {
            assert!(files(&zip[..len]).is_err(), "cut at {len}");
        }
    }

    #[test]
    fn a_file_that_unpacks_past_its_size_is_refused() {
        let zip = one_file(8, &deflate(&[7u8; 4096]), 10);
        let message = format!("{:#}", files(&zip).unwrap_err());
        assert!(message.contains("wrong size"), "{message}");
    }

    /// Endless sevens, failing once asked for more than `most`.
    struct Endless {
        given: usize,
        most: usize,
    }

    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.given += buf.len();
            if self.given > self.most {
                return Err(std::io::Error::other("read past the cap"));
            }
            buf.fill(7);
            Ok(buf.len())
        }
    }

    #[test]
    fn a_read_stops_one_byte_past_its_size() {
        let file = read_capped(Endless { given: 0, most: 11 }, 10, usize::MAX).unwrap();
        assert_eq!(file.len(), 11);
    }
}
