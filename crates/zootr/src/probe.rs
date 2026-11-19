//! `zootr probe`: quick questions about the game's data, one line per hit
//! so the output greps well.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use ounce_formats::{
    ActorPack, Byml, NavMesh, Romfs, Tagfile, files_with_suffix, messages, murmur3,
};
use roead::sarc::Sarc;

use crate::checks::{GYAML, actors, banc_files, flat, hash_of, load_banc, str_field, vec3};
use crate::gamedata::GameData;
use crate::items::scalar;
use crate::known::flag;
use crate::names::TEXT;
use crate::nav::navmesh_file;

mod climbs;
mod formats;
mod logic;
mod plan;
mod poses;
mod rooms;
mod shapes;
mod water;

use formats::{ainb, byml, files, grep, pool, restbl, strings, unpack, unpacked};
use logic::{checks, exits, gates, groups, pouch, settings, shops, tools};

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
    /// An actor pack's files, or the files whose names contain `file`.
    Pack { actor: String, file: Option<String> },
    /// Each text's murmur3, the way GameData keys its names, then the
    /// `n!(0x…)` that names it in our source (`known::hash`).
    Hash { text: Vec<String> },
    /// Every setting the app shows, by group: id, default, label,
    /// description, and a choice's options.
    Settings,
    /// Game text whose id or English contains this, any case.
    Text { pattern: String },
    /// Files anywhere in the dump whose path contains this, with sizes.
    /// Reads the `.nup`, so it reaches files zootr doesn't extract.
    Files {
        pattern: String,
        /// Also write each hit here, decompressed, under its romfs path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
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
    /// Each shape in the models whose path contains this: the lowest and
    /// highest corner of its vertices as stored, before any pose.
    /// How far below its origin each pool item's model reaches on a
    /// shelf, in the pose it plays there: the source of `shops/floors.rs`.
    Floors,
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
    Byml { pattern: String },
    /// A logic or AI graph, one node per line: index, class, values, and
    /// where each linked input comes from.
    Ainb {
        /// Romfs path, like `Logic/NAME.logic.module.ainb`,
        /// or a graph inside an actor pack as `probe grep` names it, like
        /// `Pack/Actor/NAME.pack.zs/AI/NAME.brain.root.ainb`.
        path: String,
        /// Save the graph's raw bytes here instead of printing it.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
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
        #[arg(long, value_parser = point, allow_hyphen_values = true)]
        to: Option<[f32; 3]>,
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

#[derive(clap::Args, Default)]
pub struct Filter {
    /// Actor names containing this.
    #[arg(long)]
    gyaml: Option<String>,
    /// Banc paths containing this, like an area or `_Child_`.
    #[arg(long)]
    file: Option<String>,
    /// One placement hash.
    #[arg(long)]
    hash: Option<u64>,
    /// Only actors within `--within` of X,Y,Z, measured flat.
    #[arg(long, value_parser = point, allow_hyphen_values = true)]
    near: Option<[f32; 3]>,
    #[arg(long, default_value_t = 10.0)]
    within: f32,
    /// Each placement once, though child and adult files repeat it.
    #[arg(long)]
    dedupe: bool,
    /// Every field, not only the position.
    #[arg(long)]
    fields: bool,
    /// Placements and unique hashes per actor name, instead of each one.
    #[arg(long)]
    count: bool,
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
        } => data(romfs, &key, holders, exps, member),
        Probe::Key { hash } => key(romfs, hash),
        Probe::Tables { table } => crate::gamedata::print_tables(romfs, table.as_deref()),
        Probe::Pack { actor, file } => pack(romfs, &actor, file.as_deref()),
        Probe::Hash { text } => {
            text.iter().for_each(|t| {
                let (data, ours) = (murmur3(t.as_bytes()), crate::known::hash(t));
                println!("{data:08x} n!({ours:#018x}) {t}");
            });
            Ok(())
        }
        Probe::Text { pattern } => text(romfs, &pattern),
        Probe::Settings => {
            settings();
            Ok(())
        }
        Probe::Files { pattern, out } => files(romfs, game, &pattern, out.as_deref()),
        Probe::Rooms { map } => rooms::rooms(romfs, game, &map),
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
        Probe::Nav {
            map,
            types,
            at,
            within,
        } => nav(romfs, game, &map, types, at, within),
        Probe::Plan { mix, seed, only } => plan::plan(romfs, mix.as_deref(), seed, only.as_deref()),
        Probe::Unpack { pattern, out } => unpack(romfs, game, &pattern, &out),
        Probe::Grep { needle, path } => grep(romfs, game, &needle, &path),
        Probe::Strings { path } => names(romfs, game, &path),
        Probe::Poses { path } => poses::poses(romfs, game, &path),
        Probe::Bounds { path, pose } => poses::bounds(romfs, game, &path, pose.as_deref()),
        Probe::Floors => poses::floors(romfs, game),
        Probe::Icons { pattern, out } => icons(romfs, game, &pattern, &out),
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
        Probe::Byml { pattern } => byml(romfs, game, &pattern),
        Probe::Ainb { path, out } => ainb(romfs, &path, out.as_deref()),
        Probe::Rsdb { table, pattern } => rsdb(romfs, game, &table, &pattern),
        Probe::Tags { actors } => tags(romfs, game, &actors),
        Probe::Groups { file, logic } => groups(romfs, &file, &logic),
        Probe::Restbl { paths } => restbl(romfs, &paths),
        Probe::Shops { seed } => shops(romfs, seed),
        Probe::Pool { seed, untracked } => pool(romfs, seed, untracked),
        Probe::Ledger { unknown } => crate::ledger::print(romfs, unknown),
    }
}

