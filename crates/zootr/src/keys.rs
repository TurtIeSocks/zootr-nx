//! Keys Link starts with, so the chests that held them become checks. A
//! small-key door spends its dungeon's counter and a boss door reads its
//! boss key flag, so a new game that fills both opens every door.

use std::sync::LazyLock;

use anyhow::{Context, Result};

use crate::checks::ItemCheck;
use crate::known::{Name, actor, flag, flow, item, map};
use crate::map_copies;
use crate::rng::Rng;
use crate::rules::{Ctx, Need};
use crate::settings::FORTRESS_FAST;
use crate::settings::{DungeonKeys, FortressKeys};
use crate::{key_rooms, key_tints};

pub const SMALL: &Name = item::DUNGEON_SMALL_KEY;
pub const BOSS: &Name = item::DUNGEON_BOSS_KEY;
/// What a key chest brings to the pool: the key itself stays out of it.
pub const INSTEAD: &Name = actor::T_BOX_RUPEE_RED;

/// A place that counts keys, as its `DungeonInfo` member and Banc folder
/// name: how many small keys it has, which is how many small-key doors,
/// and the boss room its boss key opens, if it has one.
pub struct Dungeon {
    pub name: &'static Name,
    pub keys: u32,
    pub boss_room: Option<&'static Name>,
}

impl Dungeon {
    const fn new(name: &'static Name, keys: u32, boss_room: Option<&'static Name>) -> Self {
        Dungeon {
            name,
            keys,
            boss_room,
        }
    }
}

/// Each dungeon with keys.
pub const DUNGEONS: [Dungeon; 8] = [
    Dungeon::new(flow::BOTTOM_OF_THE_WELL, 2, None),
    Dungeon::new(flag::FOREST_TEMPLE, 5, Some(map::PHANTOM_GANON_STAGE)),
    Dungeon::new(flag::FIRE_TEMPLE, 8, Some(map::VOLVAGIA_STAGE)),
    Dungeon::new(flag::WATER_TEMPLE, 4, Some(map::MORPHA_STAGE)),
    Dungeon::new(flag::SHADOW_TEMPLE, 5, Some(map::BONGO_BONGO_STAGE)),
    Dungeon::new(flag::SPIRIT_TEMPLE, 5, Some(map::TWINROVA_STAGE)),
    Dungeon::new(flag::GERUDO_TRAINING_GROUND, 9, None),
    Dungeon::new(flag::INSIDE_GANON_CASTLE, 1, Some(map::GANONDORF_STAGE)),
];

/// Dungeon `n` of [`DUNGEONS`], counting from 1.
pub fn dungeon(n: usize) -> &'static Dungeon {
    // Through a const reference, which is 'static, since indexing the
    // const itself would borrow a temporary.
    const ALL: &[Dungeon] = &DUNGEONS;
    &ALL[n - 1]
}

/// How many small key copies Keysanity deals dungeon `n`: one for each of
/// its keys, the chests' and the two lying in the open (Bottom of the
/// Well's and the Shadow Temple's), and logic asks for them all.
pub(crate) fn copies_of(n: usize) -> u32 {
    dungeon(n).keys
}

/// The dungeon whose maps `rel`, a Banc file, is under, by its number in
/// [`DUNGEONS`], counting from 1.
pub fn dungeon_of(rel: &str) -> Option<usize> {
    dungeon_in(rel.strip_prefix("Banc/")?.split('/').next()?)
}

/// The dungeon `area`, a Banc folder, is, by its number in [`DUNGEONS`].
pub fn dungeon_in(area: &str) -> Option<usize> {
    DUNGEONS.iter().position(|d| d.name == area).map(|i| i + 1)
}

/// The two keys lying in the open, by placement, and the Bool their
/// room's logic module sets once Link takes what lies there. The module
/// deletes the spot's actor once it's set, whatever the seed put there.
pub const LOOSE: [(u64, &Name); 2] = [
    (
        17649154319341333603,
        flag::BOTTOM_OF_THE_WELL_LEVEL_ROOM_03_GET_SMALL_KEY,
    ),
    (
        12562389953863361839,
        flag::SHADOW_TEMPLE_LEVEL_ROOM_13_GET_KEY,
    ),
];

