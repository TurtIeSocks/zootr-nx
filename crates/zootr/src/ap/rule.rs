//! A [`Need`] as a rule tree: `true`, `false`, `{items, count}`,
//! `{setting}`, `{all}` and `{any}`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::export::Namer;
use crate::rules::{Have, Need};
use crate::settings::{PROGRESSIVE, value_word};
use crate::world::Era;

/// Turns a [`Need`] into a rule tree, noting each item name a rule uses.
/// Rules are written once for every variant the export holds: an item
/// comes from any variant's pool or any variant's giver, and what an
/// option hands over at the start is that option's setting rule.
pub(super) struct Rules<'a> {
    /// What Link starts with in the base.
    pub start: &'a Have,
    /// Every item any variant pools.
    pub pooled: &'a BTreeSet<String>,
    pub names: &'a Namer,
    /// Each start flag an option sets, and the settings that set it.
    pub flag_settings: &'a BTreeMap<String, Vec<Value>>,
    /// Each item an option starts Link with, the settings and how many.
    pub item_settings: &'a BTreeMap<String, Vec<(Value, u32)>>,
    pub givers: &'a BTreeMap<&'a str, Vec<String>>,
    pub used: BTreeSet<String>,
    /// Progressive items on or off, under an `all` holding that setting's
    /// word (`Ctx::progressive`): its items go by that form's names.
    pub form: Option<bool>,
    /// Each line's name and its levels' names, lowest first (`lined`).
    pub levels: &'a [Line],
}

impl Rules<'_> {
    pub fn rule(&mut self, need: &Need, era: Era) -> Value {
        match need {
            Need::Always => true.into(),
            Need::Never => false.into(),
            Need::Era(e) => (*e == era).into(),
            Need::Setting(id) => json!({ "setting": id }),
            Need::Flag(f) if self.start.flag(f) => true.into(),
            Need::Flag(f) => {
                let mut rules: Vec<Value> = self
                    .flag_settings
                    .get(f)
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect();
                // A room's flag is an event, given where the room is.
                if let Some(events) = self.givers.get(f.as_str()) {
                    self.used.extend(events.iter().cloned());
                    rules.push(json!({ "items": events, "count": 1 }));
                }
                any(rules)
            }
            Need::Items { actors, .. } if self.form.is_none() => {
                // Outside either form, items whose names differ by it read
                // each form's names behind its word.
                let names = self.names;
                let named = |p| actors.iter().map(move |a| names.item_in(a, Some(p)).ok());
                if named(true).eq(named(false)) {
                    return self.rule_in(need, era, Some(self.names.progressive()));
                }
                let on = self.rule_in(need, era, Some(true));
                let off = self.rule_in(need, era, Some(false));
                self.collapse(on, off)
            }
            Need::Items { actors, count } => {
                let have = &self.start.items;
                let held: u32 = actors.iter().filter_map(|a| have.get(a)).sum();
                if held >= *count {
                    return true.into();
                }
                // An option that starts Link with enough of them.
                let mut given: Vec<(&Value, u32)> = Vec::new();
                for a in actors {
                    for (rule, n) in self.item_settings.get(a.as_str()).into_iter().flatten() {
                        match given.iter_mut().find(|g| g.0 == rule) {
                            Some(g) => g.1 += n,
                            None => given.push((rule, *n)),
                        }
                    }
                }
                let mut rules: Vec<Value> = (given.into_iter())
                    .filter(|(_, n)| held + n >= *count)
                    .map(|(rule, _)| rule.clone())
                    .collect();
                let mut names = BTreeSet::new();
                for a in actors {
                    if self.pooled.contains(a.as_str())
                        && let Ok(n) = self.names.item_in(a, self.form)
                    {
                        names.insert(n.to_owned());
                    }
                    names.extend(self.givers.get(a.as_str()).into_iter().flatten().cloned());
                }
                if !names.is_empty() {
                    self.used.extend(names.iter().cloned());
                    rules.push(json!({ "items": names, "count": count - held }));
                }
                any(rules)
            }
            Need::All(needs) => {
                // An item under a false `all` is never needed.
                let used = self.used.clone();
                let form = needs.iter().find_map(form).or(self.form);
                let outer = std::mem::replace(&mut self.form, form);
                let rule = all(needs.iter().map(|n| self.rule(n, era)).collect());
                self.form = outer;
                if rule == Value::Bool(false) {
                    self.used = used;
                }
                rule
            }
            Need::Any(needs) => match fork(needs) {
                Some((on, off)) => {
                    let on = self.rule_in(on, era, Some(true));
                    let off = self.rule_in(off, era, Some(false));
                    self.collapse(on, off)
                }
                None => any(needs.iter().map(|n| self.rule(n, era)).collect()),
            },
        }
    }

    /// `off` alone where the world, reading it with progressive items on
    /// (`lined`), gets `on`; else either behind its word. Each form's
    /// names count as used either way.
    fn collapse(&self, on: Value, off: Value) -> Value {
        if lined(&off, self.levels) == on {
            return off;
        }
        any(vec![
            all(vec![json!({ "setting": PROGRESSIVE }), on]),
            all(vec![
                json!({ "setting": value_word(PROGRESSIVE, "off") }),
                off,
            ]),
        ])
    }

    fn rule_in(&mut self, need: &Need, era: Era, form: Option<bool>) -> Value {
        let outer = std::mem::replace(&mut self.form, form);
        let rule = self.rule(need, era);
        self.form = outer;
        rule
    }
}

/// Each form of a [`Ctx::progressive`](crate::rules::Ctx::progressive)
/// fork, on then off.
fn fork(needs: &[Need]) -> Option<(&Need, &Need)> {
    let [Need::All(on), Need::All(off)] = needs else {
        return None;
    };
    match (on.as_slice(), off.as_slice()) {
        ([word_on, on], [word_off, off])
            if form(word_on) == Some(true) && form(word_off) == Some(false) =>
        {
            Some((on, off))
        }
        _ => None,
    }
}

