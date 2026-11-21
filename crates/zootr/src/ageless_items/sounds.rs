//! The child's Hammer sounds. The swing's state machine plays the yells by
//! name and the slam's code plays its ground shock wave, through Link's
//! sound user. The child's has neither, so his swing was silent.
//!
//! His user gains each as a copy of a call a Link has: the yells from his
//! own heavy sword swing, the slam's tree (the clang and the splashes in
//! water, lava and bog) from the adult's. The clang's audio is only in the
//! adult's sound bank, so the child loads that bank beside his own.

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, BymlDoc, ModBuilder, xlink_copy_call};

use super::files::{BANKS, SOUNDS};
use crate::known::{Name, actor, field};

/// The calls the child gains: each name, and the user and call it copies.
const GAINED: [(&Name, &Name, &Name); 4] = [
    (
        field::HAMMER_01,
        actor::PLAYER_CHILD,
        field::SWORD_ATTACK_B_01,
    ),
    (
        field::HAMMER_02,
        actor::PLAYER_CHILD,
        field::SWORD_ATTACK_B_02,
    ),
    (
        field::HAMMER_03,
        actor::PLAYER_CHILD,
        field::SWORD_ATTACK_B_03,
    ),
    (
        field::SHOCK_WAVE_GROUND,
        actor::PLAYER_ADULT,
        field::SHOCK_WAVE_GROUND,
    ),
];

/// The bank and the sound choice the slam's clang needs: which of the
/// clang's recordings plays for the ground under it.
const LOADED: [(&Name, &Name); 2] = [
    (field::BARS, field::PLAYER_ADULT_BARS),
    (field::CHOICE_ASSET, field::HAMMER_GROUND_HIT_CHOICE),
];

pub(super) fn write(builder: &mut ModBuilder) -> Result<()> {
    let sounds = called(&builder.read(SOUNDS)?)?;
    builder.write(SOUNDS, &sounds)?;
    let banks = banked(&builder.read(BANKS)?)?;
    builder.write(BANKS, &banks)
}

/// The sound users at `bytes` with the child's [`GAINED`] calls.
fn called(bytes: &[u8]) -> Result<Vec<u8>> {
    GAINED
        .iter()
        .try_fold(bytes.to_vec(), |file, (name, from, call)| {
            xlink_copy_call(&file, from, call, actor::PLAYER_CHILD, name)
        })
}

/// The bank list at `bytes` with the child loading [`LOADED`] too.
fn banked(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(bytes)?;
    let table = (doc.root.as_mut_map()?.get_mut(field::BARSLIST_TABLE.get()))
        .with_context(|| format!("no {}", field::BARSLIST_TABLE))?;
    let child = (table.as_mut_map()?.get_mut(actor::PLAYER_CHILD.get()))
        .with_context(|| format!("no {} banks", actor::PLAYER_CHILD))?
        .as_mut_map()?;
    for (key, file) in LOADED {
        let Some(Byml::Array(list)) = child.get_mut(key.get()) else {
            anyhow::bail!("{} has no {key}", actor::PLAYER_CHILD);
        };
        let file = Byml::String(file.get().into());
        ensure!(
            !list.contains(&file),
            "{} already loads it",
            actor::PLAYER_CHILD
        );
        list.push(file);
    }
    Ok(doc.to_binary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use ounce_formats::{xlink_calls, xlink_tree};

    #[test]
    fn the_child_gains_the_hammers_calls_and_every_user_keeps_its_own() {
        let Some(romfs) = dev_dump() else { return };
        let shipped = romfs.read(SOUNDS).unwrap();
        let ours = called(&shipped).unwrap();
        let (child, adult) = (actor::PLAYER_CHILD.get(), actor::PLAYER_ADULT.get());
        let (was, now) = (
            xlink_calls(&shipped, child).unwrap(),
            xlink_calls(&ours, child).unwrap(),
        );
        assert_eq!(
            now[..was.len()],
            was[..],
            "his own calls stay where they were"
        );
        let slam = xlink_tree(&shipped, adult, field::SHOCK_WAVE_GROUND).unwrap();
        assert!(slam.len() > 10, "the clang and the splashes: {slam:?}");
        let mut gained: Vec<&str> = [field::HAMMER_01, field::HAMMER_02, field::HAMMER_03]
            .map(|n| n.get())
            .to_vec();
        gained.extend(&slam);
        assert_eq!(now[was.len()..], gained[..]);
        assert_eq!(
            xlink_tree(&ours, child, field::SHOCK_WAVE_GROUND).unwrap(),
            slam
        );
        // The adult's user lies after the child's, so its offsets moved.
        assert_eq!(
            xlink_calls(&ours, adult).unwrap(),
            xlink_calls(&shipped, adult).unwrap()
        );
        assert_eq!(
            xlink_tree(&ours, adult, field::SHOCK_WAVE_GROUND).unwrap(),
            slam
        );
    }

    #[test]
    fn the_child_loads_the_adults_bank_and_the_clangs_choice() {
        let Some(romfs) = dev_dump() else { return };
        let shipped = romfs.read(BANKS).unwrap();
        let ours = BymlDoc::parse(&banked(&shipped).unwrap()).unwrap();
        let shipped = BymlDoc::parse(&shipped).unwrap();
        let banks = |doc: &BymlDoc, link: &Name, key: &Name| -> Vec<String> {
            let table = doc.root.as_map().unwrap()[field::BARSLIST_TABLE.get()]
                .as_map()
                .unwrap();
            let Byml::Array(list) = &table[link.get()].as_map().unwrap()[key.get()] else {
                panic!("no {key}");
            };
            list.iter()
                .map(|b| b.as_string().unwrap().to_string())
                .collect()
        };
        for (key, file) in LOADED {
            let mut want = banks(&shipped, actor::PLAYER_CHILD, key);
            want.push(file.get().into());
            assert_eq!(banks(&ours, actor::PLAYER_CHILD, key), want);
            let adult = banks(&shipped, actor::PLAYER_ADULT, key);
            assert!(
                adult.contains(&file.get().to_string()),
                "the adult loads it"
            );
            assert_eq!(banks(&ours, actor::PLAYER_ADULT, key), adult);
        }
    }
}
