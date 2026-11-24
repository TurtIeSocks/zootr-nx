//! New game text in every language, built at generate time from the
//! player's dump: vanilla lines with one item's name swapped for another's.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ounce_formats::{ModBuilder, Msbt, Part, Romfs, files_with_suffix, messages, replace_file};
use roead::sarc::Sarc;

use crate::hints::{self, Hint};
use crate::names::{AREAS, ITEMS, name_label};

/// Each language's text is one archive in `Mals`.
const ARCHIVES: (&str, &str) = ("Mals", ".Product.110.sarc.zs");

/// New lines for one message file, each a copy of a vanilla line that
/// names an item, naming another instead.
pub struct Lines<'a> {
    /// The message file, as `EventFlowMsg/NAME`.
    pub file: &'a str,
    /// The vanilla line's label, and the item it names.
    pub like: (&'a str, &'a str),
    /// The line copied instead where a language's vanilla line doesn't
    /// hold its item's name as written, or the new item has no name.
    pub fallback: &'a str,
    /// Each new line's label, with the item it names.
    pub new: Vec<(String, &'a str)>,
}

/// New lines for one message file, each a copy of the first of its
/// sources a language has. A label the language has already stays.
pub struct Copies<'a> {
    /// The message file, as `ActorMsg/PouchContent`.
    pub file: &'a str,
    pub new: Vec<CopyLine>,
}

/// One new line: its label, its sources in order, and what it says.
pub struct CopyLine {
    pub label: String,
    pub sources: Vec<String>,
    pub says: Says,
}

/// What a copied line says.
pub enum Says {
    /// Its source, as it is.
    Source,
    /// Its source, naming a place.
    Named(Named),
    /// These words, in every language that has a source.
    Text(String),
    /// Words and GameData tags, each tag copied from a vanilla line that
    /// reads it. A language without one of the tags goes without the line.
    /// It replaces a label that already has a line: a line the game reads
    /// by a fixed label (a banner type's) can only be taken over.
    Tagged(&'static [Piece]),
}

/// A run of a [`Says::Tagged`] line.
#[derive(Clone, Copy)]
pub enum Piece {
    Words(&'static str),
    Tag(Tagged),
}

/// A GameData tag: one of `kind` reading `name`, as some line of `file`
/// (as `LayoutMsg/NAME`) holds it.
#[derive(Clone, Copy)]
pub struct Tagged {
    pub file: &'static str,
    pub name: &'static crate::known::Name,
    pub kind: u16,
}

/// Tags that read GameData are group 2.
const GAMEDATA: u16 = 2;

impl CopyLine {
    /// A line copied as it is.
    pub fn plain(label: String, sources: Vec<String>) -> Self {
        Self {
            label,
            sources,
            says: Says::Source,
        }
    }
}

/// A copy naming a place: English reads `english`, its `{}` the place's
/// English name, and `{the} ` or `{a} ` its article, which a possessive
/// name ("Ganondorf's Castle") goes without. Every other language reads the source line, with the
/// place's own name in brackets after it when `bracketed`. A language
/// without the place's name keeps the source line.
pub struct Named {
    /// The place's label in `LocationMsg/Location`.
    pub place: &'static str,
    pub english: &'static str,
    pub bracketed: bool,
}

/// Every language's archive, by romfs path.
pub(crate) fn archives(romfs: &Romfs) -> Result<Vec<String>> {
    let found = files_with_suffix(&romfs.root().join(ARCHIVES.0), ARCHIVES.1)?;
    let mut rels = found
        .iter()
        .map(|p| romfs.rel(p))
        .collect::<Result<Vec<_>>>()?;
    rels.sort();
    Ok(rels)
}

/// Write `lines` and `copies`, one per message file, and `hints` into
/// every language's archive.
pub fn write(
    romfs: &Romfs,
    builder: &mut ModBuilder,
    lines: &[Lines],
    copies: &[Copies],
    hints: &[Hint],
) -> Result<()> {
    let lines: Vec<&Lines> = lines.iter().filter(|l| !l.new.is_empty()).collect();
    let copies: Vec<&Copies> = copies.iter().filter(|c| !c.new.is_empty()).collect();
    if lines.is_empty() && copies.is_empty() && hints.is_empty() {
        return Ok(());
    }
    let items = lines
        .iter()
        .flat_map(|l| l.new.iter().map(|(_, i)| *i).chain([l.like.1]));
    let labels = name_labels(romfs, items.chain(hints.iter().map(|h| h.item.as_str())))?;
    for rel in archives(romfs)? {
        let mut data = romfs.read(&rel)?;
        // Copies first: a line names its item from the item text, which
        // holds a key copy's name only once its copy is in.
        for c in &copies {
            data = copied(&data, &rel, c)?;
        }
        for l in &lines {
            data = language(&data, &rel, l, &labels)?.0;
        }
        if !hints.is_empty() {
            data = hints::language(&data, &rel, hints, &labels)?;
        }
        builder.write(&rel, &data)?;
    }
    Ok(())
}

/// Whether the archive at `rel` is one of the English ones.
pub(crate) fn is_english(rel: &str) -> bool {
    rel.contains("en.Product")
}

/// The label naming each of `items`.
fn name_labels<'a>(
    romfs: &Romfs,
    items: impl IntoIterator<Item = &'a str>,
) -> Result<HashMap<&'a str, String>> {
    let mut out = HashMap::new();
    for item in items {
        if let Some(label) = name_label(romfs, item)? {
            out.insert(item, label);
        }
    }
    Ok(out)
}

