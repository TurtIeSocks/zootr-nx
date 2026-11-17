//! A [`Need`] as a rule tree: `true`, `false`, `{items, count}`,
//! `{setting}`, `{all}` and `{any}`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::export::Namer;
use super::seed::Seed;
use crate::rules::Need;
use crate::world::Era;

/// Turns a [`Need`] into a rule tree, noting each item name a rule uses.
pub(super) struct Rules<'a> {
    pub seed: &'a Seed,
    pub names: &'a Namer,
    pub flag_settings: &'a BTreeMap<String, Vec<String>>,
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
            Need::Flag(f) if self.seed.start.flags.contains(f) => true.into(),
            Need::Flag(f) => {
                let ids = self.flag_settings.get(f).into_iter().flatten();
                any(ids.map(|id| json!({ "setting": id })).collect())
            }
            Need::Items { actors, count } => {
                let have = &self.seed.start.items;
                let held: u32 = actors.iter().filter_map(|a| have.get(a)).sum();
                if held >= *count {
                    return true.into();
                }
                let mut names = BTreeSet::new();
                for a in actors {
                    if self.seed.pool.iter().any(|p| p == a)
                        && let Ok(n) = self.names.item(a)
                    {
                        names.insert(n.to_owned());
                    }
                    names.extend(self.givers.get(a.as_str()).into_iter().flatten().cloned());
                }
                if names.is_empty() {
                    return false.into();
                }
                self.used.extend(names.iter().cloned());
                json!({ "items": names, "count": count - held })
            }
            Need::All(needs) => all(needs.iter().map(|n| self.rule(n, era)).collect()),
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
    let mut rest: Vec<Value> = rules
        .into_iter()
        .filter(|r| *r != Value::Bool(false))
        .collect();
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
}
