//! Tests that run many independent cases spread them over every core.

use std::sync::atomic::{AtomicUsize, Ordering};

/// `f` of each item, in the items' order, worked on by one thread a core.
/// Each thread takes the next item left, so a slow item doesn't hold up
/// the items queued behind it.
pub(crate) fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            return done;
                        };
                        done.push((i, f(item)));
                    }
                })
            })
            .collect();
        (workers.into_iter())
            .flat_map(|w| w.join().unwrap())
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_the_items_order() {
        let items: Vec<u32> = (0..1000).collect();
        let doubled = super::map(&items, |n| n * 2);
        assert_eq!(doubled, items.iter().map(|n| n * 2).collect::<Vec<_>>());
    }
}
