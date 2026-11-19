//! Work spread over every core, for the independent jobs a seed is made
//! of: blocks to encode, files to read and parse, files to compress.

use std::num::NonZero;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `f` over each of `items` on every core, the results in `items`' order.
/// Each worker takes the next item as it finishes one, so a few slow
/// items don't hold the rest. A panic in `f` panics here.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = threads().min(items.len());
    if threads <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else { break done };
                        done.push((i, f(item)));
                    }
                })
            })
            .collect();
        (workers.into_iter())
            .flat_map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });
    done.sort_unstable_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// [`par_map`] over a few of `items` per core at a time, each item with
/// its result, so only one batch of results is held at once: for results
/// much bigger than their items, like parsed files.
pub fn par_batches<T: Sync, R: Send>(
    items: Vec<T>,
    f: impl Fn(&T) -> R + Sync,
) -> impl Iterator<Item = (T, R)> {
    let batch = 2 * threads();
    let mut items = items.into_iter();
    std::iter::from_fn(move || {
        let chunk: Vec<T> = items.by_ref().take(batch).collect();
        (!chunk.is_empty()).then(|| {
            let done = par_map(&chunk, &f);
            chunk.into_iter().zip(done)
        })
    })
    .flatten()
}

/// How many threads the parallel helpers use: one per core.
pub fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, NonZero::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_come_back_in_order() {
        let items: Vec<u32> = (0..1000).collect();
        assert_eq!(
            par_map(&items, |n| n * 2),
            (0..1000).map(|n| n * 2).collect::<Vec<_>>()
        );
    }

    #[test]
    fn batches_pair_each_item_with_its_result_in_order() {
        let items: Vec<u32> = (0..1000).collect();
        let got: Vec<(u32, u32)> = par_batches(items, |n| n * 2).collect();
        assert_eq!(got, (0..1000).map(|n| (n, n * 2)).collect::<Vec<_>>());
    }

    #[test]
    fn no_items_no_results() {
        assert!(par_map(&[] as &[u8], |n| *n).is_empty());
    }
}
