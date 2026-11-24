//! The matcher's methods: code by masked windows, narrowed by the calls
//! other matches agree on and by callers; globals by the code that loads
//! them; vtables slot by slot; imports by name.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::arm;
use super::image::{Image, MANY, Window};

/// How sure a match is. Low is a value the port won't write without a
/// hand's confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// How a row was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// Its masked window matched once.
    Window,
    /// Its window matched more than once, and the calls already matched
    /// left one.
    Narrowed,
    /// Its callers' matched calls agree on it.
    Callers,
    /// Read from the branch a matched row holds.
    Branch,
    /// The end of a matched function.
    End,
    /// The code that loads it agrees on it.
    Votes,
    /// Its slots hold the functions matched for them.
    Slots,
    /// By its import's name.
    Import,
    /// From the NSO header.
    Header,
}

impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Narrowed => "narrowed",
            Self::Callers => "callers",
            Self::Branch => "branch",
            Self::End => "end",
            Self::Votes => "votes",
            Self::Slots => "slots",
            Self::Import => "import",
            Self::Header => "header",
        }
    }
}

impl Confidence {
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// A row's offset in the other build, and how it was found.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub value: u32,
    pub method: Method,
    /// The window's length in words, 0 for none.
    pub words: u32,
    /// How many places the window matched before narrowing.
    pub candidates: usize,
    pub confidence: Confidence,
    pub note: String,
}

/// A found row, or why it wasn't.
pub type Outcome = Result<Found, String>;

/// The most callers asked about a function, and uses voted on for a
/// global.
const ASKED: usize = 24;
/// The fewest first words that must agree where a function's referrers
/// say it is, when its window matches nowhere.
const AGREED: u32 = 8;
/// The fewest words a part of a window may match by.
const TRUSTED: u32 = 16;
/// The most slots read from a vtable.
const SLOTS: u32 = 64;

/// How far data and bss usually move between the two builds.
#[derive(Clone, Copy, Debug)]
pub struct Shifts {
    pub data: i64,
    pub bss: i64,
}

/// Both builds, and what matched so far.
pub struct Matcher<'a> {
    pub from: &'a Image<'a>,
    pub to: &'a Image<'a>,
    /// Each callee in `from` and what the matched windows' calls in `to`
    /// say it is, with how many say so.
    calls: HashMap<u32, BTreeMap<u32, usize>>,
    /// Every window matched once: where it starts in each build.
    pub matched: Vec<(Window, u32)>,
    seen: HashSet<u32>,
}

/// A code address still being looked for.
struct Pending {
    at: u32,
    window: Window,
    candidates: Vec<u32>,
    first: usize,
    whole: bool,
    shrunk: bool,
    callers: Option<String>,
}

