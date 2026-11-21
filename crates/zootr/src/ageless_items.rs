//! Items for both ages: each Link uses items only the other uses in the
//! game. Child Link takes the Megaton Hammer, Hookshot and Longshot, adult
//! Link the Deku Sticks and the Boomerang. The rest of the spec's items
//! join once they work (`superpowers/specs/2026-11-21-ageless-items-design.md`).
//!
//! Each gate is data, and each Link's state machine already has every
//! item's branches:
//!
//! - The pause menu's table lets the item be equipped at both ages.
//! - Link's holder makes the item's actor at load. Only an actor made
//!   there can be drawn.
//! - Link's shooter makes the other Link's projectiles: the hooks for the
//!   child, the Boomerang for the adult.
//! - Link's animations gain the other Link's archives, resized to his
//!   skeleton and renamed, beside his own.
//! - Child Link's effects are the adult's: the Hammer's dust and shake.
//!
//! The holder, shooter and animation files size their work from their
//! RESTBL entry, and each row costs more than its bytes, so each gets
//! [`SLACK`]. Without it, only one borrowed actor is made, and the
//! animation file's parse runs out of room and crashes the game at load.

use anyhow::{Context, Result, ensure};
use ounce_formats::{ActorPack, Byml, BymlDoc, BymlMap, ModBuilder, Romfs, pack_file};

use crate::game::POUCH_TABLE;
use crate::known::{Name, actor, field, item};
use crate::standins::{Edit, component_edited};

mod files;
mod fska;

pub use files::FILES;

/// The pause menu's items opened to both ages.
const ITEMS: [&Name; 5] = [
    item::HAMMER,
    item::HOOKSHOT,
    item::LONGSHOT,
    item::DEKU_STICK,
    item::BOOMERANG,
];

/// RESTBL room past a holder, shooter or animation file's new size (research
/// note, "The multi-copy limit was the RESTBL").
const SLACK: usize = 64 << 10;

/// The age word in a grown copy of a child archive's name, as long as
/// either Link's.
const GROWN: &str = "Grown";

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
    /// The borrowing Link's own: the adult's archive shrunk is the child's.
    Own,
    Grown,
}

/// What one Link borrows from the other.
struct Link {
    who: &'static Name,
    from: &'static Name,
    /// The other Link's holder rows, by key.
    holds: &'static [&'static Name],
    archives: &'static [Archive],
}

const CHILD: Link = Link {
    who: actor::PLAYER_CHILD,
    from: actor::PLAYER_ADULT,
    holds: &[item::HAMMER, item::HOOKSHOT, item::LONGSHOT],
    // The Hammer's 30 clips and the Hook's 87.
    archives: &[Archive {
        from: field::PLAYER_ADULT_02_H_ANIMATION,
        skeleton: field::PLAYER_CHILD_01_ANIMATION,
        word: Word::Own,
    }],
};

