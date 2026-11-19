//! Results that depend only on the dump, built once per dump a process
//! opens. A seed asks for some of them several times over, and the app
//! makes every seed in one process.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use ounce_formats::Romfs;

/// One result per dump, by the romfs folder it came from. Keep only small
/// results here: each stays for the life of the process.
pub(crate) struct PerDump<T>(Mutex<Vec<(PathBuf, T)>>);

impl<T: Clone> PerDump<T> {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    /// The result for `romfs`, built by `build` the first time it's asked.
    /// A failed build keeps nothing.
    pub(crate) fn get(&self, romfs: &Romfs, build: impl FnOnce() -> Result<T>) -> Result<T> {
        self.at(romfs.root(), build)
    }

    fn at(&self, root: &Path, build: impl FnOnce() -> Result<T>) -> Result<T> {
        let mut kept = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, v)) = kept.iter().find(|(r, _)| r == root) {
            return Ok(v.clone());
        }
        let v = build()?;
        kept.push((root.to_path_buf(), v.clone()));
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn a_result_is_built_once_per_dump() {
        let (a, b) = (Path::new("dump/a"), Path::new("dump/b"));
        let (kept, builds) = (PerDump::new(), Cell::new(0));
        let build = |v| {
            builds.set(builds.get() + 1);
            Ok(v)
        };
        assert_eq!(kept.at(a, || build(1)).unwrap(), 1);
        assert_eq!(kept.at(a, || build(9)).unwrap(), 1);
        assert_eq!(kept.at(b, || build(2)).unwrap(), 2);
        assert_eq!(builds.get(), 2);
        assert!(kept.at(a, || anyhow::bail!("no")).is_ok());
        let failing = PerDump::<u8>::new();
        assert!(failing.at(a, || anyhow::bail!("no")).is_err());
        assert_eq!(failing.at(a, || build(3)).unwrap(), 3);
    }
}
