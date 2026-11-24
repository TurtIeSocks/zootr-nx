//! Copies that count as nothing: items for another player's world, in an
//! Archipelago seed, and ice traps. Each is a copy of the item it shows,
//! the way a key ring is a copy of the small key: its own name, the small
//! key's pouch content and row, and a plain deal, so the game stores it as
//! nothing Link can use and counts nothing.
//!
//! Link gets the check's flag and the get. Another player's names the item
//! and its player, and the app's client sends the item on. Another game's
//! item shows a model by its Archipelago class, one no item of ours uses,
//! tinted the way keys take their dungeon's colour. A trap's get says
//! "FOOL!", and the hook freezes Link (`crate::ice_traps`).

use anyhow::{Result, ensure};
use ounce_formats::{ActorPack, BymlDoc, ModBuilder, Romfs, pack_file, set_param};

use crate::copies::{self, MADE, POUCH_TEXT, TEXT_PARTS, pointed};
use crate::ice_traps;
use crate::key_tints::paint::{Host, Look};
use crate::keys::SMALL;
use crate::known::{actor, field};
use crate::medallions::Colour;
use crate::settings::DungeonKeys;
use crate::standins::{
    POUCH, POUCH_ACTOR, Tables, actor_path, doc, made_pack, path_of, pouch_actor, raw, renamed,
    resolved, with_ref,
};
use crate::text::{Copies, CopyLine, Says};

/// One copy, at any number of checks.
#[derive(Clone, Debug, PartialEq)]
pub struct Foreign {
    /// The copy's actor, from [`actor`] or `ice_traps`.
    pub actor: String,
    /// The item it's a copy of: the item itself when it's ours.
    pub shows: String,
    /// The model it shows instead of `shows`' own: its class's, when the
    /// item is another game's.
    pub model: Option<String>,
    pub owner: Owner,
}

/// Whom a copy is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Owner {
    /// Another player: the item as its game names it, the player, and
    /// whether Archipelago classes it progression.
    Player {
        item: String,
        player: String,
        progression: bool,
    },
    /// No one: an ice trap, named on a shelf by its disguise's name line,
    /// `label`.
    Trap { label: String },
}

/// The model another game's item shows: a Poe's spirit, a flame Link
/// never gets loose, its body tinted and its glow coloured by the item's
/// class.
pub(crate) const CLASS_LOOK: Look = Look {
    host: actor::OBJ_SPIRIT_POE,
    material: Some(field::MT_BODY),
    kinds: &["Alb", "AO", "Emm", "Gn0", "Nrm", "Rgh"],
    others: &["{}_Body_Gn0"],
    copy: "Obj_RandoAp_",
    matte: false,
};
/// The spirit's glow colours, by their names' hashes: the flame's own, and
/// the lighter one round it. The flame's is purple until a Poe's colour
/// animation sets it; a copy's is its class's.
const GLOW: u64 = 0x68e1_31e7_2bcd_91ad;
const GLOW_LIGHT: u64 = 0xe647_277d_46e8_c1d8;

/// The files [`CLASS_LOOK`] is made from, spelled out like
/// [`crate::key_tints::FILES`]: extracted before a dump's names are read.
pub const FILES: [&str; 8] = [
    "Model/Obj_Spirit_Poe.Obj_Spirit_Poe.bfres.zs",
    "Tex/Mt_Body_Obj_Spirit_Poe_Alb.bntx.zs",
    "Tex/Mt_Body_Obj_Spirit_Poe_AO.bntx.zs",
    "Tex/Mt_Body_Obj_Spirit_Poe_Emm.bntx.zs",
    "Tex/Mt_Body_Obj_Spirit_Poe_Gn0.bntx.zs",
    "Tex/Mt_Body_Obj_Spirit_Poe_Nrm.bntx.zs",
    "Tex/Mt_Body_Obj_Spirit_Poe_Rgh.bntx.zs",
    "Tex/Obj_Spirit_Poe_Body_Gn0.bntx.zs",
];

