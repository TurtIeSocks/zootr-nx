//! A decoder for ZBIC: zstd frames with the magic `ZBIC` whose FSE table
//! headers are written with binary interpolative coding, the compression
//! of NSO segments from Switch firmware 22 on.
//!
//! The decoding half of ruzstd 0.9.1, a pure Rust zstd decoder (MIT, see
//! `LICENSE` and `README.md`), with ZBIC's magic and table reader (`bic`)
//! in place of zstd's. [`decode`] reads a whole segment.
#![no_std]
#![deny(trivial_casts, trivial_numeric_casts, rust_2018_idioms)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

#[cfg(feature = "std")]
pub(crate) const VERBOSE: bool = false;

macro_rules! vprintln {
    ($($x:expr),*) => {
        #[cfg(feature = "std")]
        if crate::VERBOSE {
            std::println!($($x),*);
        }
    }
}

mod bit_io;
mod common;
pub mod decoding;

pub(crate) mod blocks;

mod bic;
pub(crate) mod fse;
pub(crate) mod huff0;

#[cfg(feature = "std")]
pub mod io_std;

#[cfg(feature = "std")]
pub use io_std as io;

#[cfg(not(feature = "std"))]
pub mod io_nostd;

#[cfg(not(feature = "std"))]
pub use io_nostd as io;

/// Decodes the ZBIC frames in `src`, which hold `size` bytes in all.
pub fn decode(
    src: &[u8],
    size: usize,
) -> Result<alloc::vec::Vec<u8>, decoding::errors::FrameDecoderError> {
    let mut out = alloc::vec![0; size];
    let n = decoding::FrameDecoder::new().decode_all(src, &mut out)?;
    out.truncate(n);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    /// A frame of one segment's worth (12 bytes) in three blocks: raw,
    /// repeated and compressed (its literals stored, no sequences).
    const FRAME: &[u8] = &[
        b'Z', b'B', b'I', b'C', 0x20, 12, // magic, single segment, 12 bytes
        0x28, 0, 0, b'h', b'e', b'l', b'l', b'o', // raw, 5 bytes
        0x22, 0, 0, b'x', // 4 of one byte
        0x2d, 0, 0, 0x18, b'a', b'b', b'c', 0, // last: compressed, 5 bytes
    ];

    #[test]
    fn a_frame_decodes() {
        assert_eq!(super::decode(FRAME, 12).unwrap(), b"helloxxxxabc");
        let mut twice = FRAME.to_vec();
        twice.extend(FRAME);
        assert_eq!(
            super::decode(&twice, 24).unwrap(),
            b"helloxxxxabchelloxxxxabc"
        );
    }

    #[test]
    fn zstd_frames_and_short_targets_are_refused() {
        let mut zstd = FRAME.to_vec();
        zstd[..4].copy_from_slice(&0xFD2F_B528u32.to_le_bytes());
        assert!(super::decode(&zstd, 12).is_err());
        assert!(super::decode(FRAME, 11).is_err());
    }

    /// The decoder's error for a block that decodes past the frame's
    /// window, which a single segment frame sets to its content size.
    fn over(frame: &[u8]) -> Option<crate::decoding::errors::DecompressBlockError> {
        use crate::decoding::errors::{DecodeBlockContentError, FrameDecoderError};
        match super::decode(frame, 100) {
            Err(FrameDecoderError::FailedToReadBlockBody(
                DecodeBlockContentError::DecompressBlockError(e),
            )) => Some(e),
            _ => None,
        }
    }

    #[test]
    fn literals_past_the_block_are_refused() {
        // 12 bytes at most; a compressed block of 13 repeated literals.
        let frame = [b'Z', b'B', b'I', b'C', 0x20, 12, 0x1d, 0, 0, 0x69, b'x', 0];
        assert!(matches!(
            over(&frame),
            Some(crate::decoding::errors::DecompressBlockError::DecompressedSizeTooLarge { .. })
        ));
    }

    /// A frame of `max` bytes whose one block is `literals` and one
    /// sequence: the first literal, then 3 bytes copied from 1 back. Each
    /// table is a single symbol: literal length code 1, offset code 0
    /// (the first repeat offset, 1) and match length code 0 (3 bytes).
    fn sequence(max: u8, literals: &[u8]) -> Vec<u8> {
        let size = literals.len() as u8 + 7;
        let mut frame = vec![b'Z', b'B', b'I', b'C', 0x20, max];
        frame.extend([size << 3 | 0b101, 0, 0, (literals.len() as u8) << 3]);
        frame.extend(literals);
        frame.extend([1, 0x54, 1, 0, 0, 0x01]);
        frame
    }

    #[test]
    fn literals_after_the_last_sequence_count_toward_the_block() {
        let fits = sequence(20, b"abcdefghijklmnopq");
        assert_eq!(super::decode(&fits, 100).unwrap(), b"aaaabcdefghijklmnopq");
        assert!(matches!(
            over(&sequence(20, b"abcdefghijklmnopqr")),
            Some(
                crate::decoding::errors::DecompressBlockError::ExecuteSequencesError(
                    crate::decoding::errors::ExecuteSequencesError::TooManyBytesGenerated { .. }
                )
            )
        ));
    }
}
