//! Questions the flag map answers, each as a report that prints as lines
//! or serializes as JSON. Reports stop at [`CAP`] edges so an agent's
//! context stays small.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use anyhow::{Context, Result};
use serde::Serialize;

use super::{Edge, Graph, Kind, Link, Node};
use crate::flows;

/// Edges a report lists before it says how many more there are.
pub const CAP: usize = 50;
/// Classes `stats` lists that the asks table doesn't cover.
const UNASKED: usize = 20;
/// Shortest paths `trace` lists.
const PATHS: usize = 5;

#[derive(Clone, Copy)]
pub enum Dir {
    Up,
    Down,
}

#[derive(Serialize)]
pub struct Line<'a> {
    pub id: usize,
    pub link: Link,
    pub from: &'a Node,
    pub to: &'a Node,
    pub label: &'a str,
    pub file: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<u16>,
    /// Hops from the node asked about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

#[derive(Serialize)]
pub struct Report<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<&'a Node>,
    pub edges: Vec<Line<'a>>,
    /// Edges left out past the cap.
    pub more: usize,
}

#[derive(Serialize)]
pub struct Found<'a> {
    pub nodes: Vec<&'a Node>,
    pub more: usize,
}

#[derive(Serialize)]
pub struct Paths<'a> {
    pub paths: Vec<Vec<Line<'a>>>,
}

#[derive(Serialize)]
pub struct Stats {
    pub nodes: BTreeMap<String, usize>,
    pub edges: BTreeMap<String, usize>,
    /// States known only by hash.
    pub unnamed: usize,
    /// States with no edge at all.
    pub unlinked: usize,
    /// `$argument` names no caller passes.
    pub unresolved: usize,
    /// Edges by what they do, most first.
    pub labels: Vec<(String, usize)>,
    /// Graphs the AINB reader can't parse yet, left out.
    pub unread: usize,
    /// The node classes graphs use most that the asks table doesn't cover.
    pub unasked: Vec<(String, usize)>,
}

impl Graph {
    /// Nodes whose name holds `text`, ignoring case: exact names first,
    /// then names starting with it.
    pub fn search(&self, text: &str, kind: Option<Kind>) -> Vec<usize> {
        let want = text.to_lowercase();
        let mut found: Vec<(u8, usize, usize)> = (self.nodes.iter().enumerate())
            .filter(|(_, n)| kind.is_none_or(|k| n.kind == k))
            .filter_map(|(i, n)| {
                let name = n.name.to_lowercase();
                let rank = match (name == want, name.starts_with(&want), name.contains(&want)) {
                    (true, _, _) => 0,
                    (_, true, _) => 1,
                    (_, _, true) => 2,
                    _ => return None,
                };
                Some((rank, name.len(), i))
            })
            .collect();
        found.sort_unstable();
        found.into_iter().map(|(_, _, i)| i).collect()
    }

