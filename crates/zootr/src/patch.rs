//! Our code in the game: the hook from `zootr-hook` and its table, written
//! into `main` by a cheat file, an exefs patch, and an IPS for a Switch.
//! Each build of the game has its own [`Hook`]: its offsets, `main`'s
//! build ID, and the code `build.rs` compiled against them. The sites and
//! the table are built here; `files` writes them out and manages the files.

use anyhow::{Result, ensure};
pub use zootr_hook::OFFSETS;
use zootr_hook::entry;
use zootr_hook::look::LookSpec;
use zootr_hook::{
    Build, Credit, DEV, MENU_CYCLE, SLOTS, Shared, Spot, V65536, V131072, encode, list,
};

use crate::game::Release;
use crate::menu_slots;

mod files;
pub use files::{EXEFS, SeedTable, exefs, install, remove, remove_cheat, split};

/// The hook as `build.rs` built it for one build of the game.
pub struct Hook {
    pub release: Release,
    /// The offsets it was compiled against, which the patch is written for.
    pub build: Build,
    /// `main`'s build ID, as its NSO header's 32 byte field holds it. Its
    /// first 8 bytes name the cheat file and the rest, trailing zeros left
    /// out, the exefs patch's `@nsobid`, so a build these offsets weren't
    /// found in never loads either.
    pub id: [u8; 32],
    /// The cave's code and the far code.
    code: &'static [u8],
    far: &'static [u8],
    /// How many of the cave's bytes are code, before its strings.
    #[cfg(test)]
    code_end: &'static str,
}

