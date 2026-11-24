//! Items for both ages: each Link uses items only the other uses in the
//! game. Child Link takes the Megaton Hammer, Hookshot, Longshot, Bow,
//! magic arrows and the two-handed swords, adult Link the Deku Sticks, the
//! Slingshot and the Boomerang. The rest of the spec's items join once
//! they work (`superpowers/specs/2026-11-21-ageless-items-design.md`).
//!
//! Each gate is data, and each Link's state machine already has every
//! item's branches:
//!
//! - The pause menu's table lets the item be equipped at both ages. The
//!   two-handed swords' rows are opened by [`crate::ageless`], whose own
//!   setting leaves them out.
//! - Link's holder makes the item's actor at load. Only an actor made
//!   there can be drawn.
//! - Link's shooter makes the other Link's projectiles: the hooks for the
//!   child, the Boomerang for the adult.
//! - Link's animations gain the other Link's archives, resized to his
//!   skeleton and renamed, beside his own.
//! - Child Link takes the adult's effects, where the Hammer's dust and
//!   camera shake are.
//! - The hook's nock gives Link the drawn bow's projectile, so the child's
//!   Bow fires arrows, not his seeds (`zootr-hook`, `game/arrows.rs`).
//! - Each Link aims the Bow with the Bow's clips and the Slingshot with
//!   the Slingshot's ([`aim`]).
//! - Each Link's sounds gain the calls of what he borrows: the child's the
//!   Hammer's and the yells of the adult's two-handed swings, the adult's
//!   the Boomerang's. Each loads the other's sound bank ([`sounds`]).
//!
//! The holder, shooter and animation files size their work from their
//! RESTBL entry, and each row costs more than its bytes, so each gets
//! [`SLACK`]. Without it, only one borrowed actor is made, and the
//! animation file's parse runs out of room and crashes the game at load.

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_file};

use crate::game::POUCH_TABLE;
use crate::known::{Name, actor, field, item};
use crate::standins::{Edit, component_edited};

mod aim;
mod files;
mod fska;
mod sounds;

pub use files::{BANKS, FILES, SOUNDS};

/// The pause menu's items opened to both ages.
const ITEMS: [&Name; 15] = [
    // The Bow's four slots, each an arrow row and a loaded bow row.
    item::ARROW_NORMAL,
    actor::WEAPON_RANGE_BOW,
    item::ARROW_FIRE,
    actor::POUCH_RANGE_BOW_AND_ARROW_FIRE,
    item::ARROW_ICE,
    actor::POUCH_RANGE_BOW_AND_ARROW_ICE,
    item::ARROW_LIGHT,
    actor::POUCH_RANGE_BOW_AND_ARROW_LIGHT,
    item::HAMMER,
    item::HOOKSHOT,
    item::LONGSHOT,
    item::DEKU_STICK,
    item::BOOMERANG,
    item::DEKU_SEED,
    actor::WEAPON_RANGE_SLING_SHOT,
];

/// RESTBL room past a holder, shooter or animation file's new size (research
/// note, "The multi-copy limit was the RESTBL").
const SLACK: usize = 64 << 10;

/// The age words in a grown copy of a child archive's name and a shrunk
/// copy of an adult one, as long as either Link's.
const GROWN: &str = "Grown";
const SMALL: &str = "Small";

/// An archive a Link borrows from the other: `from`, resized to the
/// skeleton the other Link's archive `skeleton` has, renamed with `word`
/// for the other Link's age word.
struct Archive {
    from: &'static Name,
    skeleton: &'static Name,
    word: Word,
}

/// The age word a borrowed archive's name takes.
enum Word {
    /// The borrowing Link's own, for an archive he has no copy of.
    Own,
    /// Another, beside his own archive of the same number.
    Other(&'static str),
}

/// What one Link borrows from the other.
struct Link {
    who: &'static Name,
    from: &'static Name,
    /// The other Link's holder rows, by key.
    holds: &'static [&'static Name],
    /// The words the borrowed items' clips carry in their names: the long
    /// roll with the hook is the plain long roll's name with `_Hook` added,
    /// with a two-handed sword `_LSword`.
    words: &'static [&'static str],
    archives: &'static [Archive],
}