impl<'a> Matcher<'a> {
    pub fn new(from: &'a Image<'a>, to: &'a Image<'a>) -> Self {
        Self {
            from,
            to,
            calls: HashMap::new(),
            matched: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// Records a window matched once, and the calls in it.
    fn accept(&mut self, w: Window, at: u32) {
        if !self.seen.insert(w.start) {
            return;
        }
        self.matched.push((w, at));
        for from in w.each() {
            let word = self.from.word(from);
            let to = at + (from - w.start);
            if !arm::is_bl(word) || !arm::is_bl(self.to.word(to)) {
                continue;
            }
            let (Some(a), Some(b)) = (arm::target(word, from), arm::target(self.to.word(to), to))
            else {
                continue;
            };
            *self.calls.entry(a).or_default().entry(b).or_default() += 1;
        }
    }

    /// What the matched calls say `from`'s function `callee` is, if they
    /// agree, and how many say so.
    pub fn called(&self, callee: u32) -> Option<(u32, usize)> {
        let said = self.calls.get(&callee)?;
        let (&to, &n) = said.iter().max_by_key(|&(_, n)| *n)?;
        (said.len() == 1).then_some((to, n))
    }

    /// The candidates whose calls agree with every call already matched,
    /// and how many calls were checked.
    fn narrow(&self, w: Window, candidates: &[u32]) -> (Vec<u32>, usize) {
        let known: Vec<(u32, u32)> = (w.each())
            .filter(|&at| arm::is_bl(self.from.word(at)))
            .filter_map(|at| {
                let callee = arm::target(self.from.word(at), at)?;
                Some((at - w.start, self.called(callee)?.0))
            })
            .collect();
        let agrees = |&c: &u32| {
            known.iter().all(|&(off, to)| {
                let at = c + off;
                arm::target(self.to.word(at), at) == Some(to)
            })
        };
        (
            candidates.iter().copied().filter(agrees).collect(),
            known.len(),
        )
    }

    /// Where `w` matches in `to`, narrowed by the calls, and how many
    /// places it matched before that.
    fn find(&self, w: Window) -> (Vec<u32>, usize) {
        let found = self.to.search(self.from.masked(w));
        let first = found.len();
        if first <= 1 {
            return (found, first);
        }
        (self.narrow(w, &found).0, first)
    }

    /// Each code address in `to`. `ask_callers` lets a function whose own
    /// window can't settle it ask its callers.
    pub fn code(&mut self, addrs: &[u32], ask_callers: bool) -> Vec<Outcome> {
        let mut out: Vec<Option<Outcome>> = vec![None; addrs.len()];
        let mut pending = Vec::new();
        for (i, &at) in addrs.iter().enumerate() {
            if !self.from.in_text(u64::from(at)) {
                out[i] = Some(Err(format!("{at:#x} isn't code")));
                continue;
            }
            let window = self.from.window(at, false);
            let candidates = self.to.search(self.from.masked(window));
            let first = candidates.len();
            pending.push((
                i,
                Pending {
                    at,
                    window,
                    candidates,
                    first,
                    whole: false,
                    shrunk: false,
                    callers: None,
                },
            ));
        }
        loop {
            let mut progress = false;
            pending.retain_mut(|(i, p)| match self.settle(p, ask_callers, &mut progress) {
                Some(found) => {
                    out[*i] = Some(found);
                    false
                }
                None => true,
            });
            if !progress {
                break;
            }
        }
        for (i, p) in pending {
            out[i] = Some(Err(self.why(&p)));
        }
        out.into_iter().map(Option::unwrap).collect()
    }

    /// One step on `p`, the cheapest left: take a single match, narrow by
    /// calls, try the whole window, then the callers. Sets `progress` on
    /// anything learnt.
    fn settle(
        &mut self,
        p: &mut Pending,
        ask_callers: bool,
        progress: &mut bool,
    ) -> Option<Outcome> {
        let (at, first) = (p.at, p.first);
        let at_in = |w: Window, start: u32| start + (at - w.start);
        let found = |value, method, words, confidence, note: String| {
            Some(Ok(Found {
                value,
                method,
                words,
                candidates: first,
                confidence,
                note,
            }))
        };
        let sure = |w: Window| {
            if w.words >= 16 {
                Confidence::High
            } else {
                Confidence::Medium
            }
        };
        if p.candidates.len() == 1 {
            let c = p.candidates[0];
            self.accept(p.window, c);
            *progress = true;
            let method = if p.first == 1 {
                Method::Window
            } else {
                Method::Narrowed
            };
            let confidence = if p.first == 1 {
                sure(p.window)
            } else {
                Confidence::Medium
            };
            return found(
                at_in(p.window, c),
                method,
                p.window.words,
                confidence,
                String::new(),
            );
        }
        if p.candidates.len() > 1 {
            let (left, checked) = self.narrow(p.window, &p.candidates);
            if left.len() == 1 {
                self.accept(p.window, left[0]);
                *progress = true;
                let note = format!("{checked} matched calls agree");
                let value = at_in(p.window, left[0]);
                return found(
                    value,
                    Method::Narrowed,
                    p.window.words,
                    Confidence::Medium,
                    note,
                );
            }
            if left.len() < p.candidates.len() {
                p.candidates = left;
                *progress = true;
                return None;
            }
        }
        if !p.whole {
            p.whole = true;
            let whole = self.from.window(p.at, true);
            if whole != p.window {
                let (c, first) = self.find(whole);
                if c.len() == 1 {
                    self.accept(whole, c[0]);
                    *progress = true;
                    let note = format!("past its function's end ({first} matches before calls)");
                    let value = at_in(whole, c[0]);
                    return found(value, Method::Window, whole.words, sure(whole), note);
                }
            }
        }
        let entry = self.from.function(p.at).is_some_and(|f| f.start == p.at);
        if p.candidates.is_empty() && !p.shrunk {
            p.shrunk = true;
            if let Some((w, c)) = self.longest(p.window, p.at) {
                self.accept(w, c);
                *progress = true;
                let value = at_in(w, c);
                let mut note = format!(
                    "{} of its {} words, the rest changed",
                    w.words, p.window.words
                );
                let mut confidence = Confidence::Medium;
                // A function's referrers check a part that matched.
                if ask_callers && entry {
                    let (votes, asked) = self.referrers(p.at);
                    let n: usize = votes.values().sum();
                    let k = votes.get(&value).copied().unwrap_or(0);
                    if k * 3 < n * 2 {
                        let why = format!(
                            "{note}, at {value:#x}, but its referrers say {votes:x?} ({asked})"
                        );
                        return Some(Err(why));
                    }
                    if k > 0 {
                        note += &format!("; {k} of {n} matched referrers agree");
                    }
                    if k >= 2 {
                        confidence = Confidence::High;
                    }
                }
                return found(value, Method::Window, w.words, confidence, note);
            }
        }
        if ask_callers && entry && p.callers.is_none() {
            let (votes, asked) = self.referrers(p.at);
            *progress = true;
            let n: usize = votes.values().sum();
            let Some((&value, &k)) = votes.iter().max_by_key(|&(_, k)| *k) else {
                p.callers = Some(format!("{asked}, none matched once"));
                return None;
            };
            let note = format!("{k} of {n} matched referrers agree ({asked})");
            let starts: Vec<u32> = p.candidates.iter().map(|&c| at_in(p.window, c)).collect();
            if k * 3 < n * 2 || (!starts.is_empty() && !starts.contains(&value)) {
                p.callers = Some(format!(
                    "{note} on {value:#x}, which the window doesn't allow"
                ));
                return None;
            }
            // With its window matching nowhere, its first words at least
            // must agree with what its referrers say.
            let agreed = self.agreed(p.window, value);
            let confidence = match (starts.is_empty(), k >= 3 && k == n) {
                (false, true) => Confidence::High,
                (false, false) => Confidence::Medium,
                (true, true) if agreed >= AGREED => Confidence::Medium,
                (true, _) => Confidence::Low,
            };
            let note = if starts.is_empty() {
                format!(
                    "{note}; its window matches nowhere: its first {agreed} words agree there, then its code changed"
                )
            } else {
                note
            };
            return found(value, Method::Callers, p.window.words, confidence, note);
        }
        None
    }

    /// How many of `w`'s first masked words `to` holds at `at`.
    fn agreed(&self, w: Window, at: u32) -> u32 {
        let words = w.words.min((self.to.text.end.saturating_sub(at)) / 4);
        let there = self.to.masked(Window { start: at, words });
        let here = self.from.masked(w);
        here.iter().zip(there).take_while(|(a, b)| a == b).count() as u32
    }

    /// Why `p` wasn't found.
    fn why(&self, p: &Pending) -> String {
        let places = match p.first {
            0 => "nowhere".to_string(),
            n if n > MANY => format!("in more than {MANY} places"),
            n => format!("in {n} places"),
        };
        let mut why = format!("its {}-word window matched {places}", p.window.words);
        if p.first > 1 {
            why += &format!(", {} left after the matched calls", p.candidates.len());
        }
        if let Some(callers) = &p.callers {
            why += &format!("; {callers}");
        }
        why
    }

    /// The longest part of `w` holding `at` that still matches, from its
    /// start or to its end, if it matches once and is long enough to
    /// trust: for code that changed past (or before) that part.
    fn longest(&self, w: Window, at: u32) -> Option<(Window, u32)> {
        let d = (at - w.start) / 4;
        let part = |from: u32, to: u32| Window {
            start: w.start + 4 * from,
            words: to - from,
        };
        let count = |p: Window| self.to.search(self.from.masked(p)).len();
        // The longest prefix holding `at` that matches somewhere.
        let shortest = (d + 1).max(2);
        let head = (count(part(0, shortest)) > 0).then(|| {
            let (mut lo, mut hi) = (shortest, w.words);
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if count(part(0, mid)) > 0 {
                    lo = mid
                } else {
                    hi = mid - 1
                }
            }
            part(0, lo)
        });
        // The longest suffix holding it.
        let first = d.min(w.words - 2);
        let tail = (count(part(first, w.words)) > 0).then(|| {
            let (mut lo, mut hi) = (0, first);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if count(part(mid, w.words)) > 0 {
                    hi = mid
                } else {
                    lo = mid + 1
                }
            }
            part(lo, w.words)
        });
        [head, tail]
            .into_iter()
            .flatten()
            .filter(|p| p.words >= TRUSTED)
            .max_by_key(|p| p.words)
            .and_then(|p| match self.find(p).0[..] {
                [c] => Some((p, c)),
                _ => None,
            })
    }

