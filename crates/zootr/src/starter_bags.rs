//! Starter bags: with a bag setting on, Link holds no Deku Nuts, Deku
//! Sticks or Bombchus before he finds their bag, and each bag is an item
//! in the pool.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_file};

use crate::bombchu_bags;
use crate::checks::swap_strings;
use crate::key_tints::{POUCH_TEXT, TEXT_PARTS};
use crate::known::{Name, actor, field, item};
use crate::settings::{Bombchus, NewGame};
use crate::standins::{Tables, actor_path, component, doc, path_of, raw, renamed, with_ref};
use crate::text::{Copies, CopyLine, Says};

/// A starter bag's ammo: the pouch slot its rows cap it by, then each
/// pouch param that hands the bag over with the first one picked up and
/// the packs carrying a copy of it.
struct Ammo {
    slot: &'static str,
    param: &'static Name,
    packs: &'static [&'static Name],
    /// The Small bag, which has no get of its own, then the bags whose
    /// packs carry its param as their parent's: the Medium, whose get it
    /// takes, and the Large.
    bags: [&'static Name; 3],
}

const NUTS: Ammo = Ammo {
    slot: "DekuNut",
    param: item::DEKU_NUT,
    packs: &[item::DEKU_NUT, actor::OBJ_DEKU_NUT, actor::POUCH_DEKU_NUT],
    bags: [
        item::NUT_BAG_SMALL,
        item::NUT_BAG_MEDIUM,
        item::NUT_BAG_LARGE,
    ],
};
const STICKS: Ammo = Ammo {
    slot: "DekuStick",
    param: actor::OBJ_DEKU_STICK,
    packs: &[
        item::DEKU_STICK,
        actor::OBJ_DEKU_STICK,
        actor::POUCH_DEKU_STICK,
        actor::SHOP_DEKU_STICK,
        actor::WEAPON_DEKU_STICK,
    ],
    bags: [
        item::STICK_BAG_SMALL,
        item::STICK_BAG_MEDIUM,
        item::STICK_BAG_LARGE,
    ],
};

/// The components a get plays: its scene, the animation set it runs and
/// the model's own animation. The Medium bags have all three, the Small
/// ones none, and a get missing them can hold a chest open.
const GET_REFS: [&str; 3] = ["ASInfoRef", "ASRef", "AnimationRef"];
const ANIMATION: &str = GET_REFS[2];

/// The pouch param `name` in its packs.
fn pouch_param(name: &str) -> String {
    let param = field::POUCH_CONTENT_PARAM;
    format!("Component/{param}/{name}.game__component__{param}.bgyml")
}

/// Whether `map` hands over its bag with the first one picked up.
fn gives_bag(map: &BymlMap) -> bool {
    map.get(&**field::IS_ABLE_TO_GET_BAG_IF_GET) == Some(&Byml::Bool(true))
}

/// `map` keeps its bag: picking one up adds only what the bag holds.
fn keep_bag(map: &mut BymlMap) {
    map.insert(
        field::IS_ABLE_TO_GET_BAG_IF_GET.to_string().into(),
        Byml::Bool(false),
    );
}

/// `small`'s pack rewritten in place with `medium`'s get: its scene and
/// animation set as they are, and an animation param of its own naming
/// `small`'s model animation where `medium`'s names its own. `medium` and
/// `large` carry `small`'s param as their parent's, and the game keeps
/// whichever copy loads first: theirs get the same rewrite, or a Small
/// bag loading after them overflows the components it read from theirs.
fn take_get(
    builder: &mut ModBuilder,
    small: &ActorPack,
    [medium, large]: [&ActorPack; 2],
) -> Result<()> {
    let actor_at = actor_path(&small.name);
    let actor_raw = raw(small, &actor_at)?;
    let actor_doc = BymlDoc::parse(&actor_raw)?;
    let lender = medium.actor_param()?.context("no ActorParam")?;
    let mut actor = actor_doc.root.as_map()?.clone();
    let mut writer = small.writer();
    for key in GET_REFS {
        let r = component(&lender, key).with_context(|| format!("{} has no {key}", medium.name))?;
        actor = with_ref(&actor, key, r.trim_start_matches('?'))?;
    }
    let anim_like = path_of(medium, ANIMATION)?;
    let anim_at = renamed(&anim_like, &small.name);
    let anim_raw = raw(medium, &anim_like)?;
    let anim_doc = BymlDoc::parse(&anim_raw)?;
    let mut anim = anim_doc.root.as_map()?.clone();
    let own = format!("{}_Animation", medium.name);
    let swapped = swap_strings(&mut anim, &own, &format!("{}_Animation", small.name));
    ensure!(
        swapped == 1,
        "{} names its animation {swapped} times",
        medium.name
    );
    actor = with_ref(&actor, ANIMATION, &anim_at)?;
    let anim = doc(anim, &anim_doc).to_binary();
    builder.add_entry(&anim_at, &anim_like, anim_raw.len(), anim.len())?;
    writer.add_file(anim_at.clone(), anim.clone());
    for key in &GET_REFS[..1] {
        let at = path_of(medium, key)?;
        writer.add_file(at.clone(), raw(medium, &at)?);
    }
    let actor = doc(actor, &actor_doc).to_binary();
    // Each component costs the game more than its bytes: sized by its
    // bytes alone, the param's table didn't fit and the game crashed
    // loading a chest holding the bag. The Medium's has the same parts.
    let like = actor_path(&medium.name);
    let like_size = raw(medium, &like)?.len();
    builder.resize_like(&actor_at, actor_raw.len(), actor.len(), &like, like_size)?;
    writer.add_file(actor_at.clone(), actor.clone());
    builder.write(&pack_file(&small.name), &writer.to_binary())?;
    for heir in [medium, large] {
        ensure!(
            heir.raw(&actor_at).is_some(),
            "{} doesn't carry {actor_at}",
            heir.name
        );
        let mut writer = heir.writer();
        writer.add_file(actor_at.clone(), actor.clone());
        writer.add_file(anim_at.clone(), anim.clone());
        builder.write(&pack_file(&heir.name), &writer.to_binary())?;
    }
    Ok(())
}

