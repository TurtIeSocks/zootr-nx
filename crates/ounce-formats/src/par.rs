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

fn threads() -> usize {
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
    fn no_items_no_results() {
        assert!(par_map(&[] as &[u8], |n| *n).is_empty());
    }
}
