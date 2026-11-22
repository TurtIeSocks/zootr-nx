//! Item words: the game's own names for item actors, and boss defeats and
//! story events, which Link holds like items.

use anyhow::{Context, Result, ensure};

use super::Need;
use crate::progressive::Lines;

/// The game's own words naming an item actor start with one of these.
/// Rule files use `item:` words, so only code writes these.
pub(super) const ITEM_PREFIXES: &[&str] = &["Item_", "TBox_", "Armor_", "Obj_", "Weapon_"];
/// A boss defeat's id starts with this. Only a vanilla source gives one,
/// and Link holds it like an item.
pub(super) const BEAT: &str = "Beat_";
/// A story event's id, like a boss defeat's.
pub(super) const EVENT: &str = "Event_";

/// Whether `word` is a boss defeat's or a story event's id.
pub(super) fn held(word: &str) -> bool {
    [BEAT, EVENT]
        .iter()
        .any(|p| word.len() > p.len() && word.starts_with(p))
}

/// What `word` needs, if it names an item, a boss defeat or a story event.
pub(super) fn word(word: &str, progressive: bool, lines: Lines) -> Option<Result<Need>> {
    if ITEM_PREFIXES.iter().any(|p| word.starts_with(p)) {
        return Some(item(word, progressive, lines));
    }
    held(word).then(|| item(word, false, lines))
}

/// An item's name or `name*n`. In an upgrade line, `n` counts levels from
/// the name: with progressive items any of the line's items count, without
/// them it names the level itself.
fn item(word: &str, progressive: bool, lines: Lines) -> Result<Need> {
    let (actor, n) = match word.split_once('*') {
        Some((actor, n)) => (actor, n.parse::<u32>().ok().filter(|&n| n > 0)),
        None => (word, Some(1)),
    };
    let n = n.with_context(|| format!("bad count in `{word}`"))?;
    let Some((line, at)) = lines.all().find_map(|l| {
        l.iter()
            .position(|&(drop, _)| drop == actor)
            .map(|i| (l, i))
    }) else {
        return Ok(Need::Items {
            actors: vec![actor.into()],
            count: n,
        });
    };
    let level = at + n as usize;
    ensure!(level <= line.len(), "`{word}` is past the end of its line");
    let (levels, count) = match progressive {
        true => (line, level),
        // Owning a higher level does everything a lower one does.
        false => (&line[level - 1..], 1),
    };
    Ok(Need::Items {
        actors: levels.iter().map(|&(drop, _)| drop.to_string()).collect(),
        count: count as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::known::item;
    use crate::rules::Have;
    use crate::world::Era;

    fn have(items: &[&str]) -> Have {
        let mut have = Have::default();
        items.iter().for_each(|i| have.add(i));
        have
    }

    #[test]
    fn line_counts_follow_progressive_items() {
        needs_names!();
        let at = |n: u32, progressive| {
            word(
                &format!("{}*{n}", item::HOOKSHOT),
                progressive,
                Lines::default(),
            )
            .unwrap()
        };
        let longshot = |progressive| at(2, progressive).unwrap();

        assert!(!longshot(true).met(&have(&[item::HOOKSHOT]), Era::Adult));
        assert!(longshot(true).met(&have(&[item::HOOKSHOT, item::LONGSHOT]), Era::Adult));
        assert!(longshot(true).met(&have(&[item::LONGSHOT, item::LONGSHOT]), Era::Adult));
        assert_eq!(
            longshot(true),
            word(item::LONGSHOT, true, Lines::default())
                .unwrap()
                .unwrap()
        );

        assert!(!longshot(false).met(&have(&[item::HOOKSHOT, item::HOOKSHOT]), Era::Adult));
        assert!(longshot(false).met(&have(&[item::LONGSHOT]), Era::Adult));

        assert!(at(3, true).is_err());
        assert!(at(0, true).is_err());
    }

    #[test]
    fn a_boss_defeat_is_held_like_an_item_and_other_words_are_not_items() {
        assert!(word("Beat_FakeBoss", true, Lines::default()).is_some());
        assert!(word(BEAT, true, Lines::default()).is_none());
        assert!(word("always", true, Lines::default()).is_none());
    }
}
