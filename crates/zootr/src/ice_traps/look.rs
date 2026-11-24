//! The ice a freeze shows. The remake freezes Link with no ice on him, but
//! its effect file still has the full-body shells. A seed with traps
//! points his frozen effect at them, and the hook sizes them for child
//! Link (`zootr_hook::look`). The layout is in `docs/research/ice-traps.md`.

use anyhow::{Context, Result, bail, ensure};
use ounce_formats::{
    ModBuilder, Romfs, XlinkCall, xlink_asset_params, xlink_call_children, xlink_call_table,
    xlink_calls, xlink_direct_values,
};
use zootr_hook::look::{LookSpec, Sizes};

use crate::known::{Name, actor, field};

/// Link's effect file, which the dump cache extracts too. Each build has
/// it under its own stamp.
pub const FILES: [&str; 1] = ["ELink2/elink2.Product.110.belnk.zs"];
const PATH: &str = FILES[0];
/// The file's magic.
const MAGIC: &[u8] = b"XLNK";
/// The adult's effect user, which child Link uses too, and its frozen
/// condition: a grid whose two cells play the full body or the pieces
/// only, for none of a test Link's items.
const USER: &Name = actor::PLAYER_ADULT;
const FROZEN: &Name = field::CONDITION_FROZEN;
/// Where a call keeps its param position.
const PARAM: usize = 0x20;
/// How many body shells the full body's switch picks from. The hat
/// pieces stay as they are.
const SHELLS: usize = 16;
/// The fields a shell's size is set by: the adult binds it to a bone,
/// the child gets a scale.
const BONE: u32 = 12;
const SCALE: u32 = 13;
/// Child Link's scale, a direct value.
const CHILD_SIZE: f32 = 0.7;

/// Write Link's effect file with the full-body ice, and say how the hook
/// sizes it.
pub fn look(romfs: &Romfs, builder: &mut ModBuilder) -> Result<LookSpec> {
    let mut file = romfs.read(PATH)?;
    let spec = fitted(&mut file).with_context(|| format!("in {PATH}"))?;
    builder.write(PATH, &file)?;
    Ok(spec)
}

/// The calls [`fitted`] reads and writes, found by the file's structure.
struct Found {
    /// Where the frozen call's param position is, and the full body's.
    frozen: usize,
    full_body: u32,
    /// The body shells' param positions.
    records: Vec<u32>,
    /// Where the asset param records start.
    fields: usize,
    /// Child Link's scale's index among the direct values.
    child_scale: u32,
}

/// The frozen call, its full body, the shells and the tables `file`
/// keeps them in.
fn found(file: &[u8]) -> Result<Found> {
    ensure!(
        file.starts_with(MAGIC),
        "no {} magic",
        String::from_utf8_lossy(MAGIC)
    );
    let calls = xlink_call_table(file, USER)?;
    let frozen = (xlink_calls(file, USER)?.iter())
        .position(|name| *name == &**FROZEN)
        .with_context(|| format!("{} has no {}", &**USER, &**FROZEN))?;
    let (full, records) = full_body(file, &calls, frozen)?;
    let child_scale = (xlink_direct_values(file)?.iter())
        .position(|&v| f32::from_bits(v) == CHILD_SIZE)
        .with_context(|| format!("no direct value is {CHILD_SIZE}"))?;
    Ok(Found {
        frozen: calls[frozen].at + PARAM,
        full_body: calls[full].param,
        records,
        fields: xlink_asset_params(file)?,
        child_scale: child_scale as u32,
    })
}

