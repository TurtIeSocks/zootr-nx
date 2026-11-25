//! The Model tab's meshes: a palette's view, every shape moved into its
//! model's space in the bind pose, as one binary for the app to draw.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, ensure};
use ounce_formats::Romfs;
use serde::{Deserialize, Serialize};

use roead::byml::Byml;
use roead::sarc::Sarc;

use super::files::{EYE_LAYERS, PLAYER_BASE, views};
use super::preview::file_name;

/// One shape as the binary's head lists it.
#[derive(Deserialize, Serialize)]
struct Head {
    name: String,
    /// Its albedo's file name, when its material has one.
    albedo: Option<String>,
    /// Whether the palette paints that albedo: only those change with a pick.
    painted: bool,
    /// An eye's iris, which the game lays over its albedo (the sclera)
    /// where its mask's red channel is set.
    iris: Option<Iris>,
    vertices: usize,
    indices: usize,
}

/// An eye's iris texture and the mask placing it.
#[derive(Deserialize, Serialize)]
struct Iris {
    albedo: String,
    mask: String,
    /// How wide the iris and its mask show, a share of their textures'
    /// width, about the eye's centre: `IRIS_SIZE`'s fourth float.
    size: f32,
}

/// The eye material param that reads as its iris's size: centred
/// (0.5, 0.5) with 0.44 on Link and 0.5 on the child, whose iris shows
/// larger. Its name is unknown; the hash is what the game keeps.
const IRIS_SIZE: u64 = 0x0187_464e_c654_1f6e;

/// A shape ready to draw, with its material's name.
struct Drawn {
    material: String,
    head: Head,
    body: Vec<u8>,
}

/// Each view of palette `id`, by label.
pub fn labels(id: &str) -> Vec<&'static str> {
    views(id).iter().map(|v| v.label).collect()
}

/// View `view` of palette `id` as the app reads it: a u32 length and a
/// JSON list of shapes (name, albedo, vertex and index counts), padded
/// with spaces to 4 bytes, then each shape's positions (3 f32s a
/// vertex), normals (3) and UVs (2), and its triangles (u32s). With
/// it, each texture its shapes name, by its file's name, once.
/// Link's eye layers (`EYE_LAYERS`) are left out.
pub fn mesh(romfs: &Romfs, id: &str, view: usize) -> Result<(Vec<u8>, Vec<String>)> {
    let view = (views(id).get(view)).with_context(|| format!("{id} has no view {view}"))?;
    let palette = super::palette(id)?;
    let paints = |albedo: &str| {
        let mut textures = palette.writes.iter().filter_map(|w| w.texture());
        textures.any(|t| t.stem() == Some(albedo))
    };
    let mut drawn: Vec<Drawn> = Vec::new();
    for (i, path) in view.models.iter().enumerate() {
        let mut mine = shapes(&romfs.read(path)?, &paints).with_context(|| path.to_string())?;
        if let (0, Some(pack)) = (i, view.worn) {
            let hidden = hidden(romfs, path, pack)?;
            mine.retain(|d| !hidden.contains(&d.material));
        }
        mine.retain(|d| !EYE_LAYERS.contains(&d.material.as_str()));
        drawn.extend(mine);
    }
    let heads: Vec<&Head> = drawn.iter().map(|d| &d.head).collect();
    let mut json = serde_json::to_vec(&heads)?;
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut out = (json.len() as u32).to_le_bytes().to_vec();
    out.extend(json);
    for d in &drawn {
        out.extend(&d.body);
    }
    let textures = heads.iter().flat_map(|h| {
        let iris = h.iris.as_ref().map(|i| [&i.albedo, &i.mask]);
        h.albedo.iter().chain(iris.into_iter().flatten())
    });
    let textures: BTreeSet<&String> = textures.collect();
    Ok((out, textures.into_iter().cloned().collect()))
}

/// Whether texture `name` is an albedo: `…_Alb`, or a numbered variant of
/// one (`…_Alb.00`).
pub(super) fn is_albedo(name: &str) -> bool {
    let base = name
        .rsplit_once('.')
        .filter(|(_, n)| n.bytes().all(|b| b.is_ascii_digit()));
    base.map_or(name, |(base, _)| base).ends_with("_Alb")
}

