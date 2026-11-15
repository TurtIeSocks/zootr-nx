//! Chest gates from the logic graphs: what makes each chest appear or
//! unlock, read from the `Logic/*.ainb` graphs each Banc file's `AiGroups`
//! tie to its placements.
//!
//! An `AiGroups` entry names a graph and maps the graph's actor nodes to
//! placements: a reference's `Id` is a node's `InstanceName`, its `Path`
//! that name under the module instance's prefix, and its `Reference` the
//! placement hash. A nested module instance appears as an `AiGroup`
//! reference whose `Path` gives its prefix.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use anyhow::{Context, Result};
use ounce_formats::ainb::{Graph, Input, Node, NodeKind, Value};
use ounce_formats::{Byml, Romfs};

use crate::checks::{GYAML, actors, banc_files, hash_of, load_banc, str_field};
use crate::known::{Name, actor, field, flag};

const LOGIC: &str = "Logic";
const CHEST: &str = "Obj_TBox_";
const TREASURE_BOX: &Name = field::ACTOR_LOGIC_TREASURE_BOX;
/// A boss's heart container, and the logic node that makes it appear.
const HEART: &Name = actor::OBJ_HEART_CONTAINER;
const SIMPLE_ACTOR: &Name = field::ACTOR_LOGIC_SIMPLE_ACTOR;
const CREATE: &Name = field::LOGIC_CREATE;
/// The output that holds while an actor exists.
const ALIVE: &Name = field::LOGIC_ALIVE;
const MODULE_GROUP: &Name = field::AI_GROUP;
/// The logic node that starts its placement's event, and its input that
/// does.
const REQUEST_NODE: &Name = field::ACTOR_LOGIC_EVENT_REQUEST_ACTOR;
const REQUEST_TRIGGER: &Name = field::EVENT_REQUEST_TRIGGER;
/// A placed area, whose sensor reports what's inside.
const AREA_NODE: &Name = field::ACTOR_LOGIC_AREA_ACTOR;
const SET_FLAG: &Name = field::TRIGGER_GAME_DATA_SET_BOOL_TRUE;
/// What starts an event actor's event, and with it its `Event*` outputs.
const EVENT_TRIGGER: &Name = field::EVENT_REQUEST_TRIGGER;

/// One thing a gate reads.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Atom {
    /// A placed actor's output, like a Stinger's `Dead`.
    Actor {
        name: String,
        output: String,
    },
    Child,
    Adult,
    /// A flag no graph sets.
    Flag(String),
    /// A GameData enum holding a value, like the prize of Dampe's race,
    /// set to the Hookshot's appearance.
    Enum {
        name: String,
        value: String,
    },
    /// A GameData enum past a value: a quest past a step, like the Water
    /// Temple's heart container waiting for `Step_SageOfWater` past
    /// `Step07_2`. The walk doesn't know the steps' order, GameData does.
    After {
        name: String,
        value: String,
    },
    /// A BoolExp, by name, which only GameData renders.
    Exp(String),
    /// Something the walk doesn't read, described in one word.
    Todo(String),
}

/// A gate as the graphs wire it.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Const(bool),
    Atom(Atom),
    /// A flag the graphs set, with what sets it.
    SetFlag(String, Vec<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
}

impl Expr {
    /// Every atom the gate reads, however its rule folds them.
    pub fn atoms(&self) -> Vec<&Atom> {
        match self {
            Expr::Const(_) => Vec::new(),
            Expr::Atom(a) => vec![a],
            Expr::Not(e) => e.atoms(),
            Expr::SetFlag(_, v) | Expr::And(v) | Expr::Or(v) => {
                v.iter().flat_map(Expr::atoms).collect()
            }
        }
    }

    /// The GameData values that open the gate when set by hand, as a
    /// sweep's ready entries (`NAME` sets a Bool true, `NAME=VALUE` an
    /// enum): the flags a graph sets once its condition is met, like a
    /// mini-boss beaten, the flags no graph sets, and the enum values it
    /// asks. A value under a `Not` closes it, so it stays out. An enum
    /// takes the first value asked, the way the gate's first branch reads.
    pub fn opening(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for entry in self.openers() {
            let name = entry.split('=').next();
            if !found.iter().any(|f| f.split('=').next() == name) {
                found.push(entry);
            }
        }
        found
    }