/// An upgrade line's name and its levels, lowest first: each level's own
/// name, then each event that gives it.
pub type Line = (String, Vec<Vec<String>>);

/// `rule` as the world reads it with progressive items on: one of some
/// levels of a line, the lowest the nth, reads as n of the line or its
/// levels' events, the way `rules::item` reads a level each way. The
/// world's `rules.lined` does the same.
pub(super) fn lined(rule: &Value, levels: &[Line]) -> Value {
    let Value::Object(o) = rule else {
        return rule.clone();
    };
    for key in ["all", "any"] {
        if let Some(parts) = o.get(key).and_then(Value::as_array) {
            return json!({ key: parts.iter().map(|p| lined(p, levels)).collect::<Vec<_>>() });
        }
    }
    let Some(items) = o.get("items").and_then(Value::as_array) else {
        return rule.clone();
    };
    if o.get("count") != Some(&json!(1)) {
        return rule.clone();
    }
    let names: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
    for (line, steps) in levels {
        let at: Option<Vec<usize>> = (names.iter())
            .map(|n| steps.iter().position(|s| s.iter().any(|s| s == n)))
            .collect();
        if let Some(lowest) = at.and_then(|at| at.into_iter().min()) {
            let events = steps.iter().flat_map(|s| &s[1..]);
            let items: BTreeSet<&String> = events.chain([line]).collect();
            return json!({ "items": items, "count": lowest + 1 });
        }
    }
    rule.clone()
}

/// The progressive items form `need` picks, if it's that setting's word.
fn form(need: &Need) -> Option<bool> {
    match need {
        Need::Setting(id) if id == PROGRESSIVE => Some(true),
        Need::Setting(id) if *id == value_word(PROGRESSIVE, "off") => Some(false),
        _ => None,
    }
}

pub(super) fn all(rules: Vec<Value>) -> Value {
    if rules.contains(&Value::Bool(false)) {
        return false.into();
    }
    let mut rest: Vec<Value> = rules
        .into_iter()
        .filter(|r| *r != Value::Bool(true))
        .collect();
    match rest.len() {
        0 => true.into(),
        1 => rest.remove(0),
        _ => json!({ "all": rest }),
    }
}

pub(super) fn any(rules: Vec<Value>) -> Value {
    if rules.contains(&Value::Bool(true)) {
        return true.into();
    }
    let mut rest: Vec<Value> = Vec::new();
    for r in rules.into_iter().filter(|r| *r != Value::Bool(false)) {
        // Of two counts of the same items, the smaller is enough.
        let same = rest
            .iter_mut()
            .find(|o| o.get("items").is_some() && o.get("items") == r.get("items"));
        match same {
            Some(o) if o["count"].as_u64() > r["count"].as_u64() => *o = r,
            Some(_) => {}
            None => rest.push(r),
        }
    }
    match rest.len() {
        0 => false.into(),
        1 => rest.remove(0),
        _ => json!({ "any": rest }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_and_any_fold_constants() {
        let item = json!({ "items": ["Hookshot"], "count": 1 });
        assert_eq!(all(vec![true.into(), item.clone()]), item);
        assert_eq!(all(vec![false.into(), item.clone()]), Value::Bool(false));
        assert_eq!(any(vec![false.into(), item.clone()]), item);
        assert_eq!(any(vec![true.into(), item.clone()]), Value::Bool(true));
        assert_eq!(all(vec![]), Value::Bool(true));
        assert_eq!(any(vec![]), Value::Bool(false));
    }

    #[test]
    fn lined_reads_a_run_of_levels_as_that_many_of_the_line() {
        let steps = vec![vec!["Fake One".into()], vec!["Fake Two".into()]];
        let levels = [("Progressive Fake".to_owned(), steps)];
        let rule = |items: &[&str], count: u32| json!({ "items": items, "count": count });
        let lined = |r: Value| lined(&r, &levels);
        assert_eq!(
            lined(rule(&["Fake Two"], 1)),
            rule(&["Progressive Fake"], 2)
        );
        assert_eq!(
            lined(rule(&["Fake Two", "Fake One"], 1)),
            rule(&["Progressive Fake"], 1)
        );
        assert_eq!(
            lined(rule(&["Fake One"], 1)),
            rule(&["Progressive Fake"], 1)
        );
        // Another item among them, or more than one: as it is.
        assert_eq!(
            lined(rule(&["Fake Two", "Fake Other"], 1)),
            rule(&["Fake Two", "Fake Other"], 1)
        );
        assert_eq!(lined(rule(&["Fake Two"], 2)), rule(&["Fake Two"], 2));
        let nested = json!({ "any": [rule(&["Fake Two"], 1), true] });
        assert_eq!(
            lined(nested),
            json!({ "any": [rule(&["Progressive Fake"], 2), true] })
        );
        // A level's event counts as that level, and joins the line's name.
        let gift = vec![
            vec!["Fake One".into(), "Fake Gift".into()],
            vec!["Fake Two".into()],
        ];
        let levels = [("Progressive Fake".to_owned(), gift)];
        let both = rule(&["Fake Gift", "Progressive Fake"], 2);
        assert_eq!(super::lined(&rule(&["Fake Two"], 1), &levels), both);
    }

    #[test]
    fn any_keeps_the_smaller_count_of_the_same_items() {
        let n = |count: u32| json!({ "items": ["Magic Meter"], "count": count });
        assert_eq!(any(vec![n(2), n(1)]), n(1));
        assert_eq!(any(vec![n(1), n(2)]), n(1));
    }
}
