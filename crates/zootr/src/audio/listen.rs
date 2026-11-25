//! Which songs play in the game's audio dump (its target's harness lines
//! turn audio on, muted, dumped to a file: 16-bit, 48 kHz, eight channels). Each stream is
//! decoded and found in each window of the dump by normalized
//! cross-correlation: a stream that plays scores 0.5 to 1, one that
//! doesn't under 0.15, whatever else is in the mix.

use std::f64::consts::PI;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result};

use super::{STREAMS, TRACKS};

const RATE: usize = 48_000;
/// Both sides are averaged down by this before matching: 8 kHz keeps
/// enough of the music to match and makes each FFT six times smaller.
const DOWN: usize = 6;
/// The least score that counts as playing.
pub const HEARD: f32 = 0.25;

/// What one window of the dump holds.
pub struct Window {
    /// Its start, in seconds into the dump.
    pub at: f32,
    /// Its loudness (RMS of the front pair).
    pub rms: f32,
    /// The best match, if it scores at least [`HEARD`]: its name, score
    /// and where in it the window lines up, in seconds.
    pub best: Option<(String, f32, f32)>,
}

/// A stream by a track's id (its song) or its own name.
fn stream_name(name: &str) -> &str {
    TRACKS
        .iter()
        .find(|t| t.id == name)
        .and_then(|t| t.song)
        .unwrap_or(name)
}

/// The windows of `dump` from `from` to `to` seconds, `step` long, each
/// matched against `names`.
pub fn listen(
    game: &Path,
    dump: &Path,
    channels: usize,
    (from, to, step): (f32, f32, f32),
    names: &[String],
) -> Result<Vec<Window>> {
    let streams = names
        .iter()
        .map(|n| {
            let name = stream_name(n);
            let path = format!("{STREAMS}{name}.bwav");
            let found = crate::game::search(game, &path)?;
            let (_, data) = (found.into_iter().find(|(p, _)| *p == path))
                .with_context(|| format!("no stream {name}"))?;
            let channels = ounce_formats::bwav::decode(&data)?;
            Ok((name.to_owned(), down(&mono(&channels))))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut file = std::fs::File::open(dump).with_context(|| dump.display().to_string())?;
    let mut out = Vec::new();
    let mut at = from;
    while at + step <= to {
        let window = read(&mut file, channels, at, step)?;
        let rms = (window.iter().map(|&s| (s * s) as f64).sum::<f64>() / window.len().max(1) as f64)
            .sqrt() as f32;
        let window = down(&window);
        let scores = ounce_formats::par_map(&streams, |(name, stream)| {
            let (score, lag) = ncc(&window, stream);
            (name.clone(), score, lag as f32 * DOWN as f32 / RATE as f32)
        });
        let best = (scores.into_iter())
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|b| b.1 >= HEARD);
        out.push(Window { at, rms, best });
        at += step;
    }
    Ok(out)
}

/// The front pair of `secs` of the dump from `at`, averaged to one.
fn read(file: &mut std::fs::File, channels: usize, at: f32, secs: f32) -> Result<Vec<f32>> {
    let frame = 2 * channels;
    file.seek(SeekFrom::Start((at * RATE as f32) as u64 * frame as u64))?;
    let mut raw = vec![0u8; (secs * RATE as f32) as usize * frame];
    let n = file.read(&mut raw)?;
    raw.truncate(n - n % frame);
    let sample = |at: usize| i16::from_le_bytes([raw[at], raw[at + 1]]) as f32;
    Ok((raw.chunks_exact(frame).enumerate())
        .map(|(i, _)| (sample(i * frame) + sample(i * frame + 2)) / 2.0)
        .collect())
}

fn mono(channels: &[Vec<i16>]) -> Vec<f32> {
    let n = channels.iter().map(Vec::len).min().unwrap_or(0);
    (0..n)
        .map(|i| channels.iter().map(|c| c[i] as f32).sum::<f32>() / channels.len() as f32)
        .collect()
}

fn down(samples: &[f32]) -> Vec<f32> {
    samples
        .as_chunks::<DOWN>()
        .0
        .iter()
        .map(|c| c.iter().sum::<f32>() / DOWN as f32)
        .collect()
}

/// The least overlap a lag is scored over: a second, or all of the
/// shorter side. A short overlap scores high by chance.
const OVERLAP: usize = RATE / DOWN;

/// The peak normalized cross-correlation of `a` and `b` over every lag
/// where they overlap by at least [`OVERLAP`], each scored over the
/// overlap alone, so a jingle crossing a window's edge still counts. The
/// lag is where `a` starts in `b`: with `a` a window and `b` a stream,
/// where the window lines up in the stream (negative: before it starts).
pub fn ncc(a: &[f32], b: &[f32]) -> (f32, isize) {
    let least = OVERLAP.min(a.len()).min(b.len());
    if least == 0 {
        return (0.0, 0);
    }
    let n = (a.len() + b.len()).next_power_of_two();
    let spectrum = |x: &[f32]| {
        let mut re: Vec<f64> = x.iter().map(|&v| v as f64).collect();
        re.resize(n, 0.0);
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im, false);
        (re, im)
    };
    let ((ar, ai), (br, bi)) = (spectrum(a), spectrum(b));
    // b · conj(a): at index k (mod n), the sum of a[i] · b[i + k].
    let mut re: Vec<f64> = (0..n).map(|i| br[i] * ar[i] + bi[i] * ai[i]).collect();
    let mut im: Vec<f64> = (0..n).map(|i| bi[i] * ar[i] - br[i] * ai[i]).collect();
    fft(&mut re, &mut im, true);
    let (ea, eb) = (energy(a), energy(b));
    let (la, lb) = (a.len() as isize, b.len() as isize);
    let (mut best, mut at) = (0.0f64, 0isize);
    for lag in -(la - least as isize)..=lb - least as isize {
        // The overlap: b[from..to] against a[from - lag..to - lag].
        let (from, to) = (lag.max(0), (lag + la).min(lb));
        let span = |e: &[f64], s: isize, t: isize| (e[t as usize] - e[s as usize]).max(0.0);
        let norm = (span(&eb, from, to) * span(&ea, from - lag, to - lag)).sqrt();
        let score = re[lag.rem_euclid(n as isize) as usize].abs() / (norm + 1e-9);
        if score > best {
            (best, at) = (score, lag);
        }
    }
    (best as f32, at)
}

