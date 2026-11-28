//! `zootr probe`: quick questions about the game's data, one line per hit
//! so the output greps well.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;
use ounce_formats::{ActorPack, Romfs, files_with_suffix, murmur3, pack_file};

use crate::items::{scalar, show};

mod actors;
mod arsenal;
mod climbs;
mod collision;
mod colors;
mod exefs;
mod filter;
mod formats;
mod gamedata;
mod hurts;
mod logic;
mod map;
mod names;
mod navmesh;
mod plan;
mod poses;
mod rooms;
mod shapes;
mod text;
mod uvs;
mod water;

use actors::print_actors;
pub use filter::Filter;
use formats::{
    ainb, ainbs, byml, files, grep, pool, print_types, restbl, strings, unpack, unpacked,
};
use logic::{checks, doors, exits, gates, groups, pouch, settings, shops, tools};
use navmesh::nav;

#[derive(Subcommand)]
pub enum Probe {
    /// Placed actors: file, actor, placement hash and position.
    Actors(Filter),
    /// Placed obstacles: count, actor and the tool it needs, marked `guess`
    /// where the tool comes from the actor's name.
    Tools(Filter),
    /// A GameData entry by name or hex hash. A Struct lists its members.
    Data {
        key: String,
        /// Instead, every Struct member that points at this entry.
        #[arg(long)]
        holders: bool,
        /// Instead, every BoolExp whose name or expression contains `key`.
        #[arg(long)]
        exps: bool,
        /// Instead, every Struct with a member named `key`, and the entry
        /// that member points at.
        #[arg(long)]
        member: bool,
    },
    /// A placement's own key (GameData's Bool64bitKey entry), by its
    /// placement hash in decimal.
    Key { hash: u64 },
    /// Each GameData table and how many entries it holds, as the file
    /// lists them, or with `table` that table's first entries (`MetaData`
    /// for the save layout).
    Tables { table: Option<String> },
    /// How Link moves and how far his gear reaches: his numbers, adult
    /// beside child, then each piece of gear's, keys containing `grep`.
    Arsenal { grep: Option<String> },
    /// An actor pack's files, or the files whose names contain `file`.
    Pack {
        actor: String,
        file: Option<String>,
        /// Read the pack a seed wrote into this mod folder instead.
        #[arg(long)]
        mod_dir: Option<PathBuf>,
    },
    /// What each kind of hit does to an enemy: its Life, each damage type's
    /// multiplier (0 is immune), and its exceptions, named.
    Hurts {
        actors: Vec<String>,
        /// Every enemy placed in banc files containing this too, like a
        /// dungeon's name.
        #[arg(long)]
        map: Option<String>,
    },
    /// Each text's murmur3, the way GameData keys its names, then the
    /// `n!(0x…)` that names it in our source (`known::hash`).
    Hash { text: Vec<String> },
    /// Every setting the app shows, by group: id, default, label,
    /// description, and a choice's options.
    Settings,
    /// Game text whose id or English contains this, any case.
    Text {
        pattern: String,
        /// Read this mod folder's text, as a seed wrote it, instead.
        #[arg(long)]
        mod_dir: Option<PathBuf>,
        /// Show each control tag's bytes in place, as `<0e 00 03 ..>`.
        #[arg(long)]
        tags: bool,
    },
    /// Files anywhere in the dump whose path contains this, with sizes.
    /// Reads the `.nup`, so it reaches files zootr doesn't extract.
    Files {
        pattern: String,
        /// Also write each hit here, decompressed, under its romfs path.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Print each BWAV stream's length in seconds, and `loop` if it loops.
        #[arg(long)]
        length: bool,
    },
    /// A shape's Havok mesh by its name, like
    /// `<piece>__Airwall`: vertex and triangle counts and its
    /// bounds in the actor's space.
    Mesh { name: String },
    /// Writes the files whose path contains this to `--out`, decompressed
    /// and with SARC archives opened into folders, for `rg` and `ls`.
    Unpack {
        pattern: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Files whose path contains `--path` and whose bytes contain this,
    /// decompressed and looking inside SARC archives, with a hit count.
    Grep {
        #[arg(value_parser = clap::builder::NonEmptyStringValueParser::new())]
        needle: String,
        #[arg(long, default_value = "")]
        path: String,
    },
    /// The names in each file whose path contains this, decompressed and
    /// looking inside SARC archives: for formats nothing decodes yet, like
    /// an ASB's commands and clips.
    Strings { path: String },
    /// Each skeletal animation in the animation archives whose path
    /// contains this, and the scale, rotation (radians) and translation it
    /// gives each bone at its first frame.
    Poses { path: String },
    /// How far below its origin each pool item's model reaches on a
    /// shelf, in the pose it plays there: the source of `shops/floors/table.rs`.
    Floors,
    /// Each shape in the models whose path contains this: the lowest and
    /// highest corner of its vertices as stored, before any pose, and each
    /// level of detail's vertex and triangle counts, and its attributes
    /// (`same` when every vertex holds the same bytes). A texture's size and
    /// format, and whether `redraw` can write it.
    Bounds {
        path: String,
        /// Euler X, Y, Z in radians, a scale and a lift, as `probe poses`
        /// prints them: also print each shape's lowest point so posed.
        #[arg(long, num_args = 5, allow_hyphen_values = true)]
        pose: Option<Vec<f32>>,
    },
    /// Decodes every BNTX texture whose path contains this to `out`, as PNG,
    /// including those inside SARC archives.
    Icons { pattern: String, out: PathBuf },
    /// Paints each shape of the models whose path contains this to `out`,
    /// as PNG: its triangles in UV space, colored by the axis they face
    /// (X red, Y green, Z blue, dark for the negative way), with each
    /// shape's UV bounds. Shows where a face lands on its textures.
    Uvs { pattern: String, out: PathBuf },
    /// Each material of a model, by a piece of its path (`Model/` and its
    /// actor name and a dot, the dot keeping its variants out):
    /// its textures and its four-float parameters by name hash, the
    /// colors Colors can write. Linear light, as the shader reads them.
    /// A path to a file on disk (a built mod's) reads that file instead.
    Materials { model: String },
    /// Each shape of a model, by a piece of its path as for `materials`:
    /// its material and that material's textures, the bone it hangs from,
    /// its skin count (0 or 1: its vertices sit in that bone's space) and
    /// its vertex count. What the colors model viewer draws.
    Model {
        model: String,
        /// Also write the decompressed model here.
        #[arg(long)]
        raw: Option<PathBuf>,
    },
    /// A texture's average color, and the average of its texels in each
    /// hue band given (`LO-HI`, 0 to 1, wrapping past 1): the vanilla
    /// colors a Colors palette records. `--png` also writes it out.
    Band {
        texture: String,
        bands: Vec<String>,
        #[arg(long)]
        png: Option<PathBuf>,
    },
    /// An effect bundle's color constants (`Effect/` and its owner's
    /// name, or a built file's path), under the eset each one falls in:
    /// where its value is in the decompressed file, its descriptor's name
    /// hash, and its floats, linear and often above 1. With `--sum`, the
    /// sum a Colors effect write records for the esets starting with each
    /// prefix given.
    Vfx {
        bundle: String,
        #[arg(long)]
        sum: Vec<String>,
        /// Also write the bundle, decompressed, to this file.
        #[arg(long)]
        raw: Option<PathBuf>,
    },
    /// A UI layout's color palette (every layout whose name contains
    /// `name`, in every layout archive): each entry's index, color, floats
    /// and where they are in the layout, and how many copies the archive
    /// stores. Then each material's white index.
    Layout {
        name: String,
        /// Also every material, picture and text record holding this
        /// palette index at any word: where else an entry may be named.
        #[arg(long)]
        uses: Option<u32>,
        /// Also every record named this, in hex, by kind.
        #[arg(long)]
        record: Option<String>,
    },
    /// The UI atlas's textures whose names contain `name`: each one's
    /// size and average color, over every texel and over the ones that
    /// show. With `--out`, each as a PNG there too.
    Atlas {
        name: String,
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also count each texture's shown, colored texels by hue, in
        /// steps of 10 degrees: what a color band has to cover.
        #[arg(long)]
        hues: bool,
    },
    /// Paints each albedo a marked breakable shows while it holds a check
    /// and writes it to `out` as PNG, as a seed paints it.
    Marks { out: PathBuf },
    /// Paints each albedo a chest look shows (Chest Looks Match Contents)
    /// and writes it to `out` as PNG, as a seed paints it
    Chests { out: PathBuf },
    /// A save's pouch: each actor's `StockNum` and `IsGet`, where either
    /// is set. A slot number or a path to a `progress.sav`.
    Pouch {
        slot: String,
        /// Every pouch actor, held or not.
        #[arg(long)]
        all: bool,
    },
    /// Arrival points a teleport can go to, as `MAP/SPOT`, whose text
    /// contains this, any case.
    Places { filter: Option<String> },
    /// Map exits into or out of a node whose name contains this, any case:
    /// era, from, to, the flag that gates it (`-` for none, `!flag` while
    /// unset) and the rule file's need. Warp songs' pads are `StartPos`
    /// actors named after the song (`probe actors --gyaml StartPos`).
    Exits { node: String },
    /// The calls in a sound user (named for its actor), in order, or those
    /// whose name contains `grep`, any case. User `*` with `--params`:
    /// every user with a sound bank, matching streams too.
    Calls {
        user: String,
        grep: Option<String>,
        /// Instead, this call and every call under it.
        #[arg(long)]
        tree: Option<String>,
        /// Instead, the calls right under this container.
        #[arg(long, conflicts_with = "tree")]
        children: Option<String>,
        /// Instead, the call each action and property trigger plays.
        #[arg(long, conflicts_with_all = ["tree", "children"])]
        triggers: bool,
        /// Each call's kind (asset or container), its param offset (calls
        /// that share one play the same thing) and an asset's names, by
        /// field bit: `1:` is the stream it plays, `2:` its category.
        #[arg(long, conflicts_with_all = ["tree", "children", "triggers"])]
        params: bool,
    },
    /// Each chest a logic graph gates: placement hash, area and rule.
    Gates {
        /// Every chest a graph drives, gated or not, with the wiring
        /// behind its rule.
        #[arg(long)]
        trace: bool,
        /// Instead, every placement a logic node of this class drives,
        /// with the wiring behind each input, like
        /// `ActorLogicEventRequestActor`.
        #[arg(long)]
        node: Option<String>,
        /// Instead, every placement a logic node makes appear: hash, area,
        /// actor, the graph and node, how many times the graph is placed,
        /// and whether its group is quiet (reads state and asks for actors,
        /// nothing else).
        #[arg(long)]
        creators: bool,
        /// With `--creators`, only the pickup checks.
        #[arg(long, requires = "creators")]
        checks: bool,
    },
    /// Every BYML document whose path contains this, as an indented tree,
    /// one value per line, including those inside SARC archives.
    Byml {
        pattern: String,
        /// The pattern is a file on disk instead, like a seed's
        /// `romfs/Banc/...`: what a seed wrote, where the dump has the
        /// vanilla one.
        #[arg(long)]
        file: bool,
    },
    /// A logic or AI graph, one node per line: index, class, values, where
    /// each linked input comes from, then its children (`plug:#N`) and its
    /// other links by plug kind (`~k3:#N`, a "then" after a sequence step).
    Ainb {
        /// Romfs path, like `Logic/NAME.logic.module.ainb`,
        /// or a graph inside an actor pack as `probe grep` names it, like
        /// `Pack/Actor/NAME.pack.zs/AI/NAME.brain.root.ainb`.
        path: String,
        /// Save the graph's raw bytes here instead of printing it.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Every logic and AI graph the AINB reader can't parse, with why,
    /// grouped by the failing node's class.
    Ainbs,
    /// Each check the shuffle deals: hash, era, area, name, vanilla item,
    /// and what logic says it needs there, with default settings unless
    /// `--setting` says otherwise.
    Checks {
        /// Banc folders containing this, like `KokiriForest`.
        #[arg(long)]
        area: Option<String>,
        /// A setting by id, as `shuffle` takes it, e.g. `freestanding=true`.
        /// Repeatable.
        #[arg(long = "setting", value_name = "ID=VALUE", value_parser = crate::cli::args::setting)]
        settings: Vec<(String, serde_json::Value)>,
        /// Only the checks logic has but no item or flag reaches at either
        /// age: its odd cases, from a wrong-age rule to a plain `never`.
        #[arg(long)]
        never: bool,
    },
    /// How many rolled dungeon door layouts logic can beat, and how often a
    /// seed's tries would all fail. Doors shuffle `on` unless a
    /// `--setting` says otherwise.
    Doors {
        #[arg(long, default_value_t = 200, value_parser = clap::value_parser!(u32).range(1..))]
        rolls: u32,
        /// A setting by id, as `shuffle` takes it. Repeatable.
        #[arg(long = "setting", value_name = "ID=VALUE", value_parser = crate::cli::args::setting)]
        settings: Vec<(String, serde_json::Value)>,
    },
    /// How many of `seeds` seeds logic fills within its tries, and the
    /// last failure's reason: a dungeon PR's before and after.
    Seeds {
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u64).range(1..))]
        seeds: u64,
        /// Start from a favorite's settings, as `shuffle` does
        /// (`randomizer` for a dungeon PR's table), then lay each
        /// `--setting` over it.
        #[arg(long, value_name = "NAME", value_parser = crate::cli::args::favorite)]
        favorite: Option<crate::settings::Settings>,
        /// A setting by id, as `shuffle` takes it. Repeatable.
        #[arg(long = "setting", value_name = "ID=VALUE", value_parser = crate::cli::args::setting)]
        settings: Vec<(String, serde_json::Value)>,
    },
    /// The check ledger as JSON: every check with every shuffle on, dealt
    /// or left out, with what the code knows of it. `--unknown` lists only
    /// the left-out placements nothing explains yet.
    Ledger {
        #[arg(long)]
        unknown: bool,
    },
    /// Every spot of a seed with the sweep's widest mix, and how the sweep
    /// triggers it (open, talk, play) or why it skips it. The pool to
    /// confirm, for joining with the sweep's results by hash.
    Pool {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Only the spots the tracker has no flag for, which never tick
        /// as checked, with their holder.
        #[arg(long)]
        untracked: bool,
    },
    /// Rows of an RSDB table, like `EventSetting` or `Challenge`, whose
    /// row id contains `pattern`.
    Rsdb { table: String, pattern: String },
    /// Each actor's tags in the `Tag` table, a bit per actor and tag. An
    /// actor the table doesn't list has none.
    Tags { actors: Vec<String> },
    /// The RESTBL entry for each romfs path, or `-` where it has none. A
    /// pack's inner files are keyed by their path inside it.
    Restbl { paths: Vec<String> },
    /// Each shop slot: shop and number, check hash, vanilla goods, the
    /// price `--seed` rolls and every placement.
    Shops {
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// A map's floor plan as JSON, for the ledger to draw: its navmesh's
    /// faces by room, every placement, and each locked door's two rooms.
    Map {
        /// A map, by its Banc param name.
        map: String,
    },
    /// A draft names file for one area (`logic/names/`): each check the
    /// world has a location for, by room and position, its current name on
    /// the line, and its rule's name and comment, room and vanilla item as
    /// comments above.
    Names {
        /// The area, by its Archipelago name, like "Water Temple".
        area: String,
    },
    /// A map's rooms: its navmesh's faces joined without a locked door,
    /// with the chests and locks in each.
    Rooms {
        /// A map, by its Banc param name.
        map: String,
    },
    /// A map's placed actors by their solid shapes: each actor's count,
    /// whether it cuts the navmesh at run time, and its main body's parts.
    Shapes {
        /// A map, by its Banc param name.
        map: String,
        /// Only actors whose name contains this, with each placement's
        /// footprint.
        #[arg(long)]
        gyaml: Option<String>,
        /// Instead, walk across each solid the route keeps clear of, with
        /// and without the solids, and tell which walks now go round.
        #[arg(long)]
        cross: bool,
    },
    /// A map's ladders and climb walls: each one's foot and top, and the
    /// floor a route climbs between, or why it has no link.
    Climbs {
        /// A map, by its Banc param name.
        map: String,
    },
    /// A map's waters: each body's planes, its stretches of shore and the
    /// swims a route takes between them.
    Water {
        /// A map, by its Banc param name.
        map: String,
        /// Also draw it from above to this SVG file: the mesh shaded by
        /// height, each water's box, its shore points and its swims.
        #[arg(long)]
        svg: Option<std::path::PathBuf>,
        /// Draw only around this point X,Y,Z: faces within 8 m of its
        /// height, waters within 4 m.
        #[arg(long, value_parser = point, allow_hyphen_values = true)]
        at: Option<[f32; 3]>,
        /// How far round `--at` to draw, in m.
        #[arg(long, default_value_t = 30.0)]
        within: f32,
        /// Instead, the route from this point X,Y,Z to `--to`, swims and
        /// all.
        #[arg(long, value_parser = point, allow_hyphen_values = true, requires = "to")]
        from: Option<[f32; 3]>,
        #[arg(long, value_parser = point, allow_hyphen_values = true, requires = "from")]
        to: Option<[f32; 3]>,
    },
    /// Maps every offset of 1.1.1's table into another build's `main` and,
    /// once each is matched (or confirmed by hand in `port::CONFIRMED`),
    /// writes that build's table as Rust to `--out`. The report (each
    /// offset's method, window, candidates and confidence, the rooms and
    /// the field offsets left unconfirmed) goes to stdout or `--report`.
    Port {
        #[arg(long)]
        to: crate::game::Release,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long, default_value = crate::port::TABLE_FILE)]
        out: PathBuf,
    },
    /// One build's code, `main`, decoded from the dump and written flat to
    /// `--out` as it lies in memory (text, rodata and data at their
    /// offsets, bss zeroed), with its build ID, segments and MOD0.
    Main {
        #[arg(long)]
        build: crate::game::Release,
        #[arg(long)]
        out: PathBuf,
    },
    /// The sweep's plan for one round: each holder kind by how it'd be
    /// swept (open, talk, play, skip), and every skipped spot with why.
    Plan {
        /// A settings mix, by the start of its name. Defaults to the first.
        #[arg(long)]
        mix: Option<String>,
        /// The seed to shuffle.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Print the whole plan of each spot whose name holds this.
        #[arg(long)]
        only: Option<String>,
    },
    /// A map's collision meshes as the game's collision filter sorts
    /// them: each piece's meshes, invisible or seen, what each does to Link
    /// on foot, and its materials. With `--under`, the floor under each
    /// other placement instead (chests, pots, start spots).
    Collision {
        /// A map, by its Banc param name.
        map: String,
        #[arg(long)]
        under: bool,
        /// Every collision triangle over or under a point X,Y,Z instead.
        #[arg(long, value_parser = point, allow_hyphen_values = true)]
        at: Option<[f32; 3]>,
        /// Every type one mesh's tagfile declares, by its name like
        /// `<piece>__Physical`, instead.
        #[arg(long)]
        types: Option<String>,
    },
    /// A map's navmesh (`Phive/NavMesh/MAP`): counts and bounds, or with
    /// `--at`, the face under a point and the mesh's edges near it.
    Nav {
        /// A map, by its Banc param name.
        map: String,
        /// Every type the file declares, with its fields.
        #[arg(long)]
        types: bool,
        /// A point X,Y,Z, like Link's from `dev state`.
        #[arg(long, value_parser = point, allow_hyphen_values = true)]
        at: Option<[f32; 3]>,
        /// How far from `--at` to list border edges, measured flat.
        #[arg(long, default_value_t = 4.0)]
        within: f32,
    },
    /// Each `AiGroups` entry in the Banc files containing `file` whose graph
    /// contains `logic`: the graph and what its actor nodes refer to.
    Groups {
        file: String,
        #[arg(long, default_value = "")]
        logic: String,
    },
}

