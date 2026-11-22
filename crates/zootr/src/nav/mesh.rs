//! The Havok meshes a level piece collides with, read straight from the
//! dump since zootr doesn't extract them: its walls and floors
//! (`__Physical`) and its invisible walls (`__Airwall`).

use std::path::Path;

use anyhow::Result;
use ounce_formats::{Romfs, ShapeMesh};

/// Where the shape meshes live, by the name a `PhshMeshPath` gives.
const MESHES: &str = "Phive/Shape/Dcc/";

/// The mesh named `name`, like `<piece>__Airwall`, in its
/// actor's space, or `None` when the dump has none.
pub fn mesh(romfs: &Romfs, game: &Path, name: &str) -> Result<Option<ShapeMesh>> {
    (super::dump_file(romfs, game, &format!("{MESHES}{name}."))?)
        .map(|bytes| ShapeMesh::parse(&bytes))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ounce_formats::{ActorPack, pack_file};

    /// Every Deku Tree piece's mesh reads to the bounds its shape file
    /// stores, to the mesh's 1/512 m step.
    #[test]
    fn each_deku_mesh_reads_to_its_shapes_bounds() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new("../../dump/oot.nup");
        let mut checked = 0;
        crate::game::each(game, &format!("{MESHES}Land_Deku_"), |path, data| {
            let file = path.rsplit('/').next().unwrap_or_default();
            let name = file.split('.').next().unwrap_or_default();
            let actor = name.split("__").next().unwrap_or_default();
            let mesh = ShapeMesh::parse(&romfs.dicts().decompress(&path, &data)?)?;
            // A mesh can be named for a piece with no pack of its own.
            if !romfs.root().join(pack_file(actor)).exists() {
                return Ok(());
            }
            let pack = ActorPack::load(&romfs, actor)?;
            let param = format!("Phive/ShapeParam/{name}.phive__ShapeParam.gyml");
            let Some(shape) = pack.resolve(&param)? else {
                return Ok(());
            };
            let (min, max) = super::super::shape::bounds(&shape).unwrap();
            let (lo, hi) = mesh.bounds().unwrap();
            for i in 0..3 {
                assert!((lo[i] - min[i]).abs() < 0.003, "{name} min {lo:?} {min:?}");
                assert!((hi[i] - max[i]).abs() < 0.003, "{name} max {hi:?} {max:?}");
            }
            checked += 1;
            Ok(())
        })
        .unwrap();
        assert!(checked > 20, "only {checked} meshes checked");
    }
}
