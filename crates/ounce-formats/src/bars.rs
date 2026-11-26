//! BARS sound banks: a file count at 0x0C, the crc32 of each file's name
//! (sorted), then each file's AMTA and BWAV offsets, the BWAV's -1 for a
//! file with no audio here. The AMTAs follow in that order, then the BWAVs,
//! each at a multiple of 64. An AMTA ends with its file's name, at 0x24
//! plus the offset there: the name of the stream the file plays, when its
//! BWAV is only a prefetch (flag 1 at 0x0C).

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

/// One file of a bank: its name's crc32, its AMTA's offset and its BWAV's
/// (`u32::MAX` when its audio isn't here).
struct File {
    hash: u32,
    amta: usize,
    wav: u32,
}

struct Bank<'a> {
    data: &'a [u8],
    files: Vec<File>,
    /// Where every AMTA and BWAV starts, and the bank's end, in order.
    starts: Vec<usize>,
}

impl<'a> Bank<'a> {
    fn new(data: &'a [u8]) -> Result<Self> {
        ensure!(data.starts_with(MAGIC), "not a BARS");
        let count = u32_at(data, 0x0C)? as usize;
        let table = HASHES + 4 * count;
        let files = (0..count)
            .map(|i| {
                Ok(File {
                    hash: u32_at(data, HASHES + 4 * i)?,
                    amta: u32_at(data, table + 8 * i)? as usize,
                    wav: u32_at(data, table + 8 * i + 4)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut starts: Vec<usize> = files.iter().map(|f| f.amta).collect();
        starts.extend(
            files
                .iter()
                .filter(|f| f.wav != u32::MAX)
                .map(|f| f.wav as usize),
        );
        starts.push(data.len());
        starts.sort();
        Ok(Self {
            data,
            files,
            starts,
        })
    }

    /// The bytes from `at` to the next file's start.
    fn blob(&self, at: usize) -> Result<&'a [u8]> {
        let end = self.starts[self.starts.partition_point(|&s| s <= at)];
        (self.data.get(at..end)).ok_or_else(|| anyhow::anyhow!("BARS cut short at {at:#x}"))
    }

    fn wav(&self, file: &File) -> Result<Option<&'a [u8]>> {
        if file.wav == u32::MAX {
            return Ok(None);
        }
        let data = self.blob(file.wav as usize)?;
        ensure!(data.starts_with(b"BWAV"), "no BWAV at {:#x}", file.wav);
        Ok(Some(data))
    }
}

/// The BWAV the bank holds for `name`, to the next file (a BWAV's offsets
/// are its own). None when the bank has no audio by that name.
pub fn bwav<'a>(bank: &'a [u8], name: &str) -> Result<Option<&'a [u8]>> {
    let bank = Bank::new(bank)?;
    match (bank.files).binary_search_by_key(&crc32(name.as_bytes()), |f| f.hash) {
        Ok(i) => bank.wav(&bank.files[i]),
        Err(_) => Ok(None),
    }
}

/// Every file's name, from its AMTA, in the bank's order.
pub fn names(bank: &[u8]) -> Result<Vec<String>> {
    let bank = Bank::new(bank)?;
    (bank.files.iter())
        .map(|f| {
            let amta = bank.blob(f.amta)?;
            let at = 0x24 + u32_at(amta, 0x24)? as usize;
            let name = amta.get(at..).unwrap_or_default().split(|&b| b == 0).next();
            Ok(String::from_utf8_lossy(name.unwrap_or_default()).into_owned())
        })
        .collect()
}

/// A file's new audio: another file's BWAV, and that file's name, so a
/// prefetch streams the rest of its own clip.
#[derive(Clone, Copy)]
pub struct Swap<'a> {
    pub wav: &'a [u8],
    pub name: &'a str,
}

