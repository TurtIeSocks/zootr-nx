//! The matcher: maps every offset of one build's [`Build`] into another
//! build's `main`, and gives the other build's table only once every row
//! is found or confirmed by hand. Code by masked windows (narrowed by the
//! calls already matched, then by callers), globals by the code that loads
//! them, vtables slot by slot, imports by name, regions from the NSO
//! header. Spec: base game support (private notes).

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::{Result, bail};
use zootr_hook::Build;

use crate::game::Release;

mod arm;
mod fields;
mod find;
mod image;
mod rows;

use fields::Field;
use find::Matcher;
pub use find::{Confidence, Found, Method, Outcome, Shifts};
pub use image::Image;
use rows::{Kind, ROWS, Rule};

/// Rows the matcher can't settle, confirmed by hand for 1.0.0: each row's
/// name, its 1.0.0 offset and how it was confirmed. Each one's code
/// changed between the builds, so no window matches; the evidence is its
/// [`likeness`] to its 1.1.1 code, which a dump test checks stays at
/// [`LIKE`] or more. For scale: `entries.granted`, which callers match,
/// scores 0.903; unrelated functions 0.02 to 0.04.
pub const CONFIRMED: &[(&str, u32, &str)] = &[
    (
        "entries.link_update",
        0x2bc_d710,
        "likeness 0.712; its first 15 words agree, then its stack frame \
         moved, and both its callers' frames round the call",
    ),
    (
        "entries.ui_update",
        0x6b_5900,
        "likeness 0.873; its referrer's two address loads agree; it saves \
         one more register, so its body sits a word later",
    ),
];
/// How many words from each entry [`likeness`] compares.
pub const LIKENESS_WORDS: u32 = 600;
/// The least [`likeness`] a row confirmed by hand may have.
pub const LIKE: f64 = 0.6;

/// Each row in `confirmed`'s [`likeness`] to its code in `from`, where
/// `build` places it.
pub fn liked(from: &Image, build: &Build, to: &Image, confirmed: &[(&str, u32, &str)]) -> Vec<f64> {
    let had = rows::values(build);
    let at = |name: &str| rows::position(name).map(|i| had[i]);
    (confirmed.iter())
        .map(|&(name, there, _)| at(name).map_or(0.0, |at| likeness(from, at, to, there)))
        .collect()
}

/// How alike the code at `at` in `from` and `there` in `to` is: the
/// share of [`LIKENESS_WORDS`] words each, by [`arm::shape`], that
/// [`ratio`] pairs up.
pub fn likeness(from: &Image, at: u32, to: &Image, there: u32) -> f64 {
    let shapes = |img: &Image, at: u32| -> Vec<u32> {
        (0..LIKENESS_WORDS)
            .map(|i| arm::shape(img.word(at + 4 * i)))
            .collect()
    };
    ratio(&shapes(from, at), &shapes(to, there))
}

/// Ratcliff and Obershelp's ratio, as Python's `difflib` gives it with no
/// junk: the longest common run (the earliest in `a`, then in `b`), then
/// the same either side of it, and twice the words paired over both
/// lengths.
pub fn ratio(a: &[u32], b: &[u32]) -> f64 {
    let mut paired = 0;
    let mut todo = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = todo.pop() {
        let (mut at, mut bt, mut len) = (alo, blo, 0);
        let mut run = vec![0usize; bhi - blo + 1];
        for (i, &x) in a.iter().enumerate().take(ahi).skip(alo) {
            for j in (blo..bhi).rev() {
                let k = j - blo + 1;
                run[k] = if x == b[j] { run[k - 1] + 1 } else { 0 };
            }
            for j in blo..bhi {
                let k = run[j - blo + 1];
                if k > len {
                    (at, bt, len) = (i + 1 - k, j + 1 - k, k);
                }
            }
        }
        if len == 0 {
            continue;
        }
        paired += len;
        todo.push((alo, at, blo, bt));
        todo.push((at + len, ahi, bt + len, bhi));
    }
    let total = a.len() + b.len();
    if total == 0 {
        1.0
    } else {
        2.0 * paired as f64 / total as f64
    }
}