const CHILD: Link = Link {
    who: actor::PLAYER_CHILD,
    from: actor::PLAYER_ADULT,
    holds: &[
        item::HAMMER,
        item::HOOKSHOT,
        item::LONGSHOT,
        actor::WEAPON_RANGE_BOW,
    ],
    words: &["Hook", "Hammer", "Bow", "LSword"],
    // Every one of the adult's: a held item's clips are spread over all
    // six (the Hook's draw in 01, lock-on in 02, its shots in 02_H, poses
    // and rolls in 03, waits in 04, a two-handed sword's charge start in
    // Cba). A state whose clip is missing plays nothing, and Link twitches
    // or skips it: without Cba the charge skips its start, and a tap of the
    // sword spins.
    archives: &[
        Archive {
            from: field::PLAYER_ADULT_01_ANIMATION,
            skeleton: field::PLAYER_CHILD_01_ANIMATION,
            word: Word::Other(SMALL),
        },
        Archive {
            from: field::PLAYER_ADULT_02_ANIMATION,
            skeleton: field::PLAYER_CHILD_01_ANIMATION,
            word: Word::Other(SMALL),
        },
        Archive {
            from: field::PLAYER_ADULT_02_H_ANIMATION,
            skeleton: field::PLAYER_CHILD_01_ANIMATION,
            word: Word::Own,
        },
        Archive {
            from: field::PLAYER_ADULT_03_ANIMATION,
            skeleton: field::PLAYER_CHILD_01_ANIMATION,
            word: Word::Other(SMALL),
        },
        Archive {
            from: field::PLAYER_ADULT_04_ANIMATION,
            skeleton: field::PLAYER_CHILD_01_ANIMATION,
            word: Word::Other(SMALL),
        },
        Archive {
            from: field::PLAYER_ADULT_CBA_ANIMATION,
            skeleton: field::PLAYER_CHILD_01_ANIMATION,
            word: Word::Other(SMALL),
        },
    ],
};

/// The Deku Stick and the Slingshot are held. The Boomerang is a
/// projectile, so it isn't.
const ADULT: Link = Link {
    who: actor::PLAYER_ADULT,
    from: actor::PLAYER_CHILD,
    holds: &[item::DEKU_STICK, actor::WEAPON_RANGE_SLING_SHOT],
    words: &["DekuStick", "Boomerang", "Sling"],
    // Every one of the child's, as for the child.
    archives: &[
        Archive {
            from: field::PLAYER_CHILD_01_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Other(GROWN),
        },
        Archive {
            from: field::PLAYER_CHILD_02_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Other(GROWN),
        },
        Archive {
            from: field::PLAYER_CHILD_03_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Other(GROWN),
        },
        Archive {
            from: field::PLAYER_CHILD_04_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Other(GROWN),
        },
        Archive {
            from: field::PLAYER_CHILD_CBA_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Other(GROWN),
        },
    ],
};

const LINKS: [Link; 2] = [CHILD, ADULT];

fn model(archive: &str) -> String {
    format!("Model/{archive}.anim.bfres.zs")
}

fn events(archive: &str) -> String {
    format!("AnimationEvent/Animation/{archive}.anim.baev.zs")
}

/// A Link's age word: `Child` from child Link's name.
fn age(link: &Name) -> &'static str {
    &link.get()[link.stem().len()..]
}

impl Link {
    /// The name `archive` takes for this Link: its age word swapped, so
    /// it's as long as the original and sits beside it.
    fn renamed(&self, archive: &Archive) -> Result<String> {
        let word = match archive.word {
            Word::Own => age(self.who),
            Word::Other(word) => word,
        };
        let from = archive.from.get();
        let name = from.replacen(age(self.from), word, 1);
        ensure!(
            name != from && name.len() == from.len(),
            "{from} can't be renamed {word}"
        );
        Ok(name)
    }

