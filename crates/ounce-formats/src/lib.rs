//! Readers and writers for the Ounce engine's romfs formats.
//!
//! The engine is the one Tears of the Kingdom ships on. The crate handles:
//!
//! - zstd frames compressed against one of four dictionaries
//! - BYML v7 documents and SARC actor packs
//! - BFEVFL event flows, AINB node graphs, ASB animation graphs and XLink
//!   sound and effect files
//! - MSBT text
//! - BFRES models, their materials and skeletal animations, and BNTX
//!   textures
//! - Havok tagfiles inside Phive physics files (navmeshes and shape meshes)
//! - the RESTBL size table, which must cover every file the game loads
//! - the save files GameData describes

pub mod ainb;
pub mod asb;
mod bfres;
mod blocks;
mod bntx;
pub mod bwav;
mod byml_doc;
mod crc32;
mod evfl;
pub mod fska;
pub mod layout;
mod material;
mod msbt;
mod murmur3;
mod navmesh;
mod pack;
mod par;
mod reader;
mod rename;
mod restbl;
mod romfs;
mod sarc;
mod save;
mod shape_mesh;
mod tagfile;
pub mod vec3;
pub mod vfx;
mod walk;
mod xlink;
mod zs;

pub use bfres::{
    Attribute, Lod, Vertex, attribute, attributes, bones, edit_vertices, hide_shape, lods, pack,
    pointers, positions, replace_mesh, replace_mesh_like, shape_names, triangles, unpack, vertices,
};
pub use bntx::{
    Texture, format, levels, redraw, redraw_halve, redraw_named, redraws, texture_named, textures,
    textures_named, top_level,
};
pub use byml_doc::BymlDoc;
pub use crc32::crc32;
pub use evfl::{Actor, EntryPoint, Event, EventKind, Flowchart, Link, Params, Value, relink};
pub use fska::{Base, bases, names as animation_names};
pub use material::{
    ModelMaterial, material_param, materials, param, set_material_param, set_param,
};
pub use msbt::{Msbt, Part, messages};
pub use murmur3::murmur3;
pub use navmesh::{Border, Edge, Face, Footing, NavMesh, closest};
pub use pack::{ActorPack, pack_file, pack_path};
pub use par::{par_batches, par_map, threads};
pub use rename::rename;
pub use restbl::Restbl;
pub use roead::byml::{Byml, Map as BymlMap};
pub use romfs::{ModBuilder, Romfs, copy_mod, stamped, stamped_among};
pub use sarc::{copies, copies_where, replace_file};
pub use save::{BOOL, Entry as SaveEntry, KEYED, Save, width as save_width};
pub use shape_mesh::{Material, ShapeMesh};
pub use tagfile::{Field, Item, Obj, Tagfile, Type};
pub use walk::files_with_suffix;
pub use xlink::{
    Call as XlinkCall, add_case as xlink_add_case, asset_params as xlink_asset_params,
    call_children as xlink_call_children, call_names as xlink_calls, calls as xlink_call_table,
    child_names as xlink_children, copy_call as xlink_copy_call,
    copy_property as xlink_copy_property, direct_values as xlink_direct_values, nest as xlink_nest,
    property_names as xlink_properties, tree_names as xlink_tree, trigger_names as xlink_triggers,
};
pub use zs::{Dicts, frame_dict_id};

/// `data` with three floats at `at` set to `rgb`, the fourth after them
/// kept: an effect's colour constant, a layout's palette entry.
pub fn set_rgb(data: &mut [u8], at: usize, rgb: [f32; 3]) {
    for (i, v) in rgb.iter().enumerate() {
        data[at + 4 * i..at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
}
