//! The ice a freeze shows. The remake freezes Link with no ice on him, but
//! its effect file still has the full-body shells. A seed with traps
//! points his frozen effect at them, and the hook sizes them for child
//! Link (`zootr_hook::look`). The layout is in `docs/research/ice-traps.md`.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ModBuilder, Romfs};
use zootr_hook::look::{LookSpec, Sizes};

/// Link's effect file, which the dump cache extracts too.
pub const FILES: [&str; 1] = ["ELink2/elink2.Product.110.belnk.zs"];
const PATH: &str = FILES[0];
/// The file's magic.
const MAGIC: &[u8] = b"XLNK";
/// Where the adult effect user's calls start, which child Link uses too,
/// and each call's size.
const CALLS: usize = 0xe_e96c;
const CALL: usize = 0x30;
/// Where a call keeps its param position.
const PARAM: usize = 0x20;
/// The frozen condition's call. Its param position picks the pieces only,
/// which play for none of a test Link's items, or the full body.
const FROZEN: usize = 132;
const PIECES: u32 = 0x79f0;
const FULL_BODY: u32 = 0x1c10;
/// The 16 body shells' calls. The hat pieces' calls stay as they are.
const SHELLS: std::ops::RangeInclusive<usize> = 0x38d..=0x39c;
/// Where the field records start: a call's record is here plus its param
/// position.
const FIELDS: usize = 0x4640;
/// The fields a shell's size is set by: the adult binds it to a bone,
/// the child gets a scale.
const BONE: u32 = 12;
const SCALE: u32 = 13;
/// The float table, and the index in it that is child Link's scale.
const FLOATS: usize = 0x5_ee34;
const CHILD_SCALE: u32 = 0x6b;
const CHILD_SIZE: f32 = 0.7;

/// Write Link's effect file with the full-body ice, and say how the hook
/// sizes it.
pub fn look(romfs: &Romfs, builder: &mut ModBuilder) -> Result<LookSpec> {
    let mut file = romfs.read(PATH)?;
    let spec = fitted(&mut file).with_context(|| format!("in {PATH}"))?;
    builder.write(PATH, &file)?;
    Ok(spec)
}

/// Point `file`'s frozen effect at the full body, and read its shells'
/// adult and child sizes.
fn fitted(file: &mut [u8]) -> Result<LookSpec> {
    ensure!(
        file.starts_with(MAGIC),
        "no {} magic",
        String::from_utf8_lossy(MAGIC)
    );
    let frozen = CALLS + FROZEN * CALL + PARAM;
    let was = word(file, frozen)?;
    ensure!(
        was == PIECES,
        "the frozen call's param is {was:#x}, not {PIECES:#x}"
    );
    file[frozen..frozen + 4].copy_from_slice(&FULL_BODY.to_le_bytes());
    let child_size = f32::from_bits(word(file, FLOATS + 4 * CHILD_SCALE as usize)?);
    ensure!(
        child_size == CHILD_SIZE,
        "float {CHILD_SCALE:#x} is {child_size}"
    );
    let records =
        (SHELLS.map(|call| word(file, CALLS + call * CALL + PARAM))).collect::<Result<Vec<_>>>()?;
    let adults = (records.iter())
        .map(|&r| adult(file, FIELDS + r as usize))
        .collect::<Result<Vec<_>>>()?;
    let [first, rest @ ..] = &adults[..] else {
        unreachable!("16 shells")
    };
    ensure!(rest.iter().all(|a| a == first), "the shells' sizes differ");
    let (mask, bone, value_at) = *first;
    let sizes = Sizes {
        masks: [mask, mask & !(1 << BONE) | 1 << SCALE],
        values: [bone, CHILD_SCALE],
        value_at,
        count: records.len() as u32,
    };
    Ok(LookSpec { sizes, records })
}

/// The record at `at`'s size: its mask, its bone, and where the bone's
/// value sits. Its values follow the mask, one word per field set, in bit
/// order, so the scale takes the bone's place.
fn adult(file: &[u8], at: usize) -> Result<(u64, u32, u32)> {
    let mask = u64::from(word(file, at)?) | u64::from(word(file, at + 4)?) << 32;
    ensure!(mask & 1 << BONE != 0, "a shell at {at:#x} binds no bone");
    ensure!(mask & 1 << SCALE == 0, "a shell at {at:#x} has a scale");
    let value_at = 8 + 4 * (mask & ((1 << BONE) - 1)).count_ones();
    let value = word(file, at + value_at as usize)?;
    Ok((mask, value, value_at))
}

/// The little endian word at `at`.
fn word(file: &[u8], at: usize) -> Result<u32> {
    let bytes = file.get(at..at + 4).context("the file ends early")?;
    Ok(u32::from_le_bytes(bytes.try_into()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;

    #[test]
    fn only_the_frozen_call_changes_and_the_child_gets_a_scale() {
        let Some(romfs) = dev_dump() else { return };
        let game = romfs.read(PATH).unwrap();
        let mut file = game.clone();
        let spec = fitted(&mut file).unwrap();
        let changed: Vec<_> = (0..game.len()).filter(|&i| game[i] != file[i]).collect();
        let frozen = CALLS + FROZEN * CALL + PARAM;
        assert!(changed.iter().all(|i| (frozen..frozen + 4).contains(i)));
        assert_eq!(spec.records.len(), 16);
        let [adult, child] = spec.sizes.masks;
        assert_eq!(child.count_ones(), adult.count_ones());
        assert_eq!(child & 1 << SCALE, 1 << SCALE);
        let at = FIELDS + spec.records[0] as usize + spec.sizes.value_at as usize;
        assert_eq!(word(&game, at).unwrap(), spec.sizes.values[0]);
    }

    #[test]
    fn a_file_already_written_is_refused() {
        let Some(romfs) = dev_dump() else { return };
        let mut file = romfs.read(PATH).unwrap();
        fitted(&mut file).unwrap();
        assert!(fitted(&mut file).is_err());
    }
}