/// Each build's hook, in [`Release`]'s order.
static HOOKS: [Hook; 2] = [
    Hook {
        release: Release::V100,
        build: V65536,
        id: [
            0x33, 0xfa, 0x6c, 0x2c, 0x23, 0x84, 0x82, 0x82, 0xa7, 0x7e, 0x8b, 0x88, 0xa0, 0xa9,
            0x3f, 0x48, 0x87, 0xc5, 0x59, 0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        code: include_bytes!(concat!(env!("OUT_DIR"), "/v65536/hook.bin")),
        far: include_bytes!(concat!(env!("OUT_DIR"), "/v65536/far.bin")),
        #[cfg(test)]
        code_end: env!("HOOK_CODE_V65536"),
    },
    Hook {
        release: Release::V111,
        build: V131072,
        id: [
            0xe0, 0xf3, 0xcc, 0xfa, 0xf5, 0x00, 0x65, 0xae, 0x4e, 0xa0, 0x1e, 0xb5, 0xae, 0xb1,
            0x3b, 0x1a, 0x45, 0xda, 0x33, 0x48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        code: include_bytes!(concat!(env!("OUT_DIR"), "/v131072/hook.bin")),
        far: include_bytes!(concat!(env!("OUT_DIR"), "/v131072/far.bin")),
        #[cfg(test)]
        code_end: env!("HOOK_CODE_V131072"),
    },
];

/// The physics step's `cbz` over the call into Havok's trace recorder, and
/// where it skips to. Made a `b`, the recorder never runs and holds the
/// cave.
const fn gate(b: &Build) -> (u32, u32) {
    (b.branches.gate, b.branches.gate_to)
}
/// Link's "can he catch the Boomerang" check's `tbz` into its child test
/// (the age override, then `PlayerGameParam.IsChild`), and where it skips
/// to: whether the Boomerang is back. Made a `b`, the adult catches it as
/// the child does, and every selector that asks gets the same answer.
const fn catch(b: &Build) -> (u32, u32) {
    (b.branches.catch, b.branches.catch_to)
}
/// The `b`s written over the game's own code, each `(at, to)`.
const fn skips(b: &Build) -> [(u32, u32); 3] {
    [gate(b), catch(b), gear::boots(b)]
}
/// Each call site and the hook entry its `bl` goes to, as an offset into
/// the hook. The event request's string argument calls, where an opening
/// chest sets its `ContentName`, go to `content_name`. The drop reader's
/// calls, where a loading room reads a chest's placement, go to
/// `drop_actor`. The bow draw's child test, which picks its guide, goes
/// to `seed_guide`, so a child Bow says "Shoot Arrow". The equipment
/// slots' child tests go to `gear_child` ([`gear::calls`]).
const fn calls(b: &Build) -> [(u32, u32); 7] {
    let [gear_request, gear_actor] = gear::calls(b);
    [
        (b.calls.content1, entry::CONTENT_NAME),
        (b.calls.content2, entry::CONTENT_NAME),
        (b.calls.drop1, entry::DROP_ACTOR),
        (b.calls.drop2, entry::DROP_ACTOR),
        (b.calls.seed_guide, entry::SEED_GUIDE),
        gear_request,
        gear_actor,
    ]
}
/// The run node's call to the speed setter and the hook entry it goes to,
/// `run_speed`, which scales the speed in the Bunny Hood in a seed with
/// the setting.
const fn run_call(b: &Build) -> (u32, u32) {
    (b.calls.run, entry::RUN_SPEED)
}
/// The controller device's three `GetNpadStates` calls (full key,
/// handheld, joy dual) and their hook entries, which run the trade slot's
/// cycle with the three Items page hooks, in a seed with [`MENU_CYCLE`].
const fn npad_calls(b: &Build) -> [(u32, u32); 3] {
    [
        (b.calls.npad_full, entry::NPAD_FULL),
        (b.calls.npad_handheld, entry::NPAD_HANDHELD),
        (b.calls.npad_dual, entry::NPAD_DUAL),
    ]
}
/// The game's stubs for `nn::hid::GetNpadStates` those calls go to, in
/// the same order. The hook reaches them through its slots.
const fn npad_stubs(b: &Build) -> [u32; 3] {
    [
        b.imports.npad_full,
        b.imports.npad_handheld,
        b.imports.npad_dual,
    ]
}
/// Each hooked function and the entry the `b` over its `pacibsp` goes to.
/// Link's update runs the mailbox's requests and the dev switches in a
/// seed with [`DEV`], and springs ice traps. The nock sets Link's
/// projectile to the drawn bow's, so a child Bow fires arrows, and the out
/// of ammo handler's message follows that projectile, not Link's age.
const fn hooks(b: &Build) -> [(u32, u32); 14] {
    let e = &b.entries;
    [
        (e.map_loaded, entry::MAP_LOADED),
        (e.chest_made, entry::CHEST_MADE),
        (e.granted, entry::GRANTED),
        (e.link_update, entry::LINK_UPDATE),
        (e.picked, entry::PICKED),
        (e.items_built, entry::ITEMS_BUILT),
        (e.items_left, entry::ITEMS_LEFT),
        (e.ui_update, entry::UI_UPDATE),
        (e.taken, entry::TAKEN),
        (e.nock, entry::NOCKED),
        (e.emptied, entry::EMPTIED),
        (e.equipment_built, entry::EQUIPMENT_BUILT),
        (e.equipment_left, entry::EQUIPMENT_LEFT),
        (e.resident_create, entry::RESIDENT_MADE),
    ]
}
/// Whether build `b`'s shared state fits under its panic word, 8 byte
/// aligned, with the list's end right after the panic word.
const fn lists_fit(b: &Build) -> bool {
    let (list, r) = (list(b), &b.regions);
    list + size_of::<Shared>() as u32 <= r.panic
        && list.is_multiple_of(8)
        && r.panic + 16 == r.list_end
}
const _: () = assert!(lists_fit(&V65536) && lists_fit(&V131072));

/// Bytes to write at an offset into `main`.
#[derive(Debug, PartialEq)]
pub struct Patch {
    pub at: u32,
    pub bytes: Vec<u8>,
}

/// `bl to`, or `b to` without `link`, placed at `from`.
fn branch(from: u32, to: u32, link: bool) -> [u8; 4] {
    let delta = i64::from(to) - i64::from(from);
    assert!(
        delta % 4 == 0 && (-(1 << 27)..1 << 27).contains(&delta),
        "branch out of range"
    );
    let op = if link { 0x9400_0000 } else { 0x1400_0000 };
    (op | ((delta >> 2) as u32 & 0x03ff_ffff)).to_le_bytes()
}

/// `bytes` at `at`, padded to a word, or an error past `end` naming
/// `release`'s `what`.
fn fit(at: u32, mut bytes: Vec<u8>, end: u32, release: Release, what: &str) -> Result<Patch> {
    // Writes go a word at a time.
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    let free = (end - at) as usize;
    ensure!(
        bytes.len() <= free,
        "the {} {what} needs {} bytes, {free} are free",
        release.name(),
        bytes.len()
    );
    Ok(Patch { at, bytes })
}

/// The table's writes: its body, then its magic, so a table caught
/// halfway never parses. The table is whole words.
fn table_writes(Patch { at, mut bytes }: Patch) -> [Patch; 2] {
    let body = bytes.split_off(4);
    [
        Patch {
            at: at + 4,
            bytes: body,
        },
        Patch { at, bytes },
    ]
}

impl Hook {
    /// The hook built for `release`.
    pub fn of(release: Release) -> &'static Self {
        match release {
            Release::V100 => &HOOKS[0],
            Release::V111 => &HOOKS[1],
        }
    }

    /// Every build's hook.
    pub fn all() -> &'static [Self] {
        &HOOKS
    }

    /// The calls and hooked functions, each branching to its hook entry.
    fn sites(&self) -> impl Iterator<Item = Patch> + use<> {
        let b = &self.build;
        let cave = b.regions.cave;
        let calls = calls(b).into_iter().chain(npad_calls(b));
        let calls = calls.map(|(at, entry)| (at, entry, true));
        let hooks = hooks(b).into_iter().map(|(at, entry)| (at, entry, false));
        let (run, run_entry) = run_call(b);
        let (hud, hud_entry) = gear::hud(b);
        calls
            .chain(hooks)
            .chain([(hud, hud_entry, true), (run, run_entry, true)])
            .map(move |(at, entry, link)| Patch {
                at,
                bytes: branch(at, cave + entry, link).to_vec(),
            })
    }

    /// Where each of the hook's slots branches, in their order: past each
    /// hooked function's `pacibsp`, then to each controller read's stub.
    fn slot_targets(&self) -> impl Iterator<Item = u32> + use<> {
        let b = &self.build;
        (hooks(b).into_iter())
            .map(|(at, _)| at + 4)
            .chain(npad_stubs(b))
    }

    /// The hook, with each slot's `b` written in.
    fn hook_code(&self) -> Vec<u8> {
        let cave = self.build.regions.cave;
        let mut code = self.code.to_vec();
        for (i, to) in self.slot_targets().enumerate() {
            let slot = SLOTS + 4 * i as u32;
            code[slot as usize..][..4].copy_from_slice(&branch(cave + slot, to, false));
        }
        code
    }

    /// What the patch writes over a hooked function's `pacibsp`, if `at` is
    /// one.
    pub fn site(&self, at: u32) -> Option<Vec<u8>> {
        self.sites().find(|p| p.at == at).map(|p| p.bytes)
    }

    /// The skips, the hook and its far code.
    fn code(&self) -> Result<Vec<Patch>> {
        let r = &self.build.regions;
        let skips = skips(&self.build).map(|(at, to)| Patch {
            at,
            bytes: branch(at, to, false).to_vec(),
        });
        let code = fit(r.cave, self.hook_code(), r.cave_end, self.release, "hook")?;
        // The far code's room ends at the first word past it the Mac's
        // game rewrites, which a cheat can't write over.
        let far = fit(
            r.far,
            self.far.to_vec(),
            r.far_end,
            self.release,
            "far code",
        )?;
        Ok(skips.into_iter().chain([code, far]).collect())
    }

    /// The skips, the hook, its far code and every call site: the same for
    /// every seed, since each hook entry checks its own setting in the
    /// table. A game that builds the exefs patch in rebuilds only when this
    /// changes.
    pub fn code_patches(&self) -> Result<Vec<Patch>> {
        Ok(self.code()?.into_iter().chain(self.sites()).collect())
    }

    /// The skips, the hook, its table for `lines`, the
    /// [`zootr_hook::KEYED`] lines `keyed`, `flags` (with [`DEV`] always, so
    /// the mailbox answers in every seed), `credits` and `spots`, then the
    /// calls and function hooks that reach it, every one in every seed.
    pub fn patches<A: AsRef<str>>(
        &self,
        lines: &[&[(A, A)]],
        keyed: &[Vec<(String, String)>],
        flags: u32,
        credits: &[(&str, Credit)],
        spots: &[Spot],
        look: Option<&LookSpec>,
    ) -> Result<Vec<Patch>> {
        let owned: Vec<Vec<(&str, &str)>> = (lines.iter())
            .map(|l| l.iter().map(|(a, b)| (a.as_ref(), b.as_ref())).collect())
            .collect();
        let lines: Vec<&[(&str, &str)]> = owned.iter().map(Vec::as_slice).collect();
        let keyed: Vec<Vec<(&str, &str)>> = (keyed.iter())
            .map(|l| l.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect())
            .collect();
        let keyed: Vec<&[(&str, &str)]> = keyed.iter().map(Vec::as_slice).collect();
        let flags = flags | DEV;
        let cycled = menu_slots::ALL.iter().filter(|_| flags & MENU_CYCLE != 0);
        let actors: Vec<_> = cycled.clone().map(|s| s.actors()).collect();
        let slots: Vec<_> = cycled.zip(&actors).map(|(s, a)| s.spec(a)).collect();
        let table = encode(flags, &lines, &keyed, credits, &slots, spots, look);
        let r = &self.build.regions;
        let table = fit(r.table, table, r.table_end, self.release, "table")?;
        Ok((self.code()?.into_iter())
            .chain(table_writes(table))
            .chain(self.sites())
            .collect())
    }
}

