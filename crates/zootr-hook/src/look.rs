//! The ice a freeze shows, sized for Link's age. A seed with traps points
//! Link's frozen effect at the full-body shells, which are adult sized.
//! The game rereads each shell's fields every time the ice shows, so the
//! hook writes the size fields for Link's age, from the table's [`Look`].

/// Where the effect file's field records sit, from the effect system
/// `main` points at: the reads after it, then the records' own pointer.
pub const PATH: [usize; 4] = [0x8, 0x18, 0x0, 0x28];

/// A shell's size, adult first, then child: its record's field mask, and
/// the size field's value, a bone for the adult and a scale for the
/// child. Then where a record's size value sits, and how many records
/// follow.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    pub masks: [u64; 2],
    pub values: [u32; 2],
    pub value_at: u32,
    pub count: u32,
}

const SIZES: usize = size_of::<Sizes>();
const _: () = assert!(SIZES == 32);

/// The look in the table: the [`Sizes`], then each shell record's place
/// among the field records.
#[derive(Clone, Debug, PartialEq)]
pub struct Look<'a> {
    pub sizes: Sizes,
    records: &'a [u8],
}

impl<'a> Look<'a> {
    /// The look in `bytes`, `None` when the seed wrote none.
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        let (sizes, records) = bytes.split_at_checked(SIZES)?;
        // SAFETY: `Sizes` is plain numbers, all of it in `sizes`.
        let sizes = unsafe { sizes.as_ptr().cast::<Sizes>().read_unaligned() };
        let records = records.get(..4 * sizes.count as usize)?;
        Some(Self { sizes, records })
    }

    /// Each shell record's place.
    pub fn records(&self) -> impl Iterator<Item = u32> + 'a {
        self.records
            .as_chunks()
            .0
            .iter()
            .map(|&w| u32::from_le_bytes(w))
    }
}

/// What the generator writes as a [`Look`]. `count` comes from the
/// records.
#[cfg(not(target_os = "none"))]
#[derive(Clone, Debug, PartialEq)]
pub struct LookSpec {
    pub sizes: Sizes,
    pub records: Vec<u32>,
}

#[cfg(not(target_os = "none"))]
impl LookSpec {
    pub fn bytes(&self) -> Vec<u8> {
        let s = self.sizes;
        let count = u32::try_from(self.records.len()).expect("a look under 4 G records");
        let masks = s.masks.iter().flat_map(|m| m.to_le_bytes());
        let words = [s.values[0], s.values[1], s.value_at, count];
        let words = words.into_iter().chain(self.records.iter().copied());
        masks.chain(words.flat_map(u32::to_le_bytes)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES_FAKE: Sizes = Sizes {
        masks: [1 << 12 | 3, 1 << 13 | 3],
        values: [1 << 24 | 0x40, 0x6b],
        value_at: 16,
        count: 2,
    };

    fn spec() -> LookSpec {
        LookSpec {
            sizes: SIZES_FAKE,
            records: vec![0x10, 0x30],
        }
    }

    #[test]
    fn a_look_round_trips() {
        let bytes = spec().bytes();
        let look = Look::parse(&bytes).unwrap();
        assert_eq!(look.sizes, SIZES_FAKE);
        assert_eq!(look.records().collect::<Vec<_>>(), [0x10, 0x30]);
    }

    #[test]
    fn no_bytes_is_no_look() {
        assert_eq!(Look::parse(&[]), None);
    }

    #[test]
    fn a_look_cut_short_is_none() {
        let bytes = spec().bytes();
        assert_eq!(Look::parse(&bytes[..bytes.len() - 1]), None);
    }
}
