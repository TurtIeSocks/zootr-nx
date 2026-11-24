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
//!
//! Drawing and putting away an item plays a case of the user's `Equip` and
//! `UnEquip` switches, picked by the item. Each Link's switches gain the
//! other's cases for his borrowed items ([`DRAWN`]). The adult's Bow case
//! picks again by the arrows held, which the child doesn't keep, so the
//! child plays the adult's with arrows.

use anyhow::{Context, Result, ensure};
use ounce_formats::{
    Byml, BymlDoc, ModBuilder, xlink_add_case, xlink_calls, xlink_copy_call, xlink_copy_property,
    xlink_nest,
};

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

/// The draw and put-away cases each Link gains from the other: his user,
/// then each case and the other's call it plays.
const DRAWN: [(&Name, &[(&Name, &Name)]); 2] = [
    (
        actor::PLAYER_CHILD,
        &[
            (field::EQUIP_GIANTS_KNIFE, field::EQUIP_GIANTS_KNIFE),
            (field::UN_EQUIP_GIANTS_KNIFE, field::UN_EQUIP_GIANTS_KNIFE),
            (
                field::EQUIP_BROKEN_GIANTS_KNIFE,
                field::EQUIP_BROKEN_GIANTS_KNIFE,
            ),
            (
                field::UN_EQUIP_BROKEN_GIANTS_KNIFE,
                field::UN_EQUIP_BROKEN_GIANTS_KNIFE,
            ),
            (field::EQUIP_BIGGORON_SWORD, field::EQUIP_BIGGORON_SWORD),
            (
                field::UN_EQUIP_BIGGORON_SWORD,
                field::UN_EQUIP_BIGGORON_SWORD,
            ),
            (field::EQUIP_BOW, field::EQUIP_BOW_ARROWS),
            (field::UN_EQUIP_BOW, field::UN_EQUIP_BOW_ARROWS),
            (field::EQUIP_HOOKSHOT, field::EQUIP_HOOKSHOT),
            (field::UN_EQUIP_HOOKSHOT, field::UN_EQUIP_HOOKSHOT),
            (field::EQUIP_MEGATON_HAMMER, field::EQUIP_MEGATON_HAMMER),
            (
                field::UN_EQUIP_MEGATON_HAMMER,
                field::UN_EQUIP_MEGATON_HAMMER,
            ),
        ],
    ),
    (
        actor::PLAYER_ADULT,
        &[
            (field::EQUIP_BOOMERANG, field::EQUIP_BOOMERANG),
            (field::UN_EQUIP_BOOMERANG, field::UN_EQUIP_BOOMERANG),
            (field::EQUIP_DEKU_STICK, field::EQUIP_DEKU_STICK),
            (field::UN_EQUIP_DEKU_STICK, field::UN_EQUIP_DEKU_STICK),
            (field::EQUIP_SLINGSHOT, field::EQUIP_SLINGSHOT),
            (field::UN_EQUIP_SLINGSHOT, field::UN_EQUIP_SLINGSHOT),
        ],
    ),
];

/// What the adult's yells add to the child's own name for the same move:
/// a two-handed sword's swings, its jump attack, and the Hammer's jump
/// attack. Each the child lacks gains a copy of his own.
const YELLS: [&str; 3] = ["Big", "BS_", "H_"];