/// The archive at `rel` with `lines` added, and the labels that copied the
/// fallback.
fn language<'l>(
    data: &[u8],
    rel: &str,
    lines: &'l Lines,
    labels: &HashMap<&str, String>,
) -> Result<(Vec<u8>, Vec<&'l str>)> {
    let sarc = Sarc::new(data).with_context(|| format!("reading {rel}"))?;
    let get = |path: &str| {
        sarc.get_data(path)
            .with_context(|| format!("{rel} has no {path}"))
    };
    let pouch = messages(get(ITEMS)?)?;
    // Polish names are empty here.
    let name = |item: &str| {
        let name = labels.get(item).and_then(|l| pouch.get(l));
        name.filter(|n| !n.is_empty())
    };
    let path = format!("{}.msbt", lines.file);
    let mut msbt = Msbt::parse(get(&path)?)?;
    let line = |label: &str| {
        let found = msbt.get(label)?;
        found.with_context(|| format!("{rel} {path} has no {label}"))
    };
    let fallback = line(lines.fallback)?;
    // Which spelling of the vanilla name the line uses, so the new name
    // takes the same one. A line naming it in the plural (the carpet
    // merchant's "Bombchus") loses the plural with it.
    let spelled = |line: &[Part]| {
        let all = spellings(name(lines.like.1)?);
        let at = all.iter().position(|s| names(line, s))?;
        let plural = format!("{}s", all[at]);
        let old = if names(line, &plural) {
            plural
        } else {
            all[at].clone()
        };
        Some((old, at))
    };
    let like = line(lines.like.0)?;
    let (like, old) = match spelled(&like) {
        Some(old) => (like, Some(old)),
        None => {
            let joined = joined(&like);
            let old = spelled(&joined);
            (joined, old)
        }
    };
    let mut missed = Vec::new();
    for (label, item) in &lines.new {
        let text = match old.as_ref().zip(name(item)) {
            Some(((old, at), new)) => swap(&like, old, &spellings(new)[*at]),
            None => {
                missed.push(label.as_str());
                fallback.clone()
            }
        };
        msbt.set(label, &text)?;
    }
    Ok((replace_file(data, &path, &msbt.to_binary())?, missed))
}

/// The archive at `rel` with `copies` added where it has a source.
pub(crate) fn copied(data: &[u8], rel: &str, copies: &Copies) -> Result<Vec<u8>> {
    let sarc = Sarc::new(data).with_context(|| format!("reading {rel}"))?;
    let path = format!("{}.msbt", copies.file);
    let file = sarc.get_data(&path);
    let mut msbt = Msbt::parse(file.with_context(|| format!("{rel} has no {path}"))?)?;
    let areas = sarc.get_data(AREAS);
    let places = Msbt::parse(areas.with_context(|| format!("{rel} has no {AREAS}"))?)?;
    for line in &copies.new {
        let replaces = matches!(line.says, Says::Tagged(_));
        if !replaces && msbt.get(&line.label)?.is_some() {
            continue;
        }
        let mut text = None;
        for source in &line.sources {
            text = msbt.get(source)?;
            if text.is_some() {
                break;
            }
        }
        let Some(text) = text else { continue };
        let text = match &line.says {
            Says::Source => text,
            Says::Named(named) => naming(text, named, rel, &places)?,
            Says::Text(words) => vec![Part::Text(words.clone())],
            Says::Tagged(pieces) => match tagged(&sarc, pieces)? {
                Some(parts) => parts,
                None => continue,
            },
        };
        msbt.set(&line.label, &text)?;
    }
    replace_file(data, &path, &msbt.to_binary())
}

