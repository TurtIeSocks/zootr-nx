//! NSO, the Switch's executable format (an exefs's `main`, `rtld`, `sdk`):
//! a 0x100 byte header and three segments (text, rodata, data), each
//! stored plain, LZ4 or ZBIC, plus how much zeroed bss follows data.

use sha2::{Digest, Sha256};

use crate::{Error, Result, corrupt, le, u32_at};

const MAGIC: [u8; 4] = *b"NSO0";
const HEADER: usize = 0x100;
/// Segments marked compressed (flags bits 0 to 2) are ZBIC, not LZ4.
const ZBIC: u32 = 1 << 7;
/// MOD0, the module header the text's second word points at.
const MOD0: [u8; 4] = *b"MOD0";

/// The segments' names, in the header's order.
pub const SEGMENTS: [&str; 3] = ["text", "rodata", "data"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    None,
    Lz4,
    Zbic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    /// Where its stored bytes start in the file.
    pub offset: u32,
    /// Where it goes in memory, from the module's start.
    pub address: u32,
    pub size: u32,
    /// Its length in the file, compressed or not.
    pub stored: u32,
    pub compression: Compression,
    /// The SHA-256 of its decompressed bytes, if the header asks for a check.
    pub sha256: Option<[u8; 32]>,
}

impl Segment {
    pub fn end(&self) -> u64 {
        u64::from(self.address) + u64::from(self.size)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nso {
    pub flags: u32,
    /// The module's build ID, zero padded to 32 bytes.
    pub build_id: [u8; 32],
    pub segments: [Segment; 3],
    /// Zeroed bytes after data's end.
    pub bss: u32,
}

impl Nso {
    /// Reads the header at the start of `file`, checking each segment lies
    /// inside it and that they don't overlap in memory.
    pub fn parse(file: &[u8]) -> Result<Self> {
        if file.len() < HEADER || file[..4] != MAGIC {
            return Err(corrupt("not an NSO"));
        }
        let flags = u32_at(file, 0x0C);
        let segments: [Segment; 3] = std::array::from_fn(|i| {
            let at = 0x10 + 0x10 * i;
            Segment {
                offset: u32_at(file, at),
                address: u32_at(file, at + 4),
                size: u32_at(file, at + 8),
                stored: u32_at(file, 0x60 + 4 * i),
                compression: match ((flags >> i) & 1 != 0, flags & ZBIC != 0) {
                    (false, _) => Compression::None,
                    (true, false) => Compression::Lz4,
                    (true, true) => Compression::Zbic,
                },
                sha256: ((flags >> (3 + i)) & 1 != 0).then(|| le(file, 0xA0 + 0x20 * i)),
            }
        });
        for (s, name) in segments.iter().zip(SEGMENTS) {
            let end = u64::from(s.offset) + u64::from(s.stored);
            if end > file.len() as u64 {
                return Err(corrupt(format!("{name} segment past the file's end")));
            }
            if s.compression == Compression::None && s.stored != s.size {
                return Err(corrupt(format!("{name} stored plain at another size")));
            }
        }
        if segments
            .windows(2)
            .any(|w| w[0].end() > u64::from(w[1].address))
        {
            return Err(corrupt("segments out of order in memory"));
        }
        Ok(Self {
            flags,
            build_id: le(file, 0x40),
            segments,
            bss: u32_at(file, 0x3C),
        })
    }

    /// The module's size in memory: its segments and bss.
    pub fn end(&self) -> u64 {
        self.segments[2].end() + u64::from(self.bss)
    }

    /// Segment `i` of `file`, decompressed and checked against its hash.
    pub fn segment(&self, file: &[u8], i: usize) -> Result<Vec<u8>> {
        let s = &self.segments[i];
        let stored = file
            .get(s.offset as usize..)
            .and_then(|f| f.get(..s.stored as usize))
            .ok_or_else(|| corrupt(format!("{} segment past the file's end", SEGMENTS[i])))?;
        let size = s.size as usize;
        let bytes = match s.compression {
            Compression::None => stored.to_vec(),
            Compression::Lz4 => crate::lz4::decode(stored, size)?,
            Compression::Zbic => zbic::decode(stored, size)
                .map_err(|e| corrupt(format!("{} segment: {e}", SEGMENTS[i])))?,
        };
        if bytes.len() != size {
            return Err(corrupt(format!(
                "{} segment decodes to {} bytes, not {size}",
                SEGMENTS[i],
                bytes.len()
            )));
        }
        if s.sha256.is_some_and(|h| Sha256::digest(&bytes)[..] != h) {
            return Err(Error::Hash(SEGMENTS[i].into()));
        }
        Ok(bytes)
    }

    /// The module as it lies in memory: each segment at its address, the
    /// gaps between them and bss zeroed.
    pub fn image(&self, file: &[u8]) -> Result<Vec<u8>> {
        let mut image = vec![0; crate::size(self.end())?];
        for (i, s) in self.segments.iter().enumerate() {
            image[s.address as usize..][..s.size as usize].copy_from_slice(&self.segment(file, i)?);
        }
        Ok(image)
    }

    /// Where MOD0 is in `image`, as text's second word names it.
    pub fn mod0(&self, image: &[u8]) -> Result<u32> {
        let text = self.segments[0].address;
        let word = image
            .get(text as usize + 4..text as usize + 8)
            .ok_or_else(|| corrupt("no text"))?;
        let at = text
            .checked_add(u32_at(word, 0))
            .ok_or_else(|| corrupt("MOD0 past the image"))?;
        match image.get(at as usize..at as usize + 4) {
            Some(m) if m == MOD0 => Ok(at),
            _ => Err(corrupt(format!("no MOD0 at {at:#x}"))),
        }
    }
}

/// A build ID in hex, trailing zeros left out: how exefs patches and
/// loaders name the build they fit.
pub fn build_id_hex(id: &[u8; 32]) -> String {
    let used = id.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    id[..used].iter().map(|b| format!("{b:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An NSO of `text`, `rodata` and `data` laid out one after another at
    /// 0x10 apart in memory with 8 bytes of bss, each stored as given
    /// (`flags` saying how) and hashed.
    fn build(flags: u32, stored: [&[u8]; 3], sizes: [u32; 3]) -> Vec<u8> {
        let mut file = vec![0; HEADER];
        file[..4].copy_from_slice(&MAGIC);
        file[0x0C..0x10].copy_from_slice(&flags.to_le_bytes());
        file[0x3C..0x40].copy_from_slice(&8u32.to_le_bytes());
        file[0x40..0x44].copy_from_slice(&[0xe0, 0xf3, 0xcc, 0xfa]);
        for (i, s) in stored.iter().enumerate() {
            let words = [file.len() as u32, 0x10 * i as u32, sizes[i]];
            for (j, w) in words.iter().enumerate() {
                let at = 0x10 + 0x10 * i + 4 * j;
                file[at..at + 4].copy_from_slice(&w.to_le_bytes());
            }
            file[0x60 + 4 * i..][..4].copy_from_slice(&(s.len() as u32).to_le_bytes());
            file.extend_from_slice(s);
        }
        file
    }

    fn hash(file: &mut [u8], i: usize, plain: &[u8]) {
        file[0xA0 + 0x20 * i..][..0x20].copy_from_slice(&Sha256::digest(plain));
    }

    /// Text whose second word points at MOD0 at 8.
    const TEXT: &[u8] = b"\0\0\0\0\x08\0\0\0MOD0";

    #[test]
    fn plain_segments_lie_at_their_addresses() {
        let mut file = build(0b111_000, [TEXT, b"ro", b"data"], [12, 2, 4]);
        hash(&mut file, 0, TEXT);
        hash(&mut file, 1, b"ro");
        hash(&mut file, 2, b"data");
        let nso = Nso::parse(&file).unwrap();
        assert_eq!(nso.build_id[..4], [0xe0, 0xf3, 0xcc, 0xfa]);
        assert_eq!(build_id_hex(&nso.build_id), "E0F3CCFA");
        assert_eq!(nso.end(), 0x20 + 4 + 8);
        let image = nso.image(&file).unwrap();
        assert_eq!(image.len(), 0x2c);
        assert_eq!(&image[..12], TEXT);
        assert_eq!(&image[0x10..0x12], b"ro");
        assert_eq!(&image[0x20..0x24], b"data");
        assert!(image[0x24..].iter().all(|&b| b == 0));
        assert_eq!(nso.mod0(&image).unwrap(), 8);
    }

    #[test]
    fn compressed_segments_decode_by_their_flag() {
        // rodata as LZ4: "ab" then 4 bytes from 2 back.
        let lz4: &[u8] = &[0x20, b'a', b'b', 2, 0, 0x00];
        let file = build(0b010, [TEXT, lz4, b""], [12, 6, 0]);
        let nso = Nso::parse(&file).unwrap();
        assert_eq!(nso.segments[1].compression, Compression::Lz4);
        assert_eq!(&nso.image(&file).unwrap()[0x10..0x16], b"ababab");

        // The same flag with ZBIC's: a frame of one raw block.
        let zbic: &[u8] = &[b'Z', b'B', b'I', b'C', 0x20, 6, 0x31, 0, 0];
        let mut frame = zbic.to_vec();
        frame.extend(b"ababab");
        let file = build(0b1000_0010, [TEXT, &frame, b""], [12, 6, 0]);
        let nso = Nso::parse(&file).unwrap();
        assert_eq!(nso.segments[1].compression, Compression::Zbic);
        assert_eq!(&nso.image(&file).unwrap()[0x10..0x16], b"ababab");
    }

    #[test]
    fn bad_files_are_refused() {
        assert!(Nso::parse(b"NSO0").is_err());
        let mut file = build(0b001_000, [TEXT, b"", b""], [12, 0, 0]);
        // A hash that doesn't match.
        assert!(matches!(
            Nso::parse(&file).unwrap().image(&file),
            Err(Error::Hash(_))
        ));
        // A segment past the file's end.
        file[0x60..0x64].copy_from_slice(&99u32.to_le_bytes());
        assert!(Nso::parse(&file).is_err());
        // Plain, at another size than its own.
        let file = build(0, [TEXT, b"ro", b""], [12, 3, 0]);
        assert!(Nso::parse(&file).is_err());
        // Overlapping text.
        let file = build(0, [&[0; 0x14], b"", b""], [0x14, 0, 0]);
        assert!(Nso::parse(&file).is_err());
    }
}