impl Ammo {
    /// Give the Small bag the Medium one's get.
    fn take_get(&self, romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
        let [small, medium, large] = self.bags.map(|b| ActorPack::load(romfs, b));
        take_get(builder, &small?, [&medium?, &large?])
    }

    /// Make every row and param of this ammo keep its bag.
    fn keep_bag(&self, builder: &mut ModBuilder, tables: &mut Tables) -> Result<()> {
        let slot = |row: &BymlMap| {
            let cap = row.get(&**field::MAX_NUM_GAME_DATA);
            cap.and_then(|c| c.as_string().ok())
                .is_some_and(|c| c == self.slot)
        };
        let rows = tables.edit_pouch(|row| {
            let hit = slot(row) && gives_bag(row);
            if hit {
                keep_bag(row);
            }
            hit
        })?;
        ensure!(rows > 0, "no {} row hands over its bag", self.slot);
        let packs: Vec<&str> = self.packs.iter().map(|p| &***p).collect();
        builder.edit_param(&packs, &pouch_param(self.param), |param| {
            let mut doc = BymlDoc::parse(param)?;
            let map = doc.root.as_mut_map()?;
            ensure!(gives_bag(map), "{} hands over no bag", self.param);
            keep_bag(map);
            Ok(doc.to_binary())
        })
    }
}

/// With a bag setting on, its ammo stops handing the bag over, and the
/// Small bag gets a get of its own to be dealt like any item.
pub fn write(
    romfs: &Romfs,
    new_game: &NewGame,
    builder: &mut ModBuilder,
    tables: &mut Tables,
) -> Result<()> {
    for (on, ammo) in [
        (new_game.shuffle_nut_bag, NUTS),
        (new_game.shuffle_stick_bag, STICKS),
    ] {
        if on {
            ammo.keep_bag(builder, tables)?;
            ammo.take_get(romfs, builder)?;
        }
    }
    Ok(())
}