/// The materials of Link's model `link` that the armor in pack `pack`
/// hides, as the game decides: the groups its ArmorParam names, each
/// group's materials from Link's EquipmentUserParam.
fn hidden(romfs: &Romfs, link: &str, pack: &str) -> Result<BTreeSet<String>> {
    let actor = (link.strip_prefix("Model/")).and_then(|m| m.split('.').next());
    let actor = actor.with_context(|| format!("{link} isn't a model"))?;
    let base = romfs.read(PLAYER_BASE)?;
    let base = Sarc::new(&base)?;
    let param =
        format!("Component/EquipmentUserParam/{actor}.game__component__EquipmentUserParam.bgyml");
    let param = base
        .get_data(&param)
        .with_context(|| format!("no {param}"))?;
    let armor = romfs.read(pack)?;
    let armor = Sarc::new(&armor)?;
    let hides: BTreeSet<String> = (armor.files())
        .filter(|f| f.name.is_some_and(|n| n.contains("/ArmorParam/")))
        .filter_map(|f| field(f.data, "HideMaterialGroupNameList"))
        .flat_map(|list| {
            list.into_iter()
                .filter_map(|g| Some(g.as_string().ok()?.to_string()))
        })
        .collect();
    ensure!(!hides.is_empty(), "{pack} hides no group of Link's");
    let groups = field(param, "HiddenMaterialGroupList").context("Link lists no groups")?;
    let mut hidden = BTreeSet::new();
    for group in groups {
        let group = group.as_map()?;
        let name = group.get("GroupName").context("a group without a name")?;
        if hides.contains(name.as_string()?.as_str()) {
            let materials = group
                .get("MaterialNameList")
                .context("a group without materials")?;
            for m in materials.as_array()? {
                hidden.insert(m.as_string()?.to_string());
            }
        }
    }
    Ok(hidden)
}

/// The array at `key` of the BYML map `data`.
fn field(data: &[u8], key: &str) -> Option<Vec<Byml>> {
    let doc = Byml::from_binary(data).ok()?;
    Some(doc.as_map().ok()?.get(key)?.as_array().ok()?.to_vec())
}