    /// `archive`'s model as this Link has it, and its name: resized to
    /// his skeleton, his eyelids left to him, each clip of his `own`
    /// hidden, each item clip given the root of his plain one, and
    /// renamed. Its events are the original's.
    fn lent(&self, romfs: &Romfs, archive: &Archive, own: &Own) -> Result<(String, Vec<u8>)> {
        let (name, from) = (self.renamed(archive)?, archive.from.get());
        let mut bytes = romfs.read(&model(from))?;
        fska::resize(&mut bytes, &romfs.read(&model(archive.skeleton))?)?;
        fska::unbind_eyelids(&mut bytes);
        fska::hide(&mut bytes, |clip| own.clips.contains_key(clip));
        fska::rooted(&mut bytes, |clip| own.plain(self, clip))?;
        fska::rename(&mut bytes, from, &name)?;
        Ok((name, bytes))
    }
}

/// A Link's own archives, and each clip in them: the archive, and where
/// the clip's header is.
struct Own {
    archives: Vec<Vec<u8>>,
    clips: HashMap<String, (usize, usize)>,
}

impl Own {
    /// Read the archives `pack` plays.
    fn read(romfs: &Romfs, pack: &ActorPack) -> Result<Self> {
        let archives: Vec<Vec<u8>> = (own_archives(pack)?.iter())
            .map(|name| romfs.read(&model(name)))
            .collect::<Result<_>>()?;
        let clips = (archives.iter().enumerate())
            .flat_map(|(i, d)| fska::clips(d).map(move |(clip, at)| (clip.to_string(), (i, at))))
            .collect();
        Ok(Self { archives, clips })
    }

    /// The plain clip `link` has for the item clip `clip`, its item word
    /// left out: its archive, and where its header is.
    fn plain(&self, link: &Link, clip: &str) -> Option<(&[u8], usize)> {
        link.words.iter().find_map(|word| {
            let plain = clip.replacen(&format!("_{word}"), "", 1);
            let &(i, at) = self.clips.get(&plain).filter(|_| plain != clip)?;
            Some((&self.archives[i][..], at))
        })
    }
}

/// Write the setting's files: the pause menu's table, both Links' packs,
/// the borrowed archives, both Links' graphs and the sounds.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let mut table = BymlDoc::parse(&builder.read(POUCH_TABLE)?)?;
    opened(&mut table.root)?;
    crate::ageless::two_handed_opened(romfs, &mut table.root)?;
    builder.write(POUCH_TABLE, &table.to_binary())?;
    for link in &LINKS {
        let pack = ActorPack::from_bytes(link.who, builder.read(&pack_file(link.who))?)?;
        let lender = ActorPack::load(romfs, link.from)?;
        let own = Own::read(romfs, &pack)?;
        let mut writer = pack.writer();
        for (path, old, new, slack) in borrowed(link, &pack, &lender)? {
            builder.resize(&path, old.len(), new.len() + slack)?;
            writer.add_file(path, new);
        }
        builder.write(&pack_file(link.who), &writer.to_binary())?;
        for archive in link.archives {
            let (name, bytes) = link.lent(romfs, archive, &own)?;
            let from = archive.from.get();
            builder.add(&model(&name), &bytes, &model(from))?;
            builder.add(&events(&name), &romfs.read(&events(from))?, &events(from))?;
        }
    }
    for graph in files::GRAPHS {
        builder.write(graph, &aim::aimed(&builder.read(graph)?)?)?;
    }
    sounds::write(builder)
}

/// The archives `pack` plays, by name.
fn own_archives(pack: &ActorPack) -> Result<Vec<String>> {
    let animations = crate::standins::resolved(pack, &component_path(pack, field::ANIMATION_REF)?)?;
    (array(&animations, field::ANIMATION_RESOURCES)?.iter())
        .map(|archive| {
            let name = (archive.as_map()?.get(field::MODEL_PROJECT_NAME.get()))
                .with_context(|| format!("an archive of {} has no name", pack.name))?;
            Ok(name.as_string()?.to_string())
        })
        .collect()
}

/// Makes each of [`ITEMS`] usable at both ages in `root`, the pause menu's
/// table, on every page it shows on.
fn opened(root: &mut Byml) -> Result<()> {
    let mut found = [false; ITEMS.len()];
    for actor in crate::ageless::rows(root, None)? {
        let short = crate::ageless::name(actor);
        let Some(i) = short.and_then(|s| ITEMS.iter().position(|i| i.get() == s)) else {
            continue;
        };
        crate::ageless::open(actor);
        found[i] = true;
    }
    let missing: Vec<&str> = (ITEMS.iter().zip(found))
        .filter(|(_, found)| !found)
        .map(|(i, _)| i.get())
        .collect();
    ensure!(
        missing.is_empty(),
        "the pause menu lacks {}",
        missing.join(", ")
    );
    Ok(())
}

