//! NPC voices: a voice type's files, a slot taking another type whole, and
//! chaos.
//!
//! An NPC's voice is the voice param in its actor pack, naming its type.
//! A type's clips sit in its bank, one per line ("LaughS00": an emotion and
//! a take), and its info gives each clip's mouth. In chaos each type keeps
//! its own lines, and each line plays a clip of the same emotion drawn from
//! any type's, repeats allowed. A line's bank entry takes the donor clip's
//! BWAV and its name, so a prefetch streams the rest of the donor's clip
//! (heard live, 2026-11-25), and its mouth takes the donor's.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use ounce_formats::{Byml, BymlDoc, ModBuilder, bars, crc32};

use super::{TRACKS, Track, VOICE_PACKS, Via};
use crate::rng::Rng;

/// The languages a named character's clips are voiced in, a bank each.
const LANGS: [&str; 8] = [
    "EUde", "EUes", "EUfr", "EUit", "JPja", "USen", "USes", "USpt",
];

/// The language that stands for all of them where one must: a preview, or
/// a named character's clip in a type with one bank.
pub const ENGLISH: &str = "USen";

/// The language a named character's mouths are keyed by, for every one.
const MOUTHS: &str = "JPja";

/// Whether `kind` is a named character, voiced per language.
fn named(kind: &str) -> bool {
    kind.starts_with("Custom_")
}

/// The languages `kind`'s banks are in: [`ENGLISH`] alone stands for the
/// one bank of a type that isn't named.
fn langs(kind: &str) -> &'static [&'static str] {
    match named(kind) {
        true => &LANGS,
        false => &[ENGLISH],
    }
}

/// What `kind`'s clip names start with in `lang`.
fn prefix(kind: &str, lang: &str) -> String {
    match named(kind) {
        true => format!("NV_{lang}_{kind}_"),
        false => format!("NV_{kind}_"),
    }
}

/// The bank of voice type `kind` in `lang`.
pub fn bank(kind: &str, lang: &str) -> String {
    let name = prefix(kind, lang);
    format!("Sound/Resource/{}.bars.zs", name.trim_end_matches('_'))
}

/// The info of voice type `kind`: each clip's mouth.
fn info(kind: &str) -> String {
    format!("Voice/VoiceTypeInfo/{kind}.game__sound__VoiceTypeInfo.bgyml")
}

/// Every voice type with a track, each once.
fn kinds() -> impl Iterator<Item = &'static str> {
    TRACKS
        .iter()
        .filter(|t| t.via == Via::Voice)
        .map(|t| t.graph)
}

/// Every bank and info of the tracked voice types, which chaos rewrites.
pub static FILES: LazyLock<Vec<String>> = LazyLock::new(|| {
    (kinds())
        .flat_map(|kind| {
            let banks = langs(kind).iter().map(move |lang| bank(kind, lang));
            banks.chain([info(kind)])
        })
        .collect()
});

/// Each slot's packs given its donor's type. A param shared by several
/// packs is served by whichever loads first, so all are edited.
pub fn trade(builder: &mut ModBuilder, trades: &[(&Track, &Track)]) -> Result<()> {
    for (slot, donor) in trades {
        ensure!(
            donor.via == Via::Voice,
            "{} can't take {}: a voice trades only with a voice",
            slot.id,
            donor.id
        );
        let mut params: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (_, pack, param) in VOICE_PACKS.iter().filter(|(kind, ..)| *kind == slot.graph) {
            params.entry(param).or_default().push(pack);
        }
        let hash = ounce_formats::murmur3(donor.graph.as_bytes());
        let path = info(donor.graph);
        for (param, packs) in params {
            let member = format!("Component/VoiceParam/{param}.game__component__VoiceParam.bgyml");
            builder.edit_param(&packs, &member, |data| {
                let mut doc = BymlDoc::parse(data)?;
                let map = doc.root.as_mut_map()?;
                map.insert("NpcVoiceType".into(), Byml::U32(hash));
                map.insert(
                    "VoiceTypeInfoBgymlPath".into(),
                    Byml::String(path.clone().into()),
                );
                Ok(doc.to_binary())
            })?;
        }
    }
    Ok(())
}