/// The Bool set once the loose key spot placed as `hash` is taken.
pub fn loose_taken(hash: u64) -> Option<&'static str> {
    LOOSE.iter().find(|(h, _)| *h == hash).map(|(_, f)| f.get())
}

/// Whether dungeon `n` has a boss key.
pub fn has_boss(n: usize) -> bool {
    dungeon(n).boss_room.is_some()
}

/// How one dungeon's small keys come: as its ring, or one key at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum KeyMode {
    /// One copy, which fills the dungeon's counter at once
    Rings,
    /// One copy for each key, each adding one to the counter. Keys left in
    /// their chests or held from the start ask the same counts.
    #[default]
    Singles,
}

impl KeyMode {
    /// How many copies `keys` stands for: one ring, or that many keys.
    fn count(self, keys: u32) -> u32 {
        match self {
            Self::Rings => 1,
            Self::Singles => keys,
        }
    }
}

/// Each dungeon's [`KeyMode`], as a seed deals its keys. `mixed` rolls one
/// per dungeon from the seed; every other value gives all eight the same.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KeyModes {
    /// The setting's value, for what every dungeon shares
    setting: DungeonKeys,
    modes: [KeyMode; DUNGEONS.len()],
}

impl KeyModes {
    /// The modes `keys` deals by, with a mixed roll off `seed`. One `Rng`
    /// off a rotation no other roll uses (`StartingAge` takes 17, a door
    /// roll 29), drawing one bit per dungeon in [`DUNGEONS`] order.
    pub fn roll(keys: DungeonKeys, seed: u64) -> Self {
        let mut rng = Rng::new(seed.rotate_left(41));
        let modes = DUNGEONS.map(|_| match keys {
            DungeonKeys::Rings => KeyMode::Rings,
            DungeonKeys::Mixed => match rng.below(2) {
                0 => KeyMode::Rings,
                _ => KeyMode::Singles,
            },
            DungeonKeys::Vanilla | DungeonKeys::Start | DungeonKeys::Singles => KeyMode::Singles,
        });
        Self {
            setting: keys,
            modes,
        }
    }

    /// The setting every dungeon shares.
    pub fn setting(&self) -> DungeonKeys {
        self.setting
    }

    /// Dungeon `n` of [`DUNGEONS`], counting from 1. The Gerudo Fortress
    /// has no ring form and never asks.
    pub fn mode(&self, n: usize) -> KeyMode {
        self.modes[n - 1]
    }

    /// Whether the chests that held keys are checks.
    pub fn moved(&self) -> bool {
        self.setting.moved()
    }

    /// Whether small keys come as copies the hook credits to their own
    /// dungeon, dealt with the boss keys to any chest.
    pub fn copies(&self) -> bool {
        self.setting.copies()
    }

    /// Every key copy the seed deals into a key chest's place: each
    /// dungeon's small keys as its mode deals them, and each boss key.
    /// Nothing with keys vanilla or from the start. Each id comes from
    /// [`RINGS`] and [`BOSSES`], so it lives as long as the pool that holds
    /// it.
    pub fn dealt(&self) -> Vec<&'static str> {
        if !self.copies() {
            return Vec::new();
        }
        let n = 1..=DUNGEONS.len();
        let smalls = n.clone().flat_map(|n| {
            let count = self.mode(n).count(copies_of(n)) as usize;
            (0..count).map(move |_| RINGS[n - 1].as_str())
        });
        smalls
            .chain(n.filter(|&n| has_boss(n)).map(|n| BOSSES[n - 1].as_str()))
            .collect()
    }

    /// Every dungeon dealing as `keys` says, for a test that names no seed.
    #[cfg(test)]
    pub(crate) fn fixed(keys: DungeonKeys) -> Self {
        Self::roll(keys, 0)
    }

    /// Each dungeon whose small keys come as a ring, by name. Only a mixed
    /// seed mixes, so the others name none.
    pub fn rings(&self) -> Vec<&'static Name> {
        if self.setting != DungeonKeys::Mixed {
            return Vec::new();
        }
        DUNGEONS
            .iter()
            .enumerate()
            .filter(|(i, _)| self.mode(i + 1) == KeyMode::Rings)
            .map(|(_, d)| d.name)
            .collect()
    }
}

