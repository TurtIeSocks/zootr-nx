//! LZ4's block format, the older NSO segment compression: runs of literals,
//! each but the last followed by a copy from earlier output.

use crate::{Result, corrupt};

/// Decodes the LZ4 block `src` into the `size` bytes it holds.
pub fn decode(src: &[u8], size: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(size);
    let mut input = Input { src, at: 0 };
    loop {
        let token = input.byte()?;
        let literals = input.length(token >> 4)?;
        out.extend_from_slice(input.take(literals)?);
        if input.at == src.len() {
            break;
        }
        let back = usize::from(u16::from_le_bytes([input.byte()?, input.byte()?]));
        let len = input.length(token & 15)? + 4;
        if back == 0 || back > out.len() {
            return Err(corrupt("LZ4 copy from before the start"));
        }
        if out.len() + len > size {
            return Err(corrupt("LZ4 block longer than its segment"));
        }
        // A copy may overlap what it writes, so byte by byte.
        let from = out.len() - back;
        for i in 0..len {
            out.push(out[from + i]);
        }
    }
    if out.len() != size {
        return Err(corrupt("LZ4 block's size differs from its segment's"));
    }
    Ok(out)
}

struct Input<'a> {
    src: &'a [u8],
    at: usize,
}

impl<'a> Input<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let run = self.src[self.at..]
            .get(..n)
            .ok_or_else(|| corrupt("LZ4 block ends early"))?;
        self.at += n;
        Ok(run)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    /// A length whose 4 bits are `short`: 15 means bytes follow to add,
    /// each 255 meaning one more.
    fn length(&mut self, short: u8) -> Result<usize> {
        let mut n = usize::from(short);
        if short == 15 {
            loop {
                let b = self.byte()?;
                n += usize::from(b);
                if b != 255 {
                    break;
                }
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_copies_decode() {
        // "abc", then 6 bytes from 3 back (overlapping), then "xyz".
        let block = [0x32, b'a', b'b', b'c', 3, 0, 0x30, b'x', b'y', b'z'];
        assert_eq!(decode(&block, 12).unwrap(), b"abcabcabcxyz");
    }

    #[test]
    fn long_lengths_take_extra_bytes() {
        // 15 + 5 literals, then 4 + 15 + 255 + 1 bytes of the last one.
        let mut block = vec![0xff, 5];
        block.extend(b"0123456789abcdefghij");
        block.extend([1, 0, 255, 1]);
        block.push(0x00);
        let out = decode(&block, 20 + 275).unwrap();
        assert_eq!(&out[..20], b"0123456789abcdefghij");
        assert!(out[20..].iter().all(|&b| b == b'j'));
    }

    #[test]
    fn bad_blocks_are_refused() {
        // A copy from before the start.
        assert!(decode(&[0x10, b'a', 2, 0, 0x00], 6).is_err());
        // Literals past the block's end.
        assert!(decode(&[0x50, b'a'], 5).is_err());
        // More bytes than the segment holds.
        assert!(decode(&[0x32, b'a', b'b', b'c', 3, 0, 0x30, b'x', b'y', b'z'], 11).is_err());
        // Fewer.
        assert!(decode(&[0x30, b'a', b'b', b'c'], 4).is_err());
    }
}
