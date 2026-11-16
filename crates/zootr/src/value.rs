//! One GameData value element as text and as bytes. Saves and the running
//! game both write through [`encode`] and read through [`decode`], so a
//! value reads and writes the same way wherever it lives.

use anyhow::{Result, bail, ensure};
use ounce_formats::murmur3;
use serde::Serialize;

use crate::gamedata::GameData;

/// The type one element of `kind` holds: `kind` itself for a scalar.
pub fn element(kind: &str) -> &str {
    kind.strip_suffix("Array").unwrap_or(kind)
}

/// The field one element of a value is edited in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Input {
    /// One of the value's choices.
    Choice,
    Number,
    Text,
    /// Comma-separated floats: `x,y` or `x,y,z`.
    Vector,
    /// Shown, not written: what [`encode`] can't write.
    Read,
}

/// The field for one element of `kind`.
pub fn input(kind: &str) -> Input {
    match element(kind) {
        "Bool" if kind == "Bool" => Input::Choice,
        "Enum" => Input::Choice,
        "Int" | "UInt" | "Float" | "Int64" | "UInt64" => Input::Number,
        k if k.starts_with("String") || k.starts_with("WString") => Input::Text,
        "Vector2" | "Vector3" => Input::Vector,
        _ => Input::Read,
    }
}

/// `text` as one `kind` element `width` bytes wide: `true`/`false` (or
/// `1`/`0`), a number, an Enum value's name, comma-separated floats, or
/// text padded with zeros. A Bool is 1 byte in the game and 4 in a save.
pub fn encode(kind: &str, text: &str, width: usize) -> Result<Vec<u8>> {
    let kind = element(kind);
    let bytes: Vec<u8> = match kind {
        "Bool" => {
            let on = match text {
                "true" | "1" => 1u32,
                "false" | "0" => 0,
                _ => bail!("{text} isn't true or false"),
            };
            ensure!(matches!(width, 1 | 4), "a Bool isn't {width} bytes");
            return Ok(on.to_le_bytes()[..width].to_vec());
        }
        "Int" => text.parse::<i32>()?.to_le_bytes().into(),
        "UInt" => text.parse::<u32>()?.to_le_bytes().into(),
        "Float" => text.parse::<f32>()?.to_le_bytes().into(),
        "Enum" => murmur3(text.as_bytes()).to_le_bytes().into(),
        "Int64" => text.parse::<i64>()?.to_le_bytes().into(),
        "UInt64" => text.parse::<u64>()?.to_le_bytes().into(),
        "Vector2" | "Vector3" => {
            let parts: Vec<f32> = text
                .trim_matches(|c| c == '(' || c == ')')
                .split(',')
                .map(|p| p.trim().parse())
                .collect::<Result<_, _>>()?;
            let want = if kind == "Vector2" { 2 } else { 3 };
            ensure!(parts.len() == want, "a {kind} takes {want} numbers");
            parts.iter().flat_map(|f| f.to_le_bytes()).collect()
        }
        k if k.starts_with("WString") => {
            let units: Vec<u16> = text.encode_utf16().collect();
            ensure!(units.len() * 2 < width, "{text} is too long for a {kind}");
            let mut out: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
            out.resize(width, 0);
            return Ok(out);
        }
        k if k.starts_with("String") => {
            ensure!(text.len() < width, "{text} is too long for a {kind}");
            let mut out = text.as_bytes().to_vec();
            out.resize(width, 0);
            return Ok(out);
        }
        _ => bail!("{kind} isn't writable"),
    };
    ensure!(
        bytes.len() == width,
        "a {kind} is {} bytes, not {width}",
        bytes.len()
    );
    Ok(bytes)
}

