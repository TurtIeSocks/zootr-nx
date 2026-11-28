//! Skeletal poses and the model shapes they move: what a shelf pose does to
//! an item, and how low the item then reaches.

use std::path::Path;

use anyhow::Result;
use ounce_formats::{Base, Romfs};

use super::unpacked;
use crate::heights::{self, Hull, Kind, UPRIGHT};

pub(super) fn poses(romfs: &Romfs, game: &Path, path: &str) -> Result<()> {
    unpacked(romfs, game, path, |path, data, _| {
        println!("{path}");
        for anim in ounce_formats::animation_names(data) {
            let bones = ounce_formats::bases(data, &anim)?.unwrap_or_default();
            println!("  {anim}");
            for (bone, b) in bones.iter().filter(|(_, b)| *b != Default::default()) {
                println!(
                    "    {bone} S{:?} R{:?} T{:?}",
                    b.scale, b.rotate, b.translate
                );
                // A bone its curves move: how far each value ranges.
                let frames = match ounce_formats::fska::frames(data, &anim, bone) {
                    Ok(f) => f.unwrap_or_default(),
                    Err(e) => {
                        println!("      curves unread: {e}");
                        continue;
                    }
                };
                if frames.len() > 1 {
                    let range = |get: &dyn Fn(&Base) -> [f32; 3]| {
                        let lo = (0..3)
                            .map(|i| frames.iter().map(|f| get(f)[i]).fold(f32::MAX, f32::min));
                        let hi = (0..3)
                            .map(|i| frames.iter().map(|f| get(f)[i]).fold(f32::MIN, f32::max));
                        format!(
                            "{:.3?}..{:.3?}",
                            lo.collect::<Vec<_>>(),
                            hi.collect::<Vec<_>>()
                        )
                    };
                    println!(
                        "      {} frames  R {}  T {}",
                        frames.len(),
                        range(&|f| f.rotate.map_or([0.0; 3], |r| [r[0], r[1], r[2]])),
                        range(&|f| f.translate.unwrap_or_default()),
                    );
                }
            }
        }
        Ok(())
    })
}

pub(super) fn bounds(romfs: &Romfs, game: &Path, path: &str, pose: Option<&[f32]>) -> Result<()> {
    let pose = pose.map(|p| heights::frame([p[3]; 3], [p[0], p[1], p[2]], [0.0, p[4], 0.0]));
    unpacked(romfs, game, path, |path, data, _| {
        println!("{path}");
        if path.contains(".bntx") {
            let (top, format) = (
                ounce_formats::top_level(data)?,
                ounce_formats::format(data)?,
            );
            let redraws = ounce_formats::redraws(format);
            println!(
                "  {} {}x{} format {format:#x} redraws {redraws}",
                top.name, top.width, top.height
            );
        }
        for shape in ounce_formats::shape_names(data) {
            let points = ounce_formats::positions(data, &shape)?;
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for p in &points {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
            println!("  {shape} {lo:.3?} {hi:.3?}");
            match ounce_formats::lods(data, &shape) {
                Ok(lods) => {
                    let each = lods
                        .iter()
                        .map(|l| format!("{}v/{}t", l.vertices.len(), l.indices.len() / 3));
                    println!("    lods {}", each.collect::<Vec<_>>().join(" "));
                }
                Err(e) => println!("    lods: {e}"),
            }
            let attributes = ounce_formats::attributes(data, &shape)?;
            let each = attributes.iter().map(|a| {
                let varies = if a.varies { "" } else { " same" };
                format!("{}@{} {:#x}{varies}", a.name, a.offset, a.format)
            });
            println!("    attributes {}", each.collect::<Vec<_>>().join(", "));
            if let Some(pose) = &pose {
                let posed = Hull::posed(Kind::Mesh, &points, &[*pose]);
                println!("    posed lowest {:.3}", posed.lo(&UPRIGHT));
            }
        }
        Ok(())
    })
}