struct Hit {
    /// Without `Banc/` and the extension.
    file: String,
    gyaml: String,
    hash: u64,
    pos: [f32; 3],
    actor: Byml,
}

fn hits(romfs: &Romfs, f: &Filter) -> Result<Vec<Hit>> {
    let has = |want: &Option<String>, s: &str| want.as_ref().is_none_or(|w| s.contains(w.as_str()));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for rel in banc_files(romfs)? {
        if !has(&f.file, &rel) {
            continue;
        }
        for a in actors(&load_banc(romfs, &rel)?.root)? {
            let gyaml = str_field(a, GYAML).unwrap_or_default();
            let hash = hash_of(a).unwrap_or_default();
            let pos = vec3(a, "Translate").unwrap_or_default();
            let near = f.near.is_none_or(|p| flat(p, pos) <= f.within);
            if has(&f.gyaml, gyaml)
                && f.hash.is_none_or(|h| h == hash)
                && near
                && (!f.dedupe || seen.insert(hash))
            {
                out.push(Hit {
                    file: short(&rel).to_owned(),
                    gyaml: gyaml.to_owned(),
                    hash,
                    pos,
                    actor: a.clone(),
                });
            }
        }
    }
    Ok(out)
}

fn print_actors(romfs: &Romfs, f: &Filter) -> Result<()> {
    let hits = hits(romfs, f)?;
    if f.count {
        let mut by: BTreeMap<&str, (usize, HashSet<u64>)> = BTreeMap::new();
        for h in &hits {
            let e = by.entry(&h.gyaml).or_default();
            e.0 += 1;
            e.1.insert(h.hash);
        }
        for (gyaml, (n, unique)) in by {
            println!("{n:>5} {:>5} {gyaml}", unique.len());
        }
    }
    for h in hits.iter().filter(|_| !f.count) {
        let [x, y, z] = h.pos;
        let mut line = format!("{} {} {} {x:.1},{y:.1},{z:.1}", h.file, h.gyaml, h.hash);
        if let Some(p) = f.near {
            line += &format!(" d={:.1}", flat(p, h.pos));
        }
        if f.fields {
            let skip = [GYAML, "Hash", "Translate"];
            let map = h.actor.as_map()?;
            let mut rest: Vec<_> = map
                .iter()
                .filter(|(k, _)| !skip.contains(&k.as_str()))
                .collect();
            rest.sort_by_key(|(k, _)| k.as_str());
            for (k, v) in rest {
                line += &format!(" {k}={}", show(v));
            }
        } else if let Some(name) = str_field(&h.actor, "Name") {
            line += &format!(" Name={name}");
        }
        println!("{line}");
    }
    eprintln!("{} placements", hits.len());
    Ok(())
}

