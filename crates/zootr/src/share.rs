//! Share codes: a seed with its settings and extras in one line of text,
//! so a friend can build the same mod. The icons confirm the match.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::generate::Extras;
use crate::settings::{Settings, SettingsVersion};

/// Starts every code, and names the format so a later one can differ:
/// `ZR2-` codes are read with vanilla defaults.
const PREFIX: &str = "ZR2-";
/// Started codes from before the defaults went vanilla.
const OLD_PREFIX: &str = "ZR1-";
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
/// Far above any real settings, so a bad code can't ask for a huge buffer.
const MAX_JSON: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shared {
    pub seed: u64,
    pub settings: Settings,
    #[serde(default, skip_serializing_if = "Extras::is_empty")]
    pub extras: Extras,
}

/// Whether `text` looks like a share code rather than a seed number.
pub fn is_code(text: &str) -> bool {
    let text = text.trim();
    text.starts_with(PREFIX) || text.starts_with(OLD_PREFIX)
}

/// The code for a seed. It leaves the build out: placements are the same
/// on both, and the receiver's own target picks theirs.
pub fn encode(seed: u64, settings: &Settings, extras: &Extras) -> String {
    let shared = Shared {
        seed,
        settings: settings.clone(),
        extras: Extras {
            release: None,
            ..extras.clone()
        },
    };
    let json = serde_json::to_vec(&shared).expect("plain data serializes");
    let packed = zstd::bulk::compress(&json, 19).expect("compressing to memory works");
    format!("{PREFIX}{}", to_base64(&packed))
}

pub fn decode(code: &str) -> Result<Shared> {
    let code = code.trim();
    let (body, from) = match code.strip_prefix(PREFIX) {
        Some(body) => (body, SettingsVersion::CURRENT),
        None => (
            code.strip_prefix(OLD_PREFIX).context("not a share code")?,
            SettingsVersion::RANDOMIZER,
        ),
    };
    let packed = from_base64(body).context("a share code got cut or changed")?;
    let json = zstd::bulk::decompress(&packed, MAX_JSON).context("a share code got cut")?;
    let mut shared: Shared =
        serde_json::from_slice(&json).context("a share code from another version")?;
    crate::settings::upgrade(&mut shared.settings, from);
    // Codes from before [`encode`] left the build out.
    shared.extras.release = None;
    Ok(shared)
}

/// URL-safe base64 without padding, so a code survives chat apps.
fn to_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
        }
    }
    out
}

fn from_base64(text: &str) -> Result<Vec<u8>> {
    ensure!(text.len() % 4 != 1, "odd length");
    let digit = |c: u8| match ALPHABET.iter().position(|&a| a == c) {
        Some(d) => Ok(d as u32),
        None => bail!("{:?} isn't in a share code", c as char),
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0;
        for (i, &c) in chunk.iter().enumerate() {
            n |= digit(c)? << (18 - 6 * i);
        }
        for i in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Release;
    use crate::known::item;

    #[test]
    fn base64_round_trips_every_tail_length() {
        for len in 0..10u8 {
            let bytes: Vec<u8> = (0..len).map(|b| b.wrapping_mul(97)).collect();
            assert_eq!(from_base64(&to_base64(&bytes)).unwrap(), bytes);
        }
        assert_eq!(to_base64(b"Man"), "TWFu");
        assert_eq!(to_base64(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn a_code_brings_back_the_seed_settings_and_extras() {
        let settings = Settings::from([
            ("open-forest".into(), true.into()),
            ("logic".into(), "glitchless".into()),
        ]);
        let extras = Extras {
            give: vec![item::HOOKSHOT.to_string()],
            ..Default::default()
        };
        let code = encode(u64::MAX, &settings, &extras);
        assert!(is_code(&code) && !is_code("12345"));
        let back = decode(&format!("  {code}\n")).unwrap();
        assert_eq!(
            (back.seed, back.settings, back.extras),
            (u64::MAX, settings, extras)
        );
    }

    #[test]
    fn a_code_leaves_the_build_to_the_receiver() {
        let extras = Extras {
            give: vec![item::HOOKSHOT.to_string()],
            release: Some(Release::V100),
            ..Default::default()
        };
        let code = encode(7, &Settings::new(), &extras);
        let without = Extras {
            release: None,
            ..extras.clone()
        };
        assert_eq!(code, encode(7, &Settings::new(), &without));
        assert_eq!(decode(&code).unwrap().extras, without);
        let old = Shared {
            seed: 7,
            settings: Settings::new(),
            extras,
        };
        let packed = zstd::bulk::compress(&serde_json::to_vec(&old).unwrap(), 19).unwrap();
        let old = decode(&format!("{PREFIX}{}", to_base64(&packed))).unwrap();
        assert_eq!(old.extras.release, None, "an older code's build is dropped");
    }

    #[test]
    fn an_old_code_keeps_the_randomizers_defaults() {
        let settings = Settings::from([("chests".into(), false.into())]);
        let code = encode(7, &settings, &Extras::default());
        assert!(code.starts_with("ZR2-"));
        assert_eq!(decode(&code).unwrap().settings, settings);
        let old = code.replacen("ZR2-", "ZR1-", 1);
        assert!(is_code(&old));
        let mut want = crate::settings::favorites::randomizer();
        want.insert("chests".into(), false.into());
        assert_eq!(decode(&old).unwrap().settings, want);
    }

    #[test]
    fn a_damaged_code_says_so() {
        let code = encode(7, &Settings::new(), &Extras::default());
        assert!(decode(&code[..code.len() - 3]).is_err());
        assert!(decode("ZR1-!!!!").is_err());
        assert!(decode("12345").is_err());
    }
}