/// `game` is the dump, for the probes that read past the extracted romfs.
pub fn run(romfs: &Romfs, game: &Path, probe: Probe) -> Result<()> {
    match probe {
        Probe::Actors(filter) => print_actors(romfs, &filter),
        Probe::Tools(filter) => tools(romfs, &filter),
        Probe::Data {
            key,
            holders,
            exps,
            member,
        } => gamedata::data(romfs, &key, holders, exps, member),
        Probe::Key { hash } => gamedata::key(romfs, hash),
        Probe::Tables { table } => crate::gamedata::print_tables(romfs, table.as_deref()),
        Probe::Arsenal { grep } => arsenal::arsenal(romfs, grep.as_deref()),
        Probe::Hurts { actors, map } => hurts::print(romfs, &actors, map),
        Probe::Pack {
            actor,
            file,
            mod_dir,
        } => pack(romfs, &actor, file.as_deref(), mod_dir.as_deref()),
        Probe::Hash { text } => {
            text.iter().for_each(|t| {
                let (data, ours) = (murmur3(t.as_bytes()), crate::known::hash(t));
                println!("{data:08x} n!({ours:#018x}) {t}");
            });
            Ok(())
        }
        Probe::Text {
            pattern,
            mod_dir,
            tags,
        } => text::text(romfs, &pattern, mod_dir.as_deref(), tags),
        Probe::Settings => {
            settings();
            Ok(())
        }
        Probe::Files {
            pattern,
            out,
            length,
        } => files(romfs, game, &pattern, out.as_deref(), length),
        Probe::Mesh { name } => {
            let mesh = crate::nav::mesh::mesh(romfs, game, &name)?
                .with_context(|| format!("the dump has no mesh {name}"))?;
            let xyz = |p: [f32; 3]| format!("{:.3},{:.3},{:.3}", p[0], p[1], p[2]);
            let (lo, hi) = mesh.bounds().context("the mesh has no vertices")?;
            println!(
                "{} vertices, {} triangles, from {} to {}",
                mesh.vertices.len(),
                mesh.triangles.len(),
                xyz(lo),
                xyz(hi)
            );
            Ok(())
        }
        Probe::Map { map } => map::map(romfs, game, &map),
        Probe::Rooms { map } => rooms::rooms(romfs, game, &map),
        Probe::Names { area } => names::draft(romfs, &area),
        Probe::Climbs { map } => climbs::climbs(romfs, game, &map),
        Probe::Water {
            map,
            from: Some(from),
            to: Some(to),
            ..
        } => water::route(romfs, game, &map, from, to),
        Probe::Water {
            map,
            svg,
            at,
            within,
            ..
        } => water::water(romfs, game, &map, svg.map(|f| (f, at, within))),
        Probe::Shapes {
            map, cross: true, ..
        } => shapes::crossings(romfs, game, &map),
        Probe::Shapes { map, gyaml, .. } => shapes::shapes(romfs, game, &map, gyaml.as_deref()),
        Probe::Collision {
            types: Some(mesh), ..
        } => {
            let bytes = crate::nav::dump_file(romfs, game, &format!("Phive/Shape/Dcc/{mesh}."))?;
            let bytes = bytes.with_context(|| format!("no mesh {mesh}"))?;
            print_types(&bytes)?;
            let m = ounce_formats::ShapeMesh::parse(&bytes)?;
            println!("sections {:?}", m.section_sizes);
            for (key, tag) in &m.tag_runs {
                println!("tag run {key:#x} ({key}) -> {tag}");
            }
            Ok(())
        }
        Probe::Collision { map, under, at, .. } => {
            collision::collision(romfs, game, &map, under, at)
        }
        Probe::Nav {
            map,
            types,
            at,
            within,
        } => nav(romfs, game, &map, types, at, within),
        Probe::Main { build, out } => exefs::print_main(game, build, &out),
        Probe::Port { to, report, out } => exefs::port(game, to, report.as_deref(), &out),
        Probe::Plan { mix, seed, only } => plan::plan(romfs, mix.as_deref(), seed, only.as_deref()),
        Probe::Unpack { pattern, out } => unpack(romfs, game, &pattern, &out),
        Probe::Grep { needle, path } => grep(romfs, game, &needle, &path),
        Probe::Strings { path } => names(romfs, game, &path),
        Probe::Poses { path } => poses::poses(romfs, game, &path),
        Probe::Bounds { path, pose } => poses::bounds(romfs, game, &path, pose.as_deref()),
        Probe::Floors => poses::floors(romfs, game),
        Probe::Icons { pattern, out } => icons(romfs, game, &pattern, &out),
        Probe::Uvs { pattern, out } => uvs::uvs(romfs, game, &pattern, &out),
        Probe::Marks { out } => crate::marks::preview(romfs, &out),
        Probe::Chests { out } => crate::chest_looks::preview(romfs, &out),
        Probe::Materials { model } => colors::materials(romfs, game, &model),
        Probe::Model { model, raw } => colors::model(romfs, game, &model, raw.as_deref()),
        Probe::Band {
            texture,
            bands,
            png,
        } => colors::band(romfs, game, &texture, &bands, png.as_deref()),
        Probe::Layout { name, uses, record } => {
            colors::layout(romfs, &name, uses, record.as_deref())
        }
        Probe::Atlas { name, out, hues } => colors::atlas(romfs, &name, out.as_deref(), hues),
        Probe::Vfx { bundle, sum, raw } => colors::vfx(romfs, game, &bundle, &sum, raw.as_deref()),
        Probe::Pouch { slot, all } => pouch(romfs, &slot, all),
        Probe::Places { filter } => {
            let filter = filter.unwrap_or_default().to_lowercase();
            for place in crate::dev::places(romfs)? {
                let place = place.to_string();
                if place.to_lowercase().contains(&filter) {
                    println!("{place}");
                }
            }
            Ok(())
        }
        Probe::Exits { node } => exits(romfs, &node),
        Probe::Calls {
            user,
            grep,
            tree,
            children,
            triggers,
            params,
        } => {
            let file = romfs.read(crate::ageless_items::SOUNDS)?;
            let grep = grep.unwrap_or_default().to_lowercase();
            if params {
                let every = user == "*";
                let users = match every {
                    true => crate::audio::banked_users(&romfs.read(crate::ageless_items::BANKS)?)?,
                    false => vec![user],
                };
                for user in users {
                    let table = match ounce_formats::xlink_call_table(&file, &user) {
                        Err(_) if every => continue,
                        table => table?,
                    };
                    let names = ounce_formats::xlink_calls(&file, &user)?;
                    for (name, call) in names.iter().zip(&table) {
                        let (kind, names) = match call.container {
                            true => ("container", String::new()),
                            false => {
                                let names = ounce_formats::xlink_asset_names(&file, call.param)?;
                                let names = names.iter().map(|(bit, n)| format!("{bit}:{n}"));
                                ("asset", names.collect::<Vec<_>>().join(" "))
                            }
                        };
                        let seen = match every {
                            true => format!("{name} {names}"),
                            false => name.to_string(),
                        };
                        if seen.to_lowercase().contains(&grep) {
                            let at = if every {
                                format!("{user}\t")
                            } else {
                                String::new()
                            };
                            println!("{at}{name}\t{kind}\t{:#x}\t{names}", call.param);
                        }
                    }
                }
                return Ok(());
            }
            let names = match (tree, children) {
                (Some(key), _) => ounce_formats::xlink_tree(&file, &user, &key)?,
                (_, Some(key)) => ounce_formats::xlink_children(&file, &user, &key)?,
                _ if triggers => ounce_formats::xlink_triggers(&file, &user)?,
                _ => ounce_formats::xlink_calls(&file, &user)?,
            };
            for name in names {
                if name.to_lowercase().contains(&grep) {
                    println!("{name}");
                }
            }
            Ok(())
        }
        Probe::Gates {
            trace,
            node,
            creators,
            checks,
        } => gates(romfs, trace, node, creators, checks),
        Probe::Checks {
            area,
            settings,
            never,
        } => checks(romfs, area.as_deref(), &settings, never),
        Probe::Doors { rolls, settings } => doors(romfs, rolls, &settings),
        Probe::Seeds {
            seeds,
            favorite,
            settings,
        } => logic::seeds(romfs, seeds, favorite.unwrap_or_default(), &settings),
        Probe::Byml { pattern, file } => byml(romfs, game, &pattern, file),
        Probe::Ainb { path, out } => ainb(romfs, &path, out.as_deref()),
        Probe::Ainbs => ainbs(romfs),
        Probe::Rsdb { table, pattern } => rsdb(romfs, game, &table, &pattern),
        Probe::Tags { actors } => tags(romfs, game, &actors),
        Probe::Groups { file, logic } => groups(romfs, &file, &logic),
        Probe::Restbl { paths } => restbl(romfs, &paths),
        Probe::Shops { seed } => shops(romfs, seed),
        Probe::Pool { seed, untracked } => pool(romfs, seed, untracked),
        Probe::Ledger { unknown } => crate::ledger::print(romfs, unknown),
    }
}