fn key(romfs: &Romfs, hash: u64) -> Result<()> {
    let doc = ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH)?)?;
    let keyed = doc.root.as_map()?["Data"].as_map()?[ounce_formats::KEYED].as_array()?;
    let found = keyed.iter().find(|e| {
        let h = e.as_map().ok().and_then(|m| m.get("Hash"));
        h.and_then(|h| crate::gamedata::int(h).ok()) == Some(hash as i64)
    });
    match found {
        Some(e) => println!("{e:?}"),
        None => println!("no key {hash}"),
    }
    Ok(())
}

fn data(romfs: &Romfs, key: &str, holders: bool, exps: bool, member: bool) -> Result<()> {
    if exps {
        let survey = crate::flags::Survey::load(romfs, &crate::events::Flows::load(romfs)?)?;
        let gd = &survey.gamedata;
        let mut found: Vec<_> = gd
            .of_kind(flag::BOOL_EXP)
            .map(|h| Ok((gd.bare(h), gd.expr_of(h)?.unwrap_or_default())))
            .collect::<Result<_>>()?;
        found.retain(|(name, e)| name.contains(key) || e.contains(key));
        found.sort();
        found.iter().for_each(|(name, e)| println!("{name}: {e}"));
        return Ok(());
    }
    let gd = GameData::load(romfs)?;
    let hash = match u32::from_str_radix(key.trim_start_matches('#'), 16) {
        Ok(h) if key.len() <= 9 && gd.entry(h).is_some() => h,
        _ => murmur3(key.as_bytes()),
    };
    if holders || member {
        for s in gd.of_kind("Struct") {
            let members = gd.members(s)?.into_iter();
            for (m, v) in members.filter(|&(m, v)| if member { m == hash } else { v == hash }) {
                println!("{} member {} -> {}", gd.name(s), gd.name(m), gd.name(v));
            }
        }
        return Ok(());
    }
    let Some(entry) = gd.entry(hash) else {
        // The plain load only knows enum values. The flows name the rest.
        let gd = crate::recorder::gamedata(romfs)?;
        let like: Vec<_> = gd
            .names()
            .filter(|n| n.to_lowercase().contains(&key.to_lowercase()))
            .collect();
        ensure!(
            !like.is_empty(),
            "{} has no entry named or hashed {key}",
            flag::GAME_DATA
        );
        like.iter().for_each(|n| println!("{n}"));
        return Ok(());
    };
    println!("{} {}", gd.name(hash), gd.kind(hash).unwrap_or_default());
    for (k, v) in entry.as_map()?.iter().filter(|(k, _)| k.as_str() != "Hash") {
        println!("  {k}: {}", show(v));
    }
    if gd.kind(hash) == Some("Struct") {
        for (member, value) in gd.members(hash)? {
            println!("  {} -> {}", gd.name(member), gd.name(value));
        }
    }
    Ok(())
}