    /// What everything in `from` that refers to the function `callee`
    /// refers to in `to`, by how many say so, and what was asked: its
    /// calls, the code that takes its address, and the slots that hold it.
    fn referrers(&mut self, callee: u32) -> (BTreeMap<u32, usize>, String) {
        let mut votes = BTreeMap::new();
        let mut vote = |v: Option<u32>| {
            if let Some(v) = v {
                *votes.entry(v).or_default() += 1;
            }
        };
        let calls = spread(self.from.callers(callee), ASKED);
        for &site in &calls {
            vote(self.call_in_to(site));
        }
        let loads = spread(self.from.references(&[callee]), ASKED);
        for &(_, adrp) in &loads {
            vote(self.use_in_to(adrp, false));
        }
        let slots = self.from.data_holding(&[u64::from(callee)]);
        let slots = spread(slots, ASKED);
        for &(_, slot) in &slots {
            vote(self.slot_in_to(slot));
        }
        let asked = format!(
            "{} calls, {} address loads, {} slots asked",
            calls.len(),
            loads.len(),
            slots.len()
        );
        (votes, asked)
    }

    /// Where the window round `at` in `from` matches once in `to`, whole
    /// or its longest part, recorded as matched.
    fn locate(&mut self, at: u32) -> Option<(Window, u32)> {
        let w = self.from.window(at, false);
        let (w, c) = match self.find(w) {
            (c, _) if c.len() == 1 => (w, c[0]),
            (c, _) if c.is_empty() => self.longest(w, at)?,
            _ => return None,
        };
        self.accept(w, c);
        Some((w, c))
    }

