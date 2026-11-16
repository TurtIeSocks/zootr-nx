//! MurmurHash3 x86 32-bit, seed 0: how GameData keys its entries by name.

const C1: u32 = 0xCC9E_2D51;
const C2: u32 = 0x1B87_3593;

fn scramble(k: u32) -> u32 {
    k.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2)
}

pub fn murmur3(data: &[u8]) -> u32 {
    let (chunks, tail) = data.as_chunks::<4>();
    let mut h = 0u32;
    for c in chunks {
        h ^= scramble(u32::from_le_bytes(*c));
        h = h.rotate_left(13).wrapping_mul(5).wrapping_add(0xE654_6B64);
    }
    if !tail.is_empty() {
        let k = tail
            .iter()
            .rev()
            .fold(0u32, |k, &b| (k << 8) | u32::from(b));
        h ^= scramble(k);
    }
    h ^= data.len() as u32;
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

#[cfg(test)]
mod tests {
    use super::murmur3;

    #[test]
    fn known_values() {
        assert_eq!(murmur3(b""), 0);
        assert_eq!(murmur3(b"Hello, world!"), 0xC036_3E43);
        // Length not a multiple of four.
        assert_eq!(
            murmur3(b"The quick brown fox jumps over the lazy dog"),
            0x2E4F_F723
        );
    }
}
