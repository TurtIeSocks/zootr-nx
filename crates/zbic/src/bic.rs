//! ZBIC's FSE table header, in place of zstd's.
//!
//! zstd writes a table's normalized counts one by one as variable-width
//! bit fields. ZBIC writes them as digits of one number in mixed radix:
//! the table's largest symbol, its accuracy log, the counts' total, then
//! the running totals of the counts by binary interpolative coding. A
//! running total in the middle of a range lies between the totals at the
//! range's ends, so it takes one digit of base `high - low + 1`, and a
//! range whose ends are equal holds no digits at all: every total inside
//! it is the same.
//!
//! The header's first byte holds a flag (bit 7: each count is stored one
//! higher, so a count of -1, "less than one", fits) and how many bytes
//! follow it (bits 0 to 6). Those bytes are the number, its most
//! significant byte last. It's read as a stream: a digit is the value
//! modulo its base, the value is then divided by the base and topped up
//! with the next byte for as long as it's below [`FULL`].

use alloc::vec::Vec;

use crate::decoding::errors::FSETableError;

/// The value is topped up with bytes until it reaches this.
const FULL: u64 = 1 << 40;
/// How many largest symbols a header can name: 1 to 52, match lengths'.
const SYMBOLS: u64 = 52;
/// How many accuracy logs a header can name, from [`MIN_LOG`] up.
const LOGS: u64 = 8;
/// zstd's smallest accuracy log.
const MIN_LOG: u8 = 5;

/// The digits of the number a header's bytes hold.
struct Digits<'s> {
    bytes: &'s [u8],
    /// Bytes not read yet, which are `bytes[1..=left]`.
    left: usize,
    value: u64,
}

impl Digits<'_> {
    fn fill(&mut self) {
        while self.left > 0 && self.value < FULL {
            self.value = self.value << 8 | u64::from(self.bytes[self.left]);
            self.left -= 1;
        }
    }

    fn take(&mut self, base: u64) -> u64 {
        let digit = self.value % base;
        self.value /= base;
        self.fill();
        digit
    }
}

/// Reads the table header at the start of `source` into `probabilities`
/// (one count a symbol, -1 for "less than one"), refusing a table whose
/// largest symbol is past `max_symbol`. Returns the accuracy log and how
/// many bytes the header took.
pub(crate) fn read(
    source: &[u8],
    max_symbol: u8,
    probabilities: &mut Vec<i32>,
) -> Result<(u8, usize), FSETableError> {
    let first = *source.first().ok_or(FSETableError::Zbic("empty header"))?;
    let low = i64::from(first >> 7);
    let len = usize::from(first & 0x7f) + 1;
    // Never the whole source: the table's bit stream follows it.
    if len >= source.len() {
        return Err(FSETableError::Zbic("header longer than its source"));
    }
    let mut digits = Digits {
        bytes: source,
        left: len - 1,
        value: 0,
    };
    digits.fill();

    let largest = digits.take(SYMBOLS) as usize + 1;
    if largest > usize::from(max_symbol) {
        return Err(FSETableError::TooManySymbols { got: largest + 1 });
    }
    let log = digits.take(LOGS) as u8 + MIN_LOG;
    let total = 1u64 << log;
    let stored = digits.take(total) + 1;

    // `sums[i]` is the running total of the stored counts before symbol
    // `i`, over a power of two of symbols so ranges halve evenly.
    let span = (largest + 1).next_power_of_two();
    let mut sums = [0u64; 65];
    sums[span] = match low {
        0 => stored,
        _ => stored + largest as u64 + 1,
    };
    halve(&mut sums, 0, span, &mut digits)?;
    // One more digit past the ranges, between the first two totals.
    middle(&mut sums, 0, 0, 1, &mut digits)?;

    probabilities.clear();
    let mut before = 0;
    let mut left = total as i64;
    for &sum in &sums[1..=largest + 1] {
        let count = sum as i64 - before - low;
        before = sum as i64;
        if count < -1 {
            return Err(FSETableError::Zbic("count below -1"));
        }
        left -= count.abs();
        probabilities.push(count as i32);
    }
    if left != 0 {
        return Err(FSETableError::ProbabilityCounterMismatch {
            got: (total as i64 - left) as u32,
            expected_sum: total as u32,
            symbol_probabilities: probabilities.clone(),
        });
    }
    if digits.left != 0 {
        return Err(FSETableError::Zbic("bytes left over"));
    }
    Ok((log, len))
}

/// Reads the totals inside `low..high`: the middle one, then those in the
/// upper half, then those in the lower half.
fn halve(
    sums: &mut [u64],
    low: usize,
    high: usize,
    d: &mut Digits<'_>,
) -> Result<(), FSETableError> {
    if high - low < 2 {
        return Ok(());
    }
    let mid = (low + high) / 2;
    middle(sums, mid, low, high, d)?;
    halve(sums, mid, high, d)?;
    halve(sums, low, mid, d)
}