    /// Where the `bl` at `site` in `from` goes in `to`, if its window
    /// matches once.
    fn call_in_to(&mut self, site: u32) -> Option<u32> {
        let (w, c) = self.locate(site)?;
        let at = c + (site - w.start);
        let word = self.to.word(at);
        arm::target(word, at).filter(|_| arm::is_bl(word))
    }

    /// What the address the `adrp` at `adrp` makes (with its `add`, `ldr`
    /// or `str`) in `from` is in `to`, if its window matches once: or the 8
    /// bytes there, `through` a slot.
    fn use_in_to(&mut self, adrp: u32, through: bool) -> Option<u32> {
        let (lo, _) = self.from.resolve(adrp)?;
        let (w, c) = self.locate(adrp)?;
        if !w.contains(lo) {
            return None;
        }
        let at = c + (adrp - w.start);
        let (lo_to, addr) = self.to.resolve(at)?;
        if lo_to - at != lo - adrp {
            return None;
        }
        if through {
            self.to.quad(addr).and_then(|v| u32::try_from(v).ok())
        } else {
            Some(addr)
        }
    }

    /// What the vtable slot at `slot` in `from` holds in `to`: its
    /// vtable found by its neighbours, the run of code pointers round it.
    fn slot_in_to(&mut self, slot: u32) -> Option<u32> {
        let callee = self.from.quad(slot)? as u32;
        let code = |at: u32| self.from.quad(at).filter(|&f| self.from.in_text(f));
        let mut start = slot;
        while slot - start < 8 * SLOTS && start >= 8 && code(start - 8).is_some() {
            start -= 8;
        }
        let run = self.run(start);
        // Not the function asked about: its neighbours alone vouch for it.
        let others: Vec<u32> = run.iter().copied().filter(|&f| f != callee).collect();
        let map = self.slot_map(&others);
        let base = self.vtable(&run, &map).ok()?.value;
        let value = self.to.quad(base + (slot - start))?;
        u32::try_from(value).ok().filter(|_| self.to.in_text(value))
    }

