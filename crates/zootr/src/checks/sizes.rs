//! Chest sizes: each chest family's small, big and boss key actors.

use anyhow::Result;
use ounce_formats::Byml;

use super::{DROP_ACTOR, DYNAMIC, GYAML, actors_mut, str_field};
use crate::known::{Name, actor};

/// Each chest family by size ([`Size`] order), the `_NoBake` one apart.
/// The line ammo, Lens of Truth and iron chests have no other size.
const FAMILIES: [[&Name; 3]; 2] = [
    [
        actor::OBJ_T_BOX_A_01,
        actor::OBJ_T_BOX_A_02,
        actor::OBJ_T_BOX_A_03,
    ],
    [
        actor::OBJ_T_BOX_A_01_NO_BAKE,
        actor::OBJ_T_BOX_A_02_NO_BAKE,
        actor::OBJ_T_BOX_A_03_NO_BAKE,
    ],
];
/// A chest's size, as its actor shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Small,
    Big,
    /// The boss key chest: the big chest's body with a model of its own.
    Boss,
}

const SIZES: [Size; 3] = [Size::Small, Size::Big, Size::Boss];

/// Chest `gyaml`'s family and size, when it comes in more than one size.
fn family_of(gyaml: &str) -> Option<(usize, Size)> {
    let gyaml = &*crate::chest_looks::host(gyaml);
    FAMILIES.iter().enumerate().find_map(|(f, family)| {
        let at = family.iter().position(|g| *g == gyaml)?;
        Some((f, SIZES[at]))
    })
}

/// Chest `gyaml`'s size, when it comes in more than one size.
pub fn size_of(gyaml: &str) -> Option<Size> {
    family_of(gyaml).map(|(_, size)| size)
}

pub fn is_big(holder: &str) -> bool {
    size_of(holder) == Some(Size::Big)
}

/// Swap each chest that comes in more than one size for the size `to`
/// gives from its own size and its drop, keeping what it holds. Returns how
/// many changed.
pub fn resize_chests(root: &mut Byml, to: impl Fn(Size, &str) -> Size) -> Result<usize> {
    let mut resized = 0;
    for actor in actors_mut(root)? {
        let Some((family, size)) = str_field(actor, GYAML).and_then(family_of) else {
            continue;
        };
        let dynamic = actor.as_map()?.get(DYNAMIC);
        let Some(drop) = dynamic.and_then(|d| str_field(d, DROP_ACTOR)) else {
            continue;
        };
        let new = to(size, drop);
        if new != size {
            let gyaml = FAMILIES[family][new as usize];
            actor.as_mut_map()?.insert(GYAML.into(), gyaml.get().into());
            resized += 1;
        }
    }
    Ok(resized)
}

/// All small's resize: every big chest small, the boss key chests kept.
pub fn shrunk(size: Size, _drop: &str) -> Size {
    match size {
        Size::Big => Size::Small,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::tests::gyamls;
    use crate::known::item;

    /// Actors of each `gyaml`, every one dropping `drop`.
    fn placed(gyamls: &[&str], drop: &str) -> Byml {
        let actor = |g: &str| {
            let dynamic = Byml::from_iter([(DROP_ACTOR.get(), Byml::String(drop.into()))]);
            Byml::from_iter([(GYAML, g.into()), (DYNAMIC, dynamic)])
        };
        Byml::from_iter([("Actors", gyamls.iter().map(|g| actor(g)).collect())])
    }

    #[test]
    fn all_small_shrinks_big_chests_and_nothing_else() {
        needs_names!();
        let mut root = placed(
            &[
                &**actor::OBJ_T_BOX_A_02,
                &**actor::OBJ_T_BOX_A_02_NO_BAKE,
                &**actor::OBJ_T_BOX_A_03,
                // Not a word of the dump's name tables.
                "Thing_A",
            ],
            actor::OBJ_HEART_PIECE.get(),
        );
        assert_eq!(resize_chests(&mut root, shrunk).unwrap(), 2);
        assert_eq!(
            gyamls(&root),
            [
                actor::OBJ_T_BOX_A_01,
                actor::OBJ_T_BOX_A_01_NO_BAKE,
                actor::OBJ_T_BOX_A_03,
                "Thing_A"
            ]
        );
    }

    #[test]
    fn a_chest_takes_any_size_of_its_family_and_keeps_its_bake() {
        needs_names!();
        let chests = [
            actor::OBJ_T_BOX_A_01,
            actor::OBJ_T_BOX_A_02_NO_BAKE,
            actor::OBJ_T_BOX_A_03,
            actor::OBJ_T_BOX_A_06,
            actor::OBJ_T_BOX_A_01_MEMO11,
        ];
        let sized = |size: Size| {
            let chests = chests.map(|g| g.get());
            let mut root = placed(&chests, actor::OBJ_HEART_PIECE.get());
            resize_chests(&mut root, |_, _| size).unwrap();
            gyamls(&root)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let kept = [actor::OBJ_T_BOX_A_06, actor::OBJ_T_BOX_A_01_MEMO11].map(|g| g.to_string());
        let want = |s: &Name, n: &Name, b: &Name| {
            [
                s.to_string(),
                n.to_string(),
                b.to_string(),
                kept[0].clone(),
                kept[1].clone(),
            ]
        };
        use actor::*;
        assert_eq!(
            sized(Size::Small),
            want(OBJ_T_BOX_A_01, OBJ_T_BOX_A_01_NO_BAKE, OBJ_T_BOX_A_01)
        );
        assert_eq!(
            sized(Size::Big),
            want(OBJ_T_BOX_A_02, OBJ_T_BOX_A_02_NO_BAKE, OBJ_T_BOX_A_02)
        );
        assert_eq!(
            sized(Size::Boss),
            want(OBJ_T_BOX_A_03, OBJ_T_BOX_A_03_NO_BAKE, OBJ_T_BOX_A_03)
        );
    }

    #[test]
    fn a_chest_is_sized_by_its_drop_and_one_without_a_drop_stays() {
        needs_names!();
        let mut root = placed(&[actor::OBJ_T_BOX_A_01.get()], item::HOOKSHOT.get());
        let gyaml = Byml::from_iter([(GYAML, actor::OBJ_T_BOX_A_01.to_string().into())]);
        root.as_mut_map()
            .unwrap()
            .get_mut("Actors")
            .unwrap()
            .as_mut_array()
            .unwrap()
            .push(gyaml);
        let big_for_hookshot = |_, drop: &str| match drop == item::HOOKSHOT.get() {
            true => Size::Big,
            false => Size::Small,
        };
        assert_eq!(resize_chests(&mut root, big_for_hookshot).unwrap(), 1);
        assert_eq!(
            gyamls(&root),
            [actor::OBJ_T_BOX_A_02, actor::OBJ_T_BOX_A_01]
        );
    }
}