/// Every shape of the model `data`, each holding only the vertices its
/// finest level of detail uses.
fn shapes(data: &[u8], paints: &impl Fn(&str) -> bool) -> Result<Vec<Drawn>> {
    let materials = ounce_formats::materials(data)?;
    let skeleton = ounce_formats::skeleton(data)?;
    let mut out = Vec::new();
    for shape in ounce_formats::shapes_of(data)? {
        let material = materials
            .get(shape.material)
            .context("a material past the list")?;
        let vertices = ounce_formats::model_vertices(data, &skeleton, &shape.name)?;
        let uvs = ounce_formats::attribute(data, &shape.name, "_u0")?;
        let triangles = ounce_formats::triangles(data, &shape.name)?;
        // Each vertex the triangles use, numbered again in order.
        let mut kept: HashMap<usize, u32> = HashMap::new();
        let mut order = Vec::new();
        let indices: Vec<u32> = (triangles.iter())
            .map(|&v| {
                *kept.entry(v).or_insert_with(|| {
                    order.push(v);
                    order.len() as u32 - 1
                })
            })
            .collect();
        let mut body = Vec::new();
        let mut floats = |f: &[f32]| f.iter().for_each(|f| body.extend(f.to_le_bytes()));
        let at = |v: usize| vertices.get(v).context("a triangle past the vertices");
        for &v in &order {
            floats(&at(v)?.0);
        }
        for &v in &order {
            floats(&at(v)?.1);
        }
        for &v in &order {
            let uv = uvs.get(v).context("a triangle past the UVs")?;
            floats(&uv[..2]);
        }
        indices.iter().for_each(|i| body.extend(i.to_le_bytes()));
        let textures = &material.textures;
        let named = |part: &str, kind: &str| {
            (textures.iter()).find(|t| t.contains(part) && t.ends_with(kind))
        };
        let iris = named("_Iris_", "_Alb").zip(named("_Mask_", "_Gn3"));
        let albedo = (textures.iter())
            .find(|t| is_albedo(t) && Some(*t) != iris.map(|(i, _)| i))
            .map(|t| file_name(t));
        out.push(Drawn {
            material: material.name.clone(),
            head: Head {
                name: shape.name,
                albedo: albedo.map(Into::into),
                painted: albedo.is_some_and(paints),
                iris: iris.map(|(albedo, mask)| Iris {
                    size: (material.params.iter())
                        .find(|(h, _)| *h == IRIS_SIZE)
                        .map_or(1.0, |(_, v)| v[3]),
                    albedo: albedo.clone(),
                    mask: mask.clone(),
                }),
                vertices: order.len(),
                indices: indices.len(),
            },
            body,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::files;
    use super::*;

    #[test]
    fn an_albedo_may_carry_a_variants_number() {
        assert!(is_albedo("Mt_FakeBlade_Alb"));
        assert!(is_albedo("Mt_FakeBlade_Alb.00"));
        assert!(!is_albedo("Mt_FakeBlade_Rgh.00"));
        assert!(!is_albedo("Mt_FakeBlade_Alb.Big"));
    }

    /// The heads of a `mesh` binary.
    fn heads(out: &[u8]) -> Vec<Head> {
        let len = u32::from_le_bytes(out[..4].try_into().unwrap()) as usize;
        serde_json::from_slice(&out[4..4 + len]).unwrap()
    }

    #[test]
    fn every_palettes_every_view_draws_with_its_albedos() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for palette in super::super::PALETTES {
            for v in 0..views(palette.id).len() {
                let (_, textures) = mesh(&romfs, palette.id, v)
                    .unwrap_or_else(|e| panic!("{} {v}: {e:#}", palette.id));
                assert!(!textures.is_empty(), "{} {v} has no albedo", palette.id);
            }
        }
    }

    #[test]
    fn child_link_wears_links_hair_and_his_eyes_their_iris() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let child = labels("hair").iter().position(|l| *l == "Child").unwrap();
        let (out, _) = mesh(&romfs, "hair", child).unwrap();
        let heads = heads(&out);
        let hair = files::HAIR.stem().unwrap();
        assert!(
            heads
                .iter()
                .any(|h| h.albedo.as_deref() == Some(hair) && h.painted)
        );
        let eyes: Vec<_> = heads.iter().filter_map(|h| h.iris.as_ref()).collect();
        assert!(!eyes.is_empty());
        for eye in eyes {
            assert!(eye.albedo.ends_with("_Alb") && eye.mask.ends_with("_Gn3"));
        }
    }

    #[test]
    fn links_rigid_shapes_sit_on_him_not_out_on_an_arm() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        for link in [files::ADULT, files::CHILD] {
            let data = romfs.read(link).unwrap();
            let skeleton = ounce_formats::skeleton(&data).unwrap();
            let rigid = ounce_formats::shapes_of(&data).unwrap().into_iter();
            for shape in rigid.filter(|s| s.skin == 1) {
                let placed = ounce_formats::model_vertices(&data, &skeleton, &shape.name).unwrap();
                let x = placed.iter().map(|(p, _)| p[0]).sum::<f32>() / placed.len() as f32;
                assert!(x.abs() < 0.1, "{link} {}: centre x {x}", shape.name);
            }
        }
    }

    #[test]
    fn a_shield_reads_as_one_shape_with_its_albedo() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let shield = files::HYLIAN_SHIELD.stem().unwrap();
        let (out, albedos) = mesh(&romfs, "hylian-shield", 0).unwrap();
        assert_eq!(albedos, [shield]);
        let len = u32::from_le_bytes(out[..4].try_into().unwrap()) as usize;
        let heads: serde_json::Value = serde_json::from_slice(&out[4..4 + len]).unwrap();
        let shape = &heads[0];
        assert_eq!(shape["albedo"], shield);
        assert_eq!(shape["painted"], true);
        let (v, i) = (
            shape["vertices"].as_u64().unwrap() as usize,
            shape["indices"].as_u64().unwrap() as usize,
        );
        assert_eq!(i % 3, 0);
        assert_eq!(out.len(), 4 + len + v * 8 * 4 + i * 4);
    }

    #[test]
    fn a_tunic_hides_the_parts_of_link_the_game_hides_under_it() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let (out, _) = mesh(&romfs, "goron-tunic", 0).unwrap();
        let heads = heads(&out);
        let albedos: Vec<_> = heads.iter().filter_map(|h| h.albedo.as_deref()).collect();
        let has = |t: &files::Tex| albedos.contains(&t.stem().unwrap());
        assert!(has(&files::GORON_VEST));
        // The groups its ArmorParam names: his vest, cap and belts go.
        for his in [&files::ADULT_VEST, &files::ADULT_CAP, &files::ADULT_LEATHER] {
            assert!(!has(his), "{}", his.path);
        }
        // His gloves and boots stay.
        for his in [&files::ADULT_GLOVE, &files::ADULT_BOOTS] {
            assert!(has(his), "{}", his.path);
        }
    }
}
