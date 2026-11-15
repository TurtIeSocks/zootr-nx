use anyhow::{Result, bail};
use roead::{Endian, byml::Byml};

/// A BYML document plus the header facts needed to write it back the way
/// the game shipped it. Banc files are little-endian; GameData is big-endian.
pub struct BymlDoc {
    pub root: Byml,
    pub endian: Endian,
    pub version: u16,
}

impl BymlDoc {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let (endian, version) = match data.get(..4) {
            Some([b'Y', b'B', lo, hi]) => (Endian::Little, u16::from_le_bytes([*lo, *hi])),
            Some([b'B', b'Y', hi, lo]) => (Endian::Big, u16::from_be_bytes([*hi, *lo])),
            _ => bail!("not a BYML document"),
        };
        Ok(Self {
            root: Byml::from_binary(data)?,
            endian,
            version,
        })
    }

    pub fn to_binary(&self) -> Vec<u8> {
        self.root.to_binary_with_version(self.endian, self.version)
    }
}