/// What the port checks another build's rooms against: 1.1.1's hook and
/// far code, and the table a seed with every flag and the progressive
/// lines writes (a seed's credits and ammo lines come on top, which
/// `the_fullest_table_fits` counts).
pub(crate) fn needs() -> crate::port::Needs {
    use zootr_hook::{BUNNY_HOOD, FREE_BOMBCHUS, ICE_TRAPS};
    let h = Hook::of(Release::V111);
    let flags = BUNNY_HOOD | MENU_CYCLE | ICE_TRAPS | FREE_BOMBCHUS;
    let p = (h.patches(&crate::progressive::LINES, &[], flags, &[], &[], None))
        .expect("the table fits 1.1.1's room");
    let r = &h.build.regions;
    let table = p.iter().filter(|p| (r.table..r.table_end).contains(&p.at));
    crate::port::Needs {
        cave: h.hook_code().len() as u32,
        far: h.far.len() as u32,
        table: table.map(|p| p.bytes.len() as u32).sum(),
    }
}

#[cfg(all(test, feature = "emu"))]
mod emu;
mod gear;

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use crate::progressive;
    use zootr_hook::{BUNNY_HOOD, FREE_BOMBCHUS, HEADER, Header, ICE_TRAPS, MAGIC, Table, fnv};

    use super::*;

    /// Every flag that adds a site.
    pub(super) const ALL: u32 = BUNNY_HOOD | MENU_CYCLE;
    /// A holder that stands until picked, which adds the take's site.
    pub(super) const SPOT: Spot = Spot {
        flag: 1,
        name: 2,
        at: [3, 4, 5],
        placed: 6,
        twin: 7,
    };
    /// The 1.1.1 hook, which most tests check: the 1.0.0 one is the same
    /// code against other offsets.
    pub(super) fn v111() -> &'static Hook {
        Hook::of(Release::V111)
    }

    #[test]
    fn the_hooks_seed_hash_is_the_deku_seeds_name() {
        if crate::game::no_names() {
            return;
        }
        let name = crate::known::actor::SHOOTABLE_DEKU_SEED.get();
        assert_eq!(
            ounce_formats::murmur3(name.as_bytes()),
            zootr_hook::DEKU_SEED
        );
    }

    #[test]
    fn each_release_has_its_own_hook() {
        for h in Hook::all() {
            assert_eq!(Hook::of(h.release).release, h.release);
        }
        let [a, b] = [Release::V100, Release::V111].map(Hook::of);
        assert_ne!(a.id, b.id);
        assert_ne!(a.build.regions.cave, b.build.regions.cave);
        assert_eq!(b.build.regions.cave, OFFSETS.regions.cave);
    }

    #[test]
    fn the_fullest_table_fits() {
        use crate::settings::DungeonKeys;
        // The longest ammo lines, one for each prize chest. Their names
        // come from the dump.
        let mut keyed: Vec<_> = match crate::game::dev_dump() {
            Some(_) => (crate::ammo::NEEDS.iter())
                .map(|(a, _)| crate::ammo::line(a).unwrap())
                .collect(),
            None => Vec::new(),
        };
        let size =
            |l: &Vec<(String, String)>| -> usize { l.iter().map(|(a, b)| a.len() + b.len()).sum() };
        keyed.sort_by_key(|l| std::cmp::Reverse(size(l)));
        keyed.truncate(crate::gifts::PRIZE_CHESTS.len());
        for h in Hook::all() {
            for keys in [
                DungeonKeys::Vanilla,
                DungeonKeys::Rings,
                DungeonKeys::Singles,
            ] {
                let credits = crate::key_tints::credits(true, keys, true);
                let credits: Vec<_> = credits.iter().map(|(n, c)| (n.as_str(), *c)).collect();
                let p = h.patches(&progressive::LINES, &keyed, ALL, &credits, &[SPOT], None);
                assert!(
                    p.is_ok(),
                    "{} {keys:?}: {}",
                    h.release.name(),
                    p.unwrap_err()
                );
            }
        }
    }

    /// Where the table's two writes start in `patches`: after the skips,
    /// the hook and its far code.
    pub(super) const TABLE_AT: usize = skips(&OFFSETS).len() + 2;
    /// Where the sites start in `patches`, after the table.
    pub(super) const SITES_AT: usize = TABLE_AT + 2;

    /// `h`'s `patches` without its table's two writes.
    fn without_table(h: &Hook, p: Vec<Patch>) -> Vec<Patch> {
        let r = &h.build.regions;
        p.into_iter()
            .filter(|p| !(r.table..r.table_end).contains(&p.at))
            .collect()
    }

    #[test]
    fn every_seed_writes_the_same_code_whatever_its_settings() {
        for h in Hook::all() {
            let code = h.code_patches().unwrap();
            for (flags, spots) in [(0, &[][..]), (ALL | ICE_TRAPS | FREE_BOMBCHUS, &[SPOT])] {
                let p = h.patches(&progressive::LINES, &[], flags, &[], spots, None);
                assert_eq!(without_table(h, p.unwrap()), code, "{flags}");
            }
        }
    }

    #[test]
    fn every_seed_s_table_answers_the_mailbox() {
        for h in Hook::all() {
            let table = h.build.regions.table;
            let p = h
                .patches(&progressive::LINES, &[], 0, &[], &[], None)
                .unwrap();
            let magic = p.iter().find(|p| p.at == table).unwrap();
            let body = p.iter().find(|p| p.at == table + 4).unwrap();
            let header = Header::read(&[&magic.bytes[..], &body.bytes].concat());
            assert_ne!(header.flags & DEV, 0);
        }
    }

    #[test]
    fn the_code_and_table_go_in_their_places_and_read_back() {
        for h in Hook::all() {
            let r = &h.build.regions;
            let p = h
                .patches(&progressive::LINES, &[], 0, &[], &[], None)
                .unwrap();
            let [code, far, body, magic, ..] = &p[TABLE_AT - 2..] else {
                panic!("patches")
            };
            assert_eq!((code.at, code.bytes.len()), (r.cave, h.code.len()));
            assert_eq!((far.at, &far.bytes[..h.far.len()]), (r.far, h.far));
            assert_eq!((magic.at, &magic.bytes[..]), (r.table, &MAGIC[..]));
            assert_eq!(magic.at % 8, 0, "the header is read in place");
            assert_eq!(body.at, r.table + 4);
            let bytes = [&magic.bytes[..], &body.bytes].concat();
            let header = Header::read(&bytes);
            let lines = &bytes[HEADER..][..header.size as usize];
            let table = Table::parse(&header, lines).unwrap();
            for line in progressive::LINES {
                let (_, read, _) = table.find(line[1].0.as_bytes()).unwrap();
                let read: Vec<_> = read.collect();
                let names: Vec<_> = read.iter().map(|l| (l.drop, l.pouch)).collect();
                let want: Vec<_> = line
                    .iter()
                    .map(|(d, p)| (d.as_bytes(), p.as_bytes()))
                    .collect();
                assert_eq!(names, want);
            }
        }
    }

    #[test]
    fn each_slot_branches_past_its_pacibsp_or_to_its_stub() {
        let word =
            |b: &[u8], at: u32| u32::from_le_bytes(b[at as usize..][..4].try_into().unwrap());
        for h in Hook::all() {
            let (b, cave) = (&h.build, h.build.regions.cave);
            let code = h.hook_code();
            let resumes = hooks(b).into_iter().map(|(at, _)| at + 4);
            let targets: Vec<_> = resumes.chain(npad_stubs(b)).collect();
            assert_eq!(targets.len(), hooks(b).len() + npad_calls(b).len());
            for (i, &to) in targets.iter().enumerate() {
                let slot = SLOTS + 4 * i as u32;
                // brk #0 until the patch fills it.
                assert_eq!(word(h.code, slot), 0xd420_0000, "slot {i}");
                assert_eq!(code[slot as usize..][..4], branch(cave + slot, to, false));
            }
            assert_eq!(code[..SLOTS as usize], h.code[..SLOTS as usize]);
            let end = SLOTS as usize + 4 * targets.len();
            assert_eq!(code[end..], h.code[end..]);
            // Past the last slot is code, not another slot left unfilled.
            assert_ne!(word(h.code, end as u32), 0xd420_0000);
        }
    }

    #[test]
    fn the_gate_skips_the_recorder() {
        for h in Hook::all() {
            let gate = &h
                .patches(&progressive::LINES, &[], 0, &[], &[], None)
                .unwrap()[0];
            assert_eq!(gate.at, h.build.branches.gate);
            // b gate_to, two words on
            assert_eq!(gate.bytes, 0x1400_0002u32.to_le_bytes());
        }
    }

    #[test]
    fn the_catch_skips_the_child_test() {
        for h in Hook::all() {
            let catch = &h
                .patches(&progressive::LINES, &[], 0, &[], &[], None)
                .unwrap()[1];
            assert_eq!(catch.at, h.build.branches.catch);
            // b catch_to, over tbz w9, #8, catch_orig
            assert_eq!(catch.bytes, 0x1400_0002u32.to_le_bytes());
        }
    }

    #[test]
    fn the_hook_never_uses_x18() {
        use yaxpeax_arch::{Decoder, U8Reader};
        use yaxpeax_arm::armv8::a64::InstDecoder;
        let x18 = |word: &[u8]| {
            let inst = InstDecoder::default()
                .decode(&mut U8Reader::new(word))
                .unwrap();
            let text = inst.to_string();
            let hit = text
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w == "x18" || w == "w18");
            hit.then_some(text)
        };
        // mov x18, x0 and ldrb w18, [x9, #0x12]
        assert!(x18(&0xaa00_03f2u32.to_le_bytes()).is_some());
        assert!(x18(&0x3940_4932u32.to_le_bytes()).is_some());
        for h in Hook::all() {
            for (at, bytes, code) in pieces(h) {
                for (i, word) in bytes[..code].chunks(4).enumerate() {
                    assert_eq!(x18(word), None, "at {:#x}", at as usize + i * 4);
                }
            }
        }
    }

    /// Each piece of `h`: where it goes, its bytes, and how many of them
    /// are code. The cave's strings follow its code.
    fn pieces(h: &Hook) -> [(u32, &'static [u8], usize); 2] {
        let code: usize = h.code_end.parse().unwrap();
        let r = &h.build.regions;
        [(r.cave, h.code, code), (r.far, h.far, h.far.len())]
    }

    #[test]
    fn page_relative_addresses_land_in_the_hook() {
        for h in Hook::all() {
            let hook =
                pieces(h).map(|(at, bytes, _)| i64::from(at)..i64::from(at) + bytes.len() as i64);
            for (base, bytes, code) in pieces(h) {
                page_relative_addresses_land_in(&hook, base, bytes, code);
            }
        }
    }

    fn page_relative_addresses_land_in(hook: &[Range<i64>], base: u32, bytes: &[u8], code: usize) {
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        for i in 0..code / 4 {
            let adrp = word(i);
            if adrp & 0x9f00_0000 != 0x9000_0000 {
                continue;
            }
            let pages = ((adrp >> 29) & 3) | (((adrp >> 5) & 0x7ffff) << 2);
            let pages = i64::from(((pages << 11) as i32) >> 11);
            let at = i64::from(base) + i as i64 * 4;
            let page = (at & !0xfff) + (pages << 12);
            // The `add` that finishes the address, from the same register.
            let rd = adrp & 0x1f;
            let add = (i + 1..(i + 8).min(code / 4))
                .map(word)
                .find(|w| w & 0xffc0_0000 == 0x9100_0000 && (w >> 5) & 0x1f == rd);
            if let Some(add) = add {
                let target = page + i64::from((add >> 10) & 0xfff);
                assert!(
                    hook.iter().any(|h| h.contains(&target)),
                    "adrp at {at:#x} to {target:#x}"
                );
            }
        }
    }

    #[test]
    fn calls_and_hooks_branch_to_the_hook() {
        for h in Hook::all() {
            let b = &h.build;
            let p = h.patches(&progressive::LINES, &[], ALL, &[], &[SPOT], None);
            let p = p.unwrap();
            assert_eq!(p[SITES_AT..], h.sites().collect::<Vec<_>>());
            let at = |i: usize| u32::from_le_bytes(p[SITES_AT + i].bytes[..].try_into().unwrap());
            assert_eq!(at(0) >> 26, 0b100101, "a call stays a bl");
            assert_eq!(
                at(calls(b).len() + npad_calls(b).len()) >> 26,
                0b000101,
                "a hook is a b"
            );
            let (run, entry) = run_call(b);
            let last = p.last().unwrap();
            assert_eq!(last.at, run);
            assert_eq!(last.bytes, branch(run, b.regions.cave + entry, true));
        }
        assert_eq!(branch(0x100, 0x104, true), 0x9400_0001u32.to_le_bytes());
        assert_eq!(branch(0x104, 0x100, true), 0x97ff_ffffu32.to_le_bytes());
        assert_eq!(branch(0x100, 0x108, false), 0x1400_0002u32.to_le_bytes());
        assert!(std::panic::catch_unwind(|| branch(0, 1 << 27, true)).is_err());
    }

    #[test]
    fn a_table_too_big_for_the_space_is_refused() {
        let name = "Item_".repeat(40);
        let big = vec![(name.as_str(), name.as_str()); 20];
        for h in Hook::all() {
            assert!(h.patches(&[&big], &[], 0, &[], &[], None).is_err());
        }
    }

    /// `main` as the Mac's game runs it: the dump's, but for the words it
    /// rewrote, which its cheats can't write over. A word that holds the
    /// dump's original here is one it left alone.
    pub(super) const PC_MAIN: &str =
        "../../dump/0400486024DEC000_build/patched_v131072/main.patched.bin";

    /// FNV-1a over 64 bits, enough to tell two patches apart.
    fn fingerprint(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// One hash for each thing `h`'s patch writes, for a fixed seed (every
    /// setting that adds a site, a spot, credits and the progressive
    /// lines), and how many of them hold without the dump: the seed's
    /// outputs carry game names, which only the dump gives, so without it
    /// they hold stand-ins and only the code is pinned.
    fn pins(h: &Hook) -> ([(&'static str, u64); 8], usize) {
        let credits = [
            ("Key_01", Credit::Bool(0x1234_5678)),
            ("Key_02", Credit::Int(0xdead_beef, -3)),
            ("Key_03", Credit::Add(0xfeed_face, 1)),
        ];
        let all = ALL | ICE_TRAPS | FREE_BOMBCHUS;
        let seed = (h.patches(&progressive::LINES, &[], all, &credits, &[SPOT], None)).unwrap();
        let flat = |p: &[Patch]| -> Vec<u8> {
            (p.iter())
                .flat_map(|p| {
                    p.at.to_le_bytes()
                        .into_iter()
                        .chain(p.bytes.iter().copied())
                })
                .collect()
        };
        let sites: Vec<_> = h.sites().collect();
        let got = [
            ("hook", fingerprint(h.code)),
            ("far", fingerprint(h.far)),
            ("code", fingerprint(&flat(&h.code_patches().unwrap()))),
            ("sites", fingerprint(&flat(&sites))),
            ("patches", fingerprint(&flat(&seed))),
            ("pchtxt", fingerprint(h.pchtxt(&seed).as_bytes())),
            ("ips", fingerprint(&files::ips(&seed))),
            ("cheat", fingerprint(files::cheat(&seed).as_bytes())),
        ];
        let pinned = if crate::game::no_names() { 4 } else { 8 };
        (got, pinned)
    }

    /// Pins the 1.1.1 patch: one hash for each thing the patch writes, for
    /// a fixed seed (every setting that adds a site, a spot, credits and
    /// the progressive lines). Moving an offset, or the table the offsets
    /// come from, changes none of them. A change that is meant to change
    /// the patch updates these in the same commit and says why.
    #[test]
    fn the_1_1_1_patch_is_pinned() {
        let (got, pinned) = pins(v111());
        let want = [
            ("hook", 0xe07e_c926_1640_d3a7),
            ("far", 0xc4ad_4cb3_788d_2dc6),
            ("code", 0x239a_7257_4280_4086),
            ("sites", 0x0209_eeb8_fec6_37d2),
            ("patches", 0xaaeb_45bb_d473_e72e),
            ("pchtxt", 0x1b45_94a8_d22e_dc95),
            ("ips", 0x0925_074c_306f_92d4),
            ("cheat", 0x2402_0a13_e2c6_c98d),
        ];
        assert_eq!(got[..pinned], want[..pinned], "{got:#x?}");
    }

    /// The same pins for 1.0.0's patch, the same code built against
    /// `V65536`.
    #[test]
    fn the_1_0_0_patch_is_pinned() {
        let (got, pinned) = pins(Hook::of(Release::V100));
        let want = [
            ("hook", 0x2b23_a33a_a53b_e220),
            ("far", 0x4a3b_2218_b11e_471a),
            ("code", 0x60a6_280c_eab2_6b39),
            ("sites", 0x6a43_a440_d683_4bc4),
            ("patches", 0xaacd_a342_08b9_47f9),
            ("pchtxt", 0xd015_27d2_4851_ab58),
            ("ips", 0x4b05_bbfd_066c_be5b),
            ("cheat", 0xaf80_8828_1c76_4918),
        ];
        assert_eq!(got[..pinned], want[..pinned], "{got:#x?}");
    }

    #[test]
    fn every_word_the_patch_writes_over_holds_what_we_expect() {
        if let Ok(main) = std::fs::read(PC_MAIN) {
            words_hold_what_we_expect(&main, v111());
            the_1_1_1_rooms_hold_what_we_expect(&main);
        }
        if let Some((_, main)) = crate::game::dumped_main(Release::V111) {
            words_hold_what_we_expect(main, v111());
            the_1_1_1_rooms_hold_what_we_expect(main);
        }
    }

    #[test]
    fn every_word_the_1_0_0_patch_writes_over_holds_what_we_expect() {
        let Some((_, main)) = crate::game::dumped_main(Release::V100) else {
            return;
        };
        words_hold_what_we_expect(main, Hook::of(Release::V100));
    }

    /// The checks on a build's `main` with its hook `h`: each site holds
    /// the word the patch expects to replace, each call goes where the
    /// table says, and the rooms are free and big enough.
    fn words_hold_what_we_expect(main: &[u8], h: &Hook) {
        let b = &h.build;
        let word = |at: u32| u32::from_le_bytes(main[at as usize..][..4].try_into().unwrap());
        let imm = |from: u32, to: u32, shift: u32| ((to - from) / 4) << shift;
        let (gate, (catch, _)) = (gate(b), catch(b));
        assert_eq!(
            word(gate.0),
            0x3400_0008 | imm(gate.0, gate.1, 5),
            "cbz w8, gate_to"
        );
        let orig = b.branches.catch_orig;
        assert_eq!(
            word(catch),
            0x3640_0009 | imm(catch, orig, 5),
            "tbz w9, #8, catch_orig"
        );
        let (boots, boots_to) = gear::boots(b);
        assert_eq!(
            word(boots),
            0x3600_0000 | imm(boots, boots_to, 5),
            "tbz w0, #0, boots_to"
        );
        assert_eq!(word(gear::hud(b).0), 0x3957_0108, "ldrb w8, [x8, #0x5c0]");
        let f = &b.funcs;
        let callees = [
            f.setter,
            f.setter,
            f.reader,
            f.reader,
            f.childhood,
            f.childhood,
            f.childhood,
            f.run_setter,
        ];
        for ((at, _), to) in calls(b).into_iter().chain([run_call(b)]).zip(callees) {
            assert_eq!(word(at).to_le_bytes(), branch(at, to, true), "{at:#x}");
        }
        for ((at, _), stub) in npad_calls(b).into_iter().zip(npad_stubs(b)) {
            assert_eq!(word(at).to_le_bytes(), branch(at, stub, true), "{at:#x}");
        }
        for (at, _) in hooks(b) {
            assert_eq!(word(at), 0xd503_237f, "{at:#x} isn't a pacibsp");
        }
        let r = &b.regions;
        assert!(
            r.cave_end - r.cave >= h.hook_code().len() as u32,
            "the cave"
        );
        assert!(
            r.far_end - r.far >= h.far.len() as u32,
            "the far code's room"
        );
        let table = &main[r.table as usize..r.table_end as usize];
        assert!(table.iter().all(|&b| b == 0), "the table's room");
    }

    /// The 1.1.1 checks no other build shares: the recorder and the far
    /// function as 1.1.1 has them, untouched by the Mac's game, and every
    /// patch starting at a place checked above.
    fn the_1_1_1_rooms_hold_what_we_expect(main: &[u8]) {
        let (h, r) = (v111(), &v111().build.regions);
        let b = &h.build;
        let room = |at: u32, end: u32| &main[at as usize..end as usize];
        assert_eq!(fnv(room(r.cave, r.cave_end)), 0xc734_cad6, "the cave");
        assert_eq!(
            fnv(room(r.far, r.far_end)),
            0x7aea_f248,
            "the far code's room"
        );
        let sites = (skips(b).into_iter())
            .chain(calls(b))
            .chain(npad_calls(b))
            .chain(hooks(b));
        let checked: Vec<u32> = (sites.map(|(at, _)| at))
            .chain([
                run_call(b).0,
                gear::hud(b).0,
                r.cave,
                r.far,
                r.table,
                r.table + 4,
            ])
            .collect();
        for p in h
            .patches(&progressive::LINES, &[], 0, &[], &[], None)
            .unwrap()
        {
            assert!(
                checked.contains(&p.at),
                "nothing checks what {:#x} holds",
                p.at
            );
        }
    }
}