/// A bank or sound choice a Link loads beside his own: his user, the
/// list and the file.
type Loaded = (&'static Name, &'static Name, &'static Name);

/// The adult's bank, which the child loads for both settings.
const ADULT_BANK: Loaded = (actor::PLAYER_CHILD, field::BARS, field::PLAYER_ADULT_BARS);

/// The banks and sound choices each Link loads beside his own: the
/// other's bank, and the choice of the slam's clang recording for the
/// ground under it.
const LOADED: [Loaded; 3] = [
    ADULT_BANK,
    (
        actor::PLAYER_CHILD,
        field::CHOICE_ASSET,
        field::HAMMER_GROUND_HIT_CHOICE,
    ),
    (actor::PLAYER_ADULT, field::BARS, field::PLAYER_CHILD_BARS),
];

/// The boots' calls the child gains from the adult, whole: changing
/// boots, landing in them, and the Hover Boots' float.
const BOOTED: [&Name; 3] = [
    field::BOOTS_CHANGE,
    field::FOOT_STEP_BOOTS_CHANGE_LAND,
    field::HOVER_WALK_ON_AIR,
];

pub(super) fn write(builder: &mut ModBuilder) -> Result<()> {
    let sounds = called(&builder.read(SOUNDS)?)?;
    builder.write(SOUNDS, &sounds)?;
    let banks = banked(&builder.read(BANKS)?, &LOADED)?;
    builder.write(BANKS, &banks)
}

/// Write the child's boots sounds ([`booted`]), with the adult's bank,
/// where they are, loaded beside his.
pub fn write_boots(builder: &mut ModBuilder) -> Result<()> {
    let sounds = booted(&builder.read(SOUNDS)?)?;
    builder.write(SOUNDS, &sounds)?;
    let banks = banked(&builder.read(BANKS)?, &[ADULT_BANK])?;
    builder.write(BANKS, &banks)
}

/// The sound users at `bytes` with the child's steps picked by his boots
/// as the adult's are, his own steps the plain boots' case, the adult's
/// [`BOOTED`] calls, and the float played as the adult's is: as the
/// Hover Boots' float time changes.
fn booted(bytes: &[u8]) -> Result<Vec<u8>> {
    let (child, adult) = (actor::PLAYER_CHILD.get(), actor::PLAYER_ADULT.get());
    let (steps, plain) = (field::FOOT_STEP.get(), field::FOOT_STEP_NORMAL_BOOTS.get());
    let stepped = xlink_nest(bytes, adult, steps, plain, child)?;
    let booted = BOOTED.iter().try_fold(stepped, |file, call| {
        xlink_copy_call(&file, adult, call, child, call)
    })?;
    xlink_copy_property(&booted, adult, field::HOVER_WALK_ON_AIR.get(), child)
}

/// The sound users at `bytes` with each Link's [`GAINED`] calls and the
/// child's [`yells`], then each Link's [`DRAWN`] cases.
fn called(bytes: &[u8]) -> Result<Vec<u8>> {
    drawn(&gained(bytes)?)
}

/// The sound users at `bytes` with each Link's [`GAINED`] calls and the
/// child's [`yells`].
fn gained(bytes: &[u8]) -> Result<Vec<u8>> {
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

/// The sound users at `bytes` with each Link's [`DRAWN`] cases.
fn drawn(bytes: &[u8]) -> Result<Vec<u8>> {
    DRAWN
        .iter()
        .try_fold(bytes.to_vec(), |file, (link, cases)| {
            cases.iter().try_fold(file, |file, (case, body)| {
                xlink_add_case(&file, other(link).get(), case, body, link)
            })
        })
}

/// The other Link's user.
fn other(link: &Name) -> &'static Name {
    match link == actor::PLAYER_CHILD {
        true => actor::PLAYER_ADULT,
        false => actor::PLAYER_CHILD,
    }
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

/// The bank list at `bytes` with each Link loading his `loaded` too, once
/// though both settings ask.
fn banked(bytes: &[u8], loaded: &[Loaded]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(bytes)?;
    let table = (doc.root.as_mut_map()?.get_mut(field::BARSLIST_TABLE.get()))
        .with_context(|| format!("no {}", field::BARSLIST_TABLE))?
        .as_mut_map()?;
    for &(link, key, file) in loaded {
        let banks = (table.get_mut(link.get()))
            .with_context(|| format!("no {link} banks"))?
            .as_mut_map()?;
        let Some(Byml::Array(list)) = banks.get_mut(key.get()) else {
            anyhow::bail!("{link} has no {key}");
        };
        let file = Byml::String(file.get().into());
        if !list.contains(&file) {
            list.push(file);
        }
    }
    Ok(doc.to_binary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use ounce_formats::{xlink_children, xlink_properties, xlink_tree, xlink_triggers};

    #[test]
    fn the_childs_steps_pick_by_his_boots_and_plain_ones_stay_his() {
        let Some(romfs) = dev_dump() else { return };
        let shipped = romfs.read(SOUNDS).unwrap();
        let ours = booted(&shipped).unwrap();
        let (child, adult) = (actor::PLAYER_CHILD.get(), actor::PLAYER_ADULT.get());
        fn under<'a>(f: &'a [u8], user: &str, key: &Name) -> Vec<&'a str> {
            xlink_children(f, user, key.get()).unwrap()
        }
        fn tree<'a>(f: &'a [u8], user: &str, key: &Name) -> Vec<&'a str> {
            xlink_tree(f, user, key.get()).unwrap()
        }
        assert_eq!(
            under(&ours, child, field::FOOT_STEP),
            [field::FOOT_STEP_SWITCH.get()]
        );
        assert_eq!(
            under(&ours, child, field::FOOT_STEP_SWITCH),
            under(&shipped, adult, field::FOOT_STEP_SWITCH)
        );
        let normal = field::FOOT_STEP_NORMAL_BOOTS;
        assert_eq!(
            tree(&ours, child, normal)[1..],
            tree(&shipped, child, field::FOOT_STEP)[1..],
            "plain boots play his own steps"
        );
        assert!(xlink_tree(&shipped, child, field::FOOT_STEP_HOVER_BOOTS.get()).is_err());
        let hover = field::FOOT_STEP_HOVER_BOOTS;
        assert_eq!(tree(&ours, child, hover), tree(&shipped, adult, hover));
        // As names, sorted: the adult's tree without his plain steps,
        // with the child's instead.
        let mut want = tree(&shipped, adult, field::FOOT_STEP);
        for name in &tree(&shipped, adult, normal)[1..] {
            want.remove(want.iter().position(|n| n == name).unwrap());
        }
        want.extend(&tree(&shipped, child, field::FOOT_STEP)[1..]);
        let mut now = tree(&ours, child, field::FOOT_STEP);
        now.sort_unstable();
        want.sort_unstable();
        assert_eq!(now, want, "the adult's tree, his steps for the plain boots");
        for call in BOOTED {
            assert_eq!(tree(&ours, child, call), tree(&shipped, adult, call));
        }
        let (was, now) = (
            xlink_calls(&shipped, child).unwrap(),
            xlink_calls(&ours, child).unwrap(),
        );
        assert_eq!(now[..was.len()], was[..], "his calls stay where they were");
        let (was, now) = (
            xlink_triggers(&shipped, child).unwrap(),
            xlink_triggers(&ours, child).unwrap(),
        );
        let float = field::HOVER_WALK_ON_AIR.get();
        let kept: Vec<_> = now.into_iter().filter(|&c| c != float).collect();
        assert_eq!(kept, was, "each trigger plays what it did");
        fn props<'a>(f: &'a [u8], user: &str) -> Vec<(&'a str, Vec<&'a str>)> {
            xlink_properties(f, user).unwrap()
        }
        // His properties with the adult's float time among them, where
        // the adult has it, playing the float.
        let theirs = props(&shipped, adult);
        let at = theirs.iter().position(|(_, c)| *c == [float]).unwrap();
        let mut want = props(&shipped, child);
        want.insert(at, theirs[at].clone());
        assert_eq!(props(&ours, child), want);
        assert_eq!(
            xlink_calls(&ours, adult).unwrap(),
            xlink_calls(&shipped, adult).unwrap()
        );
    }

    #[test]
    fn each_link_gains_the_others_calls_and_keeps_his_own() {
        let Some(romfs) = dev_dump() else { return };
        let shipped = romfs.read(SOUNDS).unwrap();
        let ours = gained(&shipped).unwrap();
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
    fn each_link_draws_and_puts_away_the_others_items_with_the_others_sounds() {
        let Some(romfs) = dev_dump() else { return };
        let was = gained(&romfs.read(SOUNDS).unwrap()).unwrap();
        let now = drawn(&was).unwrap();
        for (link, cases) in DRAWN {
            let (link, from) = (link.get(), other(link).get());
            let mut want = xlink_calls(&was, link).unwrap();
            for switch in [field::EQUIP, field::UN_EQUIP] {
                let mut cased = xlink_children(&was, link, switch).unwrap();
                let theirs = xlink_children(&was, from, switch).unwrap();
                cased.extend(
                    cases
                        .iter()
                        .map(|(c, _)| c.get())
                        .filter(|c| theirs.contains(c)),
                );
                assert_eq!(
                    xlink_children(&now, link, switch).unwrap(),
                    cased,
                    "{switch}"
                );
            }
            for (case, body) in cases {
                let mut copied = xlink_tree(&was, from, body).unwrap();
                copied[0] = case.get();
                assert_eq!(xlink_tree(&now, link, case).unwrap(), copied);
                want.extend(copied);
            }
            let mut have = xlink_calls(&now, link).unwrap();
            have.sort();
            want.sort();
            assert_eq!(have, want, "{link} gains each tree and keeps his own");
            for user in [link, from] {
                assert_eq!(
                    xlink_triggers(&now, user).unwrap(),
                    xlink_triggers(&was, user).unwrap(),
                    "each of {user}'s triggers plays what it did"
                );
            }
        }
        assert!(drawn(&now).is_err(), "nothing left to gain");
    }

    #[test]
    fn each_link_loads_the_others_bank_and_the_child_the_clangs_choice() {
        let Some(romfs) = dev_dump() else { return };
        let shipped = romfs.read(BANKS).unwrap();
        let ours = banked(&shipped, &LOADED).unwrap();
        assert_eq!(banked(&ours, &[ADULT_BANK]).unwrap(), ours, "once");
        let ours = BymlDoc::parse(&ours).unwrap();
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
            assert!(
                banks(&shipped, other(link), key).contains(&file.get().to_string()),
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
