//! A [`Need`] as a rule tree: `true`, `false`, `{items, count}`,
//! `{setting}`, `{all}` and `{any}`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::export::Namer;
use crate::rules::{Have, Need};
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
                        && let Ok(n) = self.names.item(a)
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
                let rule = all(needs.iter().map(|n| self.rule(n, era)).collect());
                if rule == Value::Bool(false) {
                    self.used = used;
                }
                rule
            }
            Need::Any(needs) => any(needs.iter().map(|n| self.rule(n, era)).collect()),
        }
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
    fn any_keeps_the_smaller_count_of_the_same_items() {
        let n = |count: u32| json!({ "items": ["Magic Meter"], "count": count });
        assert_eq!(any(vec![n(2), n(1)]), n(1));
        assert_eq!(any(vec![n(1), n(2)]), n(1));
    }
}