/// `pieces` as one language's parts, its tags copied from that language's
/// lines, or `None` when a tag isn't found.
fn tagged(sarc: &Sarc, pieces: &[Piece]) -> Result<Option<Vec<Part>>> {
    let mut parts = Vec::with_capacity(pieces.len());
    for piece in pieces {
        parts.push(match piece {
            Piece::Words(w) => Part::Text((*w).to_owned()),
            Piece::Tag(t) => match find_tag(sarc, t)? {
                Some(tag) => tag,
                None => return Ok(None),
            },
        });
    }
    Ok(Some(parts))
}

/// The first tag in `tag`'s file that is it, as stored.
fn find_tag(sarc: &Sarc, tag: &Tagged) -> Result<Option<Part>> {
    let Some(data) = sarc.get_data(&format!("{}.msbt", tag.file)) else {
        return Ok(None);
    };
    let msbt = Msbt::parse(data)?;
    for label in msbt.labels()? {
        for part in msbt.get(label)?.unwrap_or_default() {
            if let Part::Tag(raw) = &part
                && reads(raw, tag)
            {
                return Ok(Some(part));
            }
        }
    }
    Ok(None)
}

/// Whether the stored tag `raw` is `tag`: its marker (two bytes in a wide
/// file, whose second is 0), group and kind, then the name in its
/// parameters, in either width.
pub(crate) fn reads(raw: &[u8], tag: &Tagged) -> bool {
    let at = if raw.get(1) == Some(&0) { 2 } else { 1 };
    let word = |i: usize| raw.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let name = tag.name.get();
    let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let holds = |n: &[u8]| raw.windows(n.len()).any(|w| w == n);
    word(at) == Some(GAMEDATA)
        && word(at + 2) == Some(tag.kind)
        && (holds(name.as_bytes()) || holds(&wide))
}

/// `line` naming `named`'s place, in the language of the archive at `rel`,
/// whose place names are `places`.
fn naming(line: Vec<Part>, named: &Named, rel: &str, places: &Msbt) -> Result<Vec<Part>> {
    let Some(place) = places.get(named.place)? else {
        return Ok(line);
    };
    let words: String = place
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            Part::Tag(_) => None,
        })
        .collect();
    if words.trim().is_empty() {
        return Ok(line);
    }
    if is_english(rel) {
        let article = |a: &str| match words.contains("'s") {
            true => String::new(),
            false => format!("{a} "),
        };
        let english = (named.english)
            .replace("{the} ", &article("the"))
            .replace("{a} ", &article("a"))
            .replace("{}", &words);
        return Ok(vec![Part::Text(english)]);
    }
    if !named.bracketed {
        return Ok(line);
    }
    // Chinese and Japanese write full-width brackets, with no space.
    let wide = ["zh.", "ja."].iter().any(|l| rel.contains(l));
    let (open, close) = if wide { ("（", "）") } else { (" (", ")") };
    let mut out = line;
    out.push(Part::Text(open.into()));
    out.extend(place);
    out.push(Part::Text(close.into()));
    Ok(out)
}

/// `name` as written, then with a lower case first letter, the way most
/// European languages write an item mid-sentence.
fn spellings(name: &str) -> [String; 2] {
    let mut chars = name.chars();
    let lower = chars
        .next()
        .map(|c| c.to_lowercase().chain(chars).collect())
        .unwrap_or_default();
    [name.to_owned(), lower]
}

/// Whether a text run of `line` holds `name`.
fn names(line: &[Part], name: &str) -> bool {
    line.iter()
        .any(|p| matches!(p, Part::Text(t) if t.contains(name)))
}

/// The tag some lines put between the words of an item's name, as in
/// Link the Goron's "Goron " and "Tunic".
const BETWEEN_WORDS: [u8; 2] = [14, 201];

