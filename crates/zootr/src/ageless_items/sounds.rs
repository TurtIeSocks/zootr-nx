//! The borrowed items' sounds. Link's sound user plays them by name: each
//! swing's yell from its state machine, the Hammer's ground shock wave
//! and the Boomerang's throw and catch from code. Each Link's user lacks
//! the other's, so the child's Hammer and two-handed swings and the
//! adult's Boomerang were silent.
//!
//! Each user gains them as copies of calls a Link has: the child's yells
//! from his own (his heavy sword swing for the Hammer's, the same move's
//! for each [`YELLS`] one), the slam's tree (the clang and the splashes in
//! water, lava and bog) from the adult's, the adult's Boomerang throw and
//! catch from the child's, and its throw yells from his own Deku Nut throw
//! (the child's are in the child's voice). Each item's audio is only in
//! its own Link's sound bank, so each Link loads the other's beside his
//! own.

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, BymlDoc, ModBuilder, xlink_calls, xlink_copy_call};

use super::files::{BANKS, SOUNDS};
use crate::known::{Name, actor, field};

/// The calls each Link gains: his user, each call's name, and the user and
/// call it copies.
const GAINED: [(&Name, &Name, &Name, &Name); 10] = [
    (
        actor::PLAYER_CHILD,
        field::HAMMER_01,
        actor::PLAYER_CHILD,
        field::SWORD_ATTACK_B_01,
    ),
    (
        actor::PLAYER_CHILD,
        field::HAMMER_02,
        actor::PLAYER_CHILD,
        field::SWORD_ATTACK_B_02,
    ),
    (
        actor::PLAYER_CHILD,
        field::HAMMER_03,
        actor::PLAYER_CHILD,
        field::SWORD_ATTACK_B_03,
    ),
    (
        actor::PLAYER_CHILD,
        field::SHOCK_WAVE_GROUND,
        actor::PLAYER_ADULT,
        field::SHOCK_WAVE_GROUND,
    ),
    (
        actor::PLAYER_ADULT,
        field::BOOMERANG_THROW,
        actor::PLAYER_CHILD,
        field::BOOMERANG_THROW,
    ),
    (
        actor::PLAYER_ADULT,
        field::BOOMERANG_CATCH,
        actor::PLAYER_CHILD,
        field::BOOMERANG_CATCH,
    ),
    (
        actor::PLAYER_ADULT,
        field::BOOMERANG_CATCH_NO_HAND,
        actor::PLAYER_CHILD,
        field::BOOMERANG_CATCH_NO_HAND,
    ),
    (
        actor::PLAYER_ADULT,
        field::BOOMERANG_01,
        actor::PLAYER_ADULT,
        field::THROWING_NUTS_01,
    ),
    (
        actor::PLAYER_ADULT,
        field::BOOMERANG_02,
        actor::PLAYER_ADULT,
        field::THROWING_NUTS_02,
    ),
    (
        actor::PLAYER_ADULT,
        field::BOOMERANG_03,
        actor::PLAYER_ADULT,
        field::THROWING_NUTS_03,
    ),
];

/// What the adult's yells add to the child's own name for the same move:
/// a two-handed sword's swings, its jump attack, and the Hammer's jump
/// attack. Each the child lacks gains a copy of his own.
const YELLS: [&str; 3] = ["Big", "BS_", "H_"];

/// The banks and sound choices each Link loads beside his own: the
/// other's bank, and the choice of the slam's clang recording for the
/// ground under it.
const LOADED: [(&Name, &Name, &Name); 3] = [
    (actor::PLAYER_CHILD, field::BARS, field::PLAYER_ADULT_BARS),
    (
        actor::PLAYER_CHILD,
        field::CHOICE_ASSET,
        field::HAMMER_GROUND_HIT_CHOICE,
    ),
    (actor::PLAYER_ADULT, field::BARS, field::PLAYER_CHILD_BARS),
];

pub(super) fn write(builder: &mut ModBuilder) -> Result<()> {
    let sounds = called(&builder.read(SOUNDS)?)?;
    builder.write(SOUNDS, &sounds)?;
    let banks = banked(&builder.read(BANKS)?)?;
    builder.write(BANKS, &banks)
}

/// The sound users at `bytes` with each Link's [`GAINED`] calls and the
/// child's [`yells`].
fn called(bytes: &[u8]) -> Result<Vec<u8>> {
    let child = actor::PLAYER_CHILD.get();
    let file = GAINED
        .iter()
        .try_fold(bytes.to_vec(), |file, (link, name, from, call)| {
            xlink_copy_call(&file, from, call, link, name)
        })?;
    yells(bytes)?.iter().try_fold(file, |file, (name, own)| {
        xlink_copy_call(&file, child, own, child, name)
    })
}