/// The Boomerang is a projectile, so only the Deku Stick is held.
const ADULT: Link = Link {
    who: actor::PLAYER_ADULT,
    from: actor::PLAYER_CHILD,
    holds: &[item::DEKU_STICK],
    // The Boomerang's 11 clips and the Deku Stick's 85, over three.
    archives: &[
        Archive {
            from: field::PLAYER_CHILD_01_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Grown,
        },
        Archive {
            from: field::PLAYER_CHILD_03_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Grown,
        },
        Archive {
            from: field::PLAYER_CHILD_04_ANIMATION,
            skeleton: field::PLAYER_ADULT_02_H_ANIMATION,
            word: Word::Grown,
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
            Word::Grown => GROWN,
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
    /// his skeleton and renamed. Its events are the original's.
    fn lent(&self, romfs: &Romfs, archive: &Archive) -> Result<(String, Vec<u8>)> {
        let (name, from) = (self.renamed(archive)?, archive.from.get());
        let mut bytes = romfs.read(&model(from))?;
        fska::resize(&mut bytes, &romfs.read(&model(archive.skeleton))?)?;
        fska::rename(&mut bytes, from, &name)?;
        Ok((name, bytes))
    }
}

/// Write the setting's files: the pause menu's table, both Links' packs,
/// and the borrowed archives.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder) -> Result<()> {
    let table = opened(&builder.read(POUCH_TABLE)?)?;
    builder.write(POUCH_TABLE, &table)?;
    for link in &LINKS {
        let pack = ActorPack::from_bytes(link.who, builder.read(&pack_file(link.who))?)?;
        let lender = ActorPack::load(romfs, link.from)?;
        let mut writer = pack.writer();
        for (path, old, new, slack) in borrowed(link, &pack, &lender)? {
            builder.resize(&path, old.len(), new.len() + slack)?;
            writer.add_file(path, new);
        }
        builder.write(&pack_file(link.who), &writer.to_binary())?;
        for archive in link.archives {
            let (name, bytes) = link.lent(romfs, archive)?;
            let from = archive.from.get();
            builder.add(&model(&name), &bytes, &model(from))?;
            builder.add(&events(&name), &romfs.read(&events(from))?, &events(from))?;
        }
    }
    Ok(())
}

/// The pause menu's table at `bytes` with each of [`ITEMS`] usable at both
/// ages.
fn opened(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut doc = BymlDoc::parse(bytes)?;
    let mut found = 0;
    for slots in doc.root.as_mut_map()?.values_mut() {
        let Byml::Array(slots) = slots else { continue };
        let actors = slots.iter_mut().filter_map(|slot| {
            match slot.as_mut_map().ok()?.get_mut(field::ACTOR.get()) {
                Some(Byml::Array(actors)) => Some(actors),
                _ => None,
            }
        });
        for actor in actors.flatten().filter_map(|a| a.as_mut_map().ok()) {
            let short = actor
                .get(field::ACTOR_SHORT.get())
                .and_then(|s| s.as_string().ok());
            if !short.is_some_and(|s| ITEMS.iter().any(|i| i.get() == s.as_str())) {
                continue;
            }
            for flag in [field::IS_USABLE_CHILD, field::IS_USABLE_ADULT] {
                actor.insert(flag.get().into(), Byml::Bool(true));
            }
            found += 1;
        }
    }
    ensure!(
        found >= ITEMS.len(),
        "the pause menu lacks some of the items"
    );
    Ok(doc.to_binary())
}

/// A file of a pack edited, and the RESTBL room it needs past its size.
type Borrowed = (String, Vec<u8>, Vec<u8>, usize);

/// `pack`'s files, as `link` borrows from `lender`: its holder, shooter
/// and animations, and the child's effects.
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
        let (path, old, new) = crate::ice_traps::adult_effects(pack)?;
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
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let open = opened(&vanilla).unwrap();
        let actors = |bytes: &[u8]| -> Vec<BymlMap> {
            let doc = BymlDoc::parse(bytes).unwrap();
            let mut found = vec![];
            for slots in doc.root.as_map().unwrap().values() {
                let Byml::Array(slots) = slots else { continue };
                for slot in slots.iter().filter_map(|s| s.as_map().ok()) {
                    if let Some(Byml::Array(a)) = slot.get(field::ACTOR.get()) {
                        found.extend(a.iter().filter_map(|a| a.as_map().ok()).cloned());
                    }
                }
            }
            found
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
    fn each_link_plays_the_lent_archives_and_the_child_the_adult_s_effects() {
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
            // The adult's own effects user is already his.
            let effects = file(&pack, field::E_LINK_REF);
            let user = effects[field::USER_NAME.get()].as_string().unwrap();
            assert_eq!(user.as_str(), actor::PLAYER_ADULT.get(), "{}", link.who);
        }
    }

    #[test]
    fn each_lent_archive_has_a_name_of_its_own_and_its_files_are_extracted() {
        let Some(romfs) = dev_dump() else { return };
        let mut names = HashSet::new();
        let mut read = vec![];
        for link in &LINKS {
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
        let files: HashSet<&str> = FILES.into_iter().collect();
        let read: HashSet<&str> = read.iter().map(String::as_str).collect();
        assert_eq!(read, files);
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
        assert!(holds.contains_key(item::HAMMER.get()), "ours");
        let conditions = file(&child, field::CONDITION_REF);
        let frozen = conditions.get(field::IS_ENABLE_FROZEN.get());
        assert_eq!(frozen, Some(&Byml::Bool(true)), "the ice traps'");
        drop(builder);
        let _ = std::fs::remove_dir_all(dir.with_file_name(format!(
            ".{}.part",
            dir.file_name().unwrap().to_string_lossy()
        )));
    }
}