/// Running sums of `x` squared, from 0.
fn energy(x: &[f32]) -> Vec<f64> {
    let mut sums = vec![0.0f64; x.len() + 1];
    for (i, &v) in x.iter().enumerate() {
        sums[i + 1] = sums[i] + (v as f64).powi(2);
    }
    sums
}

/// An in-place radix-2 FFT of a power-of-two length, scaled by 1/n when
/// `inverse`.
fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = 2.0 * PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in start..start + len / 2 {
                let m = k + len / 2;
                let (tr, ti) = (re[m] * cr - im[m] * ci, re[m] * ci + im[m] * cr);
                (re[m], im[m]) = (re[k] - tr, im[k] - ti);
                re[k] += tr;
                im[k] += ti;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
    if inverse {
        re.iter_mut()
            .chain(im.iter_mut())
            .for_each(|v| *v /= n as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fft_and_its_inverse_give_back_the_signal() {
        let signal: Vec<f64> = (0..64).map(|i| ((i * 7) % 11) as f64 - 5.0).collect();
        let (mut re, mut im) = (signal.clone(), vec![0.0; 64]);
        fft(&mut re, &mut im, false);
        // Its first bin is the sum.
        assert!((re[0] - signal.iter().sum::<f64>()).abs() < 1e-9);
        fft(&mut re, &mut im, true);
        assert!(re.iter().zip(&signal).all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn a_window_cut_from_a_stream_is_found_where_it_was_cut() {
        let noise = |seed: u32, n: usize| -> Vec<f32> {
            let mut x = seed;
            (0..n)
                .map(|_| {
                    x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (x >> 16) as f32 / 32768.0 - 1.0
                })
                .collect()
        };
        let stream = noise(1, 3 * OVERLAP);
        let window: Vec<f32> = stream[1234..1234 + OVERLAP]
            .iter()
            .map(|v| v * 0.5)
            .collect();
        let (score, lag) = ncc(&window, &stream);
        assert!(score > 0.99 && lag == 1234, "{score} at {lag}");
        let (other, _) = ncc(&noise(2, OVERLAP), &stream);
        assert!(other < 0.1, "{other}");
        // The stream starting partway into a longer window, and running
        // past its end: still found, by the part they share.
        let mut late = noise(3, 2 * OVERLAP);
        late[OVERLAP / 2..].copy_from_slice(&stream[..3 * OVERLAP / 2]);
        let (score, lag) = ncc(&late, &stream);
        assert!(
            score > 0.99 && lag == -(OVERLAP as isize) / 2,
            "{score} at {lag}"
        );
    }
}
