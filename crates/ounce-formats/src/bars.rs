//! BARS sound banks: a file count at 0x0C, the crc32 of each file's name
//! (sorted), then each file's AMTA and BWAV offsets, the BWAV's -1 for a
//! file with no audio here.

use anyhow::{Result, ensure};

use crate::crc32::crc32;

const MAGIC: &[u8; 4] = b"BARS";
const HASHES: usize = 0x10;

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    let bytes = b
        .get(at..at + 4)
        .ok_or_else(|| anyhow::anyhow!("BARS cut short at {at:#x}"))?;
    Ok(u32::from_le_bytes(bytes.try_into()?))
}

/// The BWAV the bank holds for `name`, running to the bank's end (its
/// offsets are its own). None when the bank has no audio by that name.
pub fn bwav<'a>(bank: &'a [u8], name: &str) -> Result<Option<&'a [u8]>> {
    ensure!(bank.starts_with(MAGIC), "not a BARS");
    let count = u32_at(bank, 0x0C)? as usize;
    let hashes = (0..count)
        .map(|i| u32_at(bank, HASHES + 4 * i))
        .collect::<Result<Vec<_>>>()?;
    let Ok(i) = hashes.binary_search(&crc32(name.as_bytes())) else {
        return Ok(None);
    };
    let at = u32_at(bank, HASHES + 4 * count + 8 * i + 4)?;
    if at == u32::MAX {
        return Ok(None);
    }
    let data = bank.get(at as usize..).filter(|d| d.starts_with(b"BWAV"));
    ensure!(data.is_some(), "{name}'s BWAV isn't at {at:#x}");
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_finds_its_bwav_and_a_streamed_one_finds_none() {
        let (a, b) = (crc32(b"Fake_A"), crc32(b"Fake_B"));
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let mut bank = b"BARS\0\0\0\0\xff\xfe\x02\x01".to_vec();
        bank.extend(2u32.to_le_bytes());
        bank.extend([lo, hi].iter().flat_map(|h| h.to_le_bytes()));
        let wav = 0x10 + 8 + 16;
        let (lo_at, hi_at) = if a < b {
            (wav, u32::MAX)
        } else {
            (u32::MAX, wav)
        };
        for at in [lo_at, hi_at] {
            bank.extend(0u32.to_le_bytes());
            bank.extend(at.to_le_bytes());
        }
        bank.extend(b"BWAV rest");
        assert_eq!(bwav(&bank, "Fake_A").unwrap(), Some(&b"BWAV rest"[..]));
        assert_eq!(bwav(&bank, "Fake_B").unwrap(), None);
        assert_eq!(bwav(&bank, "Fake_C").unwrap(), None);
    }
}
