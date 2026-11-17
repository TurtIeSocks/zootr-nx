//! Items for another player's world, in an Archipelago seed. Each is a
//! copy of the item it shows, the way a key ring is a copy of the small
//! key: its own name, the small key's pouch content and row, and a plain
//! deal, so the game stores it as nothing Link can use and counts nothing.
//! Link gets the check's flag and the get, which names the item and its
//! player. The app's client sends the item on.

use anyhow::{Result, ensure};
use ounce_formats::{ActorPack, BymlDoc, ModBuilder, Romfs};

use crate::key_tints::{POUCH_TEXT, TEXT_PARTS};
use crate::keys::SMALL;
use crate::standins::{
    POUCH, POUCH_ACTOR, Tables, actor_path, doc, path_of, pouch_actor, raw, renamed, resolved,
    with_ref,
};
use crate::text::{Copies, CopyLine, Says};

/// One item for another player, at a check.
#[derive(Clone, Debug, PartialEq)]
pub struct Foreign {
    /// The copy's actor, from [`actor`].
    pub actor: String,
    /// The item whose model it shows: the item itself when it's ours.
    pub shows: String,
    /// The item as its game names it.
    pub item: String,
    /// The player it goes to.
    pub player: String,
}

const PREFIX: &str = "Item_Rando_Ap";

/// The `n`th copy's actor.
pub fn actor(n: usize) -> String {
    format!("{PREFIX}{n:04}")
}

/// Whether `item` is a copy for another player.
pub fn is_foreign(item: &str) -> bool {
    item.starts_with(PREFIX)
}

/// The label naming `item`, if it's a copy: like a key ring, it answers
/// to its own name, whose lines [`captions`] writes.
pub fn name_label(item: &str) -> Option<String> {
    is_foreign(item).then(|| format!("{item}{}", TEXT_PARTS[0]))
}

/// The longest a player's or item's name reads in a get, past which it's
/// cut, so the window holds it.
const LONGEST: usize = 32;

/// `name` without control characters, cut to [`LONGEST`].
fn clean(name: &str) -> String {
    let kept: String = name.chars().filter(|c| !c.is_control()).collect();
    let kept = kept.trim();
    match kept.chars().count() > LONGEST {
        true => format!("{}…", kept.chars().take(LONGEST - 1).collect::<String>()),
        false => kept.to_owned(),
    }
}

impl Foreign {
    /// What the item list and spoiler call it.
    pub fn name(&self) -> String {
        format!("{} for {}", clean(&self.item), clean(&self.player))
    }

    /// Each of [`TEXT_PARTS`]' line, in every language: AP's item and
    /// player names have no translations.
    fn lines(&self) -> [String; 3] {
        let (item, player) = (clean(&self.item), clean(&self.player));
        [
            self.name(),
            format!("You found {item} for {player}!"),
            format!("Archipelago sends it to {player}."),
        ]
    }
}

/// Every copy's get text, under its own name.
pub fn captions(foreign: &[Foreign]) -> Copies<'static> {
    let new = foreign.iter().flat_map(|f| {
        TEXT_PARTS
            .iter()
            .zip(f.lines())
            .map(|(part, line)| CopyLine {
                label: format!("{}{part}", f.actor),
                sources: vec![format!("{}{part}", SMALL.get())],
                says: Says::Text(line),
            })
    });
    Copies {
        file: POUCH_TEXT,
        new: new.collect(),
    }
}

/// The copy's pack, built from the dump, if `item` is one of `foreign`.
pub fn pack(romfs: &Romfs, item: &str, foreign: &[Foreign]) -> Result<Option<ActorPack>> {
    let Some(f) = foreign.iter().find(|f| f.actor == item) else {
        return Ok(None);
    };
    let built = Built::new(romfs, f)?;
    Ok(Some(ActorPack::from_bytes(item, built.pack()?)?))
}

/// One copy: the shown item's pack under the copy's name, giving the small
/// key's pouch content, which names the copy.
struct Built<'a> {
    foreign: &'a Foreign,
    shown: ActorPack,
    key: ActorPack,
    /// Each new file: its path, contents, and the file and pack it's sized
    /// like in the RESTBL.
    files: Vec<(String, BymlDoc, String, bool)>,
}

impl<'a> Built<'a> {
    fn new(romfs: &Romfs, foreign: &'a Foreign) -> Result<Self> {
        let shown = ActorPack::load(romfs, &foreign.shows)?;
        let key = ActorPack::load(romfs, SMALL)?;
        let name = foreign.actor.as_str();
        let pouch_like = path_of(&key, POUCH)?;
        let pouch_at = renamed(&pouch_like, name);
        let pouch_doc = BymlDoc::parse(&raw(&key, &pouch_like)?)?;
        let mut pouch = resolved(&key, &pouch_like)?;
        pouch.insert(POUCH_ACTOR.get().into(), pouch_actor(name));
        let actor_like = actor_path(&shown.name);
        let actor_doc = BymlDoc::parse(&raw(&shown, &actor_like)?)?;
        let actor = with_ref(actor_doc.root.as_map()?, POUCH, &pouch_at)?;
        let files = vec![
            (actor_path(name), doc(actor, &actor_doc), actor_like, false),
            (pouch_at, doc(pouch, &pouch_doc), pouch_like, true),
        ];
        Ok(Self {
            foreign,
            shown,
            key,
            files,
        })
    }