/// Whether `item` is a ring, small key or boss key copy, the Fortress key
/// copy, or a map or compass copy, which the hook credits as Link gets it.
pub fn is_shuffled(item: &str) -> bool {
    RINGS.iter().any(|s| s == item)
        || BOSSES.iter().any(|s| s == item)
        || item == key_tints::fortress()
        || map_copies::is_copy(item)
}

/// Each dungeon's ring copy and boss key copy, by its number less one.
static RINGS: LazyLock<Vec<String>> =
    LazyLock::new(|| (1..=DUNGEONS.len()).map(key_tints::ring).collect());
static BOSSES: LazyLock<Vec<String>> =
    LazyLock::new(|| (1..=DUNGEONS.len()).map(key_tints::boss).collect());

/// What `check` gives logic when it keeps its vanilla item: a dungeon key
/// counts as its dungeon's copy, the item the key needs ask for.
pub fn held(check: &ItemCheck) -> &str {
    let copies: &[String] = match check.item.as_str() {
        i if i == SMALL => &RINGS,
        i if i == BOSS => &BOSSES,
        i => return i,
    };
    match dungeon_in(&check.area) {
        Some(n) => &copies[n - 1],
        None => &check.item,
    }
}

/// With Keysanity, every small key copy, one for each key chest, and
/// every boss key. `fill::start` hands these over with keys from the start.
pub static SINGLES: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| KeyModes::roll(DungeonKeys::Singles, 0).dealt());

/// With Fortress Keysanity, the Fortress key copy, one for each thief.
pub static FORTRESS_COPIES: LazyLock<Vec<String>> =
    LazyLock::new(|| vec![key_tints::fortress(); FORTRESS.keys as usize]);

/// Each copy `modes` and `fortress` deal takes the place of a key chest's
/// or a thief's Red Rupee, but for those `placed` already holds. With
/// nothing left to deal (an AP seed fixes every check), the copies not
/// placed here went to other worlds.
pub fn deal_copies(
    items: &mut [&str],
    modes: &KeyModes,
    fortress: FortressKeys,
    placed: &[&str],
) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let copies = modes.dealt();
    let thieves: Vec<&'static str> = match fortress.copies() {
        true => FORTRESS_COPIES.iter().map(String::as_str).collect(),
        false => Vec::new(),
    };
    let mut placed = placed.to_vec();
    for key in copies.into_iter().chain(thieves) {
        if let Some(i) = placed.iter().position(|p| *p == key) {
            placed.swap_remove(i);
            continue;
        }
        let rupee = items.iter().position(|i| *INSTEAD == **i);
        let rupee = rupee.context("too few key chests for the dungeons' keys")?;
        items[rupee] = key;
    }
    Ok(())
}

/// `count` of dungeon `n`'s small keys as its mode deals them: its ring,
/// or that many copies. Nothing for none. With keys from the start, Link
/// starts with every copy ([`crate::fill::start`]), so the need is met.
fn smalls(n: usize, count: u32, modes: &KeyModes) -> Option<Need> {
    if count == 0 {
        return None;
    }
    Some(Need::Items {
        actors: vec![key_tints::ring(n)],
        count: modes.mode(n).count(count),
    })
}

/// With keys dealt as copies or left in their chests, what logic adds to
/// `check`: the small keys its room needs in the worst order Link could
/// spend them, or all its dungeon's where `key_rooms` has no count for it.
/// A chest keeping its vanilla small key there asks for none, or it would
/// wait on itself: the checks past its doors ask for every key, so for
/// every key chest's route.
pub fn dungeon_need(check: &ItemCheck, modes: &KeyModes) -> Option<Need> {
    if modes.setting() == DungeonKeys::Vanilla && check.item == SMALL && !key_rooms::by_room(check)
    {
        return None;
    }
    let (n, count) = key_rooms::count(check)?;
    smalls(n, count, modes)
}

/// With the Fortress keys dealt as copies, what logic adds to a check whose
/// vanilla item is `item`: for the Gerudo membership card, given once the
/// fourth carpenter is free, all four keys, or one with Gerudo Fortress
/// fast.
pub fn fortress_need(item: &str, ctx: &Ctx) -> Option<Need> {
    let copies = |count| Need::Items {
        actors: vec![key_tints::fortress()],
        count,
    };
    (ctx.fortress.copies() && item == crate::carpenters::guards::CARD).then(|| {
        let fast = Need::All(vec![ctx.setting(FORTRESS_FAST), copies(1)]);
        Need::Any(vec![fast, copies(FORTRESS.keys)])
    })
}