/// Each class's colour, as Archipelago's text client shows it: plum,
/// slate blue and cyan.
const CLASSES: [(&str, Colour); 3] = [
    ("progression", [221, 160, 221]),
    ("useful", [106, 90, 205]),
    ("filler", [0, 255, 255]),
];

/// The model another game's item of `class` shows. A trap shows as
/// filler, as does a class this version doesn't know.
pub fn class_model(class: &str) -> String {
    let filler = CLASSES.len() - 1;
    let n = CLASSES.iter().position(|(c, _)| *c == class);
    CLASS_LOOK.name(n.unwrap_or(filler) + 1)
}

/// The spirit's model glowing `colour`: the flame in it, and the light
/// round it halfway to white, as a Poe's own colours are.
fn glowing(model: &[u8], colour: Colour) -> Result<Vec<u8>> {
    let linear = colour.map(|c| (c as f32 / 255.0).powf(2.2));
    let light = linear.map(|c| (c + 1.0) / 2.0);
    let model = set_param(model, GLOW, [linear[0], linear[1], linear[2], 1.0])?;
    set_param(&model, GLOW_LIGHT, [light[0], light[1], light[2], 1.0])
}

/// What follows [`MADE`] in a copy's name.
const AP: &str = "Ap";

/// The `n`th copy's actor.
pub fn actor(n: usize) -> String {
    format!("{MADE}{AP}{n:04}")
}

/// Whether `item` is a copy for another player.
pub fn is_foreign(item: &str) -> bool {
    item.strip_prefix(MADE)
        .is_some_and(|rest| rest.starts_with(AP))
}

/// Whether `item` is a copy this module writes: for another player, or an
/// ice trap.
pub fn is_copy(item: &str) -> bool {
    is_foreign(item) || crate::ice_traps::is_trap(item)
}

/// The label naming `item`, if it's a copy: like a key ring, it answers
/// to its own name, whose lines [`captions`] writes.
pub fn name_label(item: &str) -> Option<String> {
    is_copy(item).then(|| copies::label(item))
}

/// The longest a player's or item's name reads in a get, past which it's
/// cut, so the window holds it.
const LONGEST: usize = 32;

/// `name` without control characters, cut to [`LONGEST`].
pub(crate) fn clean(name: &str) -> String {
    let kept = printable(name);
    match kept.chars().count() > LONGEST {
        true => format!("{}…", kept.chars().take(LONGEST - 1).collect::<String>()),
        false => kept,
    }
}

/// `name` without control characters, which the game's text reads as
/// tags, or the spaces around it.
pub(crate) fn printable(name: &str) -> String {
    let kept: String = name.chars().filter(|c| !c.is_control()).collect();
    kept.trim().to_owned()
}

impl Foreign {
    /// What the item list and spoiler call it, `shown` naming the item it
    /// shows.
    pub fn name(&self, shown: impl FnOnce(&str) -> String) -> String {
        match &self.owner {
            Owner::Player { item, player, .. } => format!("{} for {}", clean(item), clean(player)),
            Owner::Trap { .. } => format!("{} (as {})", ice_traps::NAME, shown(&self.shows)),
        }
    }

    /// Each of [`TEXT_PARTS`]' line. Another player's say the same in every
    /// language: AP's item and player names have no translations. A trap's
    /// name is its disguise's, its get "FOOL!" and its caption a space: an
    /// empty one held the get open for good (live, 2026-11-18). The copy the
    /// room's traps give has nothing to hide, so its name, which the
    /// received notice shows, is the trap's own.
    fn lines(&self) -> [Says; 3] {
        match &self.owner {
            Owner::Player { item, player, .. } => {
                let (item, player) = (clean(item), clean(player));
                [
                    format!("{item} for {player}"),
                    format!("You found {item} for {player}!"),
                    format!("Archipelago sends it to {player}."),
                ]
                .map(Says::Text)
            }
            Owner::Trap { .. } => [
                match self.actor == ice_traps::POOLED {
                    true => Says::Text(ice_traps::NAME.into()),
                    false => Says::Source,
                },
                Says::Text(ice_traps::FOOL.into()),
                Says::Text(" ".into()),
            ],
        }
    }

