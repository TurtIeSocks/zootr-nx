//! Shrinks `world.json` without changing what any rule says: an `any` of
//! single items becomes one items rule ([`merge`]), and a rule many
//! places hold is written once ([`share`]).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// The shortest rule [`share`] writes once, as compact JSON: a shorter
/// one costs about what its ref does.
const SHORTEST: usize = 48;

/// `value` with each `any`'s count-1 item rules merged into one, which AP
/// reads the same: any of the names. A rule naming any of `lined` stays
/// apart, since `rules.lined` reads each such rule as a line's level.
pub(super) fn merge(value: &mut Value, lined: &BTreeSet<&str>) {
    match value {
        Value::Array(parts) => parts.iter_mut().for_each(|v| merge(v, lined)),
        Value::Object(o) => o.values_mut().for_each(|v| merge(v, lined)),
        _ => return,
    }
    if let Some(merged) = value.as_object().and_then(|o| merged(o, lined)) {
        *value = merged;
    }
}

fn merged(rule: &Map<String, Value>, lined: &BTreeSet<&str>) -> Option<Value> {
    let parts = rule.get("any")?.as_array()?;
    let single = |p: &&Value| {
        let names = p.get("items").and_then(Value::as_array);
        let free = |n: &Value| n.as_str().is_some_and(|n| !lined.contains(n));
        p.get("count") == Some(&json!(1)) && names.is_some_and(|n| n.iter().all(free))
    };
    let (ones, mut rest): (Vec<&Value>, Vec<&Value>) = parts.iter().partition(single);
    if ones.len() < 2 {
        return None;
    }
    let names = ones
        .iter()
        .flat_map(|p| p["items"].as_array().into_iter().flatten());
    let names: BTreeSet<&str> = names.filter_map(Value::as_str).collect();
    let one = json!({ "items": names, "count": 1 });
    rest.push(&one);
    Some(match rest.as_slice() {
        [only] => (*only).clone(),
        _ => json!({ "any": rest }),
    })
}

/// Each `all` or `any` rule `world` holds twice or more, written once in
/// `world["rules"]` and held as `{"ref": n}`. Shortest first, so a shared
/// rule refs only earlier ones.
pub(super) fn share(world: &mut Value) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    count(world, &mut counts);
    let mut shared: Vec<String> = (counts.into_iter())
        .filter(|(key, n)| *n > 1 && key.len() >= SHORTEST)
        .map(|(key, _)| key)
        .collect();
    shared.sort_by_key(String::len);
    let index: BTreeMap<&str, usize> = (shared.iter().enumerate())
        .map(|(i, key)| (key.as_str(), i))
        .collect();
    let mut rules = Vec::new();
    for key in &shared {
        let mut rule: Value = serde_json::from_str(key).expect("a rule we wrote");
        refs(&mut rule, &index);
        rules.push(rule);
    }
    refs(world, &index);
    world["rules"] = Value::Array(rules);
}

fn is_rule(value: &Value) -> bool {
    value.get("all").is_some() || value.get("any").is_some()
}

/// How many times `value` holds each rule, by its JSON.
fn count(value: &Value, counts: &mut BTreeMap<String, usize>) {
    if is_rule(value) {
        *counts.entry(value.to_string()).or_default() += 1;
    }
    match value {
        Value::Array(a) => a.iter().for_each(|v| count(v, counts)),
        Value::Object(o) => o.values().for_each(|v| count(v, counts)),
        _ => {}
    }
}

/// `value`'s shared rules below it as refs.
fn refs(value: &mut Value, index: &BTreeMap<&str, usize>) {
    let children: Box<dyn Iterator<Item = &mut Value>> = match value {
        Value::Array(a) => Box::new(a.iter_mut()),
        Value::Object(o) => Box::new(o.values_mut()),
        _ => return,
    };
    for child in children {
        match is_rule(child).then(|| index.get(child.to_string().as_str())) {
            Some(Some(&n)) => *child = json!({ "ref": n }),
            _ => refs(child, index),
        }
    }
}

/// `world` with each ref as the rule it names, as the world reads it.
#[cfg(test)]
pub(super) fn expand(world: &Value) -> Value {
    fn resolve(value: &Value, rules: &[Value]) -> Value {
        match value {
            Value::Object(o) if o.contains_key("ref") => {
                rules[o["ref"].as_u64().unwrap() as usize].clone()
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), resolve(v, rules)))
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(|v| resolve(v, rules)).collect()),
            _ => value.clone(),
        }
    }
    let mut rules: Vec<Value> = Vec::new();
    for rule in world["rules"].as_array().into_iter().flatten() {
        let rule = resolve(rule, &rules);
        rules.push(rule);
    }
    let mut out = resolve(world, &rules);
    out.as_object_mut().unwrap().remove("rules");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(names: &[&str]) -> Value {
        json!({ "items": names, "count": 1 })
    }

    #[test]
    fn an_any_of_single_items_reads_as_one_rule() {
        let lined = BTreeSet::from(["Fake Line"]);
        let mut rule =
            json!({ "any": [one(&["Fake B"]), one(&["Fake A"]), { "setting": "fake" }] });
        merge(&mut rule, &lined);
        let want = json!({ "any": [{ "setting": "fake" }, one(&["Fake A", "Fake B"])] });
        assert_eq!(rule, want);
        let mut alone = json!({ "any": [one(&["Fake B"]), one(&["Fake A"])] });
        merge(&mut alone, &lined);
        assert_eq!(alone, one(&["Fake A", "Fake B"]));
    }

    #[test]
    fn a_line_s_names_stay_apart() {
        let lined = BTreeSet::from(["Fake Line"]);
        let rule = json!({ "any": [one(&["Fake Line"]), one(&["Fake A"])] });
        let mut merged = rule.clone();
        merge(&mut merged, &lined);
        assert_eq!(merged, rule);
    }

    #[test]
    fn a_shared_rule_is_written_once_and_expands_back() {
        let long =
            |n: &str| json!({ "all": [one(&[n]), { "setting": "fake-setting-long-enough" }] });
        let outer = json!({ "any": [long("Fake A"), one(&["Fake C"])] });
        let world = json!({ "exits": [
            { "rule": outer.clone() }, { "rule": outer.clone() }, { "rule": long("Fake A") },
        ] });
        let mut shared = world.clone();
        share(&mut shared);
        let rules = shared["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(shared["exits"][0]["rule"], json!({ "ref": 1 }));
        assert_eq!(rules[1]["any"][0], json!({ "ref": 0 }));
        assert_eq!(expand(&shared), world);
    }
}
