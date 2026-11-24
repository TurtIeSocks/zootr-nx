//! Things joined into sets, chained: a union-find forest.

/// `0..n` in sets, each a tree whose root names it.
pub struct DisjointSet {
    of: Vec<usize>,
}

impl DisjointSet {
    /// `0..n`, each in a set of its own.
    pub fn new(n: usize) -> Self {
        Self {
            of: (0..n).collect(),
        }
    }

    /// The root of `i`'s set, halving the path there.
    pub fn find(&mut self, mut i: usize) -> usize {
        while self.of[i] != i {
            self.of[i] = self.of[self.of[i]];
            i = self.of[i];
        }
        i
    }

    /// Puts `a`'s and `b`'s sets together.
    pub fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        self.of[ra] = rb;
    }

    /// Each one's root, in order.
    pub fn roots(&mut self) -> Vec<usize> {
        (0..self.of.len()).map(|i| self.find(i)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_chain_and_the_rest_stay_apart() {
        let mut sets = DisjointSet::new(5);
        sets.union(0, 1);
        sets.union(3, 1);
        let r = sets.roots();
        assert_eq!((r[0], r[3]), (r[1], r[1]));
        assert_ne!(r[2], r[1]);
        assert_ne!(r[4], r[1]);
        assert_ne!(r[2], r[4]);
    }
}
