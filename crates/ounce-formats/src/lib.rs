//! Readers and writers for the Ounce engine's romfs formats.
//!
//! The engine is the one Tears of the Kingdom ships on: files are zstd
//! frames compressed against one of four dictionaries, BYML v7 documents,
//! BFEVFL event flows, AINB node graphs, SARC actor packs, MSBT text, BNTX
//! textures, Havok tagfiles inside Phive physics files, a RESTBL size table that must cover every file the game
//! loads, and the save files GameData describes.

pub mod ainb;
mod bfres;
mod blocks;
mod bntx;
mod byml_doc;
mod crc32;
mod evfl;
mod fska;
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
mod tagfile;
mod walk;
mod zs;

pub use bfres::{
    Attribute, Lod, Vertex, attributes, bones, edit_vertices, lods, pack, pointers, positions,
    replace_mesh, replace_mesh_like, shape_names, unpack, vertices,
};
pub use bntx::{
    Texture, format, levels, redraw, redraw_halve, redraws, textures, textures_named, top_level,
};
pub use byml_doc::BymlDoc;
pub use crc32::crc32;
pub use evfl::{Actor, EntryPoint, Event, EventKind, Flowchart, Link, Params, Value, relink};
pub use fska::{Base, bases, names as animation_names};
pub use material::{param, set_param};
pub use msbt::{Msbt, Part, messages};
pub use murmur3::murmur3;
pub use navmesh::{Border, Edge, Face, Footing, NavMesh, closest};
pub use pack::{ActorPack, pack_file, pack_path};
pub use par::{par_batches, par_map, threads};
pub use rename::rename;
pub use restbl::Restbl;
pub use roead::byml::{Byml, Map as BymlMap};
pub use romfs::{ModBuilder, Romfs, copy_mod};
pub use sarc::replace_file;
pub use save::{Entry as SaveEntry, KEYED, Save, width as save_width};
pub use tagfile::{Field, Item, Obj, Tagfile, Type};
pub use walk::files_with_suffix;
pub use zs::{Dicts, frame_dict_id};