    pub fn found(&self, text: &str, kind: Option<Kind>) -> Found<'_> {
        let all = self.search(text, kind);
        Found {
            more: all.len().saturating_sub(CAP),
            nodes: all.iter().take(CAP).map(|&i| &self.nodes[i]).collect(),
        }
    }

    fn line<'a>(&'a self, e: &'a Edge, depth: Option<usize>) -> Line<'a> {
        Line {
            id: e.id,
            link: e.link,
            from: &self.nodes[e.from],
            to: &self.nodes[e.to],
            label: &e.label,
            file: &e.file,
            at: e.at,
            depth,
        }
    }

    fn report(
        &self,
        node: usize,
        edges: Vec<(usize, Option<usize>)>,
        kind: Option<Kind>,
    ) -> Report<'_> {
        let other = |e: &Edge| if e.from == node { e.to } else { e.from };
        let kept: Vec<_> = (edges.into_iter())
            .filter(|(e, _)| kind.is_none_or(|k| self.nodes[other(&self.edges[*e])].kind == k))
            .collect();
        Report {
            node: Some(&self.nodes[node]),
            more: kept.len().saturating_sub(CAP),
            edges: (kept.iter().take(CAP))
                .map(|&(e, d)| self.line(&self.edges[e], d))
                .collect(),
        }
    }

    /// One node and every edge touching it, what feeds it first.
    pub fn around(&self, node: usize, kind: Option<Kind>) -> Report<'_> {
        let edges = self.ins[node]
            .iter()
            .chain(&self.outs[node])
            .map(|&e| (e, None));
        self.report(node, edges.collect(), kind)
    }

    /// What sets a state.
    pub fn writers(&self, node: usize) -> Report<'_> {
        let edges = self.ins[node]
            .iter()
            .filter(|&&e| self.edges[e].link == Link::Writes);
        self.report(node, edges.map(|&e| (e, None)).collect(), None)
    }

    /// What asks a state.
    pub fn readers(&self, node: usize) -> Report<'_> {
        let edges = self.outs[node]
            .iter()
            .filter(|&&e| self.edges[e].link == Link::Reads);
        self.report(node, edges.map(|&e| (e, None)).collect(), None)
    }

    /// Every edge within `depth` hops upstream or downstream, nearest first.
    pub fn walk(&self, node: usize, dir: Dir, depth: usize, kind: Option<Kind>) -> Report<'_> {
        let mut seen = vec![false; self.nodes.len()];
        seen[node] = true;
        let (mut todo, mut edges) = (VecDeque::from([(node, 0)]), Vec::new());
        while let Some((n, d)) = todo.pop_front() {
            if d == depth {
                continue;
            }
            let (list, far): (_, fn(&Edge) -> usize) = match dir {
                Dir::Up => (&self.ins[n], |e| e.from),
                Dir::Down => (&self.outs[n], |e| e.to),
            };
            for &e in list {
                edges.push((e, Some(d + 1)));
                let next = far(&self.edges[e]);
                if !std::mem::replace(&mut seen[next], true) {
                    todo.push_back((next, d + 1));
                }
            }
        }
        let mut report = self.report(node, edges, None);
        if let Some(k) = kind {
            report.edges.retain(|l| l.from.kind == k || l.to.kind == k);
        }
        report
    }

    /// The shortest paths from `from` to `to`, following edges forward.
    pub fn trace(&self, from: usize, to: usize) -> Paths<'_> {
        // Each node's distance and the edges reaching it at that distance.
        let mut dist = vec![usize::MAX; self.nodes.len()];
        let mut via: Vec<Vec<usize>> = vec![Vec::new(); self.nodes.len()];
        dist[from] = 0;
        let mut todo = VecDeque::from([from]);
        while let Some(n) = todo.pop_front() {
            if dist[to] != usize::MAX && dist[n] >= dist[to] {
                break;
            }
            for &e in &self.outs[n] {
                let next = self.edges[e].to;
                if dist[next] == usize::MAX {
                    dist[next] = dist[n] + 1;
                    todo.push_back(next);
                }
                if dist[next] == dist[n] + 1 {
                    via[next].push(e);
                }
            }
        }
        let mut paths = Vec::new();
        let mut stack = vec![(to, Vec::new())];
        while let Some((n, tail)) = stack.pop() {
            if paths.len() == PATHS {
                break;
            }
            if n == from {
                let path: Vec<usize> = tail.iter().rev().copied().collect();
                paths.push(
                    path.iter()
                        .map(|&e| self.line(&self.edges[e], None))
                        .collect(),
                );
                continue;
            }
            for &e in &via[n] {
                let mut t = tail.clone();
                t.push(e);
                stack.push((self.edges[e].from, t));
            }
        }
        Paths { paths }
    }

    /// The lines an edge cites: the flow event, the event table row, or
    /// the BoolExp.
    pub fn source(&self, edge: usize) -> Result<String> {
        let e = self
            .edges
            .get(edge)
            .with_context(|| format!("no edge #{edge}"))?;
        let line = self.line(e, None).to_string();
        let cited = match (e.at, &self.flows, &self.gamedata) {
            (Some(at), ..) if self.scene.contains_key(&e.file) => {
                let graph = &self.scene[&e.file];
                crate::graphs::node_line(at.into(), &graph.nodes[usize::from(at)])
            }
            (Some(at), Some(flows), _) => {
                let chart = flows.0.get(&e.file).context("flow not loaded")?;
                let want = format!("#{at} ");
                (flows::render(chart).lines())
                    .find(|l| l.starts_with(&want))
                    .map(str::to_owned)
                    .unwrap_or_default()
            }
            (None, _, Some(gd)) if e.link == Link::Reads => {
                let exp = &self.nodes[e.to].name;
                gd.expr(exp)?
                    .map(|x| format!("{exp} = {x}"))
                    .unwrap_or_default()
            }
            _ => String::new(),
        };
        Ok(format!("{line}\n{cited}"))
    }

    pub fn stats(&self) -> Stats {
        let mut nodes = BTreeMap::new();
        let (mut unnamed, mut unlinked, mut unresolved) = (0, 0, 0);
        for (i, n) in self.nodes.iter().enumerate() {
            *nodes
                .entry(format!("{:?}", n.kind).to_lowercase())
                .or_default() += 1;
            if n.kind == Kind::State {
                unnamed += usize::from(n.name.starts_with('#'));
                unresolved += usize::from(n.name.starts_with('$'));
                unlinked += usize::from(self.ins[i].is_empty() && self.outs[i].is_empty());
            }
        }
        let (mut edges, mut labels) = (BTreeMap::new(), BTreeMap::<String, usize>::new());
        for e in &self.edges {
            *edges
                .entry(format!("{:?}", e.link).to_lowercase())
                .or_default() += 1;
            let head = e.label.split([' ', '=']).next().unwrap_or_default();
            *labels
                .entry(format!("{:?} {head}", e.link).to_lowercase())
                .or_default() += 1;
        }
        let mut labels: Vec<_> = labels.into_iter().collect();
        labels.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let asked = |i: usize| {
            self.ins[i]
                .iter()
                .any(|&e| self.edges[e].link == Link::Asks)
        };
        let mut unasked: Vec<_> = (0..self.nodes.len())
            .filter(|&i| self.nodes[i].kind == Kind::Class && !asked(i))
            .map(|i| (self.nodes[i].name.clone(), self.outs[i].len()))
            .collect();
        unasked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        unasked.truncate(UNASKED);
        Stats {
            nodes,
            edges,
            unnamed,
            unlinked,
            unresolved,
            labels,
            unread: self.unread.len(),
            unasked,
        }
    }
}