/// Counts one more `key` in `by`, making its value with `make` the first
/// time it's seen.
fn tally<'a, T>(
    by: &mut BTreeMap<&'a str, (usize, T)>,
    key: &'a str,
    make: impl FnOnce() -> Result<T>,
) -> Result<()> {
    match by.entry(key) {
        std::collections::btree_map::Entry::Occupied(e) => e.into_mut().0 += 1,
        std::collections::btree_map::Entry::Vacant(e) => {
            e.insert((1, make()?));
        }
    }
    Ok(())
}

fn pack(romfs: &Romfs, actor: &str, file: Option<&str>, mod_dir: Option<&Path>) -> Result<()> {
    let pack = match mod_dir {
        None => ActorPack::load(romfs, actor)?,
        Some(dir) => {
            let rel = pack_file(actor);
            let at = dir.join("romfs").join(&rel);
            let raw = std::fs::read(&at).with_context(|| format!("reading {}", at.display()))?;
            ActorPack::from_bytes(actor, romfs.dicts().decompress(&rel, &raw)?)?
        }
    };
    for name in pack.files() {
        match file {
            None => println!("{name}"),
            Some(want) if name.contains(want) && name.ends_with(".ainb") => {
                println!("{name}");
                for s in strings(pack.raw(&name).unwrap_or_default()) {
                    println!("  {s}");
                }
            }
            Some(want) if name.contains(want) => {
                println!("{name}");
                let map = pack.resolve(&name)?.unwrap_or_default();
                let mut keys: Vec<_> = map.iter().collect();
                keys.sort_by_key(|(k, _)| k.as_str());
                for (k, v) in keys {
                    println!("  {k}: {}", show(v));
                }
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn names(romfs: &Romfs, game: &Path, path: &str) -> Result<()> {
    unpacked(romfs, game, path, |path, data, _| {
        println!("{path}");
        for name in strings(data) {
            println!("  {name}");
        }
        Ok(())
    })
}

fn rsdb(romfs: &Romfs, game: &Path, table: &str, pattern: &str) -> Result<()> {
    unpacked(romfs, game, &format!("RSDB/{table}."), |_, data, _| {
        let doc = ounce_formats::BymlDoc::parse(data)?;
        for row in doc.root.as_array()? {
            let name = row.as_map()?.get("__RowId").map(scalar).unwrap_or_default();
            if name.contains(pattern) {
                println!("{name} {}", show(row));
            }
        }
        Ok(())
    })
}

fn tags(romfs: &Romfs, game: &Path, actors: &[String]) -> Result<()> {
    unpacked(romfs, game, "RSDB/Tag.", |_, data, _| {
        let table = crate::rsdb::Tags::parse(data)?;
        for actor in actors {
            match table.of(actor) {
                Some(on) => println!("{actor}: {}", on.join(" ")),
                None => println!("{actor}: not listed"),
            }
        }
        Ok(())
    })
}

fn icons(romfs: &Romfs, game: &Path, pattern: &str, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    unpacked(romfs, game, pattern, |path, data, member| {
        if member && !path.ends_with(".bntx") {
            return Ok(());
        }
        match ounce_formats::textures(data) {
            Ok(all) => {
                let redrawn = match ounce_formats::format(data) {
                    Ok(f) if ounce_formats::redraws(f) => format!("format {f:#06x}, redrawable"),
                    Ok(f) => format!("format {f:#06x}, not redrawable"),
                    Err(e) => format!("format unread: {e:#}"),
                };
                for t in all {
                    std::fs::write(
                        out.join(format!("{}.png", t.name)),
                        crate::sprites::png(&t)?,
                    )?;
                    println!("{}x{} {} {path} ({redrawn})", t.width, t.height, t.name);
                }
            }
            Err(e) => println!("FAIL {path}: {e:#}"),
        }
        Ok(())
    })
}

/// Every actor pack's name by its murmur3, which is how the pouch keys
/// its items.
fn actor_names(romfs: &Romfs) -> Result<HashMap<u32, String>> {
    let (dir, suffix) = ("Pack/Actor", ".pack.zs");
    Ok(files_with_suffix(&romfs.root().join(dir), suffix)?
        .iter()
        .filter_map(|p| {
            p.file_name()?
                .to_str()?
                .strip_suffix(suffix)
                .map(str::to_owned)
        })
        .map(|name| (murmur3(name.as_bytes()), name))
        .collect())
}

fn point(arg: &str) -> Result<[f32; 3]> {
    let v: Vec<f32> = arg
        .split(',')
        .map(|n| n.trim().parse())
        .collect::<Result<_, _>>()?;
    v.try_into().map_err(|_| anyhow::anyhow!("want X,Y,Z"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arguments `run` can't use are refused when parsed: an empty grep
    /// needle (no window of 0 bytes), `--to` without `--from`, and 0 rolls
    /// (odds of 0 out of 0).
    #[test]
    fn unusable_arguments_are_refused() {
        #[derive(clap::Parser)]
        struct Args {
            #[command(subcommand)]
            probe: Probe,
        }
        let parse = |line: &[&str]| {
            <Args as clap::Parser>::try_parse_from(
                std::iter::once("zootr").chain(line.iter().copied()),
            )
            .map(|a| a.probe)
        };
        assert!(parse(&["grep", ""]).is_err());
        assert!(parse(&["grep", "Fake"]).is_ok());
        assert!(parse(&["water", "FakeMap", "--to", "1,2,3"]).is_err());
        assert!(parse(&["water", "FakeMap", "--from", "1,2,3", "--to", "4,5,6"]).is_ok());
        assert!(parse(&["doors", "--rolls", "0"]).is_err());
        assert!(matches!(
            parse(&["doors", "--rolls", "1"]),
            Ok(Probe::Doors { rolls: 1, .. })
        ));
    }

    #[test]
    fn points_parse_with_negatives() {
        assert_eq!(point("1,-2.5, 3").unwrap(), [1.0, -2.5, 3.0]);
        assert!(point("1,2").is_err());
    }
}