fn pack(romfs: &Romfs, actor: &str, file: Option<&str>) -> Result<()> {
    let pack = ActorPack::load(romfs, actor)?;
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

fn text(romfs: &Romfs, pattern: &str) -> Result<()> {
    let sarc = Sarc::new(romfs.read(TEXT)?).with_context(|| format!("reading {TEXT}"))?;
    let want = pattern.to_lowercase();
    for f in sarc.files() {
        let Some(name) = f.name().filter(|n| n.ends_with(".msbt")) else {
            continue;
        };
        let mut found: Vec<_> = messages(f.data())
            .with_context(|| name.to_owned())?
            .into_iter()
            .filter(|(id, t)| [id, t].iter().any(|s| s.to_lowercase().contains(&want)))
            .collect();
        found.sort();
        for (id, t) in found {
            println!("{name}:{id} {}", t.replace('\n', " / "));
        }
    }
    Ok(())
}

fn nav(
    romfs: &Romfs,
    game: &Path,
    map: &str,
    types: bool,
    at: Option<[f32; 3]>,
    within: f32,
) -> Result<()> {
    let bytes = navmesh_file(romfs, game, map)?
        .with_context(|| format!("{map} has no navmesh of its own (Hyrule Field's is in tiles)"))?;
    if types {
        let file = Tagfile::parse(&bytes)?;
        for (i, t) in file.types.iter().enumerate().skip(1) {
            println!("{i} {} parent {} size {}", t.name, t.parent, file.size(i));
            for f in &t.fields {
                println!("    {} @{} {}", f.name, f.offset, file.types[f.ty].name);
            }
        }
        return Ok(());
    }
    let mesh = NavMesh::parse(&bytes)?;
    let xyz = |p: [f32; 3]| format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]);
    println!(
        "{} faces, {} edges ({} on the border), {} vertices, from {} to {}, kept {:.2} from walls",
        mesh.faces.len(),
        mesh.edges.len(),
        mesh.edges.iter().filter(|e| e.across.is_none()).count(),
        mesh.vertices.len(),
        xyz(mesh.min),
        xyz(mesh.max),
        mesh.erosion,
    );
    let Some(p) = at else { return Ok(()) };
    match mesh.footing(p, 3.0) {
        Some(f) => println!(
            "{} stands on face {}, floor {:+.1}",
            xyz(p),
            f.face,
            f.height - p[1]
        ),
        None => println!("{} has no face within 3 of its height", xyz(p)),
    }
    println!("distance  x,z toward  past it  edge");
    for b in mesh.borders(p, within, 3.0, 0.5) {
        let past = b.beyond.map_or("no mesh".into(), |h| format!("{h:+.1}"));
        let [x, z] = b.toward;
        println!(
            "{:>8.1}  {x:+.2},{z:+.2}  {past:>7}  {}",
            b.distance, b.edge
        );
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

/// A value on one line: maps as `{k: v}` in key order, arrays as `[..]`.
fn show(v: &Byml) -> String {
    match v {
        Byml::Map(m) => {
            let mut pairs: Vec<_> = m.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            let pairs: Vec<_> = pairs
                .iter()
                .map(|(k, v)| format!("{k}: {}", show(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        Byml::Array(a) => format!("[{}]", a.iter().map(show).collect::<Vec<_>>().join(", ")),
        other => scalar(other),
    }
}

fn short(rel: &str) -> &str {
    let rel = rel.strip_prefix("Banc/").unwrap_or(rel);
    rel.strip_suffix(".bcett.byml.zs").unwrap_or(rel)
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
    use crate::known::{actor, map};

    #[test]
    fn points_parse_with_negatives() {
        assert_eq!(point("1,-2.5, 3").unwrap(), [1.0, -2.5, 3.0]);
        assert!(point("1,2").is_err());
    }

    /// The runs D start in Kokiri Forest stands on the mesh. Where walk-to
    /// stopped against a tree trunk, Link stood off it, inside the gap the
    /// mesh keeps from walls, with the trunk's border under a metre away.
    #[test]
    fn kokiri_navmesh_matches_where_link_stood() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let game = Path::new("../../dump/oot.nup");
        let mesh = NavMesh::parse(
            &navmesh_file(&romfs, game, map::KOKIRI_FOREST_CHILD)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let start = mesh.footing([-32.0, 121.2, -65.2], 3.0).unwrap();
        assert!((start.height - 121.2).abs() < 0.1, "{start:?}");
        let trunk = [-45.2, 113.0, -42.4];
        assert_eq!(mesh.footing(trunk, 3.0), None);
        let wall = mesh.borders(trunk, 2.0, 3.0, 0.5)[0];
        assert!(wall.distance < 1.0 && wall.beyond.is_none(), "{wall:?}");
    }

    #[test]
    fn shows_maps_in_key_order_on_one_line() {
        let mut m = ounce_formats::BymlMap::default();
        m.insert("b".into(), Byml::I32(2));
        m.insert("a".into(), Byml::Array(vec![Byml::Bool(true)]));
        assert_eq!(show(&Byml::Map(m)), "{a: [true], b: 2}");
    }

    #[test]
    fn hundred_gold_skulltulas_once_deduped() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gold = |dedupe| Filter {
            gyaml: Some(actor::ENEMY_SKULLTULA_GOLD.to_string()),
            dedupe,
            ..Filter::default()
        };
        let exact = |hits: Vec<Hit>| {
            hits.iter()
                .filter(|h| h.gyaml == actor::ENEMY_SKULLTULA_GOLD)
                .count()
        };
        assert_eq!(exact(hits(&romfs, &gold(false)).unwrap()), 114);
        assert_eq!(exact(hits(&romfs, &gold(true)).unwrap()), 100);
    }
}