/// `line` with each `BETWEEN_WORDS` tag between two text runs dropped and
/// the runs joined, so a name broken by one reads whole.
fn joined(line: &[Part]) -> Vec<Part> {
    let text = |p: Option<&Part>| matches!(p, Some(Part::Text(_)));
    let mut out: Vec<Part> = Vec::new();
    for (i, part) in line.iter().enumerate() {
        match part {
            Part::Tag(t)
                if t.starts_with(&BETWEEN_WORDS) && text(out.last()) && text(line.get(i + 1)) => {}
            Part::Text(t) => match out.last_mut() {
                Some(Part::Text(last)) => last.push_str(t),
                _ => out.push(part.clone()),
            },
            tag => out.push(tag.clone()),
        }
    }
    out
}

/// `line` with `old` swapped for `new` in its text, tags untouched.
fn swap(line: &[Part], old: &str, new: &str) -> Vec<Part> {
    line.iter()
        .map(|p| match p {
            Part::Text(t) => Part::Text(t.replace(old, new)),
            tag => tag.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::known::{actor, field, flag, item};
    use ounce_formats::frame_dict_id;

    use super::*;
    use crate::checks::Drop;
    use crate::game::dev_dump;
    use crate::scrubs::{self, SCRUBS};
    use crate::settings::NewGame;

    fn hammer_at_kokiri() -> HashMap<u64, Drop> {
        HashMap::from([(SCRUBS[0].hash(), Drop::from(item::HAMMER.to_string()))])
    }

    /// A message's text runs joined, and its tags.
    fn split(parts: &[Part]) -> (String, Vec<&Part>) {
        let text = parts.iter().filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            Part::Tag(_) => None,
        });
        let tags = parts.iter().filter(|p| matches!(p, Part::Tag(_)));
        (text.collect(), tags.collect())
    }

    #[test]
    fn a_copy_takes_the_first_source_there_and_keeps_a_line_already_there() {
        let Some(romfs) = dev_dump() else { return };
        let rels = archives(&romfs).unwrap();
        let rel = rels.iter().find(|r| r.contains("USen")).unwrap();
        let data = romfs.read(rel).unwrap();
        let sources =
            |stem: &str, kinds: &[&str]| kinds.iter().map(|k| format!("{stem}_{k}")).collect();
        let copies = Copies {
            file: &format!("{}/PouchContent", field::ACTOR_MSG),
            new: vec![
                CopyLine::plain(
                    format!("{}_ShopCaption", item::MELODY_FIRE_BOLERO),
                    sources(item::MELODY_FIRE_BOLERO, &["Caption", field::GET_CAPTION]),
                ),
                CopyLine::plain(
                    actor::OBJ_HEART_SHOP_CAPTION.to_string(),
                    sources(actor::OBJ_HEART, &[field::GET_CAPTION]),
                ),
            ],
        };
        let written = copied(&data, rel, &copies).unwrap();
        let read = |d: &[u8]| {
            let sarc = Sarc::new(d).unwrap();
            Msbt::parse(
                sarc.get_data(&format!("{}/PouchContent.msbt", field::ACTOR_MSG))
                    .unwrap(),
            )
            .unwrap()
        };
        let (old, new) = (read(&data), read(&written));
        let line = |m: &Msbt, label: &str| m.get(label).unwrap().unwrap();
        // A song has no menu caption: the next source stands in.
        assert_eq!(
            line(&new, &format!("{}_ShopCaption", item::MELODY_FIRE_BOLERO)),
            line(&old, &format!("{}_GetCaption", item::MELODY_FIRE_BOLERO))
        );
        assert_eq!(
            line(&new, actor::OBJ_HEART_SHOP_CAPTION),
            line(&old, actor::OBJ_HEART_SHOP_CAPTION)
        );
    }

    #[test]
    fn a_key_copy_at_a_scrub_is_named_by_its_dungeon() {
        let Some(romfs) = dev_dump() else { return };
        let dir = std::env::temp_dir().join(format!("zootr-text-copy-{}", std::process::id()));
        let forest = crate::keys::DUNGEONS
            .iter()
            .position(|d| d.name == crate::known::flag::FOREST_TEMPLE)
            .unwrap();
        let copy = crate::key_tints::boss(forest + 1);
        let drops = HashMap::from([(SCRUBS[0].hash(), Drop::from(copy.clone()))]);
        let new_game = NewGame {
            scrubs: true,
            keys: crate::settings::DungeonKeys::Singles,
            ..Default::default()
        };
        crate::generate::write_mod(
            &romfs,
            &dir,
            &drops,
            &new_game,
            &[],
            &crate::progress::Run::silent(),
        )
        .unwrap();
        let lines = scrubs::lines(&drops, Default::default());
        let (label, _) = lines.new.iter().find(|(_, i)| *i == copy).unwrap();
        let rel = archives(&romfs)
            .unwrap()
            .into_iter()
            .find(|r| r.contains("USen"))
            .unwrap();
        let data = crate::game::read_built(&romfs, &dir, &rel);
        let sarc = Sarc::new(&data).unwrap();
        let path = format!("{}.msbt", lines.file);
        let msbt = Msbt::parse(sarc.get_data(&path).unwrap()).unwrap();
        let (text, _) = split(&msbt.get(label).unwrap().unwrap());
        std::fs::remove_dir_all(dir).unwrap();
        // The copy's own name, not the line naming no item.
        assert!(text.contains("Forest Temple"), "{text}");
    }

    /// A mod dealing `drops` with scrubs on, in a folder of its own.
    fn scrub_mod(romfs: &Romfs, drops: &HashMap<u64, Drop>, tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zootr-text-{tag}-{}", std::process::id()));
        let new_game = NewGame {
            scrubs: true,
            ..Default::default()
        };
        let run = crate::progress::Run::silent();
        crate::generate::write_mod(romfs, &dir, drops, &new_game, &[], &run).unwrap();
        dir
    }

    /// In every language of the mod in `dir`, `lines`' line naming the
    /// hammer keeps its vanilla line's tags, or is its fallback. In English
    /// it reads as the vanilla line with the Piece of Heart swapped out.
    fn hammer_named(romfs: &Romfs, dir: &std::path::Path, lines: &Lines) {
        let (label, _) = lines.new.iter().find(|(_, i)| *i == item::HAMMER).unwrap();
        let rels = archives(romfs).unwrap();
        assert_eq!(rels.len(), 16);
        for rel in &rels {
            let raw = std::fs::read(dir.join("romfs").join(rel)).unwrap();
            let vanilla = std::fs::read(romfs.root().join(rel)).unwrap();
            assert_eq!(frame_dict_id(&raw), frame_dict_id(&vanilla), "{rel}");
            let data = romfs.dicts().decompress(rel, &raw).unwrap();
            let sarc = Sarc::new(&data).unwrap();
            let path = format!("{}.msbt", lines.file);
            let msbt = Msbt::parse(sarc.get_data(&path).unwrap()).unwrap();
            let line = |label: &str| msbt.get(label).unwrap().unwrap();
            let (new, like) = (line(label), line(lines.like.0));
            let ((new_text, new_tags), (old_text, old_tags)) = (split(&new), split(&like));
            assert!(new == line(lines.fallback) || new_tags == old_tags, "{rel}");
            if rel.contains("USen") {
                assert!(old_text.contains("Piece of Heart"));
                assert_eq!(
                    new_text,
                    old_text.replace("Piece of Heart", "Megaton Hammer")
                );
                assert!(!new_tags.is_empty());
                assert_eq!(new_tags, old_tags);
            }
        }
    }

    #[test]
    fn a_hammer_at_kokiris_scrub_is_named_in_every_language() {
        let Some(romfs) = dev_dump() else { return };
        let drops = hammer_at_kokiri();
        let dir = scrub_mod(&romfs, &drops, "kokiri");
        let lines = scrubs::lines(&drops, Default::default());
        hammer_named(&romfs, &dir, &lines);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_riddle_scrub_says_a_fairy_bought_what_it_was_dealt() {
        let Some(romfs) = dev_dump() else { return };
        let riddle =
            (SCRUBS.iter()).find(|s| s.area == flag::HYRULE_FIELD && s.name == "heart piece");
        let drops = HashMap::from([(riddle.unwrap().hash(), Drop::from(item::HAMMER.to_string()))]);
        let dir = scrub_mod(&romfs, &drops, "riddle");
        let lines = scrubs::sold_out(&drops);
        assert_eq!(lines.len(), 3);
        for l in &lines {
            hammer_named(&romfs, &dir, l);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_scrub_and_link_the_goron_both_name_what_they_give() {
        let Some(romfs) = dev_dump() else { return };
        let dir = std::env::temp_dir().join(format!("zootr-text-two-{}", std::process::id()));
        let mut drops = hammer_at_kokiri();
        let goron = crate::gifts::GIFTS
            .iter()
            .find(|g| g.giver == "Link the Goron");
        drops.insert(goron.unwrap().hash(), Drop::from(item::HAMMER.to_string()));
        let new_game = NewGame {
            scrubs: true,
            ..Default::default()
        };
        crate::generate::write_mod(
            &romfs,
            &dir,
            &drops,
            &new_game,
            &[],
            &crate::progress::Run::silent(),
        )
        .unwrap();
        let mut lines = crate::gifts::lines(&drops, Default::default());
        lines.push(scrubs::lines(&drops, Default::default()));
        assert_eq!(lines.len(), 2);
        let rel = archives(&romfs)
            .unwrap()
            .into_iter()
            .find(|r| r.contains("USen"));
        let rel = rel.unwrap();
        let data = crate::game::read_built(&romfs, &dir, &rel);
        let sarc = Sarc::new(&data).unwrap();
        for l in &lines {
            let msbt = Msbt::parse(sarc.get_data(&format!("{}.msbt", l.file)).unwrap()).unwrap();
            let line = |label: &str| split(&msbt.get(label).unwrap().unwrap()).0;
            let (new, old) = (line(&l.new[0].0), line(l.like.0));
            assert_ne!(new, old, "{}", l.file);
            assert!(new.contains("Megaton Hammer"), "{}: {new}", l.file);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_line_naming_bombchus_in_the_plural_names_the_dealt_item() {
        let Some(romfs) = dev_dump() else { return };
        let dir = std::env::temp_dir().join(format!("zootr-text-chus-{}", std::process::id()));
        let carpet = crate::gifts::GIFTS
            .iter()
            .find(|g| g.giver == "Carpet merchant");
        let drops = HashMap::from([(carpet.unwrap().hash(), Drop::from(item::HAMMER.to_string()))]);
        let new_game = NewGame {
            on: [crate::settings::MERCHANTS.into()].into(),
            ..Default::default()
        };
        crate::generate::write_mod(
            &romfs,
            &dir,
            &drops,
            &new_game,
            &[],
            &crate::progress::Run::silent(),
        )
        .unwrap();
        let lines = crate::gifts::lines(&drops, Default::default());
        let [l] = &lines[..] else {
            panic!("{} lines", lines.len())
        };
        let rel = archives(&romfs)
            .unwrap()
            .into_iter()
            .find(|r| r.contains("USen"));
        let rel = rel.unwrap();
        let data = crate::game::read_built(&romfs, &dir, &rel);
        let sarc = Sarc::new(&data).unwrap();
        let msbt = Msbt::parse(sarc.get_data(&format!("{}.msbt", l.file)).unwrap()).unwrap();
        let line = |label: &str| split(&msbt.get(label).unwrap().unwrap()).0;
        let (new, old) = (line(&l.new[0].0), line(l.like.0));
        assert!(old.contains("Bombchus"), "{old}");
        assert_eq!(new, old.replace("Bombchus", "Megaton Hammer"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_languages_without_item_names_fall_back() {
        let Some(romfs) = dev_dump() else { return };
        let drops = hammer_at_kokiri();
        let lines = scrubs::lines(&drops, Default::default());
        let items = lines.new.iter().map(|(_, i)| *i).chain([lines.like.1]);
        let labels = name_labels(&romfs, items).unwrap();
        let mut fell_back = Vec::new();
        for rel in archives(&romfs).unwrap() {
            let data = romfs.read(&rel).unwrap();
            let (_, missed) = language(&data, &rel, &lines, &labels).unwrap();
            let lang = rel.trim_start_matches("Mals/").trim_end_matches(ARCHIVES.1);
            fell_back.extend(missed.iter().map(|m| format!("{lang} {m}")));
        }
        // Polish item names are empty in this file.
        let langs = ["EUpl"];
        let want = langs.iter().flat_map(|lang| {
            let labels = lines.new.iter().map(move |(l, _)| format!("{lang} {l}"));
            labels.collect::<Vec<_>>()
        });
        assert_eq!(fell_back, want.collect::<Vec<_>>());
    }
}