fn link_word(link: Link) -> &'static str {
    match link {
        Link::Writes => "writes",
        Link::Reads => "read by",
        Link::Runs => "runs",
        Link::Drives => "drives",
        Link::Places => "places",
        Link::Has => "has",
        Link::Uses => "used by",
        Link::Asks => "asked by",
    }
}

impl fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let at = self.at.map(|a| format!(" #{a}")).unwrap_or_default();
        let depth = self.depth.map(|d| format!("{d} ")).unwrap_or_default();
        write!(
            f,
            "{depth}#{} {} {} {}  {}  ({}{at})",
            self.id,
            self.from.id,
            link_word(self.link),
            self.to.id,
            self.label,
            self.file
        )
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.detail {
            Some(d) => write!(f, "{}  ({d})", self.id),
            None => write!(f, "{}", self.id),
        }
    }
}

fn more(f: &mut fmt::Formatter, n: usize) -> fmt::Result {
    match n {
        0 => Ok(()),
        n => writeln!(f, "{n} more, narrow with --kind"),
    }
}

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(n) = self.node {
            writeln!(f, "{n}")?;
        }
        if self.edges.is_empty() {
            writeln!(f, "no edges")?;
        }
        self.edges.iter().try_for_each(|l| writeln!(f, "{l}"))?;
        more(f, self.more)
    }
}

impl fmt::Display for Found<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.nodes.iter().try_for_each(|n| writeln!(f, "{n}"))?;
        more(f, self.more)
    }
}