/// The command that writes 1.0.0's table, which its header names.
pub const COMMAND: &str = "zootr probe port --to 1.0.0";
/// Where it writes it, from the repo's root.
pub const TABLE_FILE: &str = "crates/zootr-hook/src/build/v65536.rs";

/// 1.0.0's table as its generated source holds it.
pub fn base_source(b: &Build) -> String {
    let about = "Game 1.0.0's (package v65536), matched from [`V131072`].";
    rows::source(b, "V65536", about, COMMAND)
}

/// What goes in each room, in bytes: today's hook, far code and fullest
/// table.
#[derive(Clone, Copy, Debug)]
pub struct Needs {
    pub cave: u32,
    pub far: u32,
    pub table: u32,
}

/// A room our code goes in, as found, and what it needs.
#[derive(Clone, Debug)]
pub struct Room {
    pub name: &'static str,
    /// Its size, if both ends were found.
    pub size: Option<u32>,
    pub needs: u32,
}

/// Every row's outcome, the rooms, and the hook's field offsets.
pub struct Port {
    /// Each row's offset in the build mapped from, in [`ROWS`]' order.
    pub from: Vec<u32>,
    pub found: Vec<Outcome>,
    pub rooms: Vec<Room>,
    pub fields: Vec<Field>,
}

/// How far data and bss usually move from one release to the other:
/// sloperinaGZ's per-build table moves every GOT slot by -0x65ca8 and
/// most of bss by -0x70460 from 1.1.1 to 1.0.0.
pub fn usual(from: Release, to: Release) -> Shifts {
    let sign = match (from, to) {
        (Release::V111, Release::V100) => 1,
        (Release::V100, Release::V111) => -1,
        _ => 0,
    };
    Shifts {
        data: -0x65ca8 * sign,
        bss: -0x70460 * sign,
    }
}

/// Where the NSO header and MOD0 put `rule` in `img`.
fn place(img: &Image, rule: Rule) -> u32 {
    let list_end = img.bss_end.next_multiple_of(0x1000);
    match rule {
        Rule::TextEnd => img.text.end,
        // The table's header is read in place, a word at a time.
        Rule::RodataEnd => img.rodata.end.next_multiple_of(8),
        Rule::DataStart => img.data.start,
        Rule::BssEnd => img.bss_end.next_multiple_of(16),
        Rule::ListEnd => list_end,
        Rule::Panic => list_end - 16,
    }
}

/// The indexes of the rows of a kind.
fn of_kind(f: impl Fn(Kind) -> bool) -> Vec<usize> {
    (0..ROWS.len()).filter(|&i| f(ROWS[i].kind)).collect()
}

/// Maps `build`, which fits `from`, into `to`. `usual` is how far globals
/// usually move; `needs` what goes in the rooms.
pub fn port(from: &Image, build: &Build, to: &Image, usual: Shifts, needs: Needs) -> Port {
    let src = rows::values(build);
    let mut m = Matcher::new(from, to);
    let mut found: Vec<Option<Outcome>> = vec![None; ROWS.len()];
    let values = |idx: &[usize]| idx.iter().map(|&i| src[i]).collect::<Vec<_>>();

    // Code, with the functions the End rows end.
    let code = of_kind(|k| k == Kind::Code);
    let ends = of_kind(|k| k == Kind::End);
    let starts: Vec<Result<u32, String>> = (ends.iter())
        .map(|&i| {
            let f = from.before(src[i]).ok_or("no function ends there")?;
            match from.next_function(f.start) {
                Some(next) if next == src[i] => Ok(f.start),
                _ => Err(format!("the function at {:#x} doesn't end there", f.start)),
            }
        })
        .collect();
    let mut addrs = values(&code);
    addrs.extend(starts.iter().flatten());
    let mut out = m.code(&addrs, true).into_iter();
    for &i in &code {
        found[i] = out.next();
    }
    for (&i, start) in ends.iter().zip(starts) {
        found[i] = Some(start.and_then(|_| end(to, out.next().unwrap())));
    }

    for (i, row) in ROWS.iter().enumerate() {
        match row.kind {
            Kind::Target(of) => found[i] = Some(target(from, to, &src, &found, of, src[i])),
            Kind::Import => found[i] = Some(m.import(src[i])),
            Kind::Header(rule) => found[i] = Some(header(from, to, rule, src[i])),
            _ => {}
        }
    }
    let globals = of_kind(|k| k == Kind::Global);
    for (&i, o) in globals
        .iter()
        .zip(m.globals(&values(&globals), Some(usual)))
    {
        found[i] = Some(o);
    }
    let vtables = of_kind(|k| k == Kind::Vtable);
    for (&i, o) in vtables.iter().zip(m.vtables(&values(&vtables))) {
        found[i] = Some(o);
    }

    let mut found: Vec<Outcome> = found.into_iter().map(Option::unwrap).collect();
    cross_check(&m, &src, &mut found);
    let rooms = rooms(&mut found, needs);
    let fields = fields::confirm(from, &m.matched);
    Port {
        from: src,
        found,
        rooms,
        fields,
    }
}

