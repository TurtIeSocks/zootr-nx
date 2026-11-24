//! Looking a graph over: finding a node, and reading back its queries,
//! children and links.

use anyhow::{Context, Result};

use super::{BOOL_SELECTOR, CHILD, File, NAME, Node, QUERY_BASE, QUERY_COUNT, UNSET, word};

impl File {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The first node of class `name`, like `OneshotDoNothing`.
    pub fn find(&self, name: &str) -> Option<usize> {
        let named = |n: &Node| self.str(n.u32_at(NAME)).is_ok_and(|s| s == name);
        self.nodes.iter().position(named)
    }

    /// The first bool selector, which picks a child by a bool input.
    pub fn find_bool_selector(&self) -> Option<usize> {
        self.nodes.iter().position(|n| n.kind() == BOOL_SELECTOR)
    }

    /// The query nodes `node` waits on, by node index.
    pub fn queries(&self, node: usize) -> Result<Vec<usize>> {
        let n = self.node(node)?;
        let (base, count) = (
            n.u16_at(QUERY_BASE) as usize,
            n.u16_at(QUERY_COUNT) as usize,
        );
        let held = self
            .queries
            .get(base * 4..(base + count) * 4)
            .context("queries past the table")?;
        let queries: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| self.nodes[i].is_query())
            .collect();
        held.chunks(4)
            .map(|q| {
                let place = u16::from_le_bytes([q[0], q[1]]) as usize;
                queries
                    .get(place)
                    .copied()
                    .context("a query past the query nodes")
            })
            .collect()
    }

    /// `node`'s child plugs, by name and the node each points at.
    pub fn children(&self, node: usize) -> Result<Vec<(String, i32)>> {
        let plugs = &self.node(node)?.plugs[CHILD];
        plugs
            .iter()
            .map(|r| Ok((self.str(word(r, 4))?.to_owned(), word(r, 0) as i32)))
            .collect()
    }

    /// `node`'s plugs past its children (kind 3 and up), by kind and the
    /// node each points at.
    pub fn links(&self, node: usize) -> Result<Vec<(usize, i32)>> {
        let plugs = &self.node(node)?.plugs;
        Ok((plugs.iter().enumerate().skip(CHILD + 1))
            .flat_map(|(kind, records)| records.iter().map(move |r| (kind, word(r, 0) as i32)))
            .collect())
    }

    /// The first child plug still pointing nowhere, by node and plug name.
    pub(in crate::ainb) fn unset(&self) -> Option<(usize, String)> {
        let nodes = self.nodes.iter().enumerate();
        nodes
            .flat_map(|(n, node)| node.plugs[CHILD].iter().map(move |r| (n, r)))
            .find_map(|(n, r)| {
                (word(r, 0) as i32 == UNSET)
                    .then(|| (n, self.str(word(r, 4)).unwrap_or("?").into()))
            })
    }
}
