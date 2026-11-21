//! Bounds-checked little-endian reads over a byte slice.

use anyhow::{Context, Result};

pub(crate) struct Reader<'a>(pub(crate) &'a [u8]);

impl<'a> Reader<'a> {
    pub(crate) fn bytes(&self, at: usize, n: usize) -> Result<&'a [u8]> {
        self.0
            .get(at..at + n)
            .with_context(|| format!("read of {n} bytes at {at:#x} past the end"))
    }
    pub(crate) fn u8(&self, at: usize) -> Result<u8> {
        Ok(self.bytes(at, 1)?[0])
    }
    pub(crate) fn u16(&self, at: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(at, 2)?.try_into()?))
    }
    pub(crate) fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(at, 4)?.try_into()?))
    }
    pub(crate) fn i32(&self, at: usize) -> Result<i32> {
        Ok(i32::from_le_bytes(self.bytes(at, 4)?.try_into()?))
    }
    pub(crate) fn u64(&self, at: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(at, 8)?.try_into()?))
    }
    pub(crate) fn f32(&self, at: usize) -> Result<f32> {
        Ok(f32::from_le_bytes(self.bytes(at, 4)?.try_into()?))
    }
}