/// Where the function matched as `start` ends in `to`: where the next
/// one starts.
fn end(to: &Image, start: Outcome) -> Outcome {
    let f = start?;
    let at = f.value;
    if to.function(at).is_none_or(|g| g.start != at) {
        return Err(format!("{at:#x} doesn't start a function"));
    }
    let value = to.next_function(at).ok_or("no function follows")?;
    Ok(Found {
        value,
        method: Method::End,
        note: format!("the end of the function at {at:#x}"),
        ..f
    })
}

/// Where the branch the row `of` holds goes in `to`, after checking it
/// goes to `want` in `from`.
fn target(
    from: &Image,
    to: &Image,
    src: &[u32],
    found: &[Option<Outcome>],
    of: &str,
    want: u32,
) -> Outcome {
    let i = rows::position(of).unwrap();
    let at = src[i];
    if arm::target(from.word(at), at) != Some(want) {
        return Err(format!("the word at {at:#x} doesn't branch to {want:#x}"));
    }
    let site = match &found[i] {
        Some(Ok(f)) => f,
        _ => return Err(format!("{of} wasn't found")),
    };
    let value =
        (arm::target(to.word(site.value), site.value)).ok_or("its word there isn't a branch")?;
    Ok(Found {
        value,
        method: Method::Branch,
        note: format!("where {of} goes"),
        ..site.clone()
    })
}

/// `rule`'s place in `to`, after checking it gives `want` in `from`.
fn header(from: &Image, to: &Image, rule: Rule, want: u32) -> Outcome {
    let had = place(from, rule);
    if had != want {
        return Err(format!(
            "{rule:?} gives {had:#x} where the table has {want:#x}"
        ));
    }
    Ok(Found {
        value: place(to, rule),
        method: Method::Header,
        words: 0,
        candidates: 1,
        confidence: Confidence::High,
        note: format!("{rule:?}"),
    })
}

/// Checks each code and import row against what the matched calls say
/// it is: a disagreement unfinds it, agreement from two or more calls
/// raises it to high.
fn cross_check(m: &Matcher, src: &[u32], found: &mut [Outcome]) {
    for (i, row) in ROWS.iter().enumerate() {
        if !matches!(row.kind, Kind::Code | Kind::Import) {
            continue;
        }
        let (Some((to, n)), Ok(f)) = (m.called(src[i]), &mut found[i]) else {
            continue;
        };
        if f.value != to {
            let why = format!(
                "found {:#x} by {}, but {n} matched calls go to {to:#x}",
                f.value,
                f.method.name()
            );
            found[i] = Err(why);
            continue;
        }
        f.note += &format!(
            "{}{n} matched calls agree",
            if f.note.is_empty() { "" } else { "; " }
        );
        if n >= 2 && f.confidence == Confidence::Medium {
            f.confidence = Confidence::High;
        }
    }
}

/// The cave, the far code's room and the table's room, each from its two
/// rows. The cave or far room too small for today's code unfinds its
/// start.
fn rooms(found: &mut [Outcome], needs: Needs) -> Vec<Room> {
    let at = |name: &str| rows::position(&format!("regions.{name}")).unwrap();
    let rooms = [
        ("cave", "cave_end", needs.cave, true),
        ("far", "far_end", needs.far, true),
        ("table", "table_end", needs.table, false),
    ];
    (rooms.into_iter())
        .map(|(start, end, needs, binding)| {
            let size = match (&found[at(start)], &found[at(end)]) {
                (Ok(s), Ok(e)) => e.value.checked_sub(s.value),
                _ => None,
            };
            if binding && size.is_some_and(|s| s < needs) {
                let why = format!(
                    "its room is {} bytes, today's code needs {needs}",
                    size.unwrap()
                );
                found[at(start)] = Err(why);
            }
            Room {
                name: start,
                size,
                needs,
            }
        })
        .collect()
}