    /// Where each of [`TEXT_PARTS`]' line is copied from: the small key's,
    /// and for a trap's name its disguise's first.
    fn sources(&self, part: &str) -> Vec<String> {
        let key = format!("{}{part}", SMALL.get());
        match &self.owner {
            Owner::Trap { label } if part == TEXT_PARTS[0] => vec![label.clone(), key],
            _ => vec![key],
        }
    }
}

/// Every copy's get text, under its own name.
pub fn captions(foreign: &[Foreign]) -> Copies<'static> {
    let new = foreign.iter().flat_map(|f| {
        TEXT_PARTS
            .iter()
            .zip(f.lines())
            .map(|(part, says)| CopyLine {
                label: format!("{}{part}", f.actor),
                sources: f.sources(part),
                says,
            })
    });
    Copies {
        file: POUCH_TEXT,
        new: new.collect(),
    }
}

/// The copy's pack, built from the dump, if `item` is one of `foreign`.
/// `keys` is the seed's, which a key copy it shows is built with.
pub fn pack(
    romfs: &Romfs,
    item: &str,
    foreign: &[Foreign],
    keys: DungeonKeys,
) -> Result<Option<ActorPack>> {
    let Some(f) = foreign.iter().find(|f| f.actor == item) else {
        return Ok(None);
    };
    let key = ActorPack::load(romfs, SMALL)?;
    let built = Built::new(romfs, f, &key, keys)?;
    Ok(Some(ActorPack::from_bytes(item, built.pack())?))
}

/// Which pack a copy's new file is sized like in the RESTBL.
#[derive(Clone, Copy)]
enum Source {
    Key,
    Shown,
}

/// A copy's new file: its path, contents, and the file it's sized like.
struct NewFile {
    path: String,
    data: Vec<u8>,
    like: String,
    source: Source,
}

impl NewFile {
    fn new(path: String, doc: &BymlDoc, like: String, source: Source) -> Self {
        Self {
            path,
            data: doc.to_binary(),
            like,
            source,
        }
    }
}

/// One copy: the shown item's pack under the copy's name, giving the small
/// key's pouch content, which names the copy.
struct Built<'a> {
    foreign: &'a Foreign,
    shown: ActorPack,
    key: &'a ActorPack,
    files: Vec<NewFile>,
}

impl<'a> Built<'a> {
    /// `key` is the small key's pack, loaded once for every copy.
    fn new(
        romfs: &Romfs,
        foreign: &'a Foreign,
        key: &'a ActorPack,
        keys: DungeonKeys,
    ) -> Result<Self> {
        let shown = match made_pack(romfs, &foreign.shows, keys)? {
            Some(made) => made,
            None => ActorPack::load(romfs, &foreign.shows)?,
        };
        let name = foreign.actor.as_str();
        let pouch_like = path_of(key, POUCH)?;
        let pouch_at = renamed(&pouch_like, name);
        let pouch_doc = BymlDoc::parse(&raw(key, &pouch_like)?)?;
        let mut pouch = resolved(key, &pouch_like)?;
        pouch.insert(POUCH_ACTOR.get().into(), pouch_actor(name));
        let actor_like = actor_path(&shown.name);
        let (actor_doc, info) = match &foreign.model {
            Some(model) => {
                let [(_, actor, _), info] = pointed(&shown, name, model)?;
                (actor, Some(info))
            }
            None => (BymlDoc::parse(&raw(&shown, &actor_like)?)?, None),
        };
        let actor = with_ref(actor_doc.root.as_map()?, POUCH, &pouch_at)?;
        let mut files = vec![
            NewFile::new(
                actor_path(name),
                &doc(actor, &actor_doc),
                actor_like,
                Source::Shown,
            ),
            NewFile::new(pouch_at, &doc(pouch, &pouch_doc), pouch_like, Source::Key),
        ];
        files.extend(info.map(|(path, info, like)| NewFile::new(path, &info, like, Source::Shown)));
        Ok(Self {
            foreign,
            shown,
            key,
            files,
        })
    }