/// The total at `mid`, between those at `low` and `high`. Equal ends fill
/// the whole range between them.
fn middle(
    sums: &mut [u64],
    mid: usize,
    low: usize,
    high: usize,
    d: &mut Digits<'_>,
) -> Result<(), FSETableError> {
    let (from, to) = (sums[low], sums[high]);
    match to.checked_sub(from) {
        Some(0) => sums[low + 1..high].fill(from),
        Some(gap) => sums[mid] = from + d.take(gap + 1),
        None => return Err(FSETableError::Zbic("running totals fall")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fse::FSETable;
    use alloc::vec;

    /// A header for `counts` at accuracy `log`, for counts small enough
    /// that the whole number fits in the first fill. Its digits are
    /// worked out the way [`read`] takes them, in the same order.
    fn header(counts: &[i32], log: u8) -> Vec<u8> {
        let low = counts.contains(&-1);
        let largest = counts.len() - 1;
        let stored: Vec<u64> = counts
            .iter()
            .map(|&c| (c + i32::from(low)) as u64)
            .collect();
        let span = counts.len().next_power_of_two();
        let mut sums = vec![0u64; span + 1];
        for i in 0..counts.len() {
            sums[i + 1] = sums[i] + stored[i];
        }
        let end = sums[counts.len()];
        sums[counts.len()..].fill(end);
        let total = 1u64 << log;
        let mut digits = vec![
            (largest as u64 - 1, SYMBOLS),
            (u64::from(log - MIN_LOG), LOGS),
        ];
        let sum = counts.iter().map(|&c| i64::from(c.abs())).sum::<i64>();
        let first = match low {
            false => end - 1,
            true => end - largest as u64 - 2,
        };
        assert_eq!(sum, total as i64);
        digits.push((first, total));
        steps(&sums, 0, span, &mut digits);
        if sums[1] != sums[0] {
            digits.push((sums[0], sums[1] + 1));
        }
        let mut number = 0u128;
        for &(digit, base) in digits.iter().rev() {
            number = number * u128::from(base) + u128::from(digit);
        }
        assert!(number < u128::from(FULL), "too big for one fill");
        let mut bytes = vec![];
        while number > 0 {
            bytes.push(number as u8);
            number >>= 8;
        }
        let mut out = vec![u8::from(low) << 7 | bytes.len() as u8];
        out.extend(bytes);
        out.push(0); // the table's bit stream
        out
    }

    fn steps(sums: &[u64], low: usize, high: usize, digits: &mut Vec<(u64, u64)>) {
        if high - low < 2 || sums[low] == sums[high] {
            return;
        }
        let mid = (low + high) / 2;
        digits.push((sums[mid] - sums[low], sums[high] - sums[low] + 1));
        steps(sums, mid, high, digits);
        steps(sums, low, mid, digits);
    }

    fn reads(counts: &[i32], log: u8) {
        let bytes = header(counts, log);
        let mut probabilities = vec![];
        let (got_log, used) = read(&bytes, 52, &mut probabilities).unwrap();
        assert_eq!((got_log, used), (log, bytes.len() - 1));
        assert_eq!(probabilities, counts);
    }

    #[test]
    fn tables_read_back() {
        reads(&[16, 16], 5);
        reads(&[20, 0, 4, 8], 5);
        reads(&[1, 30, 0, 1], 5);
        reads(&[-1, 29, -1, 0, 0, 1], 5);
        reads(&[0, 64], 6);
    }

    #[test]
    fn a_read_table_decodes_like_one_built_from_its_counts() {
        let counts = [-1, 10, 0, 21];
        let mut read_table = FSETable::new(52);
        read_table.build_decoder(&header(&counts, 5), 9).unwrap();
        let mut built = FSETable::new(52);
        built.build_from_probabilities(5, &counts).unwrap();
        let entries = |t: &FSETable| {
            t.decode
                .iter()
                .map(|e| (e.symbol, e.base_line, e.num_bits))
                .collect::<Vec<_>>()
        };
        assert_eq!(entries(&read_table), entries(&built));
    }

    #[test]
    fn bad_headers_are_refused() {
        let good = header(&[20, 0, 4, 8], 5);
        let mut p = vec![];
        // No room for the bit stream after it.
        assert!(read(&good[..good.len() - 1], 52, &mut p).is_err());
        // A largest symbol past the table's.
        assert!(read(&good, 2, &mut p).is_err());
        // Counts that don't add up.
        let mut bad = good.clone();
        bad[1] ^= 0x40;
        assert!(read(&bad, 52, &mut p).is_err());
        // An accuracy log past the caller's.
        let mut table = FSETable::new(52);
        assert!(table.build_decoder(&header(&[64, 0], 6), 5).is_err());
    }
}