/// Each bag whose get text a seed writes, since the game has none (the
/// small nut and stick bags came only with a first nut or stick), with
/// the bag whose lines it's copied from in every language and its own
/// lines: name (none keeps the game's), get and caption.
type GetText = (&'static Name, &'static Name, [Option<&'static str>; 3]);
const GET_TEXT: [GetText; 6] = [
    (
        item::NUT_BAG_SMALL,
        item::NUT_BAG_MEDIUM,
        [
            None,
            Some("You got a Deku-nut bag!"),
            Some("Now you can carry Deku nuts! This bag fits up to 20 of them."),
        ],
    ),
    (
        item::STICK_BAG_SMALL,
        item::STICK_BAG_MEDIUM,
        [
            None,
            Some("You got a Deku-stick bag!"),
            Some("Now you can carry Deku sticks! This bag fits up to 10 of them."),
        ],
    ),
    (
        bombchu_bags::SMALL,
        item::BOMB_BAG_SMALL,
        [
            Some("Bombchu Bag"),
            Some("You found a bombchu bag!"),
            Some("Now you can carry bombchus! This bag fits up to 20 of them."),
        ],
    ),
    (
        bombchu_bags::MEDIUM,
        item::BOMB_BAG_MEDIUM,
        [
            Some("Big Bombchu Bag"),
            Some("You got a bigger bombchu bag!"),
            Some("You can carry more bombchus now! This bag fits up to 30 of them."),
        ],
    ),
    (
        bombchu_bags::LARGE,
        item::BOMB_BAG_LARGE,
        [
            Some("Biggest Bombchu Bag"),
            Some("You got the biggest bombchu bag!"),
            Some("You can carry a ton of bombchus now! This bag fits up to 40 of them."),
        ],
    ),
    (
        crate::magic::meter::METER,
        item::FAIRY_BLESS_MP_DOUBLE,
        [
            Some("Magic Meter"),
            Some("You got a magic meter!"),
            Some("Now you can use magic! Spells and magic items draw on this meter."),
        ],
    ),
];

/// The bags whose get text a seed writes.
#[cfg(test)]
pub const GET_TEXTS: [&Name; 6] = [
    GET_TEXT[0].0,
    GET_TEXT[1].0,
    GET_TEXT[2].0,
    GET_TEXT[3].0,
    GET_TEXT[4].0,
    GET_TEXT[5].0,
];

/// The get text of each bag `new_game` adds, which the game lacks, and the
/// Magic Meter's, which every seed has.
pub fn captions(new_game: &NewGame) -> Copies<'static> {
    let added = added(new_game);
    let meter = crate::magic::meter::NAME;
    let bags = GET_TEXT
        .iter()
        .filter(|(bag, ..)| added.contains(&bag.get()) || bag.get() == meter);
    let new = bags
        .flat_map(|(bag, like, lines)| {
            let parts = TEXT_PARTS.iter().zip(lines);
            parts.filter_map(move |(part, line)| {
                Some(CopyLine {
                    label: format!("{bag}{part}"),
                    sources: vec![format!("{like}{part}")],
                    says: Says::Text((*line)?.into()),
                })
            })
        })
        .collect();
    Copies {
        file: POUCH_TEXT,
        new,
    }
}

/// The items `new_game` adds to the pool, each taking a junk item's place:
/// the bags it shuffles, and the Magic Meter when magic doesn't come with
/// the first magic item.
pub fn added(new_game: &NewGame) -> Vec<&'static str> {
    let nut = new_game.shuffle_nut_bag.then(|| item::NUT_BAG_SMALL.get());
    let stick = new_game
        .shuffle_stick_bag
        .then(|| item::STICK_BAG_SMALL.get());
    let bombchus = match new_game.bombchus {
        Bombchus::Bag => &bombchu_bags::BAGS[..],
        _ => &[],
    };
    let meter = (!new_game.magic_with_first_item).then_some(crate::magic::meter::NAME);
    nut.into_iter()
        .chain(stick)
        .chain(bombchus.iter().copied())
        .chain(meter)
        .chain(crate::masks::pooled(new_game))
        .collect()
}

/// The junk an added item takes the place of, cheapest first.
pub const JUNK: [&Name; 4] = [
    actor::T_BOX_RUPEE_GREEN,
    actor::T_BOX_RUPEE_BLUE,
    actor::T_BOX_RUPEE_RED,
    actor::T_BOX_RUPEE_PURPLE,
];