/// A file of a pack edited, and the RESTBL room it needs past its size.
type Borrowed = (String, Vec<u8>, Vec<u8>, usize);

/// `pack`'s files, as `link` borrows from `lender`: its holder, shooter
/// and animations.
fn borrowed(link: &Link, pack: &ActorPack, lender: &ActorPack) -> Result<Vec<Borrowed>> {
    let param = param(pack)?;
    let theirs = |key: &Name| -> Result<BymlMap> {
        let at = component_path(lender, key)?;
        crate::standins::resolved(lender, &at)
    };
    let held = theirs(field::ACTOR_HOLDER_REF)?;
    let held = map(&held, field::ACTOR_HOLD_SETTINGS)?;
    let holder = component_edited(pack, &param, field::ACTOR_HOLDER_REF, |_, own| {
        let rows = own_map(own, field::ACTOR_HOLD_SETTINGS)?;
        for key in link.holds {
            let row = held
                .get(key.get())
                .with_context(|| format!("{} holds no {key}", link.from))?;
            rows.insert(key.get().into(), row.clone());
        }
        Ok(())
    })?;
    let shot = theirs(field::SHOOTER_REF)?;
    let shot = array(&shot, field::SHOOTABLE_ACTOR_SETTINGS)?;
    let shooter = component_edited(pack, &param, field::SHOOTER_REF, |_, own| {
        let rows = own_array(own, field::SHOOTABLE_ACTOR_SETTINGS)?;
        let mine: Vec<Byml> = rows.iter().filter_map(shoots).cloned().collect();
        rows.extend(
            shot.iter()
                .filter(|r| shoots(r).is_some_and(|a| !mine.contains(a)))
                .cloned(),
        );
        Ok(())
    })?;
    let names: Vec<String> = (link.archives.iter())
        .map(|a| link.renamed(a))
        .collect::<Result<_>>()?;
    let animations = component_edited(pack, &param, field::ANIMATION_REF, |_, own| {
        let list = own_array(own, field::ANIMATION_RESOURCES)?;
        for name in &names {
            let mut archive = BymlMap::default();
            archive.insert(
                field::MODEL_PROJECT_NAME.get().into(),
                Byml::String(name.as_str().into()),
            );
            list.push(Byml::Map(archive));
        }
        Ok(())
    })?;
    let roomy = |(path, old, new): Edit| (path, old, new, SLACK);
    let mut edits = vec![roomy(holder), roomy(shooter), roomy(animations)];
    if link.who == CHILD.who {
        let (path, old, new) = crate::ice_traps::adult_effects(pack, &param)?;
        edits.push((path, old, new, 0));
    }
    Ok(edits)
}

/// The actor a shooter row makes.
fn shoots(row: &Byml) -> Option<&Byml> {
    row.as_map().ok()?.get(field::ACTOR.get())
}

fn param(pack: &ActorPack) -> Result<BymlMap> {
    pack.actor_param()?
        .with_context(|| format!("{} has no {}", pack.name, field::ACTOR_PARAM))
}

fn component_path(pack: &ActorPack, key: &Name) -> Result<String> {
    crate::standins::path_of(pack, key.get())
}

fn map<'a>(m: &'a BymlMap, key: &Name) -> Result<&'a BymlMap> {
    Ok(m.get(key.get())
        .with_context(|| format!("no {key}"))?
        .as_map()?)
}

fn array<'a>(m: &'a BymlMap, key: &Name) -> Result<&'a [Byml]> {
    match m.get(key.get()) {
        Some(Byml::Array(a)) => Ok(a),
        _ => anyhow::bail!("no {key}"),
    }
}

fn own_map<'a>(m: &'a mut BymlMap, key: &Name) -> Result<&'a mut BymlMap> {
    Ok(m.get_mut(key.get())
        .with_context(|| format!("no {key}"))?
        .as_mut_map()?)
}