/// One element's bytes as text [`encode`] takes back. Types it can't
/// write show their size.
pub fn decode(gd: &GameData, kind: &str, b: &[u8]) -> String {
    let word = |at: usize| {
        b.get(at..at + 4)
            .map_or(0, |w| u32::from_le_bytes(w.try_into().unwrap()))
    };
    let quad = |at: usize| {
        b.get(at..at + 8)
            .map_or(0, |w| u64::from_le_bytes(w.try_into().unwrap()))
    };
    let floats = |n: usize| {
        (0..n)
            .map(|i| f32::from_bits(word(i * 4)).to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    match element(kind) {
        "Bool" if kind == "Bool" => (b.first().copied().unwrap_or(0) != 0).to_string(),
        "Int" => (word(0) as i32).to_string(),
        "UInt" => word(0).to_string(),
        "Float" => f32::from_bits(word(0)).to_string(),
        "Enum" => gd.name(word(0)),
        "Int64" => (quad(0) as i64).to_string(),
        "UInt64" => quad(0).to_string(),
        "Vector2" => floats(2),
        "Vector3" => floats(3),
        k if k.starts_with("WString") => {
            let units: Vec<u16> = b
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| u16::from_le_bytes(c))
                .take_while(|&u| u != 0)
                .collect();
            String::from_utf16_lossy(&units)
        }
        k if k.starts_with("String") => {
            let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            String::from_utf8_lossy(&b[..end]).into_owned()
        }
        _ => format!("{} bytes", b.len()),
    }
}

/// `text` as the raw bits of a scalar a save or GameData holds in 32 bits,
/// refusing an Enum value no enum has.
pub fn scalar(gd: &GameData, kind: &str, text: &str) -> Result<u32> {
    let raw = u32::from_le_bytes(encode(kind, text, 4)?.try_into().unwrap());
    if element(kind) == "Enum" && gd.name(raw) != text {
        bail!(
            "{text} isn't a value of any {} enum",
            crate::known::flag::GAME_DATA
        );
    }
    Ok(raw)
}

/// A scalar's raw bits as text, the way [`scalar`] reads it back.
pub fn render(gd: &GameData, kind: &str, raw: u32) -> String {
    decode(gd, kind, &raw.to_le_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_encode_as_the_game_stores_them() {
        assert_eq!(encode("Bool", "true", 1).unwrap(), [1]);
        assert_eq!(encode("Bool", "1", 4).unwrap(), [1, 0, 0, 0]);
        assert_eq!(encode("Bool", "0", 1).unwrap(), [0]);
        assert_eq!(encode("Int", "-1", 4).unwrap(), [0xff; 4]);
        assert_eq!(
            encode("Enum", "New", 4).unwrap(),
            murmur3(b"New").to_le_bytes()
        );
        assert_eq!(
            encode("String16", "Kak", 16).unwrap(),
            *b"Kak\0\0\0\0\0\0\0\0\0\0\0\0\0"
        );
        assert_eq!(encode("Int64", "-2", 8).unwrap(), (-2i64).to_le_bytes());
        assert_eq!(
            encode("Vector3", "(1, 2.5, -3)", 12).unwrap(),
            [1f32, 2.5, -3.]
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            encode("WString16", "Hi", 32).unwrap()[..4],
            [b'H', 0, b'i', 0]
        );
        assert_eq!(encode("IntArray", "7", 4).unwrap(), 7i32.to_le_bytes());
        assert!(encode("String16", "0123456789abcdef", 16).is_err());
        assert!(encode("Bool", "yes", 1).is_err());
        assert!(encode("Vector3", "0", 12).is_err());
        assert!(encode("Int", "1", 8).is_err());
        assert!(encode("Binary", "1", 4).is_err());
    }

    #[test]
    fn what_decodes_encodes_back() {
        let gd = GameData::default();
        for (kind, text, width) in [
            ("Bool", "true", 4),
            ("Bool", "false", 1),
            ("Int", "-7", 4),
            ("UInt", "4000000000", 4),
            ("Float", "1.5", 4),
            ("Int64", "-9000000000", 8),
            ("UInt64", "18000000000000000000", 8),
            ("Vector3", "1,-2,3.5", 12),
            ("String32", "KokiriForest", 32),
            ("WString16", "Link", 32),
            ("FloatArray", "-1", 4),
        ] {
            let bytes = encode(kind, text, width).unwrap();
            assert_eq!(decode(&gd, kind, &bytes), text, "{kind}");
        }
    }

    #[test]
    fn each_type_gets_its_field() {
        for (kind, want) in [
            ("Bool", Input::Choice),
            ("Enum", Input::Choice),
            ("EnumArray", Input::Choice),
            ("Int64", Input::Number),
            ("String64", Input::Text),
            ("Vector3", Input::Vector),
            ("BoolArray", Input::Read),
            ("Binary", Input::Read),
            ("Struct", Input::Read),
        ] {
            assert_eq!(input(kind), want, "{kind}");
        }
    }
}
