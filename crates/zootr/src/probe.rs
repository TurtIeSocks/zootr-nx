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
use crate::known::{field, flag};
use crate::names::TEXT;
use crate::nav::navmesh_file;
use crate::tools::Tools;

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
    },
    /// An actor pack's files, or the files whose names contain `file`.
    Pack { actor: String, file: Option<String> },
    /// The murmur3 of each text, the way GameData keys its names.
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
    },
    /// Every BYML document whose path contains this, as an indented tree,
    /// one value per line, including those inside SARC archives.
    Byml { pattern: String },
    /// A logic or AI graph, one node per line: index, class, values, and
    /// where each linked input comes from.
    Ainb {
        /// Romfs path, like `Logic/Market_Common_f252.logic.module.ainb`,
        /// or a graph inside an actor pack as `probe grep` names it, like
        /// `Pack/Actor/Enemy_Scrub_Business.pack.zs/AI/Enemy_Scrub_Business.brain.root.ainb`.
        path: String,
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
    },
    /// Every spot of a seed with the sweep's widest mix, and how the sweep
    /// triggers it (open, talk, play) or why it skips it. The pool to
    /// confirm, for joining with the sweep's results by hash.
    Pool {
        #[arg(long, default_value_t = 1)]
        seed: u64,
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
    /// A map's navmesh (`Phive/NavMesh/MAP`): counts and bounds, or with
    /// `--at`, the face under a point and the mesh's edges near it.
    Nav {
        /// A map, like `KokiriForest_Child`.
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
        Probe::Data { key, holders, exps } => data(romfs, &key, holders, exps),
        Probe::Pack { actor, file } => pack(romfs, &actor, file.as_deref()),
        Probe::Hash { text } => {
            text.iter()
                .for_each(|t| println!("{:08x} {t}", murmur3(t.as_bytes())));
            Ok(())
        }
        Probe::Text { pattern } => text(romfs, &pattern),
        Probe::Settings => {
            settings();
            Ok(())
        }
        Probe::Files { pattern, out } => files(romfs, game, &pattern, out.as_deref()),
        Probe::Nav {
            map,
            types,
            at,
            within,
        } => nav(romfs, game, &map, types, at, within),
        Probe::Grep { needle, path } => grep(romfs, game, &needle, &path),
        Probe::Strings { path } => names(romfs, game, &path),
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
        Probe::Gates { trace, node } => gates(romfs, trace, node),
        Probe::Checks { area, settings } => checks(romfs, area.as_deref(), &settings),
        Probe::Byml { pattern } => byml(romfs, game, &pattern),
        Probe::Ainb { path } => ainb(romfs, &path),
        Probe::Rsdb { table, pattern } => rsdb(romfs, game, &table, &pattern),
        Probe::Tags { actors } => tags(romfs, game, &actors),
        Probe::Groups { file, logic } => groups(romfs, &file, &logic),
        Probe::Restbl { paths } => restbl(romfs, &paths),
        Probe::Shops { seed } => shops(romfs, seed),
        Probe::Pool { seed } => pool(romfs, seed),
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

fn data(romfs: &Romfs, key: &str, holders: bool, exps: bool) -> Result<()> {
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
    if holders {
        for s in gd.of_kind("Struct") {
            for (member, _) in gd.members(s)?.into_iter().filter(|&(_, v)| v == hash) {
                println!("{} member {}", gd.name(s), gd.name(member));
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

/// The names in a file's bytes, sorted: AINB keeps node, parameter and
/// blackboard names in one string pool, an ASB its commands and clips.
fn strings(data: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = data
        .split(|b| !b.is_ascii_graphic())
        .filter(|s| s.len() >= 4 && s[0].is_ascii_alphabetic())
        .filter(|s| {
            s.iter()
                .all(|b| b.is_ascii_alphanumeric() || b"._:".contains(b))
        })
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    out.sort();
    out.dedup();
    out
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

fn files(romfs: &Romfs, game: &Path, pattern: &str, out: Option<&Path>) -> Result<()> {
    let found = crate::game::search(game, pattern)?;
    for (path, data) in &found {
        println!("{:>10} {path}", data.len());
        if let Some(out) = out {
            let dest = out.join(path.strip_suffix(".zs").unwrap_or(path));
            std::fs::create_dir_all(dest.parent().context("no parent")?)?;
            std::fs::write(&dest, romfs.dicts().decompress(path, data)?)?;
        }
    }
    eprintln!("{} files", found.len());
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

/// Hands `f` each file whose path contains `pattern`, decompressed, or each
/// member in its place when it's a SARC archive, marked as one.
fn unpacked(
    romfs: &Romfs,
    game: &Path,
    pattern: &str,
    mut f: impl FnMut(&str, &[u8], bool) -> Result<()>,
) -> Result<()> {
    crate::game::each(game, pattern, |path, data| {
        let data = romfs.dicts().decompress(&path, &data)?;
        match Sarc::new(&data) {
            Ok(sarc) => {
                for file in sarc.files() {
                    if let Some(name) = file.name() {
                        f(&format!("{path}/{name}"), file.data(), true)?;
                    }
                }
                Ok(())
            }
            Err(_) => f(&path, &data, false),
        }
    })
}

fn grep(romfs: &Romfs, game: &Path, needle: &str, path: &str) -> Result<()> {
    let needle = needle.as_bytes();
    let mut found = 0;
    unpacked(romfs, game, path, |path, data, _| {
        let n = data.windows(needle.len()).filter(|w| *w == needle).count();
        if n > 0 {
            println!("{n:>5} {path}");
            found += 1;
        }
        Ok(())
    })?;
    eprintln!("{found} files");
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

fn byml(romfs: &Romfs, game: &Path, pattern: &str) -> Result<()> {
    unpacked(romfs, game, pattern, |path, data, _| {
        if let Ok(doc) = ounce_formats::BymlDoc::parse(data) {
            println!("{path}");
            print!("{}", tree(&doc.root, 1));
        }
        Ok(())
    })
}

/// A value as indented lines: a map's keys in order, an array's items by
/// index, a scalar on its key's line.
fn tree(v: &Byml, depth: usize) -> String {
    let line = |key: &str, v: &Byml| match v {
        Byml::Map(_) | Byml::Array(_) => {
            format!("{:w$}{key}:\n{}", "", tree(v, depth + 1), w = depth * 2)
        }
        Byml::BinaryData(b) | Byml::FileData(b) => {
            format!("{:w$}{key}: <{} bytes>\n", "", b.len(), w = depth * 2)
        }
        v => format!("{:w$}{key}: {}\n", "", scalar(v), w = depth * 2),
    };
    match v {
        Byml::Map(m) => {
            let mut pairs: Vec<_> = m.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            pairs.iter().map(|(k, v)| line(k, v)).collect()
        }
        Byml::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, v)| line(&i.to_string(), v))
            .collect(),
        Byml::BinaryData(b) | Byml::FileData(b) => {
            format!("{:w$}<{} bytes>\n", "", b.len(), w = depth * 2)
        }
        v => format!("{:w$}{}\n", "", scalar(v), w = depth * 2),
    }
}

fn restbl(romfs: &Romfs, paths: &[String]) -> Result<()> {
    let table = ounce_formats::Restbl::parse(&romfs.read(&romfs.restbl_rel()?)?)?;
    for p in paths {
        let size = table.get(p).map_or("-".into(), |s| s.to_string());
        println!("{size} {p}");
    }
    Ok(())
}

fn shops(romfs: &Romfs, seed: u64) -> Result<()> {
    for slot in crate::shops::slots(romfs)? {
        let price = crate::shops::price(seed, slot.hash());
        let rest: Vec<String> = slot.placements[1..].iter().map(u64::to_string).collect();
        println!(
            "{} {} {} {} {price} {}",
            slot.shop,
            slot.number,
            slot.hash(),
            slot.goods,
            rest.join(",")
        );
    }
    Ok(())
}

fn groups(romfs: &Romfs, file: &str, logic: &str) -> Result<()> {
    for rel in banc_files(romfs)?.into_iter().filter(|r| r.contains(file)) {
        let doc = load_banc(romfs, &rel)?;
        let Some(groups) = doc.root.as_map()?.get(&**field::AI_GROUPS) else {
            continue;
        };
        for g in groups.as_array()? {
            if str_field(g, "Logic").is_some_and(|l| l.contains(logic)) {
                println!("{rel} {}", show(g));
            }
        }
    }
    Ok(())
}

fn icons(romfs: &Romfs, game: &Path, pattern: &str, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    unpacked(romfs, game, pattern, |path, data, member| {
        if member && !path.ends_with(".bntx") {
            return Ok(());
        }
        match ounce_formats::textures(data) {
            Ok(all) => {
                for t in all {
                    std::fs::write(
                        out.join(format!("{}.png", t.name)),
                        crate::sprites::png(&t)?,
                    )?;
                    println!("{}x{} {} {path}", t.width, t.height, t.name);
                }
            }
            Err(e) => println!("FAIL {path}: {e:#}"),
        }
        Ok(())
    })
}

fn gates(romfs: &Romfs, trace: bool, node: Option<String>) -> Result<()> {
    if let Some(node) = node {
        for g in crate::gates::trace_node(romfs, &node)? {
            println!("{} {}\n    {}", g.hash, g.area, g.expr);
        }
        return Ok(());
    }
    let (mut wired, mut gated) = (0, 0);
    for g in crate::gates::trace(romfs)? {
        let rule = crate::gates::rule(&g.expr);
        wired += !g.expr.atoms().is_empty() as usize;
        gated += rule.is_some() as usize;
        match (trace, rule) {
            (true, rule) => {
                let rule = rule.unwrap_or_default();
                println!("{} {} {rule}\n    {}", g.hash, g.area, g.expr);
            }
            (false, Some(rule)) => println!("{} {} {rule}", g.hash, g.area),
            (false, None) => {}
        }
    }
    eprintln!("{wired} chests wired to a graph, {gated} of them gated");
    Ok(())
}

fn pouch(romfs: &Romfs, slot: &str, all: bool) -> Result<()> {
    let gd = GameData::load(romfs)?;
    let save = crate::savefile::load(&crate::savefile::path(slot)?)?;
    let names = actor_names(romfs)?;
    let value = |hash: u32| {
        save.get(hash)
            .and_then(|e| save.scalar(e))
            .unwrap_or_default()
    };
    for (actor, stock) in crate::inventory::stock_keys(&gd)? {
        let name = names
            .get(&actor)
            .cloned()
            .unwrap_or_else(|| format!("#{actor:08x}"));
        let (num, got) = (
            value(stock) as i32,
            value(murmur3(format!("IsGet.{name}").as_bytes())),
        );
        if all || num != 0 || got != 0 {
            println!("{num:>5} {} {name}", if got != 0 { "got" } else { "   " });
        }
    }
    for e in save.entries() {
        let held: Vec<_> = save
            .strings(*e)
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if !held.is_empty() {
            println!("{} {} {held:?}", e.kind, gd.name(e.hash));
        }
    }
    Ok(())
}

fn tools(romfs: &Romfs, filter: &Filter) -> Result<()> {
    let mut tools = Tools::new(romfs);
    let mut counts = BTreeMap::<_, usize>::new();
    for hit in hits(romfs, filter)? {
        if let Some(rule) = tools.of(&hit.actor)? {
            let guess = tools.guessed(&hit.gyaml)?;
            *counts.entry((hit.gyaml, rule, guess)).or_default() += 1;
        }
    }
    for ((gyaml, rule, guess), n) in counts {
        let guess = if guess { "guess" } else { "" };
        println!("{n:>5} {gyaml:<56} {rule:<44} {guess}");
    }
    Ok(())
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

/// Print a graph, one node per line.
fn ainb(romfs: &Romfs, path: &str) -> Result<()> {
    use ounce_formats::ainb::{Graph, Value};
    let graph = Graph::parse(&graph_bytes(romfs, path)?)?;
    let show = |v: &Value| match v {
        Value::String(s) => format!("{s:?}"),
        Value::Pointer => "ptr".into(),
        v => format!("{v:?}"),
    };
    for (i, node) in graph.nodes.iter().enumerate() {
        let props = node
            .properties
            .iter()
            .map(|(n, v)| format!("{n}={}", show(v)));
        let inputs = node
            .inputs
            .iter()
            .map(|input| match input.sources.as_slice() {
                [] => format!("{}={}", input.name, show(&input.value)),
                links => {
                    let from: Vec<_> = links
                        .iter()
                        .map(|l| format!("#{}.{}", l.node, l.output))
                        .collect();
                    format!("{}<-{}", input.name, from.join("|"))
                }
            });
        let all: Vec<_> = props.chain(inputs).collect();
        let outputs: Vec<_> = node.outputs.iter().map(|o| o.name.as_str()).collect();
        println!(
            "#{i} {:?} {} {} -> {}",
            node.kind,
            node.name,
            all.join(" "),
            outputs.join(",")
        );
    }
    Ok(())
}

/// A graph's bytes: a romfs file, or a file inside an actor pack when the
/// path runs through `Pack/Actor/NAME.pack.zs/`.
fn graph_bytes(romfs: &Romfs, path: &str) -> Result<Vec<u8>> {
    let Some((actor, inner)) = in_pack(path) else {
        let Some((pack, inner)) = path.split_once(".pack.zs/") else {
            return romfs.read(path);
        };
        // A shared pack, like `Pack/AI.Global.Product.111.pack.zs`.
        let data = romfs.read(&format!("{pack}.pack.zs"))?;
        let sarc = Sarc::new(&data)?;
        let file = sarc.get_data(inner);
        return Ok(file
            .with_context(|| format!("{pack} has no {inner}"))?
            .to_vec());
    };
    let pack = ActorPack::load(romfs, actor)?;
    let data = pack.raw(inner).with_context(|| {
        format!("{actor}'s pack has no {inner}. `zootr probe pack {actor}` lists its files")
    })?;
    Ok(data.to_vec())
}

/// Each spot of the widest sweep mix: hash, area, trigger or skip reason,
/// name.
fn pool(romfs: &Romfs, seed: u64) -> Result<()> {
    use crate::sweep::night::{self, MIXES, mix};
    use crate::sweep::plan::Trigger;
    let dir = std::env::temp_dir().join("zootr-probe-pool");
    let (_, settings) = mix(MIXES.len() - 1);
    let places = crate::dev::places(romfs)?;
    for p in night::pool(romfs, &dir, seed, &settings, &places)? {
        let how = match &p.trigger {
            Trigger::Skip(why) => format!("skip: {why}"),
            t => night::kind(t).into(),
        };
        println!(
            "{:<20} {:<24} {how:<10} {}",
            p.spot.hash,
            p.spot.area,
            night::name(&p)
        );
    }
    Ok(())
}

/// The actor and the file inside its pack, for a path into an actor pack.
fn in_pack(path: &str) -> Option<(&str, &str)> {
    let (pack, inner) = path.split_once(".pack.zs/")?;
    Some((pack.strip_prefix("Pack/Actor/")?, inner))
}

/// Each dealt check with what logic says it needs, one line per era.
fn checks(
    romfs: &Romfs,
    area: Option<&str>,
    settings: &[(String, serde_json::Value)],
) -> Result<()> {
    use crate::generate::Spot;
    use crate::settings::{NewGame, Settings};
    let new_game = NewGame::from_settings(&settings.iter().cloned().collect::<Settings>(), &[])?;
    let all = crate::checks::dealt(romfs, &new_game)?;
    let fixed = crate::shuffle::Fixed::new();
    let deal = crate::shuffle::deal(&all, &fixed, &new_game)?;
    let book = crate::rules::Book::load()?;
    let ctx = crate::rules::Ctx::new(&new_game);
    let world = crate::logic::World::build(romfs, &book, &ctx, &all)?;
    let free = deal
        .free
        .iter()
        .filter(|c| area.is_none_or(|a| c.area.contains(a)));
    let names = crate::names::Names::load(romfs, deal.free.iter().map(|c| c.item.as_str()))?;
    let mut spots: Vec<Spot> = free
        .map(|&check| {
            Spot::new(
                &crate::shuffle::Placement {
                    check,
                    item: &check.item,
                },
                &names,
            )
        })
        .collect();
    crate::places::fill(romfs, &mut spots)?;
    spots.sort_by(|a, b| (&a.area, a.name()).cmp(&(&b.area, b.name())));
    for s in &spots {
        for l in world.locations.iter().filter(|l| l.hash == s.hash) {
            let (node, era) = &world.places[l.place];
            println!(
                "{:<20} {era:<5} {node:<28} {:<40} {:<24} {}",
                s.hash,
                s.name(),
                s.vanilla,
                l.need
            );
        }
    }
    Ok(())
}

/// Prints the settings schema a group at a time, for auditing its copy.
fn settings() {
    use crate::settings::{GROUPS, Kind, schema};
    let defs = schema();
    for group in GROUPS {
        println!("## {group}");
        for d in defs.iter().filter(|d| d.group == group) {
            let default = match &d.kind {
                Kind::Toggle { default } => default.to_string(),
                Kind::Choice { default, .. } => default.clone(),
            };
            let nest = [
                d.parent.map(|p| format!("under {p}")),
                d.subgroup.map(|s| format!("in {s}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            println!(
                "{} = {default} {nest}\n  {}: {}",
                d.id, d.label, d.description
            );
            if let Kind::Choice { options, .. } = &d.kind {
                options
                    .iter()
                    .for_each(|o| println!("    {}: {}", o.id, o.label));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field, map};

    #[test]
    fn names_are_four_or_more_name_bytes() {
        let data = b"\0\x01Rotate_Shop\0ab\0Item_Common\xff";
        assert_eq!(strings(data), ["Item_Common", field::ROTATE_SHOP]);
    }

    #[test]
    fn a_byml_tree_puts_one_value_on_each_line_under_its_key() {
        let map = |pairs: Vec<(&str, Byml)>| {
            Byml::Map(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
        };
        let array = Byml::Array(vec![Byml::I32(1), Byml::String("x".into())]);
        let doc = map(vec![
            ("b", array),
            ("a", map(vec![("c", Byml::Bool(true))])),
        ]);
        assert_eq!(tree(&doc, 0), "a:\n  c: true\nb:\n  0: 1\n  1: x\n");
    }

    #[test]
    fn points_parse_with_negatives() {
        assert_eq!(point("1,-2.5, 3").unwrap(), [1.0, -2.5, 3.0]);
        assert!(point("1,2").is_err());
    }

    #[test]
    fn a_path_through_an_actor_pack_names_the_actor_and_its_file() {
        let path = &format!(
            "Pack/Actor/{}.pack.zs/AI/{}.brain.root.ainb",
            actor::ENEMY_SCRUB_BUSINESS,
            actor::ENEMY_SCRUB_BUSINESS
        );
        assert_eq!(
            in_pack(path),
            Some((
                &**actor::ENEMY_SCRUB_BUSINESS,
                format!("AI/{}.brain.root.ainb", actor::ENEMY_SCRUB_BUSINESS).as_str()
            ))
        );
        assert_eq!(in_pack("Logic/Market_Common_f252.logic.module.ainb"), None);
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let graph = ounce_formats::ainb::Graph::parse(&graph_bytes(&romfs, path).unwrap()).unwrap();
        assert!(!graph.nodes.is_empty());
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