/// The bank with some files' audio swapped: `new` gives a file, by its
/// name's crc32, a [`Swap`], or None to keep its own. Each file keeps its
/// place and its hash, which the game looks it up by.
pub fn rebuild<'a>(bank: &'a [u8], new: impl Fn(u32) -> Option<Swap<'a>>) -> Result<Vec<u8>> {
    let bank = Bank::new(bank)?;
    let table = HASHES + 4 * bank.files.len();
    let first = (bank.files.iter().map(|f| f.amta).min()).unwrap_or(table + 8 * bank.files.len());
    let mut out = bank.data[..first].to_vec();
    let mut amtas = vec![];
    for file in &bank.files {
        amtas.push(out.len() as u32);
        let size = u32_at(bank.data, file.amta + 8)? as usize;
        let amta = &bank.blob(file.amta)?[..size];
        let Some(swap) = new(file.hash) else {
            out.extend_from_slice(amta);
            continue;
        };
        let start = out.len();
        out.extend_from_slice(&amta[..0x24 + u32_at(amta, 0x24)? as usize]);
        out.extend_from_slice(swap.name.as_bytes());
        out.push(0);
        out.resize(out.len().next_multiple_of(4), 0);
        let size = (out.len() - start) as u32;
        out[start + 8..][..4].copy_from_slice(&size.to_le_bytes());
    }
    let mut wavs = vec![];
    for file in &bank.files {
        let Some(own) = bank.wav(file)? else {
            wavs.push(u32::MAX);
            continue;
        };
        out.resize(out.len().next_multiple_of(64), 0);
        wavs.push(out.len() as u32);
        out.extend_from_slice(new(file.hash).map_or(own, |s| s.wav));
    }
    for (i, (amta, wav)) in amtas.into_iter().zip(wavs).enumerate() {
        out[table + 8 * i..][..4].copy_from_slice(&amta.to_le_bytes());
        out[table + 8 * i + 4..][..4].copy_from_slice(&wav.to_le_bytes());
    }
    let size = out.len() as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(out)
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

    /// A bank of `files` (name, BWAV), each AMTA ending with its name,
    /// laid out as the game's are.
    fn fake_bank(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut files: Vec<_> = (files.iter())
            .map(|(n, w)| (crc32(n.as_bytes()), *n, *w))
            .collect();
        files.sort();
        let n = files.len();
        let mut bank = b"BARS\0\0\0\0\xff\xfe\x02\x01".to_vec();
        bank.extend((n as u32).to_le_bytes());
        bank.extend(files.iter().flat_map(|(h, ..)| h.to_le_bytes()));
        let table = bank.len();
        bank.resize(table + 8 * n, 0);
        for (i, (_, name, _)) in files.iter().enumerate() {
            let at = bank.len();
            bank[table + 8 * i..][..4].copy_from_slice(&(at as u32).to_le_bytes());
            bank.extend(b"AMTA\xff\xfe\0\x05");
            bank.resize(at + 0x24, 0);
            bank.extend(4u32.to_le_bytes());
            bank.extend(name.as_bytes());
            bank.push(0);
            bank.resize(bank.len().next_multiple_of(4), 0);
            let size = (bank.len() - at) as u32;
            bank[at + 8..][..4].copy_from_slice(&size.to_le_bytes());
        }
        for (i, (.., wav)) in files.iter().enumerate() {
            bank.resize(bank.len().next_multiple_of(64), 0);
            let here = bank.len() as u32;
            bank[table + 8 * i + 4..][..4].copy_from_slice(&here.to_le_bytes());
            bank.extend_from_slice(wav);
        }
        let size = bank.len() as u32;
        bank[4..8].copy_from_slice(&size.to_le_bytes());
        bank
    }

    #[test]
    fn a_swap_gives_one_file_new_audio_and_a_name_and_the_rest_stay() {
        let bank = fake_bank(&[("Fake_A", b"BWAV short"), ("Fake_B", b"BWAV b")]);
        let long = b"BWAV a much longer take than before".to_vec();
        let a = crc32(b"Fake_A");
        let swap = Swap {
            wav: &long,
            name: "Fake_Donor_With_A_Long_Name",
        };
        let out = rebuild(&bank, |h| (h == a).then_some(swap)).unwrap();
        // A blob runs to the next one, its padding with it.
        assert!(bwav(&out, "Fake_A").unwrap().unwrap().starts_with(&long));
        assert!(
            bwav(&out, "Fake_B")
                .unwrap()
                .unwrap()
                .starts_with(b"BWAV b")
        );
        let mut names = names(&out).unwrap();
        names.sort();
        assert_eq!(names, ["Fake_B", "Fake_Donor_With_A_Long_Name"]);
        assert_eq!(u32_at(&out, 4).unwrap() as usize, out.len());
        let unchanged = rebuild(&bank, |_| None).unwrap();
        assert_eq!(
            unchanged, bank,
            "a bank given nothing new comes back as it was"
        );
    }
}