/// With keys dealt as copies or left in their chests, what logic adds to
/// the way into `room`, a boss room: its dungeon's boss key and small keys.
pub fn boss_door_need(room: &str, modes: &KeyModes) -> Option<Need> {
    let i = DUNGEONS
        .iter()
        .position(|d| d.boss_room.is_some_and(|r| *r == *room))?;
    let small = smalls(i + 1, copies_of(i + 1), modes)?;
    Some(Need::All(vec![holds(key_tints::boss(i + 1)), small]))
}

/// A ring's, small key's or boss key copy's name, after its dungeon's:
/// `area` names one by its id. The ring copy is a single small key where
/// its dungeon's keys come one at a time.
pub fn copy_name(actor: &str, area: impl Fn(&str) -> String, modes: &KeyModes) -> Option<String> {
    if let Some(name) = map_copies::copy_name(actor, &area) {
        return Some(name);
    }
    let (dungeon, english) = key_tints::english_name(actor, modes)?;
    Some(english.replace("{}", &area(dungeon.name)))
}

fn holds(actor: String) -> Need {
    Need::Items {
        actors: vec![actor],
        count: 1,
    }
}

/// The Gerudo Fortress's keys, one for each thief, each its own actor.
pub const FORTRESS_KEY: &str = "Obj_Dungeon_SmallKey_GerudoFortress_";

/// Whether `item` is a dungeon's small or boss key.
pub fn is_dungeon_key(item: &str) -> bool {
    item == SMALL || item == BOSS
}

/// Whether `item` is a boss key, a dungeon's copy of one included.
pub fn is_boss(item: &str) -> bool {
    item == BOSS || (1..=DUNGEONS.len()).any(|n| key_tints::boss(n) == item)
}

/// Whether `item` is a small key of any kind: a dungeon's, the Gerudo
/// Fortress's, or a key ring or key copy.
pub fn is_small(item: &str) -> bool {
    item == SMALL
        || item.starts_with(FORTRESS_KEY)
        || item == key_tints::fortress()
        || (1..=DUNGEONS.len()).any(|n| key_tints::ring(n) == item)
}

/// Whether `item` is any key a new game can start with.
pub fn is_key(item: &str) -> bool {
    is_dungeon_key(item) || item.starts_with(FORTRESS_KEY)
}

/// What `check` brings to the pool: its item, `INSTEAD` for a key, or for
/// a map or compass its dungeon's copy.
pub fn pooled(check: &ItemCheck) -> &str {
    match is_key(&check.item) {
        true => INSTEAD,
        false => map_copies::pooled(check).unwrap_or(&check.item),
    }
}

/// The Gerudo Fortress counts keys like a dungeon, one for each thief.
pub const FORTRESS: Dungeon = Dungeon::new(flag::GERUDO_FORTRESS, 4, None);

/// The dungeon table's fields for a held boss key, map and compass. The
/// dump keeps them only as hashes, so they're spelled out here.
pub(crate) const IS_GET_BOSS_KEY: &str = "IsGetBossKey";
pub(crate) const IS_GET_MAP: &str = "IsGetMap";
pub(crate) const IS_GET_COMPASS: &str = "IsGetCompass";

/// The save's `field` of `dungeon`, its member of the dungeon table.
pub(crate) fn dungeon_flag(dungeon: &str, field: &str) -> String {
    format!("{}.{dungeon}.{field}", flag::DUNGEON_INFO)
}

/// The Bool `dungeon`'s boss key sets.
pub fn boss_flag(dungeon: &Name) -> String {
    dungeon_flag(dungeon, IS_GET_BOSS_KEY)
}

/// The Int counting `dungeon`'s small keys.
pub fn small_flag(dungeon: &Name) -> String {
    dungeon_flag(dungeon, flag::SMALL_KEY_NUM)
}

