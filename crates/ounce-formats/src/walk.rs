use std::path::{Path, PathBuf};

use anyhow::Result;

/// Every file under `dir` whose name ends with `suffix`, sorted, so callers
/// that seed a randomizer from this list get the same order on every machine.
pub fn files_with_suffix(dir: &Path, suffix: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.to_string_lossy().ends_with(suffix) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}