    /// The pack the file at `like` comes from.
    fn like_pack(&self, from_key: bool) -> &ActorPack {
        match from_key {
            true => &self.key,
            false => &self.shown,
        }
    }

    fn pack(&self) -> Result<Vec<u8>> {
        let mut writer = self.shown.writer();
        for (path, data, ..) in &self.files {
            writer.add_file(path.clone(), data.to_binary());
        }
        Ok(writer.to_binary())
    }

    fn write(&self, builder: &mut ModBuilder) -> Result<()> {
        for (path, data, like, from_key) in &self.files {
            let like_size = raw(self.like_pack(*from_key), like)?.len();
            builder.add_entry(path, like, like_size, data.to_binary().len())?;
        }
        builder.add(
            &format!("Pack/Actor/{}.pack.zs", self.foreign.actor),
            &self.pack()?,
            &format!("Pack/Actor/{}.pack.zs", self.shown.name),
        )
    }
}

/// Writes each copy and its rows: the shown item's actor rows, the small
/// key's pouch row with a plain deal, answering to its own name.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    foreign: &[Foreign],
) -> Result<()> {
    for f in foreign {
        Built::new(romfs, f)?.write(builder)?;
        tables.copy(&f.actor, &f.shows)?;
        tables.copy_pouch(&f.actor, SMALL)?;
        tables.name_itself(&f.actor)?;
    }
    let names: Vec<String> = foreign.iter().map(|f| f.actor.clone()).collect();
    let plain = tables.plain_deal(&names)?;
    ensure!(plain == names.len(), "{plain} copy rows of {}", names.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dev_dump;
    use crate::known::{field, flag, item};
    use crate::standins::{PLAIN_DEAL, row};
    use ounce_formats::{Msbt, Part, pack_file};
    use roead::sarc::Sarc;

    fn hookshot_for(player: &str) -> Foreign {
        Foreign {
            actor: actor(1),
            shows: item::HOOKSHOT.to_string(),
            item: "Hookshot".into(),
            player: player.into(),
        }
    }

    #[test]
    fn a_name_loses_control_characters_and_is_cut_to_fit() {
        assert_eq!(hookshot_for("Player\n2").name(), "Hookshot for Player2");
        let long = "x".repeat(40);
        let cut = format!("{}…", "x".repeat(LONGEST - 1));
        assert_eq!(hookshot_for(&long).name(), format!("Hookshot for {cut}"));
    }

    #[test]
    fn a_copy_is_named_by_its_own_line() {
        assert!(is_foreign(&actor(12)) && !is_foreign(item::HOOKSHOT));
        assert_eq!(name_label(&actor(3)), Some(format!("{}_Name", actor(3))));
        assert_eq!(name_label(item::HOOKSHOT), None);
    }

    #[test]
    fn a_copy_shows_its_item_gives_nothing_and_answers_to_itself() {
        let Some(romfs) = dev_dump() else { return };
        let foreign = [hookshot_for("Player2")];
        let dir = std::env::temp_dir().join(format!("zootr-foreign-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(&romfs, &mut builder, &mut tables, &foreign).unwrap();
        builder.finish().unwrap();
        let copy = &foreign[0].actor;

        let rel = pack_file(copy);
        let raw = std::fs::read(dir.join("romfs").join(&rel)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let bytes = romfs.dicts().decompress(&rel, &raw).unwrap();
        let written = ActorPack::from_bytes(copy, bytes.to_vec()).unwrap();
        let built = pack(&romfs, copy, &foreign).unwrap().unwrap();
        assert_eq!(built.files(), written.files());
        // The shown item's model, the small key's pouch content naming the copy.
        let shown = ActorPack::load(&romfs, item::HOOKSHOT).unwrap();
        let model = |p: &ActorPack| path_of(p, crate::standins::MODEL).unwrap();
        assert_eq!(model(&built), model(&shown));
        let pouch = resolved(&built, &path_of(&built, POUCH).unwrap()).unwrap();
        assert_eq!(pouch.get(POUCH_ACTOR.get()), Some(&pouch_actor(copy)));

        let deal = row(&tables.pouch, copy).unwrap();
        let deal = deal.get(field::POUCH_SPECIAL_DEAL.get()).unwrap();
        assert_eq!(deal.as_string().unwrap(), PLAIN_DEAL);
        let actor = row(&tables.actor, copy).unwrap();
        let called = actor.get(flag::ACTOR_NAME.get()).unwrap();
        assert_eq!(called.as_string().unwrap(), copy);
    }

    #[test]
    fn a_copy_s_get_names_the_item_and_its_player_in_every_language() {
        let Some(romfs) = dev_dump() else { return };
        let foreign = [hookshot_for("Player2")];
        let copies = captions(&foreign);
        for rel in crate::text::archives(&romfs).unwrap() {
            let data = crate::text::copied(&romfs.read(&rel).unwrap(), &rel, &copies).unwrap();
            let sarc = Sarc::new(&data).unwrap();
            let msbt = Msbt::parse(sarc.get_data(&format!("{POUCH_TEXT}.msbt")).unwrap()).unwrap();
            let line = |part: &str| msbt.get(&format!("{}{part}", actor(1))).unwrap();
            assert_eq!(
                line(TEXT_PARTS[1]),
                Some(vec![Part::Text("You found Hookshot for Player2!".into())]),
                "{rel}"
            );
        }
    }
}
