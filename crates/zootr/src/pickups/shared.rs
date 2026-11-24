//! The shared AI pack ([`GLOBAL_AI`]): graphs every pickup runs, which the
//! seed edits in place. A new AI or blackboard file name crashes or
//! silences the actor (research: upgrades off chests), so every branch the
//! seed adds goes into a graph the game already runs.

use anyhow::{Context, Result};
use ounce_formats::ainb::{File, Value};
use ounce_formats::{ModBuilder, Romfs};
use roead::sarc::{Sarc, SarcWriter};

use super::{GLOBAL_AI, stale, taken};
use crate::known::{Name, field};

/// One graph's path in the pack, and what makes its new bytes from the
/// pack.
type Edit = (String, fn(&Sarc) -> Result<Vec<u8>>);

/// Writes the pack with the take tick in the pickup event graph, and the
/// stale-drop branch in the drop graph when `steps_built`, each size table
/// entry resized.
pub fn write(romfs: &Romfs, builder: &mut ModBuilder, steps_built: bool) -> Result<()> {
    let mut edits: Vec<Edit> = vec![(taken::graph(), taken::ticked)];
    if steps_built {
        edits.push((stale::graph(), stale::branched));
    }
    let data = romfs.read(GLOBAL_AI)?;
    let pack = Sarc::new(&data)?;
    let mut writer = SarcWriter::from_sarc(&pack);
    let mut sizes = Vec::new();
    for (file, edit) in &edits {
        let old = (pack.get_data(file)).with_context(|| format!("no {file}"))?;
        let new = edit(&pack)?;
        sizes.push((file, old.len(), new.len()));
        writer = writer.with_file(file.as_str(), new);
    }
    builder.write(GLOBAL_AI, &writer.to_binary())?;
    for (file, old, new) in sizes {
        builder.resize(file, old, new)?;
    }
    Ok(())
}

/// Copies the pack's first node of class `class` onto the end of `f`.
pub fn copy(pack: &Sarc, f: &mut File, class: &str) -> Result<usize> {
    let (from, node) = template(pack, class)?;
    f.copy_node(&from, node)
}

/// Copies onto `f` the name check both branches open with: the file name
/// of the pickup's own actor, whether it starts with `token`, and a node of
/// class `class` on the Bool `bool_prefix` + that name. Returns the check,
/// whose `Result` reads it, and that node.
pub fn name_check(
    pack: &Sarc,
    f: &mut File,
    token: &str,
    bool_prefix: &str,
    class: &Name,
) -> Result<(usize, usize)> {
    let copy = |f: &mut File, class: &Name| self::copy(pack, f, class.get());
    let own = copy(f, field::QUERY_GENERIC_GET_OWN_ACTOR_LINK)?;
    let name = copy(f, field::QUERY_GENERIC_GET_ACTOR_NAME)?;
    let starts = copy(f, field::QUERY_GENERIC_STRING_STARTS_WITH)?;
    let concat = copy(f, field::QUERY_GENERIC_CONCAT_STRING)?;
    let flag = copy(f, class)?;

    let file_name = field::ACTOR_FILE_NAME.get();
    let full_name = field::GAME_DATA_FULL_NAME.get();
    let text = |s: &str| Value::String(s.into());
    f.link(name, "Actor", own, "Actor")?;
    f.link(starts, "Input", name, file_name)?;
    f.set_input(starts, "Token", &text(token))?;
    f.set_input(concat, "Input0", &text(bool_prefix))?;
    f.link(concat, "Input1", name, file_name)?;
    f.set_input(concat, "Input2", &text(""))?;
    f.set_input(flag, full_name, &text(""))?;
    f.link(flag, full_name, concat, "Output")?;
    Ok((starts, flag))
}

/// `vanilla`'s own bool selector and do-nothing leaf, for copying.
pub fn selector_and_leaf(vanilla: &File) -> Result<(usize, usize)> {
    let select = vanilla.find_bool_selector().context("no bool selector")?;
    let nothing = (vanilla.find(field::ONESHOT_DO_NOTHING.get())).context("no do-nothing leaf")?;
    Ok((select, nothing))
}

/// The first node of class `class` the writer can copy, from the pack's
/// graphs in name order, so every seed takes the same one.
fn template(pack: &Sarc, class: &str) -> Result<(File, usize)> {
    let mut graphs: Vec<_> = pack
        .files()
        .filter(|f| f.name().is_some_and(|n| n.ends_with(".ainb")))
        .filter(|f| f.data().windows(class.len()).any(|w| w == class.as_bytes()))
        .collect();
    graphs.sort_by_key(|f| f.name().map(str::to_owned));
    for g in graphs {
        let Ok(from) = File::parse(g.data()) else {
            continue;
        };
        let Some(node) = from.find(class) else {
            continue;
        };
        if from.clone().copy_node(&from, node).is_ok() {
            return Ok((from, node));
        }
    }
    anyhow::bail!("no graph in the shared pack has a {class} to copy")
}