impl Port {
    /// The build in the other `main`: every row found and not low, or
    /// confirmed by hand in `confirmed`. Refuses while any row is neither.
    pub fn build(&self, confirmed: &[(&str, u32, &str)]) -> Result<Build> {
        let mut values = Vec::new();
        let mut missing = Vec::new();
        for (row, o) in ROWS.iter().zip(&self.found) {
            let name = row.name();
            let hand = confirmed.iter().find(|c| c.0 == name).map(|c| c.1);
            match (o, hand) {
                (Ok(f), Some(h)) if f.value != h => {
                    bail!("{name} matched {:#x}, confirmed by hand as {h:#x}", f.value)
                }
                (Ok(f), _) if f.confidence > Confidence::Low => values.push(f.value),
                (_, Some(h)) => values.push(h),
                (Ok(f), None) => missing.push(format!("{name}: low ({})", f.note)),
                (Err(why), None) => missing.push(format!("{name}: {why}")),
            }
        }
        if !missing.is_empty() {
            bail!(
                "{} rows unmatched, so no table is written:\n  {}",
                missing.len(),
                missing.join("\n  ")
            );
        }
        Ok(rows::assemble(&values))
    }

    /// Per row: its offset in each build, the method, the window, the
    /// candidates, the confidence and a note; then counts, every row
    /// unmatched, the rooms and the field offsets left unconfirmed.
    /// `confirmed` are the rows confirmed by hand, `liked` their
    /// [`likeness`] each.
    pub fn report(
        &self,
        from: &str,
        to: &str,
        confirmed: &[(&str, u32, &str)],
        liked: &[f64],
    ) -> String {
        let mut out = String::new();
        let w = &mut out;
        let _ = writeln!(
            w,
            "{:<26} {:>10} {:>10}  {:<8} {:>5} {:>5}  {:<6}  note",
            "row", from, to, "method", "words", "cands", "conf"
        );
        let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for ((row, o), &had) in ROWS.iter().zip(&self.found).zip(&self.from) {
            let name = row.name();
            match o {
                Ok(f) => {
                    *counts
                        .entry((f.method.name(), f.confidence.name()))
                        .or_default() += 1;
                    let _ = writeln!(
                        w,
                        "{name:<26} {had:>#10x} {:>#10x}  {:<8} {:>5} {:>5}  {:<6}  {}",
                        f.value,
                        f.method.name(),
                        f.words,
                        f.candidates,
                        f.confidence.name(),
                        f.note
                    );
                }
                Err(why) => {
                    *counts.entry(("unmatched", "")).or_default() += 1;
                    let _ = writeln!(w, "{name:<26} {had:>#10x} {:>10}  {why}", "-");
                }
            }
        }
        let _ = writeln!(w, "\ncounts");
        for ((method, confidence), n) in counts {
            let _ = writeln!(w, "  {method:<10} {confidence:<6} {n}");
        }
        let _ = writeln!(w, "\nconfirmed by hand");
        for ((name, v, why), l) in confirmed.iter().zip(liked) {
            let under = if *l < LIKE { ", under the least" } else { "" };
            let _ = writeln!(w, "  {name:<26} {v:>#10x}  measured {l:.3}{under}: {why}");
        }
        let _ = writeln!(w, "\nrooms");
        for r in &self.rooms {
            let size = r.size.map_or("-".to_string(), |s| s.to_string());
            let fits = match r.size {
                Some(s) if s >= r.needs => "fits",
                Some(_) => "doesn't fit",
                None => "unknown",
            };
            let _ = writeln!(
                w,
                "  {:<6} {size:>6} bytes, needs {:>6}: {fits}",
                r.name, r.needs
            );
        }
        let unconfirmed: Vec<&Field> = self.fields.iter().filter(|f| !f.confirmed).collect();
        let _ = writeln!(
            w,
            "\nfield offsets no matched window reads ({} of {})",
            unconfirmed.len(),
            self.fields.len()
        );
        for f in unconfirmed {
            let _ = writeln!(w, "  {:<20} {:>#7x}  {}", f.name, f.value, f.file);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use zootr_hook::{V65536, V131072};

    use super::*;
    use crate::game::dumped_main;

    /// `release`'s `main` from the dev dump, read for matching once a run.
    fn image(release: Release) -> Option<&'static Image<'static>> {
        static IMAGES: [OnceLock<Image<'static>>; 2] = [OnceLock::new(), OnceLock::new()];
        let (nso, bytes) = dumped_main(release)?;
        Some(IMAGES[release as usize].get_or_init(|| Image::new(nso, bytes).unwrap()))
    }

    /// 1.1.1's table mapped into 1.0.0, once a run.
    fn to_base() -> Option<&'static Port> {
        static PORT: OnceLock<Port> = OnceLock::new();
        let (from, to) = (image(Release::V111)?, image(Release::V100)?);
        let usual = usual(Release::V111, Release::V100);
        Some(PORT.get_or_init(|| port(from, &V131072, to, usual, crate::patch::needs())))
    }

    #[test]
    fn every_row_confirmed_by_hand_is_like_its_1_1_1_code() {
        let (Some(update), Some(base)) = (image(Release::V111), image(Release::V100)) else {
            return;
        };
        let liked = liked(update, &V131072, base, CONFIRMED);
        for (&(name, there, why), &l) in CONFIRMED.iter().zip(&liked) {
            assert!(
                l >= LIKE,
                "{name} at {there:#x} is only {l:.3} like its 1.1.1 code"
            );
            assert!(
                why.contains(&format!("likeness {l:.3}")),
                "{name}: {l:.3}, {why}"
            );
        }
        // The scale: a row callers match, and an unrelated function.
        let granted = V131072.entries.granted;
        let unrelated = likeness(update, granted, base, 0x6b_5900);
        let granted = likeness(update, granted, base, 0x113_78f0);
        assert!(
            granted > 0.9 && unrelated < 0.1,
            "{granted:.3}, {unrelated:.3}"
        );
    }

    #[test]
    fn the_ratio_is_difflibs() {
        // SequenceMatcher(None, a, b, autojunk=False).ratio() for each.
        assert_eq!(ratio(&[1, 2, 9, 3, 4], &[1, 2, 3, 4]), 8.0 / 9.0);
        assert_eq!(ratio(&[1, 2, 3, 4], &[2, 3, 4, 1]), 0.75);
        assert_eq!(ratio(&[1, 2], &[3, 4]), 0.0);
        assert_eq!(ratio(&[], &[]), 1.0);
    }

    #[test]
    fn every_offset_maps_to_itself_in_its_own_build() {
        let Some(img) = image(Release::V111) else {
            return;
        };
        let usual = usual(Release::V111, Release::V111);
        let p = port(img, &V131072, img, usual, crate::patch::needs());
        let built = p.build(&[]).map_err(|e| e.to_string());
        assert_eq!(
            built,
            Ok(V131072),
            "{}",
            p.report("1.1.1", "1.1.1", &[], &[])
        );
    }

    #[test]
    fn every_offset_found_in_1_0_0_maps_back() {
        let (Some(there), Some(base), Some(update)) =
            (to_base(), image(Release::V100), image(Release::V111))
        else {
            return;
        };
        let usual = usual(Release::V100, Release::V111);
        let back = port(base, &V65536, update, usual, crate::patch::needs());
        let want = rows::values(&V131072);
        let mut wrong = Vec::new();
        let rows = ROWS.iter().zip(&there.found).zip(&back.found).zip(want);
        for (((row, there), back), want) in rows {
            // A row whose own window matched nowhere, its code changed, may
            // not come back: 1.1.1 may hold a twin of its 1.0.0 code as
            // well. It mustn't come back wrong.
            let changed = there.as_ref().is_ok_and(|f| f.method == Method::Callers)
                || CONFIRMED.iter().any(|c| c.0 == row.name());
            match back {
                Ok(f) if f.value == want => {}
                Err(_) if changed => {}
                Ok(f) if changed && f.confidence == Confidence::Low => {}
                _ => wrong.push(format!("{}: {back:x?}", row.name())),
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn the_1_0_0_table_is_what_the_port_writes() {
        let Some(p) = to_base() else {
            return;
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(TABLE_FILE);
        let file = std::fs::read_to_string(path).ok();
        let written = p.build(CONFIRMED).ok().map(|b| base_source(&b));
        assert_eq!(file, written, "run `{COMMAND}` again");
    }

    /// sloperinaGZ's 1.0.0 offsets for our rows: numbers from its
    /// per-build table, test data only. Then the ones the NSO header gives,
    /// and its event request sub-object's vtable's first slot.
    const SLOPERINA: &[(&str, u32)] = &[
        ("entries.granted", 0x113_78f0),
        ("funcs.delete", 0xe6_3be0),
        ("funcs.make_arg", 0xd3_c9a0),
        ("funcs.request", 0xd3_d2b0),
        ("funcs.release_link", 0x27_c630),
        ("funcs.set_time", 0x33f_1580),
        ("funcs.move_to", 0x6d_7730),
        ("got.events", 0x4c1_83e8),
        ("got.gamedata", 0x4c1_84d8),
        ("got.world", 0x4c1_8bf0),
        ("got.pouch_menu", 0x4c1_8428),
        ("vtables.player", 0x4ac_0e30),
        ("vtables.movement", 0x4ae_7458),
        ("vtables.controller", 0x4bc_f308),
        ("regions.bss_end", 0x4e2_a070),
        ("vtables.link_empty", 0x491_01e8 + 0x10),
        ("regions.panic", 0x4e2_aff0),
        ("regions.table_end", 0x47e_9000),
        ("regions.list_end", 0x4e2_b000),
        ("regions.text_end", 0x3be_6850),
        // Rodata's end, 0x47e_8254, rounded up to a word for the table.
        ("regions.table", 0x47e_8258),
    ];

    #[test]
    fn sloperina_gzs_1_0_0_offsets_agree() {
        let Some(p) = to_base() else {
            return;
        };
        let wrong: Vec<String> = (SLOPERINA.iter())
            .filter_map(|&(name, want)| {
                let got = p.found[rows::position(name).unwrap()]
                    .as_ref()
                    .map(|f| f.value);
                (got != Ok(want)).then(|| format!("{name}: {got:x?}, not {want:#x}"))
            })
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    fn found(value: u32) -> Outcome {
        Ok(Found {
            value,
            method: Method::Window,
            words: 65,
            candidates: 1,
            confidence: Confidence::High,
            note: String::new(),
        })
    }

    #[test]
    fn a_row_unmatched_holds_the_table_back_until_confirmed() {
        let from = rows::values(&V131072);
        let mut found: Vec<Outcome> = from.iter().map(|&v| found(v)).collect();
        found[0] = Err("its window matched nowhere".into());
        let p = Port {
            from: from.clone(),
            found,
            rooms: Vec::new(),
            fields: Vec::new(),
        };
        let err = p.build(&[]).unwrap_err().to_string();
        assert!(
            err.contains("entries.map_loaded: its window matched nowhere"),
            "{err}"
        );
        let hand = [("entries.map_loaded", from[0], "read by hand")];
        assert_eq!(p.build(&hand).unwrap(), V131072);
        let wrong = [("entries.granted", 1, "a typo")];
        assert!(p.build(&wrong).is_err());
        let text = p.report("1.1.1", "1.1.1", &hand, &[0.5]);
        assert!(
            text.contains("measured 0.500, under the least: read by hand"),
            "{text}"
        );
    }

    #[test]
    fn the_usual_shifts_turn_round() {
        let (there, back) = (
            usual(Release::V111, Release::V100),
            usual(Release::V100, Release::V111),
        );
        assert_eq!((there.data, there.bss), (-0x65ca8, -0x70460));
        assert_eq!((back.data, back.bss), (0x65ca8, 0x70460));
        assert_eq!(usual(Release::V100, Release::V100).data, 0);
    }
}