fn own_array<'a>(m: &'a mut BymlMap, key: &Name) -> Result<&'a mut Vec<Byml>> {
    match m.get_mut(key.get()) {
        Some(Byml::Array(a)) => Ok(a),
        _ => anyhow::bail!("no {key}"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::game::dev_dump;

    /// The value under `key` in `pack`'s component file `at`, resolved.
    fn file(pack: &ActorPack, at: &Name) -> BymlMap {
        crate::standins::resolved(pack, &component_path(pack, at).unwrap()).unwrap()
    }

    /// `pack` with `link`'s borrowed files in it.
    fn borrowing(romfs: &Romfs, link: &Link) -> ActorPack {
        let pack = ActorPack::load(romfs, link.who).unwrap();
        let lender = ActorPack::load(romfs, link.from).unwrap();
        let mut writer = pack.writer();
        for (path, _, new, _) in borrowed(link, &pack, &lender).unwrap() {
            writer.add_file(path, new);
        }
        ActorPack::from_bytes(link.who, writer.to_binary()).unwrap()
    }

    /// The actors `pack`'s shooter makes.
    fn shot(pack: &ActorPack) -> HashSet<String> {
        let file = file(pack, field::SHOOTER_REF);
        let rows = array(&file, field::SHOOTABLE_ACTOR_SETTINGS).unwrap();
        (rows.iter().filter_map(shoots))
            .map(|a| a.as_string().unwrap().to_string())
            .collect()
    }

    #[test]
    fn the_menu_opens_the_items_to_both_ages_and_nothing_else() {
        let Some(romfs) = dev_dump() else { return };
        let vanilla = BymlDoc::parse(&romfs.read(POUCH_TABLE).unwrap())
            .unwrap()
            .root;
        let mut open = vanilla.clone();
        opened(&mut open).unwrap();
        let actors = |root: &Byml| -> Vec<BymlMap> {
            let mut root = root.clone();
            let rows = crate::ageless::rows(&mut root, None).unwrap();
            rows.into_iter().map(|a| a.clone()).collect()
        };
        let (was, now) = (actors(&vanilla), actors(&open));
        assert_eq!(was.len(), now.len());
        let mut opened = HashSet::new();
        for (was, now) in was.iter().zip(&now) {
            let short = now[field::ACTOR_SHORT.get()]
                .as_string()
                .unwrap()
                .to_string();
            match ITEMS.iter().any(|i| i.get() == short) {
                true => {
                    for flag in [field::IS_USABLE_CHILD, field::IS_USABLE_ADULT] {
                        assert_eq!(now.get(flag.get()), Some(&Byml::Bool(true)), "{short}");
                    }
                    opened.insert(short);
                }
                false => assert_eq!(was, now, "{short}"),
            }
        }
        assert_eq!(opened.len(), ITEMS.len());
    }

    #[test]
    fn the_menu_check_counts_items_not_rows() {
        let Some(romfs) = dev_dump() else { return };
        // Every row of the first item renamed to the second: as many rows
        // match as before, but the first item is gone.
        let mut doc = BymlDoc::parse(&romfs.read(POUCH_TABLE).unwrap()).unwrap();
        let mut renamed = 0;
        for actor in crate::ageless::rows(&mut doc.root, None).unwrap() {
            if crate::ageless::name(actor) == Some(ITEMS[0].get()) {
                actor.insert(
                    field::ACTOR_SHORT.get().into(),
                    Byml::String(ITEMS[1].get().into()),
                );
                renamed += 1;
            }
        }
        assert!(renamed > 0);
        let error = opened(&mut doc.root).unwrap_err().to_string();
        assert!(error.contains(ITEMS[0].get()), "{error}");
    }

    #[test]
    fn each_link_holds_the_borrowed_items_and_both_shoot_alike() {
        let Some(romfs) = dev_dump() else { return };
        let [child, adult] = LINKS.map(|l| borrowing(&romfs, &l));
        for (pack, link) in [(&child, &CHILD), (&adult, &ADULT)] {
            let lender = ActorPack::load(&romfs, link.from).unwrap();
            let theirs = file(&lender, field::ACTOR_HOLDER_REF);
            let theirs = map(&theirs, field::ACTOR_HOLD_SETTINGS).unwrap();
            let mine = file(pack, field::ACTOR_HOLDER_REF);
            let mine = map(&mine, field::ACTOR_HOLD_SETTINGS).unwrap();
            for key in link.holds {
                assert_eq!(mine.get(key.get()), theirs.get(key.get()), "{key}");
            }
        }
        let vanilla = LINKS.map(|l| shot(&ActorPack::load(&romfs, l.who).unwrap()));
        let union: HashSet<String> = vanilla.iter().flatten().cloned().collect();
        assert!(
            vanilla.iter().all(|v| v.len() < union.len()),
            "each lacks some"
        );
        assert_eq!(shot(&child), union);
        assert_eq!(shot(&adult), union);
    }

    #[test]
    fn each_link_plays_the_lent_archives() {
        let Some(romfs) = dev_dump() else { return };
        for link in &LINKS {
            let pack = borrowing(&romfs, link);
            let animations = file(&pack, field::ANIMATION_REF);
            let list = array(&animations, field::ANIMATION_RESOURCES).unwrap();
            let names: Vec<String> = (list.iter().rev().take(link.archives.len()))
                .map(|a| {
                    a.as_map().unwrap()[field::MODEL_PROJECT_NAME.get()]
                        .as_string()
                        .unwrap()
                        .to_string()
                })
                .collect();
            let want: Vec<String> = (link.archives.iter().rev())
                .map(|a| link.renamed(a).unwrap())
                .collect();
            assert_eq!(names, want);
        }
    }

    /// Cba was missed once: the child's two-handed charge skipped its start
    /// and every tap spun.
    #[test]
    fn each_link_borrows_every_archive_the_other_plays_but_cutscenes() {
        let Some(romfs) = dev_dump() else { return };
        for link in &LINKS {
            let lender = ActorPack::load(&romfs, link.from).unwrap();
            let theirs: HashSet<String> = (own_archives(&lender).unwrap().into_iter())
                .filter(|a| a.contains(age(link.from)) && !a.contains("_Demo_"))
                .collect();
            let lent: HashSet<String> = (link.archives.iter())
                .map(|a| a.from.get().to_string())
                .collect();
            assert_eq!(lent, theirs, "{}", link.who);
        }
    }

    #[test]
    fn each_lent_archive_has_a_name_of_its_own_and_its_files_are_extracted() {
        let Some(romfs) = dev_dump() else { return };
        let mut names = HashSet::new();
        let mut read = vec![];
        for link in &LINKS {
            let pack = ActorPack::load(&romfs, link.who).unwrap();
            read.extend(own_archives(&pack).unwrap().iter().map(|a| model(a)));
            for archive in link.archives {
                let name = link.renamed(archive).unwrap();
                assert_eq!(name.len(), archive.from.len());
                assert!(romfs.read(&model(&name)).is_err(), "{name} ships");
                assert!(names.insert(name));
                read.extend([
                    model(archive.from),
                    events(archive.from),
                    model(archive.skeleton),
                ]);
            }
        }
        read.extend([SOUNDS, BANKS].map(String::from));
        read.extend(files::GRAPHS.map(String::from));
        let files: HashSet<&str> = FILES.into_iter().collect();
        let read: HashSet<&str> = read.iter().map(String::as_str).collect();
        assert_eq!(read, files);
    }

    #[test]
    fn a_lent_archive_hides_every_clip_its_link_has_and_keeps_the_rest() {
        let Some(romfs) = dev_dump() else { return };
        for link in &LINKS {
            let mine = Own::read(&romfs, &ActorPack::load(&romfs, link.who).unwrap()).unwrap();
            let own = &mine.clips;
            assert!(own.len() > 1000, "{}: {} clips", link.who, own.len());
            for archive in link.archives {
                let shipped = romfs.read(&model(archive.from)).unwrap();
                let shipped: Vec<&str> = fska::clips(&shipped).map(|(c, _)| c).collect();
                let (_, lent) = link.lent(&romfs, archive, &mine).unwrap();
                let lent: Vec<&str> = fska::clips(&lent).map(|(c, _)| c).collect();
                assert_eq!(lent.len(), shipped.len());
                for (was, now) in shipped.iter().zip(&lent) {
                    match own.contains_key(*was) {
                        true => assert_eq!(now.as_bytes()[0], b'~', "{was}"),
                        false => assert_eq!(was, now),
                    }
                }
                assert!(lent.iter().all(|c| !own.contains_key(*c)));
            }
        }
    }

    #[test]
    fn a_lent_item_clip_moves_as_its_links_plain_one() {
        let Some(romfs) = dev_dump() else { return };
        for link in &LINKS {
            let own = Own::read(&romfs, &ActorPack::load(&romfs, link.who).unwrap()).unwrap();
            let (mut rooted, mut two_handed) = (0, 0);
            for archive in link.archives {
                let (_, lent) = link.lent(&romfs, archive, &own).unwrap();
                for (clip, at) in fska::clips(&lent) {
                    let Some((plain, from)) = own.plain(link, clip) else {
                        continue;
                    };
                    let (mine, theirs) =
                        (fska::root_bytes(&lent, at), fska::root_bytes(plain, from));
                    if !theirs.is_empty() {
                        assert_eq!(mine, theirs, "{clip}");
                        rooted += 1;
                        two_handed += usize::from(clip.contains("_LSword"));
                    }
                }
            }
            assert!(rooted >= 18, "{}: {rooted}", link.who);
            // The child's 12 rolls and 2 climb-ups with a two-handed
            // sword: the camera trailed him in each.
            let want = if link.who == actor::PLAYER_CHILD {
                14
            } else {
                0
            };
            assert!(two_handed >= want, "{}: {two_handed}", link.who);
        }
    }

    /// Each map's hasher is seeded anew, so a lend that followed a map's
    /// order took room in a different order each time.
    #[test]
    fn a_lend_writes_the_same_bytes_every_time() {
        let Some(romfs) = dev_dump() else { return };
        for link in &LINKS {
            let own = Own::read(&romfs, &ActorPack::load(&romfs, link.who).unwrap()).unwrap();
            for archive in link.archives {
                let (_, first) = link.lent(&romfs, archive, &own).unwrap();
                for _ in 0..3 {
                    let (_, again) = link.lent(&romfs, archive, &own).unwrap();
                    assert!(first == again, "{}", archive.from);
                }
            }
        }
    }

    fn effects_user(pack: &ActorPack) -> String {
        let effects = file(pack, field::E_LINK_REF);
        effects[field::USER_NAME.get()]
            .as_string()
            .unwrap()
            .to_string()
    }

    #[test]
    fn child_link_takes_the_adults_effects_and_the_adult_keeps_his() {
        let Some(romfs) = dev_dump() else { return };
        let child = borrowing(&romfs, &CHILD);
        assert_eq!(effects_user(&child), actor::PLAYER_ADULT.get());
        let shipped = ActorPack::load(&romfs, ADULT.who).unwrap();
        assert_eq!(
            effects_user(&borrowing(&romfs, &ADULT)),
            effects_user(&shipped)
        );
    }

    #[test]
    fn with_ice_traps_too_child_link_keeps_both_settings_edits() {
        let Some(romfs) = dev_dump() else { return };
        let dir = std::env::temp_dir().join(format!("zootr-ageless-items-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        write(&romfs, &mut builder).unwrap();
        crate::ice_traps::thaw(&mut builder).unwrap();
        let child = builder.read(&pack_file(CHILD.who)).unwrap();
        let child = ActorPack::from_bytes(CHILD.who, child).unwrap();
        let holder = file(&child, field::ACTOR_HOLDER_REF);
        let holds = map(&holder, field::ACTOR_HOLD_SETTINGS).unwrap();
        assert!(holds.contains_key(item::HOOKSHOT.get()), "ours");
        let conditions = file(&child, field::CONDITION_REF);
        let frozen = conditions.get(field::IS_ENABLE_FROZEN.get());
        assert_eq!(frozen, Some(&Byml::Bool(true)), "the ice traps'");
        assert_eq!(effects_user(&child), actor::PLAYER_ADULT.get(), "both");
        drop(builder);
        let _ = std::fs::remove_dir_all(dir.with_file_name(format!(
            ".{}.part",
            dir.file_name().unwrap().to_string_lossy()
        )));
    }
}