    /// The pack a new file's `like` comes from.
    fn like_pack(&self, source: Source) -> &ActorPack {
        match source {
            Source::Key => self.key,
            Source::Shown => &self.shown,
        }
    }

    fn pack(&self) -> Vec<u8> {
        let mut writer = self.shown.writer();
        for f in &self.files {
            writer.add_file(f.path.clone(), f.data.clone());
        }
        writer.to_binary()
    }

    fn write(&self, builder: &mut ModBuilder) -> Result<()> {
        for f in &self.files {
            let like_size = raw(self.like_pack(f.source), &f.like)?.len();
            builder.add_entry(&f.path, &f.like, like_size, f.data.len())?;
        }
        builder.add(
            &pack_file(&self.foreign.actor),
            &self.pack(),
            &pack_file(&self.shown.name),
        )
    }
}

/// Writes each copy and its rows: the shown item's actor rows, the small
/// key's pouch row with a plain deal, answering to its own name. With
/// another game's items, each class's model too.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    tables: &mut Tables,
    foreign: &[Foreign],
    keys: DungeonKeys,
) -> Result<()> {
    // A seed without copies needs nothing, not even the small key's pack.
    if foreign.is_empty() {
        return Ok(());
    }
    if foreign.iter().any(|f| f.model.is_some()) {
        let host = Host::read(romfs, &CLASS_LOOK)?;
        for (n, (_, colour)) in CLASSES.iter().enumerate() {
            let model = glowing(&host.model, *colour)?;
            host.write_as(builder, &CLASS_LOOK.name(n + 1), &model, *colour, false)?;
        }
    }
    let key = ActorPack::load(romfs, SMALL)?;
    for f in foreign {
        Built::new(romfs, f, &key, keys)?.write(builder)?;
        tables.copy(&f.actor, &f.shows)?;
        tables.copy_pouch(&f.actor, SMALL)?;
        tables.name_itself(&f.actor)?;
        if let Some(model) = &f.model {
            // The shown item's animations, the spirit's bounds, and heaps
            // for the bigger of the two.
            let bounds = [field::MODEL_AABB_MAX, field::MODEL_AABB_MIN];
            tables.take(&f.actor, CLASS_LOOK.host, &bounds)?;
            tables.point(&f.actor, model)?;
        }
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
    use crate::key_tints;
    use crate::known::{field, flag, item};
    use crate::medallions::model_path;
    use crate::standins::{PLAIN_DEAL, row};
    use ounce_formats::{Msbt, Part};
    use roead::sarc::Sarc;

    fn hookshot_for(player: &str) -> Foreign {
        Foreign {
            actor: actor(1),
            shows: item::HOOKSHOT.to_string(),
            model: None,
            owner: Owner::Player {
                item: "Hookshot".into(),
                player: player.into(),
                progression: true,
            },
        }
    }

    #[test]
    fn the_spelled_out_files_are_the_class_look_s_and_copies_fit_the_host() {
        if crate::game::no_names() {
            return;
        }
        assert_eq!(CLASS_LOOK.files().collect::<Vec<_>>(), FILES);
        assert_eq!(CLASS_LOOK.name(1).len(), CLASS_LOOK.host.get().len());
    }

    #[test]
    fn each_class_shows_its_own_copy_and_a_trap_or_new_class_filler_s() {
        let model = |class| class_model(class);
        let [progression, useful, filler] = [1, 2, 3].map(|n| CLASS_LOOK.name(n));
        assert_eq!(model("progression"), progression);
        assert_eq!(model("useful"), useful);
        assert_eq!(model("filler"), filler);
        assert_eq!(model("trap"), filler);
        assert_eq!(model("a class from a later Archipelago"), filler);
    }

    #[test]
    fn a_class_copy_shows_its_tinted_model_and_every_class_s_is_written() {
        let Some(romfs) = dev_dump() else { return };
        let mut copy = hookshot_for("Player2");
        copy.model = Some(class_model("useful"));
        let foreign = [copy];
        let dir = std::env::temp_dir().join(format!("zootr-foreign-class-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        write(
            &romfs,
            &mut builder,
            &mut tables,
            &foreign,
            DungeonKeys::default(),
        )
        .unwrap();
        builder.finish().unwrap();
        let painted: Vec<bool> = (1..=CLASSES.len())
            .map(|n| {
                dir.join("romfs")
                    .join(model_path(&CLASS_LOOK.name(n)))
                    .exists()
            })
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(painted, [true; 3]);

        let built = pack(&romfs, &foreign[0].actor, &foreign, DungeonKeys::default())
            .unwrap()
            .unwrap();
        let info = path_of(&built, crate::standins::MODEL).unwrap();
        let info = resolved(&built, &info).unwrap();
        let shows = info.get(field::FMDB_NAME.get()).unwrap();
        assert_eq!(shows.as_string().unwrap(), &CLASS_LOOK.name(2));
        let row = row(&tables.actor, &foreign[0].actor).unwrap();
        let shows = row.get(field::FMDB_NAME.get()).unwrap();
        assert_eq!(shows.as_string().unwrap(), &CLASS_LOOK.name(2));
    }

    #[test]
    fn a_name_loses_control_characters_and_is_cut_to_fit() {
        assert_eq!(
            hookshot_for("Player\n2").name(str::to_owned),
            "Hookshot for Player2"
        );
        let long = "x".repeat(40);
        let cut = format!("{}…", "x".repeat(LONGEST - 1));
        assert_eq!(
            hookshot_for(&long).name(str::to_owned),
            format!("Hookshot for {cut}")
        );
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
        write(
            &romfs,
            &mut builder,
            &mut tables,
            &foreign,
            DungeonKeys::default(),
        )
        .unwrap();
        builder.finish().unwrap();
        let copy = &foreign[0].actor;

        let rel = pack_file(copy);
        let bytes = crate::game::read_built(&romfs, &dir, &rel);
        std::fs::remove_dir_all(&dir).unwrap();
        let written = ActorPack::from_bytes(copy, bytes.to_vec()).unwrap();
        let built = pack(&romfs, copy, &foreign, DungeonKeys::default())
            .unwrap()
            .unwrap();
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
    fn a_copy_of_an_item_the_seed_builds_shows_its_model() {
        let Some(romfs) = dev_dump() else { return };
        let keys = DungeonKeys::default();
        let boss = (1..).find(|&n| crate::keys::has_boss(n)).unwrap();
        let made = [crate::magic::meter::NAME.to_owned(), key_tints::boss(boss)];
        let foreign: Vec<Foreign> = (made.into_iter().enumerate())
            .map(|(n, shows)| Foreign {
                shows,
                actor: actor(n + 1),
                ..hookshot_for("Player2")
            })
            .collect();
        let dir = std::env::temp_dir().join(format!("zootr-foreign-made-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let mut tables = Tables::load(&romfs).unwrap();
        crate::magic::meter::write(&romfs, &mut builder, &mut tables).unwrap();
        key_tints::write(&romfs, &mut builder, &mut tables, true, keys, false).unwrap();
        write(&romfs, &mut builder, &mut tables, &foreign, keys).unwrap();
        builder.finish().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let shows = |p: &ActorPack| {
            let info = resolved(p, &path_of(p, crate::standins::MODEL).unwrap()).unwrap();
            info.get(field::FMDB_NAME.get()).unwrap().clone()
        };
        for f in &foreign {
            let built = pack(&romfs, &f.actor, &foreign, keys).unwrap().unwrap();
            let made = made_pack(&romfs, &f.shows, keys).unwrap().unwrap();
            assert_eq!(shows(&built), shows(&made), "{}", f.shows);
        }
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