/// Put each of `added` in `pool` in place of the cheapest rupee it holds.
/// One with no rupee left to take is left out. Returns each one put in,
/// with the rupee it took.
pub fn swap_in<'a>(pool: &mut [&'a str], added: &[&'a str]) -> Vec<(&'a str, &'a str)> {
    let mut traded = Vec::new();
    for &item in added {
        let at = JUNK
            .iter()
            .find_map(|j| pool.iter().position(|i| *i == j.get()));
        if let Some(at) = at {
            traded.push((item, pool[at]));
            pool[at] = item;
        }
    }
    traded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_setting_adds_its_items() {
        needs_names!();
        let meter = crate::magic::meter::NAME;
        assert_eq!(added(&NewGame::default()), [meter]);
        let game = NewGame {
            magic_with_first_item: true,
            ..Default::default()
        };
        assert!(added(&game).is_empty());
        let all = NewGame {
            shuffle_nut_bag: true,
            shuffle_stick_bag: true,
            bombchus: Bombchus::Bag,
            ..Default::default()
        };
        let mut want = vec![item::NUT_BAG_SMALL.get(), item::STICK_BAG_SMALL.get()];
        want.extend(bombchu_bags::BAGS);
        want.push(meter);
        assert_eq!(added(&all), want);
    }

    #[test]
    fn a_shuffled_bag_stays_with_its_bag_and_the_other_hands_its_over() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-bags-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        let new_game = NewGame {
            shuffle_nut_bag: true,
            ..Default::default()
        };
        write(&romfs, &new_game, &mut builder, &mut tables).unwrap();
        tables.write(&mut builder).unwrap();
        let written = builder.finish().unwrap();
        let sticks = ounce_formats::pack_file(item::DEKU_STICK);
        assert!(!written.contains(&sticks));
        for pack in NUTS.packs {
            let file = ounce_formats::pack_file(pack);
            assert!(written.contains(&file), "{file}");
            let raw = std::fs::read(dir.join("romfs").join(&file)).unwrap();
            let data = romfs.dicts().decompress(&file, &raw).unwrap();
            let sarc = roead::sarc::Sarc::new(data).unwrap();
            let param = sarc.get_data(&pouch_param(NUTS.param)).unwrap();
            let doc = BymlDoc::parse(param).unwrap();
            assert!(!gives_bag(doc.root.as_map().unwrap()), "{pack}");
        }
        let small = NUTS.bags[0];
        let file = ounce_formats::pack_file(small);
        let raw = std::fs::read(dir.join("romfs").join(&file)).unwrap();
        let data = romfs.dicts().decompress(&file, &raw).unwrap();
        let pack = ActorPack::from_bytes(small, data).unwrap();
        let param = pack.actor_param().unwrap().unwrap();
        for key in GET_REFS {
            assert!(component(&param, key).is_some(), "{key}");
        }
        let anim = path_of(&pack, ANIMATION).unwrap();
        let anim = pack.resolve(&anim).unwrap().unwrap();
        let names = format!("{:?}", anim);
        assert!(names.contains(&format!("{small}_Animation")), "{names}");
        assert!(pack.raw(&path_of(&pack, GET_REFS[0]).unwrap()).is_some());
        // The game keeps one copy of a param by its path, whichever pack
        // loads it first: every bag above the Small carries the same one.
        let own = actor_path(small);
        for heir in &NUTS.bags[1..] {
            let file = ounce_formats::pack_file(heir);
            let raw = std::fs::read(dir.join("romfs").join(&file)).unwrap();
            let data = romfs.dicts().decompress(&file, &raw).unwrap();
            let heir = ActorPack::from_bytes(heir, data).unwrap();
            assert_eq!(heir.raw(&own), pack.raw(&own), "{}", heir.name);
        }
        // Sized by its bytes alone, the game crashed loading a chest
        // holding the bag: past its bytes, it costs what the Medium's does.
        let table = romfs.restbl_rel().unwrap();
        let written = std::fs::read(dir.join("romfs").join(&table)).unwrap();
        let written = romfs.dicts().decompress(&table, &written).unwrap();
        let restbl = ounce_formats::Restbl::parse(&written).unwrap();
        let medium = ActorPack::load(&romfs, NUTS.bags[1]).unwrap();
        let over = |name, size: usize| {
            restbl.get(&actor_path(name)).unwrap() as usize - size.next_multiple_of(32)
        };
        let small_size = pack.raw(&own).unwrap().len();
        let medium_size = medium.raw(&actor_path(NUTS.bags[1])).unwrap().len();
        assert!(over(small, small_size) >= over(NUTS.bags[1], medium_size));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn each_added_bag_gets_its_missing_lines_and_keeps_the_rest() {
        needs_names!();
        let game = NewGame {
            magic_with_first_item: true,
            ..Default::default()
        };
        let meter = crate::magic::meter::NAME;
        let labels: Vec<_> = (captions(&game).new.into_iter()).map(|l| l.label).collect();
        assert_eq!(labels, TEXT_PARTS.map(|p| format!("{meter}{p}")));
        let nuts = NewGame {
            shuffle_nut_bag: true,
            ..game.clone()
        };
        let labels: Vec<_> = captions(&nuts).new.into_iter().map(|l| l.label).collect();
        let bag = item::NUT_BAG_SMALL;
        assert_eq!(
            labels[..2],
            [format!("{bag}_GetMessage"), format!("{bag}_GetCaption")]
        );
        let chus = NewGame {
            bombchus: Bombchus::Bag,
            ..game
        };
        assert_eq!(captions(&chus).new.len(), 9 + 3);
    }

    #[test]
    fn a_bag_takes_the_cheapest_rupee() {
        needs_names!();
        let (red, green) = (actor::T_BOX_RUPEE_RED.get(), actor::T_BOX_RUPEE_GREEN.get());
        let mut pool = [red, "Item_A", green, green];
        let traded = swap_in(
            &mut pool,
            &[
                "Item_FakeBag",
                "Item_FakeBag2",
                "Item_FakeBag3",
                "Item_FakeBag4",
            ],
        );
        assert_eq!(
            pool,
            ["Item_FakeBag3", "Item_A", "Item_FakeBag", "Item_FakeBag2"]
        );
        assert_eq!(
            traded,
            [
                ("Item_FakeBag", green),
                ("Item_FakeBag2", green),
                ("Item_FakeBag3", red)
            ]
        );
    }
}