    /// The age the gate opens for, `Some(true)` for a child, when only
    /// one can: what the hand sets opens, and only the age atoms hold.
    pub fn age(&self) -> Option<bool> {
        match (self.opens(true), self.opens(false)) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        }
    }

    fn opens(&self, child: bool) -> bool {
        match self {
            Expr::Const(b) => *b,
            Expr::Atom(Atom::Child) => child,
            Expr::Atom(Atom::Adult) => !child,
            Expr::Not(e) => match **e {
                Expr::Atom(Atom::Child) => !child,
                Expr::Atom(Atom::Adult) => child,
                _ => true,
            },
            Expr::Atom(_) | Expr::SetFlag(..) => true,
            Expr::And(v) => v.iter().all(|e| e.opens(child)),
            Expr::Or(v) => v.iter().any(|e| e.opens(child)),
        }
    }

    fn openers(&self) -> Vec<String> {
        match self {
            Expr::SetFlag(name, _) | Expr::Atom(Atom::Flag(name)) => vec![name.clone()],
            Expr::Atom(Atom::Enum { name, value }) => vec![format!("{name}={value}")],
            Expr::And(v) | Expr::Or(v) => v.iter().flat_map(Expr::openers).collect(),
            Expr::Const(_) | Expr::Atom(_) | Expr::Not(_) => Vec::new(),
        }
    }

    /// The quest steps and BoolExps the gate opens on, outside a `Not`:
    /// each [`Atom::After`] and [`Atom::Exp`], for GameData to resolve.
    pub fn quests(&self) -> Vec<&Atom> {
        match self {
            Expr::Atom(a @ (Atom::After { .. } | Atom::Exp(_))) => vec![a],
            Expr::SetFlag(_, v) | Expr::And(v) | Expr::Or(v) => {
                v.iter().flat_map(Expr::quests).collect()
            }
            Expr::Const(_) | Expr::Atom(_) | Expr::Not(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Gate {
    pub hash: u64,
    /// The Banc folder its graph is placed in, e.g. `SpiritTemple`.
    pub area: String,
    pub expr: Expr,
}

/// Every chest gated by a logic graph, by placement hash, as a rule.
pub fn gates(romfs: &Romfs) -> Result<BTreeMap<u64, String>> {
    Ok(trace(romfs)?
        .into_iter()
        .filter_map(|g| Some((g.hash, rule(&g.expr)?)))
        .collect())
}

/// Every chest a logic graph drives, with what drives it, whether or not
/// that collapses to a rule.
pub fn trace(romfs: &Romfs) -> Result<Vec<Gate>> {
    drives(
        romfs,
        TREASURE_BOX,
        |gyaml| gyaml.starts_with(CHEST),
        // The chest inputs a gate is read from: whether it exists, and
        // whether it opens.
        Some(&[CREATE, "Unlock"]),
    )
}

/// Every boss heart container a logic graph makes appear, with what does.
pub fn hearts(romfs: &Romfs) -> Result<Vec<Gate>> {
    drives(romfs, SIMPLE_ACTOR, |gyaml| gyaml == HEART, Some(&[CREATE]))
}

/// Every placement a logic node named `node` drives, with all its inputs:
/// what starts an `ActorLogicEventRequestActor`'s event, say.
pub fn trace_node(romfs: &Romfs, node: &str) -> Result<Vec<Gate>> {
    drives(romfs, node, |_| true, None)
}

/// The areas whose sensors start each event request actor's event as Link
/// steps in, by the request actor's placement hash.
pub fn sensors(romfs: &Romfs) -> Result<HashMap<u64, Vec<u64>>> {
    let world = World::load(romfs)?;
    let mut found: HashMap<u64, Vec<u64>> = HashMap::new();
    for i in 0..world.instances.len() {
        let graph = world.graph(i);
        let feeders = |n: &Node, input: Option<&str>| {
            let inputs = n
                .inputs
                .iter()
                .filter(|i| input.is_none_or(|name| i.name == name));
            inputs
                .flat_map(|i| &i.sources)
                .map(|l| l.node)
                .collect::<Vec<_>>()
        };
        for node in graph.nodes.iter().filter(|n| n.name == REQUEST_NODE) {
            let Some(hash) = world.placement(i, node.str(field::INSTANCE_NAME)) else {
                continue;
            };
            let (mut seen, mut todo) = (HashSet::new(), feeders(node, Some(REQUEST_TRIGGER)));
            while let Some(n) = todo.pop() {
                let feeder = &graph.nodes[n];
                if !seen.insert(n) {
                    continue;
                }
                if feeder.name == AREA_NODE {
                    let area = world.placement(i, feeder.str(field::INSTANCE_NAME));
                    found.entry(hash).or_default().extend(area);
                } else {
                    todo.extend(feeders(feeder, None));
                }
            }
        }
    }
    Ok(found)
}

/// Each placement whose actor `keep` takes that a node named `node`
/// drives, with what drives the `inputs` it reads (all when `None`), one
/// copy of the graph or another.
fn drives(
    romfs: &Romfs,
    node: &str,
    keep: impl Fn(&str) -> bool,
    inputs: Option<&[&str]>,
) -> Result<Vec<Gate>> {
    let world = World::load(romfs)?;
    let mut by_hash: BTreeMap<u64, (String, Vec<Expr>)> = BTreeMap::new();
    for (i, inst) in world.instances.iter().enumerate() {
        for n in world.graph(i).nodes.iter().filter(|n| n.name == node) {
            let Some(hash) = world.placement(i, n.str(field::INSTANCE_NAME)) else {
                continue;
            };
            if !keep(world.gyaml(hash)) {
                continue;
            }
            let read: Vec<&Input> = match inputs {
                Some(names) => names.iter().filter_map(|name| n.input(name)).collect(),
                None => n.inputs.iter().collect(),
            };
            let reads = (read.into_iter())
                .map(|input| world.input(i, input, &Seen::default()))
                .collect();
            let area = world.bancs[inst.banc].area.clone();
            let copies = &mut by_hash.entry(hash).or_insert((area, Vec::new())).1;
            copies.push(Expr::And(reads));
        }
    }
    Ok(by_hash
        .into_iter()
        .map(|(hash, (area, copies))| Gate {
            hash,
            area,
            expr: one_or(copies, Expr::Or),
        })
        .collect())
}

fn one_or(mut v: Vec<Expr>, many: fn(Vec<Expr>) -> Expr) -> Expr {
    match v.len() {
        1 => v.remove(0),
        _ => many(v),
    }
}

struct Banc {
    area: String,
    groups: Vec<Group>,
}

struct Group {
    logic: String,
    /// Placement hash by path.
    refs: HashMap<String, u64>,
    /// Module instance name and path of each nested `AiGroup`.
    modules: Vec<(String, String)>,
}

/// A graph placed once: the top of an `AiGroups` entry, or a module inside
/// another instance.
struct Instance {
    graph: String,
    banc: usize,
    group: usize,
    /// Prepended to an actor node's `InstanceName` to find its reference.
    /// `None` when no `AiGroup` reference names the module.
    prefix: Option<String>,
    /// The instance and module node it was placed by.
    parent: Option<(usize, usize)>,
}

struct World {
    bancs: Vec<Banc>,
    graphs: HashMap<String, Graph>,
    instances: Vec<Instance>,
    /// Instance of each `(instance, module node)`.
    children: HashMap<(usize, usize), usize>,
    gyamls: HashMap<u64, String>,
    /// Each flag's setters: instance and node.
    setters: HashMap<String, Vec<(usize, usize)>>,
}

impl World {
    fn load(romfs: &Romfs) -> Result<Self> {
        let mut world = World {
            bancs: Vec::new(),
            graphs: HashMap::new(),
            instances: Vec::new(),
            children: HashMap::new(),
            gyamls: HashMap::new(),
            setters: HashMap::new(),
        };
        for rel in banc_files(romfs)? {
            let doc = load_banc(romfs, &rel)?;
            for a in actors(&doc.root)? {
                if let (Some(hash), Some(gyaml)) = (hash_of(a), str_field(a, GYAML)) {
                    world.gyamls.entry(hash).or_insert_with(|| gyaml.to_owned());
                }
            }
            let area = rel.trim_start_matches("Banc/");
            world.bancs.push(Banc {
                area: area.split('/').next().unwrap_or(area).to_owned(),
                groups: groups(&doc.root)
                    .with_context(|| format!("{} in {rel}", field::AI_GROUPS))?,
            });
        }
        for b in 0..world.bancs.len() {
            for g in 0..world.bancs[b].groups.len() {
                let graph = graph_name(&world.bancs[b].groups[g].logic);
                world.place(romfs, graph, b, g, Some(String::new()), None)?;
            }
        }
        let mut setters: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        for i in 0..world.instances.len() {
            for (n, node) in world.graph(i).nodes.iter().enumerate() {
                if let Some(flag) = node
                    .str(field::GAME_DATA_NAME_BOOL)
                    .filter(|_| node.name == SET_FLAG)
                {
                    setters.entry(flag.to_owned()).or_default().push((i, n));
                }
            }
        }
        world.setters = setters;
        Ok(world)
    }

    /// Adds an instance of `graph` and, depth first, its modules'.
    fn place(
        &mut self,
        romfs: &Romfs,
        graph: String,
        banc: usize,
        group: usize,
        prefix: Option<String>,
        parent: Option<(usize, usize)>,
    ) -> Result<()> {
        if !self.graphs.contains_key(&graph) {
            let rel = format!("{LOGIC}/{graph}.ainb");
            let parsed = Graph::parse(&romfs.read(&rel)?).with_context(|| rel.clone())?;
            self.graphs.insert(graph.clone(), parsed);
        }
        let at = self.instances.len();
        self.instances.push(Instance {
            graph: graph.clone(),
            banc,
            group,
            prefix: prefix.clone(),
            parent,
        });
        let modules: Vec<_> = self.graphs[&graph]
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.module)
            .map(|(n, node)| {
                (
                    n,
                    node.name.clone(),
                    node.str(field::INSTANCE_NAME).map(str::to_owned),
                )
            })
            .collect();
        for (n, name, instance_name) in modules {
            let group_data = &self.bancs[banc].groups[group];
            let child_prefix = prefix.as_ref().zip(instance_name).and_then(|(p, inst)| {
                group_data.modules.iter().find_map(|(name, path)| {
                    let at = path
                        .strip_prefix(p.as_str())?
                        .strip_suffix(&**MODULE_GROUP)?;
                    let one_deeper = at.ends_with('/') && at.matches('/').count() == 1;
                    (*name == inst && one_deeper).then(|| format!("{p}{at}"))
                })
            });
            self.children.insert((at, n), self.instances.len());
            self.place(romfs, name, banc, group, child_prefix, Some((at, n)))?;
        }
        Ok(())
    }

    fn graph(&self, inst: usize) -> &Graph {
        &self.graphs[&self.instances[inst].graph]
    }

    /// The placement an actor node in `inst` names.
    fn placement(&self, inst: usize, name: Option<&str>) -> Option<u64> {
        let i = &self.instances[inst];
        let path = format!("{}{}", i.prefix.as_ref()?, name?);
        self.bancs[i.banc].groups[i.group].refs.get(&path).copied()
    }

    fn gyaml(&self, hash: u64) -> &str {
        self.gyamls.get(&hash).map_or("", String::as_str)
    }
}

/// `Logic/Module/X/X_1234.logic.module.ain` names the graph
/// `X_1234.logic.module`, kept flat under `Logic/`.
fn graph_name(logic: &str) -> String {
    let file = logic.rsplit('/').next().unwrap_or(logic);
    file.strip_suffix(".ain").unwrap_or(file).to_owned()
}

fn groups(root: &Byml) -> Result<Vec<Group>> {
    let Some(groups) = root.as_map()?.get(field::AI_GROUPS.get()) else {
        return Ok(Vec::new());
    };
    groups
        .as_array()?
        .iter()
        .map(|g| {
            let mut group = Group {
                logic: str_field(g, "Logic")
                    .with_context(|| format!("an {} without Logic", field::AI_GROUP))?
                    .to_owned(),
                refs: HashMap::new(),
                modules: Vec::new(),
            };
            let refs = g.as_map()?.get("References").map(Byml::as_array);
            for r in refs.transpose()?.unwrap_or_default() {
                let path = str_field(r, "Path").context("a reference without Path")?;
                if str_field(r, "Id") == Some(MODULE_GROUP) {
                    if let Some(inst) = str_field(r, field::INSTANCE_NAME) {
                        group.modules.push((inst.to_owned(), path.to_owned()));
                    }
                } else if let Some(hash) = r.as_map()?.get("Reference") {
                    group.refs.insert(path.to_owned(), hash.as_int::<u64>()?);
                }
            }
            Ok(group)
        })
        .collect()
}

/// Nodes and flags already on the path being walked, which a loop reads as
/// false: a latch only holds once something else sets it.
#[derive(Clone, Default)]
struct Seen {
    outputs: HashSet<(usize, usize, String)>,
    flags: HashSet<String>,
}

/// The walk back from an input to what drives it.
impl World {
    /// What drives an input: its sources, or its constant.
    fn input(&self, inst: usize, input: &Input, seen: &Seen) -> Expr {
        if input.sources.is_empty() {
            return match input.value {
                Value::Bool(b) => Expr::Const(b),
                _ => Expr::Atom(Atom::Todo(format!("input:{}", input.name))),
            };
        }
        let exprs = input
            .sources
            .iter()
            .map(|l| self.output(inst, l.node, &l.output, seen))
            .collect();
        one_or(exprs, Expr::Or)
    }

    fn named_input(&self, inst: usize, node: usize, name: &str, seen: &Seen) -> Expr {
        match self.graph(inst).nodes[node].input(name) {
            Some(input) => self.input(inst, input, seen),
            None => Expr::Atom(Atom::Todo(format!("no-input:{name}"))),
        }
    }

    /// Every linked bool input's sources, for the boolean nodes.
    fn operands(&self, inst: usize, node: usize, seen: &Seen) -> Vec<Expr> {
        self.graph(inst).nodes[node]
            .inputs
            .iter()
            .flat_map(|i| &i.sources)
            .map(|l| self.output(inst, l.node, &l.output, seen))
            .collect()
    }

    fn output(&self, inst: usize, n: usize, out: &str, seen: &Seen) -> Expr {
        let mut seen = seen.clone();
        if !seen.outputs.insert((inst, n, out.to_owned())) {
            return Expr::Const(false);
        }
        let seen = &seen;
        let node = &self.graph(inst).nodes[n];
        let truth = |e: Expr| match out {
            "False" => Expr::Not(Box::new(e)),
            _ => e,
        };
        if node.kind == NodeKind::ModuleInput {
            let name = node.str("Name").unwrap_or_default();
            return match self.instances[inst].parent {
                Some((p, m)) => self.named_input(p, m, name, seen),
                None => Expr::Atom(Atom::Todo(format!("root-input:{name}"))),
            };
        }
        if node.module {
            let child = self.children[&(inst, n)];
            let found: Vec<_> = self
                .graph(child)
                .nodes
                .iter()
                .filter(|m| m.kind == NodeKind::ModuleOutput && m.str("Name") == Some(out))
                .flat_map(|m| &m.inputs)
                .map(|i| self.input(child, i, seen))
                .collect();
            return match found.is_empty() {
                true => Expr::Atom(Atom::Todo(format!("{}.{out}", node.name))),
                false => one_or(found, Expr::Or),
            };
        }
        let name = node.name.as_str();
        match name {
            _ if name.starts_with(&**field::OPERATE_AND) => {
                truth(Expr::And(self.operands(inst, n, seen)))
            }
            _ if name.starts_with(&**field::OPERATE_OR) => {
                truth(Expr::Or(self.operands(inst, n, seen)))
            }
            _ if name == field::OPERATE_INVERT_BOOL => {
                Expr::Not(Box::new(one_or(self.operands(inst, n, seen), Expr::And)))
            }
            _ if name == field::HOLD_IS_CHILDHOOD_OR_ADULTHOOD => match out {
                "Adulthood" => Expr::Atom(Atom::Adult),
                "Childhood" => Expr::Atom(Atom::Child),
                _ => Expr::Atom(Atom::Todo(format!("{name}.{out}"))),
            },
            _ if name == field::HOLD_GAME_DATA_GET_BOOL_SIMPLE => {
                let flag = node.str(field::GAME_DATA_NAME_BOOL).unwrap_or_default();
                self.flag(flag, seen)
            }
            _ if name == field::HOLD_GAME_DATA_ENUM_CHECK_ORDER => {
                let held = Expr::Atom(Atom::Enum {
                    name: node
                        .str(field::GAME_DATA_NAME_ENUM)
                        .unwrap_or_default()
                        .to_owned(),
                    value: node.str(field::ENUM_NAME).unwrap_or_default().to_owned(),
                });
                match out {
                    _ if out == field::IS_EQUAL => held,
                    _ if out == field::IS_NOT_EQUAL => Expr::Not(Box::new(held)),
                    _ => Expr::Atom(Atom::Todo(format!("{name}.{out}"))),
                }
            }
            _ if name == field::HOLD_CHALLENGE_CURRENT_STEP_CHECK_ORDER
                && out == flag::IS_AFTER =>
            {
                let quest = node.str(field::CHALLENGE_NAME).map(crate::rsdb::flow_name);
                Expr::Atom(Atom::After {
                    name: format!("{}{}", crate::steps::SAVED, quest.unwrap_or_default()),
                    value: node
                        .str(field::CHALLENGE_STEP_NAME)
                        .unwrap_or_default()
                        .to_owned(),
                })
            }
            _ if name == field::HOLD_GAME_DATA_GET_BOOL_EXP_SIMPLE => {
                let exp = node.str(field::GAME_DATA_NAME_BOOL_EXP).unwrap_or_default();
                Expr::Atom(Atom::Exp(exp.to_owned()))
            }
            _ if name == field::HOLD_GENERAL_DELAY_BOOL => {
                one_or(self.operands(inst, n, seen), Expr::Or)
            }
            _ if out.starts_with("Event") && node.input(EVENT_TRIGGER).is_some() => {
                self.named_input(inst, n, EVENT_TRIGGER, seen)
            }
            _ if name.starts_with(&**field::ACTOR_LOGIC) => self.actor(inst, n, out, seen),
            _ => Expr::Atom(Atom::Todo(format!("{name}.{out}"))),
        }
    }

    /// An actor's output, which only fires while the actor exists. Where
    /// the output needs nothing, like an enemy's death, so does meeting it.
    fn actor(&self, inst: usize, n: usize, out: &str, seen: &Seen) -> Expr {
        let node = &self.graph(inst).nodes[n];
        let exists = match node.input(CREATE) {
            Some(i) if !i.sources.is_empty() => Some(self.input(inst, i, seen)),
            _ => None,
        };
        if out == ALIVE {
            return exists.unwrap_or(Expr::Const(true));
        }
        let instance_name = node.str(field::INSTANCE_NAME);
        let name = match self.placement(inst, instance_name) {
            Some(hash) => self.gyaml(hash).to_owned(),
            None => format!("unplaced:{}", instance_name.unwrap_or(&node.name)),
        };
        let fired = Atom::Actor {
            name,
            output: out.to_owned(),
        };
        match exists.filter(|_| atom(&fired) != Rule::Const(true)) {
            Some(e) => Expr::And(vec![Expr::Atom(fired), e]),
            None => Expr::Atom(fired),
        }
    }

    /// A flag, through whatever graphs set it.
    fn flag(&self, flag: &str, seen: &Seen) -> Expr {
        let Some(setters) = self.setters.get(flag) else {
            return Expr::Atom(Atom::Flag(flag.to_owned()));
        };
        if seen.flags.contains(flag) {
            return Expr::Const(false);
        }
        let mut seen = seen.clone();
        seen.flags.insert(flag.to_owned());
        let drives = setters
            .iter()
            .map(|&(i, n)| self.named_input(i, n, field::IS_DRIVE, &seen))
            .collect();
        Expr::SetFlag(flag.to_owned(), drives)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |f: &mut fmt::Formatter<'_>, v: &[Expr], sep: &str| {
            let shown: Vec<_> = v.iter().map(Expr::to_string).collect();
            write!(f, "({})", shown.join(sep))
        };
        match self {
            Expr::Const(b) => write!(f, "{b}"),
            Expr::Atom(Atom::Actor { name, output }) => write!(f, "{name}.{output}"),
            Expr::Atom(Atom::Child) => f.write_str("Childhood"),
            Expr::Atom(Atom::Adult) => f.write_str("Adulthood"),
            Expr::Atom(Atom::Flag(flag)) => write!(f, "flag({flag})"),
            Expr::Atom(Atom::Enum { name, value }) => write!(f, "{name}=={value}"),
            Expr::Atom(Atom::After { name, value }) => write!(f, "{name}>{value}"),
            Expr::Atom(Atom::Exp(exp)) => write!(f, "exp({exp})"),
            Expr::Atom(Atom::Todo(t)) => write!(f, "?{t}"),
            Expr::SetFlag(flag, drives) => {
                write!(f, "{flag}<=")?;
                list(f, drives, " | ")
            }
            Expr::And(v) => list(f, v, " & "),
            Expr::Or(v) => list(f, v, " | "),
            Expr::Not(e) => write!(f, "!{e}"),
        }
    }
}

/// A gate in the rule language, or `None` when it needs nothing.
pub fn rule(expr: &Expr) -> Option<String> {
    match Rule::of(expr) {
        Rule::Const(true) => None,
        Rule::Const(false) => Some("todo:never".into()),
        r => Some(r.to_string()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rule {
    Const(bool),
    Word(String),
    And(BTreeSet<Rule>),
    Or(BTreeSet<Rule>),
}

impl Rule {
    fn of(expr: &Expr) -> Rule {
        match expr {
            Expr::Const(b) => Rule::Const(*b),
            Expr::Atom(a) => atom(a),
            Expr::SetFlag(_, drives) => Rule::any(drives.iter().map(Rule::of)),
            Expr::And(v) => Rule::all(v.iter().map(Rule::of)),
            Expr::Or(v) => Rule::any(v.iter().map(Rule::of)),
            Expr::Not(e) => Rule::of(e).negated(),
        }
    }

    /// The rule language has no `not`, so a negated word stays a `todo:`.
    fn negated(self) -> Rule {
        match self {
            Rule::Const(b) => Rule::Const(!b),
            Rule::Word(w) => Rule::Word(format!("todo:not:{}", w.trim_start_matches("todo:"))),
            Rule::And(parts) => Rule::any(parts.into_iter().map(Rule::negated)),
            Rule::Or(parts) => Rule::all(parts.into_iter().map(Rule::negated)),
        }
    }

    fn all(parts: impl Iterator<Item = Rule>) -> Rule {
        Rule::join(parts, true)
    }

    fn any(parts: impl Iterator<Item = Rule>) -> Rule {
        Rule::join(parts, false)
    }

    /// `and` when `all`, else `or`: flattened, deduplicated, and with the
    /// constants folded.
    fn join(parts: impl Iterator<Item = Rule>, all: bool) -> Rule {
        let mut set = BTreeSet::new();
        for p in parts {
            match p {
                Rule::Const(b) if b == all => {}
                Rule::Const(_) => return Rule::Const(!all),
                Rule::And(inner) if all => set.extend(inner),
                Rule::Or(inner) if !all => set.extend(inner),
                other => {
                    set.insert(other);
                }
            }
        }
        let absorbed: Vec<_> = set
            .iter()
            .filter(|r| set.iter().any(|other| other != *r && other.absorbs(r, all)))
            .cloned()
            .collect();
        for r in absorbed {
            set.remove(&r);
        }
        match set.len() {
            0 => Rule::Const(all),
            1 => set.pop_first().unwrap_or(Rule::Const(all)),
            _ if all => Rule::And(set),
            _ => Rule::Or(set),
        }
    }

    /// Whether `other` is redundant beside `self`: under `or`, `a` makes
    /// `a and b` redundant, and under `and`, `a` makes `a or b` redundant.
    fn absorbs(&self, other: &Rule, all: bool) -> bool {
        let inner = match (other, all) {
            (Rule::And(inner), false) | (Rule::Or(inner), true) => inner,
            _ => return false,
        };
        match (self, all) {
            (Rule::And(mine), false) | (Rule::Or(mine), true) => mine.is_subset(inner),
            _ => inner.contains(self),
        }
    }
}

fn atom(a: &Atom) -> Rule {
    let word = |w: &str| Rule::Word(w.to_owned());
    match a {
        Atom::Child => word("child"),
        Atom::Adult => word("adult"),
        Atom::Flag(f) => Rule::Word(format!("flag:{f}")),
        Atom::Enum { name, value } => Rule::Word(format!("enum:{name}={value}")),
        Atom::Todo(t) => Rule::Word(format!("todo:{t}")),
        Atom::After { name, value } => Rule::Word(format!("todo:{name}>{value}")),
        Atom::Exp(exp) => Rule::Word(format!("todo:exp:{exp}")),
        Atom::Actor { name, output } => match output.as_str() {
            "Dead" if name.starts_with("Enemy_") => Rule::Const(true),
            // A step switch reports its first press as `FirstOn`.
            _ if output == field::ON_STATE || output == field::FIRST_ON => {
                Rule::Word(format!("switch:{name}"))
            }
            "Burning" => word("fire"),
            _ if output == field::IS_HIT_BLUE_FIRE => word("blue-fire"),
            _ if output == field::IS_LIGHT_MAX => word("light"),
            _ => Rule::Word(format!("todo:{name}.{output}")),
        },
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (parts, sep) = match self {
            Rule::Const(b) => return write!(f, "{b}"),
            Rule::Word(w) => return f.write_str(w),
            Rule::And(parts) => (parts, " and "),
            Rule::Or(parts) => (parts, " or "),
        };
        let shown: Vec<_> = parts
            .iter()
            .map(|p| match p {
                Rule::And(_) | Rule::Or(_) => format!("({p})"),
                _ => p.to_string(),
            })
            .collect();
        f.write_str(&shown.join(sep))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::{actor, field, flag};

    fn actor(name: &str, output: &str) -> Expr {
        Expr::Atom(Atom::Actor {
            name: name.into(),
            output: output.into(),
        })
    }

    fn flag(name: &str) -> Expr {
        Expr::Atom(Atom::Flag(name.into()))
    }

    fn held(name: &str, value: &str) -> Expr {
        Expr::Atom(Atom::Enum {
            name: name.into(),
            value: value.into(),
        })
    }

    #[test]
    fn a_gate_opens_on_its_flags_and_first_enum_values_never_under_a_not() {
        let gate = Expr::And(vec![
            Expr::Or(vec![
                Expr::And(vec![held("Prize", "Appear"), flag("Shown")]),
                held("Prize", "Opened"),
                Expr::SetFlag("Beaten".into(), vec![Expr::Const(true)]),
                Expr::Not(Box::new(flag("Gone"))),
            ]),
            Expr::Not(Box::new(held("Other", "Off"))),
            Expr::Atom(Atom::Adult),
        ]);
        assert_eq!(gate.opening(), ["Prize=Appear", "Shown", "Beaten"]);
        assert_eq!(rule(&held("Prize", "Appear")).unwrap(), "enum:Prize=Appear");
    }

    #[test]
    fn a_gate_names_its_age_only_when_every_branch_needs_it() {
        let adult = Expr::Atom(Atom::Adult);
        let colossus = Expr::Or(vec![
            Expr::And(vec![actor("Switch", field::FIRST_ON), adult.clone()]),
            Expr::And(vec![adult.clone(), Expr::SetFlag("Latch".into(), vec![])]),
        ]);
        assert_eq!(colossus.age(), Some(false));
        let either = Expr::Or(vec![adult, flag("Shown")]);
        assert_eq!(either.age(), None);
        let grown = Expr::Not(Box::new(Expr::Atom(Atom::Child)));
        assert_eq!(Expr::And(vec![grown, flag("Shown")]).age(), Some(false));
        assert_eq!(Expr::Atom(Atom::Child).age(), Some(true));
    }

    #[test]
    fn rules_drop_what_needs_nothing() {
        let stingers = Expr::Or(vec![
            actor(actor::ENEMY_STINGER, "Dead"),
            Expr::SetFlag("Cleared".into(), vec![actor(actor::ENEMY_STINGER, "Dead")]),
        ]);
        assert_eq!(rule(&stingers), None);
        assert_eq!(
            rule(&Expr::And(vec![Expr::Const(true), flag("F")])).unwrap(),
            "flag:F"
        );
        assert_eq!(rule(&Expr::Const(false)).unwrap(), "todo:never");
    }

    #[test]
    fn a_flag_the_graphs_set_reads_as_its_setter() {
        let hit = actor(actor::OBJ_SWITCH_HIT_A_05, field::ON_STATE);
        let latch = Expr::SetFlag("Thing_On".into(), vec![hit.clone()]);
        assert_eq!(
            rule(&Expr::Or(vec![hit, latch])).unwrap(),
            format!("switch:{}", actor::OBJ_SWITCH_HIT_A_05)
        );
    }

    #[test]
    fn rules_absorb_and_bracket() {
        let switch = actor(actor::OBJ_STEP_SWITCH_A_01, field::ON_STATE);
        let adult = Expr::Atom(Atom::Adult);
        let absorbed = Expr::Or(vec![switch.clone(), Expr::And(vec![adult.clone(), switch])]);
        assert_eq!(
            rule(&absorbed).unwrap(),
            format!("switch:{}", actor::OBJ_STEP_SWITCH_A_01)
        );
        let either = Expr::Or(vec![
            actor(actor::OBJ_CANDLE_STICK_A_02, "Burning"),
            actor(actor::OBJ_SOLAR_SWITCH_A_01, field::IS_LIGHT_MAX),
        ]);
        let both = Expr::And(vec![either, adult, Expr::Atom(Atom::Child)]);
        assert_eq!(rule(&both).unwrap(), "adult and child and (fire or light)");
    }

    #[test]
    fn negations_fold_or_stay_todo() {
        let unset = Expr::SetFlag("Cleared".into(), vec![Expr::Const(false)]);
        assert_eq!(rule(&Expr::Not(Box::new(unset))), None);
        let not_either = Expr::Not(Box::new(Expr::Or(vec![flag("A"), flag("B")])));
        assert_eq!(
            rule(&not_either).unwrap(),
            "todo:not:flag:A and todo:not:flag:B"
        );
    }

    #[test]
    fn module_paths_name_graphs_flat() {
        let logic = format!(
            "Logic/Module/Area_A/{}.ain",
            field::DESERT_COLOSSUS_COMMON_7E8D_LOGIC_MODULE
        );
        assert_eq!(
            graph_name(&logic),
            field::DESERT_COLOSSUS_COMMON_7E8D_LOGIC_MODULE
        );
    }

    #[test]
    fn chest_gates_from_the_dev_dump() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        const JABU_BOOMERANG: u64 = 5527401621172639818;
        const MIRROR_SHIELD: u64 = 10012167258643363548;
        const FIRE_KEY: u64 = 12945617941305968586;
        const SPIRIT_BOMBCHUS: u64 = 1501288453962537671;
        let traced = trace(&romfs).unwrap();
        let jabu = traced.iter().find(|g| g.hash == JABU_BOOMERANG).unwrap();
        let stinger = Atom::Actor {
            name: actor::ENEMY_STINGER.to_string(),
            output: "Dead".into(),
        };
        assert!(jabu.expr.atoms().contains(&&stinger));
        let wired = traced.iter().filter(|g| !g.expr.atoms().is_empty());
        assert_eq!(wired.count(), 79);

        let gates = gates(&romfs).unwrap();
        assert_eq!(gates.len(), 54);
        assert!(!gates.contains_key(&JABU_BOOMERANG));
        assert_eq!(
            gates[&MIRROR_SHIELD],
            format!("adult and switch:{}", actor::OBJ_STEP_SWITCH_A_01)
        );
        assert_eq!(
            gates[&FIRE_KEY],
            format!("flag:{}", flag::NPC_GORON_004_DELETE)
        );
        assert_eq!(
            gates[&SPIRIT_BOMBCHUS],
            format!("switch:{}", actor::OBJ_SWITCH_HIT_A_05)
        );
    }

    #[test]
    fn a_chest_that_rises_opens_with_the_flag_its_graph_sets() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        // Dodongo's Cavern's Red Rupee chest, risen once the Lizalfos fall.
        const DODONGO_RED_RUPEE: u64 = 7764016096998075927;
        const FIRE_KEY: u64 = 12945617941305968586;
        let traced = trace(&romfs).unwrap();
        let flags = |hash| {
            let gate = traced.iter().find(|g| g.hash == hash).unwrap();
            gate.expr.opening().join(" ")
        };
        assert_eq!(
            flags(DODONGO_RED_RUPEE),
            flag::GIMMICK_DEFEAT_ENEMY_OPEN_DODONGO_CAVE_01
        );
        assert_eq!(flags(FIRE_KEY), flag::NPC_GORON_004_DELETE);
    }
}
