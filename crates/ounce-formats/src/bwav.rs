//! BWAV streams: the header (channels, rate, length, loop) and the
//! DSP-ADPCM samples. Each channel has a 0x4C-byte info block from 0x10:
//! codec, pan, rate, sample count, 16 coefficients at +0x10, its data's
//! offset at +0x30, and the loop's end and start at +0x3C and +0x40 (end
//! -1 for no loop).

use anyhow::{Result, ensure};

const MAGIC: &[u8; 4] = b"BWAV";
const INFO: usize = 0x10;
const INFO_SIZE: usize = 0x4C;
const DSP_ADPCM: u16 = 1;
/// Samples in one 8-byte ADPCM frame: a header byte, then 7 bytes of nibbles.
const FRAME_SAMPLES: usize = 14;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub channels: u16,
    pub rate: u32,
    pub samples: u32,
    /// Where a looping stream goes back to, in samples. None for a one-shot.
    pub loop_start: Option<u32>,
}

impl Header {
    pub fn seconds(&self) -> f32 {
        self.samples as f32 / self.rate as f32
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

/// The header, from the stream's first `0x10 + 0x4C` bytes at least.
pub fn header(b: &[u8]) -> Result<Header> {
    ensure!(
        b.len() >= INFO + INFO_SIZE && &b[..4] == MAGIC,
        "not a BWAV"
    );
    let channels = u16_at(b, 0x0E);
    ensure!(channels > 0, "a BWAV with no channels");
    let loop_end = u32_at(b, INFO + 0x3C) as i32;
    Ok(Header {
        channels,
        rate: u32_at(b, INFO + 4),
        samples: u32_at(b, INFO + 8),
        loop_start: (loop_end != -1).then(|| u32_at(b, INFO + 0x40)),
    })
}

/// Every channel's samples, decoded.
pub fn decode(b: &[u8]) -> Result<Vec<Vec<i16>>> {
    let head = header(b)?;
    ensure!(
        b.len() >= INFO + INFO_SIZE * head.channels as usize,
        "BWAV cut short"
    );
    (0..head.channels as usize)
        .map(|c| {
            let info = INFO + c * INFO_SIZE;
            ensure!(u16_at(b, info) == DSP_ADPCM, "channel {c} isn't DSP-ADPCM");
            let coefs: Vec<i32> = (0..16)
                .map(|i| u16_at(b, info + 0x10 + 2 * i) as i16 as i32)
                .collect();
            let data = u32_at(b, info + 0x30) as usize;
            ensure!(data <= b.len(), "channel {c}'s data is past the end");
            Ok(adpcm(&b[data..], u32_at(b, info + 8) as usize, &coefs))
        })
        .collect()
}

/// `n` samples of DSP-ADPCM, stopping early where the data runs out.
fn adpcm(data: &[u8], n: usize, coefs: &[i32]) -> Vec<i16> {
    let mut out = Vec::with_capacity(n);
    let (mut h1, mut h2) = (0i32, 0i32);
    for frame in data.as_chunks::<8>().0 {
        let scale = 1i32 << (frame[0] & 0xF);
        let pair = (frame[0] >> 4) as usize * 2;
        let (c1, c2) = (coefs[pair], coefs[pair + 1]);
        for k in 0..FRAME_SAMPLES {
            if out.len() == n {
                return out;
            }
            let byte = frame[1 + k / 2];
            let nibble = if k % 2 == 0 { byte >> 4 } else { byte & 0xF };
            let nibble = ((nibble as i32) << 28) >> 28;
            let s = (((nibble * scale) << 11) + 1024 + c1 * h1 + c2 * h2) >> 11;
            let s = s.clamp(i16::MIN as i32, i16::MAX as i32);
            out.push(s as i16);
            (h2, h1) = (h1, s);
        }
    }
    out
}

/// `channels` as a 16-bit stereo WAV: even channels mixed to the left,
/// odd to the right, a mono stream on both.
pub fn wav(channels: &[Vec<i16>], rate: u32) -> Vec<u8> {
    let n = channels.iter().map(Vec::len).min().unwrap_or(0);
    let side = |odd: usize| -> Vec<&Vec<i16>> {
        let picked: Vec<_> = channels.iter().skip(odd).step_by(2).collect();
        if picked.is_empty() {
            channels.iter().collect()
        } else {
            picked
        }
    };
    let (left, right) = (side(0), side(1));
    let mix = |side: &[&Vec<i16>], i: usize| {
        let sum: i32 = side.iter().map(|c| c[i] as i32).sum();
        (sum / side.len().max(1) as i32) as i16
    };
    let data = (n * 4) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    // A 16-byte PCM format chunk: format 1, two channels, the rate, bytes a
    // second, bytes a frame, bits a sample.
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for i in 0..n {
        out.extend_from_slice(&mix(&left, i).to_le_bytes());
        out.extend_from_slice(&mix(&right, i).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-channel stream of `frames`, its coefficients `coefs`.
    fn stream(coefs: [i16; 16], samples: u32, loop_end: i32, frames: &[[u8; 8]]) -> Vec<u8> {
        let data = INFO + INFO_SIZE;
        let mut b = vec![0u8; data];
        b[..4].copy_from_slice(MAGIC);
        b[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes());
        b[INFO..INFO + 2].copy_from_slice(&DSP_ADPCM.to_le_bytes());
        b[INFO + 4..INFO + 8].copy_from_slice(&48000u32.to_le_bytes());
        b[INFO + 8..INFO + 12].copy_from_slice(&samples.to_le_bytes());
        for (i, c) in coefs.iter().enumerate() {
            let at = INFO + 0x10 + 2 * i;
            b[at..at + 2].copy_from_slice(&c.to_le_bytes());
        }
        b[INFO + 0x30..INFO + 0x34].copy_from_slice(&(data as u32).to_le_bytes());
        b[INFO + 0x3C..INFO + 0x40].copy_from_slice(&loop_end.to_le_bytes());
        b[INFO + 0x40..INFO + 0x44].copy_from_slice(&7u32.to_le_bytes());
        frames.iter().for_each(|f| b.extend_from_slice(f));
        b
    }

    #[test]
    fn the_header_reads_length_and_loop() {
        let one_shot = header(&stream([0; 16], 96000, -1, &[])).unwrap();
        assert_eq!(one_shot.seconds(), 2.0);
        assert_eq!(one_shot.loop_start, None);
        let looping = header(&stream([0; 16], 48000, 40000, &[])).unwrap();
        assert_eq!(looping.loop_start, Some(7));
        assert!(header(b"RIFF").is_err());
    }

    #[test]
    fn adpcm_scales_nibbles_and_follows_its_history() {
        // Scale 2^0, no prediction: each nibble is its sample, sign-extended.
        let plain = stream([0; 16], 4, -1, &[[0x00, 0x17, 0xF8, 0, 0, 0, 0, 0]]);
        assert_eq!(decode(&plain).unwrap(), [vec![1, 7, -1, -8]]);
        // Coefficient 1.0 (2048) on the last sample: each nibble adds on.
        let mut coefs = [0; 16];
        coefs[0] = 2048;
        let ramp = stream(coefs, 3, -1, &[[0x00, 0x11, 0x10, 0, 0, 0, 0, 0]]);
        assert_eq!(decode(&ramp).unwrap(), [vec![1, 2, 3]]);
    }

    #[test]
    fn a_wav_is_stereo_whatever_the_stream_has() {
        let sample = |b: &[u8], frame: usize, side: usize| {
            let at = 44 + frame * 4 + side * 2;
            i16::from_le_bytes([b[at], b[at + 1]])
        };
        let mono = wav(&[vec![100, -100]], 48000);
        assert_eq!(&mono[..4], b"RIFF");
        assert_eq!(mono.len(), 44 + 2 * 4);
        assert_eq!((sample(&mono, 1, 0), sample(&mono, 1, 1)), (-100, -100));
        // Four channels: 0 and 2 to the left, 1 and 3 to the right.
        let four = wav(&[vec![10], vec![20], vec![30], vec![40]], 48000);
        assert_eq!((sample(&four, 0, 0), sample(&four, 0, 1)), (20, 30));
    }
}