/// The new-game defaults that fill the counters and boss key flags of
/// every dungeon, and the Fortress's counter when `fortress`.
pub fn defaults(dungeons: bool, fortress: bool) -> Vec<String> {
    let mut edits = Vec::new();
    if dungeons {
        for d in &DUNGEONS {
            edits.push(format!("{}={}", small_flag(d.name), d.keys));
            if d.boss_room.is_some() {
                edits.push(format!("{}=true", boss_flag(d.name)));
            }
        }
    }
    if fortress {
        edits.push(format!("{}={}", small_flag(FORTRESS.name), FORTRESS.keys));
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::GameData;
    use crate::known::{actor, flag, flow, item, map};
    use crate::presets;

    #[test]
    fn the_defaults_name_each_dungeon_with_its_count() {
        let edits = defaults(true, false);
        assert_eq!(edits.len(), 8 + 6);
        for line in [
            &format!("{}=5", small_flag(flag::FOREST_TEMPLE)),
            &format!("{}=true", boss_flag(flag::FOREST_TEMPLE)),
            &format!("{}=1", small_flag(flag::INSIDE_GANON_CASTLE)),
            &format!("{}=true", boss_flag(flag::INSIDE_GANON_CASTLE)),
            &format!("{}=9", small_flag(flag::GERUDO_TRAINING_GROUND)),
        ] {
            assert!(edits.contains(&line.to_owned()), "{line}");
        }
        assert!(
            !edits
                .iter()
                .any(|e| e.contains(&format!("{}.IsGet", flow::BOTTOM_OF_THE_WELL)))
        );
        assert!(!edits.iter().any(|e| e.contains("Fortress")));
        assert_eq!(
            defaults(false, true),
            [format!("{}=4", small_flag(flag::GERUDO_FORTRESS))]
        );
        assert!(defaults(false, false).is_empty());
    }

    #[test]
    fn each_ring_and_boss_key_takes_a_red_rupee_s_place() {
        needs_names!();
        let red = INSTEAD.get();
        let dealt = KeyModes::fixed(DungeonKeys::Rings).dealt();
        let mut items = vec![red; dealt.len() + 1];
        items.push(item::HOOKSHOT.get());
        deal_copies(
            &mut items,
            &KeyModes::fixed(DungeonKeys::Rings),
            FortressKeys::Vanilla,
            &[],
        )
        .unwrap();
        assert_eq!(items.iter().filter(|i| **i == red).count(), 1);
        assert!(dealt.iter().all(|k| items.contains(k)));
        assert_eq!(dealt.len(), DUNGEONS.len() + 6);
        assert!(
            deal_copies(
                &mut [red; 3],
                &KeyModes::fixed(DungeonKeys::Rings),
                FortressKeys::Vanilla,
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn keysanity_deals_a_copy_for_every_key_chest() {
        needs_names!();
        let red = INSTEAD.get();
        // The 37 small key chests, the 2 loose keys and the 6 boss key chests.
        assert_eq!(SINGLES.len(), 39 + 6);
        let mut items = vec![red; SINGLES.len()];
        deal_copies(
            &mut items,
            &KeyModes::fixed(DungeonKeys::Singles),
            FortressKeys::Vanilla,
            &[],
        )
        .unwrap();
        assert!(!items.contains(&red));
        let forest = DUNGEONS
            .iter()
            .position(|d| d.name == flag::FOREST_TEMPLE)
            .unwrap();
        let ring = key_tints::ring(forest + 1);
        let held = items.iter().filter(|i| **i == ring).count() as u32;
        assert_eq!(held, copies_of(forest + 1));
        let mut start = vec![red; 2];
        deal_copies(
            &mut start,
            &KeyModes::fixed(DungeonKeys::Start),
            FortressKeys::Start,
            &[],
        )
        .unwrap();
        assert_eq!(start, [red; 2]);
    }

    #[test]
    fn loose_keys_are_the_two_lying_and_tick_by_what_their_module_sets() {
        needs_names!();
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let found = crate::pickups::LOOSE_KEYS.checks(&romfs).unwrap();
        let found: std::collections::BTreeSet<u64> = found.iter().map(|c| c.hash).collect();
        let listed = LOOSE.iter().map(|l| l.0).collect();
        assert_eq!(found, listed);
        let creators = crate::gates::creators(&romfs).unwrap();
        for (hash, taken) in LOOSE {
            let module = creators.iter().find(|c| c.hash == hash).unwrap();
            let rel = format!("Logic/{}.ainb", module.graph);
            let patch = ounce_formats::ainb::Patch::new(romfs.read(&rel).unwrap()).unwrap();
            let sets = patch.graph().nodes.iter().any(|n| {
                n.name == crate::known::field::TRIGGER_GAME_DATA_SET_BOOL_TRUE.get()
                    && n.str(crate::known::field::GAME_DATA_NAME_BOOL) == Some(taken.get())
            });
            assert!(sets, "{} never sets {}", module.graph, taken.get());
            assert_eq!(loose_taken(hash), Some(taken.get()));
        }
    }

    #[test]
    fn copies_placed_in_other_worlds_leave_nothing_to_deal() {
        needs_names!();
        // An AP seed fixes every check: the copies it put elsewhere aren't
        // among this world's placements, and no item is left to swap.
        let copy = key_tints::fortress();
        let placed = [copy.as_str()];
        deal_copies(
            &mut [],
            &KeyModes::fixed(DungeonKeys::Vanilla),
            FortressKeys::Singles,
            &placed,
        )
        .unwrap();
        deal_copies(
            &mut [],
            &KeyModes::fixed(DungeonKeys::Singles),
            FortressKeys::Singles,
            &[],
        )
        .unwrap();
    }

    #[test]
    fn fortress_keysanity_deals_a_copy_for_every_thief_and_the_card_asks_for_all() {
        needs_names!();
        let red = INSTEAD.get();
        let mut items = vec![red; 5];
        deal_copies(
            &mut items,
            &KeyModes::fixed(DungeonKeys::Vanilla),
            FortressKeys::Singles,
            &[],
        )
        .unwrap();
        let copy = key_tints::fortress();
        assert_eq!(items.iter().filter(|i| **i == copy).count(), 4);
        assert_eq!(items.iter().filter(|i| **i == red).count(), 1);
        // A copy placed by hand is one of the four.
        let mut items = vec![red; 5];
        let placed = [copy.as_str()];
        deal_copies(
            &mut items,
            &KeyModes::fixed(DungeonKeys::Vanilla),
            FortressKeys::Singles,
            &placed,
        )
        .unwrap();
        assert_eq!(items.iter().filter(|i| **i == copy).count(), 3);
        assert!(is_shuffled(&copy));
        let ctx = |fortress_keys, on: &[&str]| {
            Ctx::new(&crate::settings::NewGame {
                fortress_keys,
                on: on.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            })
        };
        let card = crate::carpenters::guards::CARD.get();
        let keysanity = ctx(FortressKeys::Singles, &[]);
        let fast = ctx(FortressKeys::Singles, &[FORTRESS_FAST]);
        let held = |n: usize| {
            let mut have = crate::rules::Have::default();
            (0..n).for_each(|_| have.add(&copy));
            have
        };
        let met = |ctx: &Ctx, n| {
            let need = fortress_need(card, ctx).unwrap();
            need.met(&held(n), crate::world::Era::Adult)
        };
        assert!(!met(&keysanity, 3) && met(&keysanity, 4));
        assert!(!met(&fast, 0) && met(&fast, 1));
        assert_eq!(fortress_need(card, &ctx(FortressKeys::Start, &[])), None);
        assert_eq!(fortress_need(item::HOOKSHOT.get(), &keysanity), None);
        let area = |id: &str| format!("<{id}>");
        let name = copy_name(
            &key_tints::fortress(),
            area,
            &KeyModes::fixed(DungeonKeys::Vanilla),
        );
        assert_eq!(name, Some(format!("<{}> Small Key", FORTRESS.name)));
    }

    #[test]
    fn each_dungeon_s_key_checks_match_its_copies() {
        needs_names!();
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let mut checks = crate::checks::all(&romfs).unwrap();
        checks.extend(crate::pickups::LOOSE_KEYS.checks(&romfs).unwrap());
        let mut seen = std::collections::HashSet::new();
        let mut counts = [0u32; 8];
        for c in checks.iter().filter(|c| c.item == SMALL.get()) {
            if let Some(n) = dungeon_of(&c.file).filter(|_| seen.insert(c.hash)) {
                counts[n - 1] += 1;
            }
        }
        assert_eq!(counts, std::array::from_fn(|i| copies_of(i + 1)));
    }

    #[test]
    fn logic_asks_a_dungeon_s_checks_for_its_ring_and_its_boss_door_for_both() {
        needs_names!();
        let at = |file: String| ItemCheck {
            area: crate::checks::area_of(&file).into(),
            file,
            ..crate::narrowing::tests::chest(1, "Fake_Item")
        };
        let forest = at(format!("Banc/{}/Fake.bcett.byml.zs", flag::FOREST_TEMPLE));
        let n = dungeon_of(&forest.file).unwrap();
        let rings = KeyModes::fixed(DungeonKeys::Rings);
        let need = dungeon_need(&forest, &rings).unwrap().to_string();
        assert_eq!(need, key_tints::ring(n));
        assert!(dungeon_need(&at("Banc/FakeField/Fake.bcett.byml.zs".into()), &rings).is_none());
        // Keys from the start ask what Singles does: Link starts with them.
        let need = |keys| dungeon_need(&forest, &KeyModes::fixed(keys)).map(|n| n.to_string());
        assert_eq!(need(DungeonKeys::Start), need(DungeonKeys::Singles));
        let door = boss_door_need(map::PHANTOM_GANON_STAGE, &rings)
            .unwrap()
            .to_string();
        assert!(door.contains(&key_tints::boss(n)) && door.contains(&key_tints::ring(n)));
        assert!(boss_door_need(map::GOHMA_STAGE, &rings).is_none());
        // Keysanity asks for every copy the dungeon's chests hold.
        let all = dungeon_need(&forest, &KeyModes::fixed(DungeonKeys::Singles)).unwrap();
        let Need::Items { actors, count } = all else {
            panic!("{all}")
        };
        assert_eq!((actors, count), (vec![key_tints::ring(n)], copies_of(n)));
    }

    #[test]
    fn a_mixed_seed_rolls_each_dungeon_and_rolls_it_the_same_every_time() {
        let rolls = KeyModes::roll(DungeonKeys::Mixed, 7);
        assert_eq!(rolls, KeyModes::roll(DungeonKeys::Mixed, 7));
        let rolls = |seed: u64| KeyModes::roll(DungeonKeys::Mixed, seed);
        // A seed mixes them: some dungeons take a ring, some their keys.
        let mixed = (0..64)
            .filter(|&seed| {
                let rings = rolls(seed).rings().len();
                rings > 0 && rings < DUNGEONS.len()
            })
            .count();
        assert!(mixed > 32, "only {mixed} of 64 seeds mix");
        // Each dungeon rolls both ways over the seeds, so it rolls its own.
        for n in 1..=DUNGEONS.len() {
            let modes: Vec<KeyMode> = (0..64).map(|seed| rolls(seed).mode(n)).collect();
            assert!(
                modes.contains(&KeyMode::Rings) && modes.contains(&KeyMode::Singles),
                "dungeon {n} always rolls {modes:?}"
            );
        }
        // Every other setting gives all eight dungeons the same mode, and
        // names no ring: only a mixed seed mixes.
        for keys in [
            DungeonKeys::Rings,
            DungeonKeys::Singles,
            DungeonKeys::Vanilla,
            DungeonKeys::Start,
        ] {
            let modes = KeyModes::roll(keys, 7);
            let want = match keys {
                DungeonKeys::Rings => KeyMode::Rings,
                _ => KeyMode::Singles,
            };
            assert!(
                (1..=DUNGEONS.len()).all(|n| modes.mode(n) == want),
                "{keys:?}"
            );
            assert!(modes.rings().is_empty(), "{keys:?} names a ring");
        }
    }

    #[test]
    fn a_mixed_seed_deals_one_copy_for_a_ring_and_one_for_every_other_key() {
        needs_names!();
        let red = INSTEAD.get();
        let modes = KeyModes::roll(DungeonKeys::Mixed, 7);
        let dealt = modes.dealt();
        let bosses = DUNGEONS.iter().filter(|d| d.boss_room.is_some()).count();
        let want: usize = (1..=DUNGEONS.len())
            .map(|n| modes.mode(n).count(copies_of(n)) as usize)
            .sum::<usize>()
            + bosses;
        assert_eq!(dealt.len(), want);
        let mut items = vec![red; dealt.len()];
        deal_copies(&mut items, &modes, FortressKeys::Vanilla, &[]).unwrap();
        assert!(!items.contains(&red));
        for n in 1..=DUNGEONS.len() {
            let held = items.iter().filter(|i| **i == key_tints::ring(n)).count() as u32;
            assert_eq!(held, modes.mode(n).count(copies_of(n)), "dungeon {n}");
        }
    }

    #[test]
    fn a_mixed_seed_asks_its_ring_dungeons_for_one_copy_and_the_rest_for_their_own_count() {
        needs_names!();
        let modes = KeyModes::roll(DungeonKeys::Mixed, 7);
        let at = |dungeon: &Name| ItemCheck {
            area: dungeon.get().into(),
            file: format!("Banc/{}/Fake.bcett.byml.zs", dungeon.get()),
            ..crate::narrowing::tests::chest(1, "Fake_Item")
        };
        for (i, d) in DUNGEONS.iter().enumerate() {
            let n = i + 1;
            let Need::Items { actors, count } = dungeon_need(&at(d.name), &modes).unwrap() else {
                panic!("{modes:?}")
            };
            assert_eq!(actors, vec![key_tints::ring(n)], "{}", d.name);
            assert_eq!(count, modes.mode(n).count(copies_of(n)), "{}", d.name);
        }
    }

    #[test]
    fn a_copy_is_named_after_its_dungeon() {
        let area = |id: &str| format!("<{id}>");
        let forest = DUNGEONS.iter().position(|d| d.name == flag::FOREST_TEMPLE);
        let n = forest.unwrap() + 1;
        let named = |a: String| copy_name(&a, area, &KeyModes::fixed(DungeonKeys::Rings));
        assert_eq!(
            named(key_tints::ring(n)).unwrap(),
            format!("<{}> Key Ring", flag::FOREST_TEMPLE)
        );
        assert_eq!(
            named(key_tints::boss(n)).unwrap(),
            format!("<{}> Boss Key", flag::FOREST_TEMPLE)
        );
        assert!(named(item::HOOKSHOT.to_string()).is_none());
        assert_eq!(
            copy_name(
                &key_tints::ring(n),
                area,
                &KeyModes::fixed(DungeonKeys::Singles)
            )
            .unwrap(),
            format!("<{}> Small Key", flag::FOREST_TEMPLE)
        );
    }

    #[test]
    fn a_key_chest_brings_a_red_rupee() {
        needs_names!();
        let chest = |item: &str| ItemCheck {
            item: item.into(),
            ..crate::gifts::check(
                String::new(),
                "A",
                1,
                actor::OBJ_T_BOX_A_02.to_string(),
                item,
            )
        };
        assert_eq!(pooled(&chest(SMALL)), INSTEAD);
        assert_eq!(pooled(&chest(BOSS)), INSTEAD);
        assert_eq!(pooled(&chest(item::HOOKSHOT)), item::HOOKSHOT);
    }

    #[test]
    fn the_defaults_apply_and_match_the_games_keys() {
        use crate::checks::{GYAML, actors, banc_files, hash_of, load_banc, str_field};
        use std::collections::BTreeSet;
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        let mut doc =
            ounce_formats::BymlDoc::parse(&romfs.read(crate::gamedata::PATH).unwrap()).unwrap();
        presets::apply(&gd, &mut doc.root, &defaults(true, true)).unwrap();
        let mut held = BTreeSet::new();
        let mut lying = BTreeSet::new();
        for rel in banc_files(&romfs).unwrap() {
            let area = crate::checks::area_of(&rel).to_owned();
            let doc = load_banc(&romfs, &rel).unwrap();
            for actor in actors(&doc.root).unwrap() {
                let hash = hash_of(actor).unwrap();
                if str_field(actor, GYAML) == Some(actor::OBJ_DUNGEON_SMALL_KEY) {
                    lying.insert((area.clone(), hash));
                }
                let item = actor.as_map().unwrap().get(crate::checks::DYNAMIC);
                let item = item.and_then(|d| str_field(d, crate::checks::DROP_ACTOR));
                if let Some(item) = item.filter(|i| is_dungeon_key(i)) {
                    held.insert((area.clone(), item.to_owned(), hash));
                }
            }
        }
        for Dungeon {
            name,
            keys,
            boss_room,
        } in DUNGEONS
        {
            let chests = |key: &str| held.iter().filter(|h| h.0 == name && h.1 == key).count();
            let lying = lying.iter().filter(|l| l.0 == name).count();
            assert_eq!(chests(SMALL) + lying, keys as usize, "{name}");
            assert_eq!(chests(BOSS), usize::from(boss_room.is_some()), "{name}");
        }
        assert_eq!(held.len(), 43);
    }
}