/// The frozen grid's full-body cell and its shells' param positions. Both
/// cells play the hat pieces. The full body plays one switch of
/// [`SHELLS`] beside them, and the pieces the head, the upper body, the
/// hands and a lower body switch, so it's the cell whose own assets the
/// other cell all plays too.
fn full_body(file: &[u8], calls: &[XlinkCall], frozen: usize) -> Result<(usize, Vec<u32>)> {
    let under = |i: usize| xlink_call_children(file, USER, i);
    let cells = under(frozen)?;
    ensure!(
        cells.len() == 2,
        "the frozen call has {} cells, not 2: is the file already written?",
        cells.len()
    );
    let (mut assets, mut switches) = ([vec![], vec![]], [vec![], vec![]]);
    for (k, cell) in cells.clone().enumerate() {
        for i in under(cell)? {
            match calls[i].container {
                true => switches[k].push(i),
                false => assets[k].push(calls[i].param),
            }
        }
    }
    let full = (0..2)
        .filter(|&k| assets[k].iter().all(|p| assets[1 - k].contains(p)))
        .collect::<Vec<_>>();
    let [full] = full[..] else {
        bail!(
            "{} cells of the frozen call play only shared assets",
            full.len()
        );
    };
    let [switch] = switches[full][..] else {
        bail!("the full body has {} switches, not 1", switches[full].len());
    };
    let shells = under(switch)?;
    ensure!(
        shells.len() == SHELLS && shells.clone().all(|i| !calls[i].container),
        "the full body's switch plays {} calls, not {SHELLS} shells",
        shells.len()
    );
    Ok((cells.start + full, shells.map(|i| calls[i].param).collect()))
}

/// Point `file`'s frozen effect at the full body, and read its shells'
/// adult and child sizes.
fn fitted(file: &mut [u8]) -> Result<LookSpec> {
    let found = found(file)?;
    let at = found.frozen;
    file[at..at + 4].copy_from_slice(&found.full_body.to_le_bytes());
    let adults = (found.records.iter())
        .map(|&r| adult(file, found.fields + r as usize))
        .collect::<Result<Vec<_>>>()?;
    let [first, rest @ ..] = &adults[..] else {
        unreachable!("{SHELLS} shells")
    };
    ensure!(rest.iter().all(|a| a == first), "the shells' sizes differ");
    let (mask, bone, value_at) = *first;
    let sizes = Sizes {
        masks: [mask, mask & !(1 << BONE) | 1 << SCALE],
        values: [bone, found.child_scale],
        value_at,
        count: found.records.len() as u32,
    };
    Ok(LookSpec {
        sizes,
        records: found.records,
    })
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
    fn the_1_1_1_file_is_found_where_it_always_was() {
        let Some(romfs) = dev_dump() else { return };
        let found = found(&romfs.read(PATH).unwrap()).unwrap();
        // The adult's calls start at 0xee96c, 0x30 bytes each; the frozen
        // call is the 132nd and its shells 0x38d to 0x39c.
        assert_eq!(found.frozen, 0xe_e96c + 132 * 0x30 + PARAM);
        assert_eq!(found.full_body, 0x1c10);
        assert_eq!(found.fields, 0x4640);
        assert_eq!(found.child_scale, 0x6b);
        let calls = xlink_call_table(&romfs.read(PATH).unwrap(), USER).unwrap();
        let shells: Vec<_> = (0x38d..=0x39c).map(|i| calls[i].param).collect();
        assert_eq!(found.records, shells);
    }

    #[test]
    fn only_the_frozen_call_changes_and_the_child_gets_a_scale() {
        let Some(romfs) = dev_dump() else { return };
        let game = romfs.read(PATH).unwrap();
        let mut file = game.clone();
        let spec = fitted(&mut file).unwrap();
        let changed: Vec<_> = (0..game.len()).filter(|&i| game[i] != file[i]).collect();
        let found = found(&game).unwrap();
        assert!(
            changed
                .iter()
                .all(|i| (found.frozen..found.frozen + 4).contains(i))
        );
        assert_eq!(spec.records.len(), SHELLS);
        let [adult, child] = spec.sizes.masks;
        assert_eq!(child.count_ones(), adult.count_ones());
        assert_eq!(child & 1 << SCALE, 1 << SCALE);
        let at = found.fields + spec.records[0] as usize + spec.sizes.value_at as usize;
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
