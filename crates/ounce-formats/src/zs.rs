//! zstd with the game's dictionaries.
//!
//! `Pack/ZsDic.pack.zs` is a plain zstd frame holding a SARC of four
//! dictionaries. Which one a file uses follows from its name.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use roead::sarc::Sarc;

/// zstd level for files we write. The game only needs a valid frame with
/// the right dictionary, so the level trades generation time for mod size.
/// Level 10 spent about 1 s of a 5 s seed for 2 MB less mod.
const LEVEL: i32 = 3;

/// RSDB tables ship as plain frames too.
const NO_DICT: &[&str] = &["ZsDic.pack.zs", ".rsizetable.zs", ".rstbl.byml.zs"];

/// The dictionary ID a zstd frame was written with, 0 for none.
pub fn frame_dict_id(frame: &[u8]) -> u32 {
    zstd::zstd_safe::get_dict_id_from_frame(frame).map_or(0, |id| id.get())
}

pub struct Dicts {
    zs: Vec<u8>,
    bcett: Vec<u8>,
    pack: Vec<u8>,
    bkprb: Vec<u8>,
}

impl Dicts {
    pub fn load(romfs_root: &Path) -> Result<Self> {
        let path = romfs_root.join("Pack/ZsDic.pack.zs");
        let raw = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let sarc = Sarc::new(zstd::decode_all(raw.as_slice())?)?;
        let get = |name: &str| {
            sarc.get_data(name)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| anyhow!("{name} missing from ZsDic.pack.zs"))
        };
        Ok(Self {
            zs: get("zs.zsdic")?,
            bcett: get("bcett.byml.zsdic")?,
            pack: get("pack.zsdic")?,
            bkprb: get("bkprb.zsdic")?,
        })
    }

    /// The dictionary for a romfs file name, or `None` when it isn't zstd.
    fn for_name(&self, name: &str) -> Option<Option<&[u8]>> {
        if !name.ends_with(".zs") {
            return None;
        }
        // Read before the dictionaries are loaded, so shipped without one.
        // A dictionary frame here hangs the game on boot.
        if NO_DICT.iter().any(|s| name.ends_with(s)) {
            return Some(None);
        }
        let dict = if name.ends_with(".bcett.byml.zs") {
            &self.bcett
        } else if name.ends_with(".pack.zs") {
            &self.pack
        } else if name.ends_with(".bkprb.zs") {
            &self.bkprb
        } else {
            &self.zs
        };
        Some(Some(dict))
    }

    /// Decompress `data` if `name` says it is zstd; otherwise return it as is.
    pub fn decompress(&self, name: &str, data: &[u8]) -> Result<Vec<u8>> {
        let Some(dict) = self.for_name(name) else {
            return Ok(data.to_vec());
        };
        let size = zstd::zstd_safe::get_frame_content_size(data)
            .map_err(|e| anyhow!("{name}: bad zstd frame header: {e:?}"))?
            .ok_or_else(|| anyhow!("{name}: zstd frame has no content size"))?;
        let mut d = zstd::bulk::Decompressor::with_dictionary(dict.unwrap_or_default())?;
        d.decompress(data, size as usize)
            .with_context(|| format!("decompressing {name}"))
    }

    /// Compress `data` the way the game expects for `name`. Bulk compression
    /// records the content size in the frame header, as the originals do.
    pub fn compress(&self, name: &str, data: &[u8]) -> Result<Vec<u8>> {
        let Some(dict) = self.for_name(name) else {
            return Ok(data.to_vec());
        };
        let mut c = zstd::bulk::Compressor::with_dictionary(LEVEL, dict.unwrap_or_default())?;
        Ok(c.compress(data)?)
    }
}