impl fmt::Display for Paths<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.paths.is_empty() {
            return writeln!(f, "no path");
        }
        for (i, path) in self.paths.iter().enumerate() {
            writeln!(f, "path {} ({} hops)", i + 1, path.len())?;
            path.iter().try_for_each(|l| writeln!(f, "  {l}"))?;
        }
        Ok(())
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let list = |m: &BTreeMap<String, usize>| {
            m.iter()
                .map(|(k, v)| format!("{v} {k}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(f, "nodes: {}", list(&self.nodes))?;
        writeln!(f, "edges: {}", list(&self.edges))?;
        writeln!(
            f,
            "states: {} unnamed, {} unlinked, {} unresolved arguments",
            self.unnamed, self.unlinked, self.unresolved
        )?;
        writeln!(f, "graphs the reader can't parse yet: {}", self.unread)?;
        writeln!(f, "classes most used that the asks table doesn't cover:")?;
        for (class, graphs) in &self.unasked {
            writeln!(f, "  {graphs:>6} {class}")?;
        }
        writeln!(f, "edges by action:")?;
        self.labels
            .iter()
            .try_for_each(|(k, v)| writeln!(f, "  {v:>6} {k}"))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Graph, Kind, Link};
    use super::{CAP, Dir};

    /// A graph of events named in `edges`, each `from > to`.
    fn graph(edges: &[&str]) -> Graph {
        let mut g = Graph::default();
        for e in edges {
            let (from, to) = e.split_once('>').unwrap();
            let (from, to) = (
                g.node(Kind::Event, from, || None),
                g.node(Kind::Event, to, || None),
            );
            g.edge(from, to, Link::Runs, String::new(), "FakeFlow", None);
        }
        g
    }

    #[test]
    fn trace_lists_every_shortest_path_and_no_longer_one() {
        let g = graph(&["a>b", "b>d", "a>c", "c>d", "a>e", "e>f", "f>d"]);
        let paths = g.trace(g.find("a").unwrap(), g.find("d").unwrap()).paths;
        assert_eq!(paths.len(), 2);
        assert!(
            paths
                .iter()
                .all(|p| p.len() == 2 && p[0].from.name == "a" && p[1].to.name == "d")
        );
        assert!(
            g.trace(g.find("d").unwrap(), g.find("a").unwrap())
                .paths
                .is_empty()
        );
    }

    #[test]
    fn a_walk_stops_at_its_depth_and_says_each_edges_hops() {
        let g = graph(&["a>b", "b>c", "c>d"]);
        let down = g.walk(g.find("a").unwrap(), Dir::Down, 2, None);
        let hops: Vec<_> = down
            .edges
            .iter()
            .map(|l| (l.to.name.as_str(), l.depth))
            .collect();
        assert_eq!(hops, [("b", Some(1)), ("c", Some(2))]);
        let up = g.walk(g.find("d").unwrap(), Dir::Up, 1, None);
        assert_eq!(up.edges.len(), 1);
    }

    #[test]
    fn a_report_stops_at_the_cap_and_counts_the_rest() {
        let edges: Vec<String> = (0..CAP + 7).map(|i| format!("hub>n{i}")).collect();
        let g = graph(&edges.iter().map(String::as_str).collect::<Vec<_>>());
        let r = g.around(g.find("hub").unwrap(), None);
        assert_eq!((r.edges.len(), r.more), (CAP, 7));
    }

    #[test]
    fn find_takes_an_id_a_name_or_text_one_name_holds() {
        let g = graph(&["FakeDoor_Open>FakeDoor_Shut", "FakeGate>FakeGate_Shut"]);
        assert_eq!(
            g.find("event:FakeGate").unwrap(),
            g.find("FakeGate").unwrap()
        );
        assert_eq!(g.nodes[g.find("door_open").unwrap()].name, "FakeDoor_Open");
        let err = g.find("shut").unwrap_err().to_string();
        assert!(err.contains("matches 2 nodes"), "{err}");
        assert!(g.find("nothing").is_err());
    }
}