/// Each [`YELLS`] call the adult has and the child lacks, and the child's
/// own it copies.
fn yells(bytes: &[u8]) -> Result<Vec<(String, String)>> {
    let child = xlink_calls(bytes, actor::PLAYER_CHILD.get())?;
    let found: Vec<_> = (xlink_calls(bytes, actor::PLAYER_ADULT.get())?.into_iter())
        .filter(|name| !child.contains(name))
        .filter_map(|name| {
            let own = YELLS.iter().find_map(|y| name.strip_prefix(y))?;
            child
                .contains(&own)
                .then(|| (name.to_owned(), own.to_owned()))
        })
        .collect();
    ensure!(!found.is_empty(), "the adult has no yell the child lacks");
    Ok(found)
}

/// The bank list at `bytes` with each Link loading his [`LOADED`] too.
fn banked(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(bytes)?;
    let table = (doc.root.as_mut_map()?.get_mut(field::BARSLIST_TABLE.get()))
        .with_context(|| format!("no {}", field::BARSLIST_TABLE))?
        .as_mut_map()?;
    for (link, key, file) in LOADED {
        let banks = (table.get_mut(link.get()))
            .with_context(|| format!("no {link} banks"))?
            .as_mut_map()?;
        let Some(Byml::Array(list)) = banks.get_mut(key.get()) else {
            anyhow::bail!("{link} has no {key}");
        };
        let file = Byml::String(file.get().into());
        ensure!(!list.contains(&file), "{link} already loads it");
        list.push(file);
    }
    Ok(doc.to_binary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use ounce_formats::xlink_tree;

    #[test]
    fn each_link_gains_the_others_calls_and_keeps_his_own() {
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
        let yells = yells(&shipped).unwrap();
        gained.extend(yells.iter().map(|(name, _)| name.as_str()));
        assert_eq!(now[was.len()..], gained[..]);
        assert_eq!(
            xlink_tree(&ours, child, field::SHOCK_WAVE_GROUND).unwrap(),
            slam
        );
        // The adult's user lies after the child's, so its offsets moved.
        let (was, now) = (
            xlink_calls(&shipped, adult).unwrap(),
            xlink_calls(&ours, adult).unwrap(),
        );
        assert_eq!(now[..was.len()], was[..], "his own calls stay too");
        let mut gained = vec![];
        for call in [
            field::BOOMERANG_THROW,
            field::BOOMERANG_CATCH,
            field::BOOMERANG_CATCH_NO_HAND,
        ] {
            let tree = xlink_tree(&shipped, child, call).unwrap();
            assert_eq!(xlink_tree(&ours, adult, call).unwrap(), tree);
            gained.extend(tree);
        }
        gained.extend(
            [
                field::BOOMERANG_01,
                field::BOOMERANG_02,
                field::BOOMERANG_03,
            ]
            .map(|n| n.get()),
        );
        assert_eq!(now[was.len()..], gained[..]);
        assert_eq!(
            xlink_tree(&ours, adult, field::SHOCK_WAVE_GROUND).unwrap(),
            slam
        );
    }

    #[test]
    fn each_link_loads_the_others_bank_and_the_child_the_clangs_choice() {
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
        for (link, key, file) in LOADED {
            let mut want = banks(&shipped, link, key);
            want.push(file.get().into());
            assert_eq!(banks(&ours, link, key), want);
            let other = match link == actor::PLAYER_CHILD {
                true => actor::PLAYER_ADULT,
                false => actor::PLAYER_CHILD,
            };
            assert!(
                banks(&shipped, other, key).contains(&file.get().to_string()),
                "the other Link loads {file}"
            );
        }
    }

    #[test]
    fn each_two_handed_and_hammer_jump_yell_is_his_own_for_the_same_move() {
        let Some(romfs) = dev_dump() else { return };
        let shipped = romfs.read(SOUNDS).unwrap();
        let yells = yells(&shipped).unwrap();
        // Three strengths of five swings, and two jump attacks of three.
        assert_eq!(yells.len(), 21, "{yells:?}");
        for prefix in YELLS {
            assert!(yells.iter().any(|(name, _)| name.starts_with(prefix)));
        }
        for (name, own) in &yells {
            assert!(name.ends_with(own.as_str()), "{name} from {own}");
        }
        let ours = called(&shipped).unwrap();
        let child = xlink_calls(&ours, actor::PLAYER_CHILD.get()).unwrap();
        for (name, _) in &yells {
            assert!(child.contains(&name.as_str()), "{name}");
        }
        assert!(super::yells(&ours).is_err(), "nothing left to gain");
    }
}