/// A clip: its type, and its line ("LaughS00").
type Clip = (&'static str, String);

/// A line's emotion, its take dropped: "LaughS".
fn emotion(line: &str) -> &str {
    line.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// Each of `slots`' lines given a clip of the same emotion, drawn for
/// `seed`. A named character's languages all play the same draw.
pub fn chaos(builder: &mut ModBuilder, slots: &[&Track], seed: u64) -> Result<()> {
    if slots.is_empty() {
        return Ok(());
    }
    // Every bank and mouth read before any is rewritten: a donor read after
    // its own lines were swapped would hand on its draws.
    let (mut banks, mut mouths) = (HashMap::new(), HashMap::new());
    for kind in kinds() {
        for lang in langs(kind) {
            banks.insert(bank(kind, lang), builder.read(&bank(kind, lang))?);
        }
        let doc = BymlDoc::parse(&builder.read(&info(kind))?)?;
        for (name, mouth) in dict(&doc)? {
            mouths.insert(name.to_string(), mouth.clone());
        }
    }
    let (mut lines, mut pool) = (HashMap::new(), HashMap::<_, Vec<Clip>>::new());
    for kind in kinds() {
        let start = prefix(kind, ENGLISH);
        let mut own: Vec<String> = (bars::names(&banks[&bank(kind, ENGLISH)])?.iter())
            .filter_map(|n| n.strip_prefix(&start).map(str::to_owned))
            .collect();
        own.sort();
        for line in &own {
            let clips = pool.entry(emotion(line).to_owned()).or_default();
            clips.push((kind, line.clone()));
        }
        lines.insert(kind, own);
    }
    let mut rng = Rng::new(seed ^ u64::from(crate::build::fnv1a(b"npc voice chaos")));
    for slot in slots {
        let kind = slot.graph;
        let mut draws = vec![];
        for line in &lines[kind] {
            let clips = &pool[emotion(line)];
            let clip = &clips[rng.below(clips.len() as u64) as usize];
            if *clip != (kind, line.clone()) {
                draws.push((line, clip));
            }
        }
        for lang in langs(kind) {
            let swaps = (draws.iter())
                .map(|(line, (from, theirs))| {
                    let (path, name) =
                        (bank(from, lang), format!("{}{theirs}", prefix(from, lang)));
                    let wav = (bars::bwav(&banks[&path], &name)?)
                        .with_context(|| format!("{path} has no {name}"))?;
                    let hash = crc32(format!("{}{line}", prefix(kind, lang)).as_bytes());
                    Ok((hash, name, wav))
                })
                .collect::<Result<Vec<_>>>()?;
            let path = bank(kind, lang);
            let built = bars::rebuild(&banks[&path], |hash| {
                let (_, name, wav) = swaps.iter().find(|(h, ..)| *h == hash)?;
                Some(bars::Swap { wav, name })
            })?;
            builder.write(&path, &built)?;
        }
        let mut doc = BymlDoc::parse(&builder.read(&info(kind))?)?;
        let own = (doc.root.as_mut_map()?.get_mut("MouthOpenInfoDict"))
            .context("no MouthOpenInfoDict")?
            .as_mut_map()?;
        for (line, (from, theirs)) in &draws {
            let donor = mouths.get(&format!("{}{theirs}", prefix(from, MOUTHS)));
            let slot = own.get_mut(format!("{}{line}", prefix(kind, MOUTHS)).as_str());
            if let (Some(donor), Some(slot)) = (donor, slot) {
                *slot = donor.clone();
            }
        }
        builder.write(&info(kind), &doc.to_binary())?;
    }
    Ok(())
}

/// An info's mouths, by clip name.
fn dict(doc: &BymlDoc) -> Result<&ounce_formats::BymlMap> {
    let map = doc.root.as_map()?.get("MouthOpenInfoDict");
    Ok(map.context("no MouthOpenInfoDict")?.as_map()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ageless_items::SOUNDS;
    use crate::audio::{Audio, CHAOS, track};

    /// Every pack of a voice slot's type names the donor's, and a voice-only
    /// seed leaves the sounds file alone.
    #[test]
    fn a_voice_type_takes_its_donors_in_every_pack() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-npc-voice-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let audio = Audio::from([
            ("npc-navi".into(), "npc-darunia".into()),
            ("npc-darunia".into(), "npc-navi".into()),
            ("npc-kokiri-boy".into(), "npc-cow".into()),
        ]);
        crate::audio::write(&mut builder, &audio, 0).unwrap();
        builder.finish().unwrap();
        let sounds = dir.join("romfs").join(SOUNDS).exists();
        let mut seen = 0;
        for (slot, donor) in &audio {
            let (slot, donor) = (track(slot).unwrap(), track(donor).unwrap());
            for (_, pack, param) in VOICE_PACKS.iter().filter(|(k, ..)| *k == slot.graph) {
                let rel = ounce_formats::pack_file(pack);
                let built = crate::game::read_built(&romfs, &dir, &rel);
                let sarc = roead::sarc::Sarc::new(&built).unwrap();
                let member =
                    format!("Component/VoiceParam/{param}.game__component__VoiceParam.bgyml");
                let doc = BymlDoc::parse(sarc.get_data(&member).unwrap()).unwrap();
                let map = doc.root.as_map().unwrap();
                let hash = ounce_formats::murmur3(donor.graph.as_bytes());
                assert_eq!(map["NpcVoiceType"], Byml::U32(hash), "{pack}");
                let Byml::String(path) = &map["VoiceTypeInfoBgymlPath"] else {
                    panic!("{pack}")
                };
                assert!(
                    path.contains(&format!("/{}.", donor.graph)),
                    "{pack}: {path}"
                );
                seen += 1;
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
        assert!(!sounds, "a voice-only seed rewrote the sounds file");
        assert!(seen > 8, "{seen} packs");
    }

    /// Every type in chaos: each line plays a clip of its emotion, its
    /// BWAV and mouth the donor's, every language the same draw, and
    /// Navi's lock-on cry hers.
    #[test]
    fn in_chaos_each_line_plays_a_clip_of_its_emotion() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("zootr-chaos-{}", std::process::id()));
        let mut builder = ModBuilder::new(&romfs, &dir).unwrap();
        let audio: Audio = (TRACKS.iter().filter(|t| t.via == Via::Voice))
            .map(|t| (t.id.to_owned(), CHAOS.to_owned()))
            .collect();
        crate::audio::write(&mut builder, &audio, 7).unwrap();
        builder.finish().unwrap();
        let vanilla = |path: &str| romfs.read(path).unwrap();
        let built = |path: &str| crate::game::read_built(&romfs, &dir, path);
        let trim = |w: &[u8]| w[..w.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1)].to_vec();
        let (mut wavs, mut mouths) = (HashMap::new(), HashMap::new());
        for kind in kinds() {
            for lang in langs(kind) {
                let bank = vanilla(&bank(kind, lang));
                for name in bars::names(&bank).unwrap() {
                    let wav = trim(bars::bwav(&bank, &name).unwrap().unwrap());
                    wavs.insert(name, wav);
                }
            }
            let doc = BymlDoc::parse(&romfs.read(&info(kind)).unwrap()).unwrap();
            for (key, mouth) in dict(&doc).unwrap() {
                mouths.insert(key.to_string(), mouth.clone());
            }
        }
        let line = |name: &str| name.rsplit('_').next().unwrap().to_owned();
        let mouth =
            |name: &str, lang: &str| name.replacen(&format!("_{lang}_"), &format!("_{MOUTHS}_"), 1);
        let (mut lines, mut changed, mut navi) = (0, 0, 0);
        for kind in kinds() {
            let doc = BymlDoc::parse(&built(&info(kind))).unwrap();
            let own = dict(&doc).unwrap();
            let mut draws: Vec<Vec<String>> = vec![];
            for lang in langs(kind) {
                let path = bank(kind, lang);
                let (bank, before) = (built(&path), bars::names(&vanilla(&path)).unwrap());
                let after = bars::names(&bank).unwrap();
                let mut plain = vec![];
                for (old, new) in before.iter().zip(&after) {
                    plain.push(new.replacen(&format!("_{lang}_"), "_", 1));
                    assert_eq!(emotion(&line(old)), emotion(&line(new)), "{old} took {new}");
                    if old == new {
                        continue;
                    }
                    let wav = trim(bars::bwav(&bank, old).unwrap().unwrap());
                    assert_eq!(wav, wavs[new], "{old} took {new}'s name, not its audio");
                    if let (Some(want), Some(got)) = (
                        mouths.get(&mouth(new, lang)),
                        own.get(mouth(old, lang).as_str()),
                    ) {
                        assert_eq!(got, want, "{old}'s mouth");
                    }
                    if *lang == ENGLISH {
                        changed += 1;
                        if old.contains("LockOn") {
                            navi += 1;
                            assert!(new.starts_with("NV_USen_Custom_Navi_"), "{old} took {new}");
                        }
                    }
                }
                lines += usize::from(*lang == ENGLISH) * before.len();
                // Each bank sorts by its names' hashes, which hold the language.
                plain.sort();
                draws.push(plain);
            }
            assert!(
                draws.windows(2).all(|w| w[0] == w[1]),
                "{kind}'s languages drew apart"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
        assert!(changed * 2 > lines, "{changed} of {lines} lines changed");
        assert!(navi > 0, "Navi has no lock-on cry");
    }
}