    /// The code pointers from `at` on, at most [`SLOTS`].
    fn run(&self, at: u32) -> Vec<u32> {
        (0..SLOTS)
            .map_while(|k| self.from.quad(at + 8 * k).filter(|&f| self.from.in_text(f)))
            .map(|f| f as u32)
            .collect()
    }

    /// Each of `funcs` matched in `to`, where it's matched and not low.
    fn slot_map(&mut self, funcs: &[u32]) -> HashMap<u32, u32> {
        let mut funcs = funcs.to_vec();
        funcs.sort_unstable();
        funcs.dedup();
        let found = self.code(&funcs, false);
        (funcs.iter().zip(found))
            .filter_map(|(&f, o)| {
                o.ok()
                    .filter(|o| o.confidence > Confidence::Low)
                    .map(|o| (f, o.value))
            })
            .collect()
    }

    /// Each global (a GOT slot or what lives in bss) in `to`, by the code
    /// that loads it: straight, or through a slot that holds it.
    pub fn globals(&mut self, addrs: &[u32], usual: Option<Shifts>) -> Vec<Outcome> {
        let values: Vec<u64> = addrs.iter().map(|&a| u64::from(a)).collect();
        let slots = self.from.data_holding(&values);
        let wanted: Vec<u32> = addrs
            .iter()
            .copied()
            .chain(slots.iter().map(|s| s.1))
            .collect();
        let refs = self.from.references(&wanted);
        addrs
            .iter()
            .map(|&g| {
                let through: Vec<u32> = (slots.iter())
                    .filter(|s| s.0 == u64::from(g))
                    .map(|s| s.1)
                    .collect();
                let uses: Vec<(u32, bool)> = (refs.iter())
                    .filter_map(|&(to, at)| {
                        (to == g)
                            .then_some((at, false))
                            .or_else(|| through.contains(&to).then_some((at, true)))
                    })
                    .collect();
                self.global(g, &spread(uses, ASKED), usual)
            })
            .collect()
    }

    fn global(&mut self, g: u32, uses: &[(u32, bool)], usual: Option<Shifts>) -> Outcome {
        let mut votes = BTreeMap::new();
        for &(adrp, through) in uses {
            if let Some(v) = self.use_in_to(adrp, through) {
                *votes.entry((v, through)).or_insert(0usize) += 1;
            }
        }
        let mut tally: BTreeMap<u32, usize> = BTreeMap::new();
        for (&(v, _), &k) in &votes {
            *tally.entry(v).or_default() += k;
        }
        let n: usize = tally.values().sum();
        let (value, k) = (tally.iter())
            .max_by_key(|&(_, k)| *k)
            .map(|(&v, &k)| (v, k))
            .ok_or_else(|| format!("{} uses, none matched once", uses.len()))?;
        if k * 3 < n * 2 {
            return Err(format!("its uses disagree: {tally:x?}"));
        }
        let slotted = votes
            .iter()
            .filter(|((v, t), _)| *v == value && *t)
            .map(|(_, k)| k)
            .sum::<usize>();
        let mut note = format!("{k} of {n} matched uses agree ({slotted} through a GOT slot)");
        let mut confidence = if k >= 3 && k == n {
            Confidence::High
        } else {
            Confidence::Medium
        };
        if let Some(usual) = usual {
            let expected = if g < self.from.data.end {
                usual.data
            } else {
                usual.bss
            };
            let shift = i64::from(value) - i64::from(g);
            if shift != expected {
                note += &format!(
                    "; moved {}, not the usual {}",
                    signed(shift),
                    signed(expected)
                );
                confidence = confidence.min(Confidence::Medium);
            }
        }
        Ok(Found {
            value,
            method: Method::Votes,
            words: 0,
            candidates: n,
            confidence,
            note,
        })
    }

    /// Each vtable in `to`, by where its slots' functions, matched first,
    /// sit together.
    pub fn vtables(&mut self, addrs: &[u32]) -> Vec<Outcome> {
        let slots: Vec<Vec<u32>> = addrs.iter().map(|&v| self.run(v)).collect();
        let map = self.slot_map(&slots.concat());
        slots.iter().map(|s| self.vtable(s, &map)).collect()
    }

    fn vtable(&self, slots: &[u32], map: &HashMap<u32, u32>) -> Outcome {
        let mapped: Vec<(u32, u32)> = (0u32..)
            .zip(slots)
            .filter_map(|(k, f)| Some((k, *map.get(f)?)))
            .collect();
        let values: Vec<u64> = mapped.iter().map(|&(_, f)| u64::from(f)).collect();
        let mut votes: BTreeMap<u32, usize> = BTreeMap::new();
        for (value, at) in self.to.data_holding(&values) {
            for &(k, f) in &mapped {
                if u64::from(f) == value && at >= 8 * k {
                    *votes.entry(at - 8 * k).or_default() += 1;
                }
            }
        }
        let n = mapped.len();
        let (base, k) = (votes.iter())
            .max_by_key(|&(_, k)| *k)
            .map(|(&b, &k)| (b, k))
            .ok_or_else(|| {
                format!(
                    "{n} of its {} slots matched, none found in data",
                    slots.len()
                )
            })?;
        let tied = votes.values().filter(|&&v| v == k).count();
        if k * 3 < n * 2 || k < 2 || tied > 1 {
            let also = tied - 1;
            return Err(format!(
                "{k} of {n} matched slots agree on {base:#x} ({also} other places as well)"
            ));
        }
        Ok(Found {
            value: base,
            method: Method::Slots,
            words: 0,
            candidates: votes.len(),
            confidence: if k >= 3 && k == n {
                Confidence::High
            } else {
                Confidence::Medium
            },
            note: format!("{k} of {n} matched slots agree, of {}", slots.len()),
        })
    }

    /// An import's stub in `to`, by the name `from`'s stub at `at` jumps
    /// through.
    pub fn import(&self, at: u32) -> Outcome {
        let name =
            (self.from.import_name(at)).ok_or_else(|| format!("{at:#x} isn't an import's stub"))?;
        let value = (self.to.import_stub(name)).ok_or_else(|| format!("no stub for {name}"))?;
        Ok(Found {
            value,
            method: Method::Import,
            words: 0,
            candidates: 1,
            confidence: Confidence::High,
            note: name.to_string(),
        })
    }
}

/// `v` in hex with its sign.
fn signed(v: i64) -> String {
    let sign = if v < 0 { "-" } else { "" };
    format!("{sign}{:#x}", v.unsigned_abs())
}

/// At most `max` of `v`, spread evenly over it.
fn spread<T: Copy>(v: Vec<T>, max: usize) -> Vec<T> {
    if v.len() <= max {
        return v;
    }
    (0..max).map(|i| v[i * v.len() / max]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreading_keeps_the_ends_apart() {
        assert_eq!(spread(vec![1, 2, 3], 5), [1, 2, 3]);
        assert_eq!(spread((0..10).collect(), 5), [0, 2, 4, 6, 8]);
        assert_eq!(signed(-0x703f8), "-0x703f8");
        assert_eq!(signed(0x10), "0x10");
    }
}
