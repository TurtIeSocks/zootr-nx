//! When a row of the export holds: a rule over setting words, found from
//! the seeds the exporter built, one per variant of the offered options.
//!
//! A row's count in any settings is its count in the base plus, for each
//! variant those settings match, what that variant changed against the
//! seed it was varied from. Tests check that sum against real seeds.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::rule::{all, any};
use crate::settings::{Settings, value_word};

/// One option the rows depend on, with every value it can take.
#[derive(Clone, Debug)]
pub struct Domain {
    pub id: String,
    /// The first is the base's.
    pub values: Vec<Value>,
}

/// One built seed's settings over the base, and the seed it was varied
/// from.
#[derive(Clone, Debug)]
pub struct Variant {
    pub overlay: Settings,
    /// Index into the variants, or `None` for the base.
    pub from: Option<usize>,
}

impl Variant {
    /// Whether `settings` match this variant.
    fn holds(&self, settings: &BTreeMap<&str, &Value>) -> bool {
        (self.overlay.iter()).all(|(k, v)| settings.get(k.as_str()) == Some(&v))
    }
}

/// A row's count in the base and in each variant's seed.
pub struct Counts<'a> {
    pub base: u32,
    pub by_variant: &'a [u32],
}

/// The row's `when` rules, one per count step past the count it always
/// has: `(count, rule)` pairs, the first with no rule. A row always
/// present once gives `[(1, true)]`.
pub fn steps(domains: &[Domain], variants: &[Variant], counts: &Counts) -> Vec<(u32, Value)> {
    let deltas: Vec<i64> = (variants.iter().enumerate())
        .map(|(i, v)| {
            let from = v.from.map_or(counts.base, |f| counts.by_variant[f]);
            i64::from(counts.by_variant[i]) - i64::from(from)
        })
        .collect();
    // Only the options of variants that change the row matter.
    let touched: Vec<&Domain> = (domains.iter())
        .filter(|d| {
            (variants.iter().zip(&deltas)).any(|(v, d2)| *d2 != 0 && v.overlay.contains_key(&d.id))
        })
        .collect();
    let count = |at: &[usize]| -> i64 {
        let settings: BTreeMap<&str, &Value> = (touched.iter().zip(at))
            .map(|(d, i)| (d.id.as_str(), &d.values[*i]))
            .collect();
        let mut n = i64::from(counts.base);
        for (v, d) in variants.iter().zip(&deltas) {
            // A variant on an option outside `touched` changes nothing here.
            if *d != 0 && v.holds(&settings) {
                n += d;
            }
        }
        n
    };
    let table: Vec<(Vec<usize>, i64)> = assignments(&touched)
        .into_iter()
        .map(|a| {
            let n = count(&a);
            (a, n)
        })
        .collect();
    let low = table.iter().map(|r| r.1).min().unwrap_or(0).max(0);
    let high = table.iter().map(|r| r.1).max().unwrap_or(0).max(0);
    let mut out: Vec<(u32, Value)> = Vec::new();
    if low > 0 {
        out.push((low as u32, true.into()));
    }
    for step in low + 1..=high {
        let truth: Vec<(Vec<usize>, bool)> =
            table.iter().map(|(a, n)| (a.clone(), *n >= step)).collect();
        let rule = simplest(&touched, &truth);
        match out.last_mut() {
            Some(last) if last.1 == rule => last.0 += 1,
            _ => out.push((1, rule)),
        }
    }
    out
}

/// Every assignment of a value to each domain, by value index.
fn assignments(domains: &[&Domain]) -> Vec<Vec<usize>> {
    let mut out = vec![vec![]];
    for d in domains {
        out = (out.into_iter())
            .flat_map(|a| {
                (0..d.values.len()).map(move |i| {
                    let mut a = a.clone();
                    a.push(i);
                    a
                })
            })
            .collect();
    }
    out
}

/// Per domain, the value indexes a cube allows.
type Cube = Vec<Vec<usize>>;

fn inside(cube: &Cube, a: &[usize]) -> bool {
    cube.iter().zip(a).all(|(allowed, i)| allowed.contains(i))
}

/// Whether every assignment in `cube` is true.
fn valid(cube: &Cube, truth: &[(Vec<usize>, bool)]) -> bool {
    (truth.iter()).all(|(a, t)| *t || !inside(cube, a))
}

/// The shortest conjunction of setting words true in exactly the true
/// assignments, or a union of conjunctions when none is.
fn simplest(domains: &[&Domain], truth: &[(Vec<usize>, bool)]) -> Value {
    let trues: Vec<&Vec<usize>> = truth.iter().filter(|r| r.1).map(|r| &r.0).collect();
    if trues.is_empty() {
        return false.into();
    }
    // The smallest cube holding every true assignment.
    let hull: Cube = (0..domains.len())
        .map(|d| {
            let mut v: Vec<usize> = trues.iter().map(|a| a[d]).collect();
            v.sort();
            v.dedup();
            v
        })
        .collect();
    if valid(&hull, truth) {
        return conjunction(domains, &hull);
    }
    // Grow a cube from each true assignment not yet covered.
    let mut cubes: Vec<Cube> = Vec::new();
    for a in trues {
        if cubes.iter().any(|c| inside(c, a)) {
            continue;
        }
        let mut cube: Cube = a.iter().map(|i| vec![*i]).collect();
        for (d, domain) in domains.iter().enumerate() {
            for i in 0..domain.values.len() {
                if cube[d].contains(&i) {
                    continue;
                }
                let mut grown = cube.clone();
                grown[d].push(i);
                grown[d].sort();
                if valid(&grown, truth) {
                    cube = grown;
                }
            }
        }
        cubes.push(cube);
    }
    any(cubes.iter().map(|c| conjunction(domains, c)).collect())
}

fn conjunction(domains: &[&Domain], cube: &Cube) -> Value {
    let parts = (domains.iter().zip(cube))
        .filter(|(d, allowed)| allowed.len() < d.values.len())
        .map(|(d, allowed)| {
            let words = allowed
                .iter()
                .map(|i| json!({ "setting": word(&d.id, &d.values[*i]) }));
            any(words.collect())
        });
    all(parts.collect())
}

/// The setting word for `id` at `value`: a toggle's id when on, `id=off`
/// when off, a choice's `id=option`.
pub fn word(id: &str, value: &Value) -> String {
    match value {
        Value::Bool(true) => id.to_owned(),
        Value::Bool(false) => value_word(id, "off"),
        Value::String(option) => value_word(id, option),
        other => format!("{id}={other}"),
    }
}

#[cfg(test)]
/// Whether `rule`, a tree of setting words, holds for `settings` (each
/// offered option's value).
pub fn holds(rule: &Value, settings: &Settings) -> bool {
    match rule {
        Value::Bool(b) => *b,
        Value::Object(o) if o.contains_key("setting") => {
            let w = o["setting"].as_str().unwrap_or_default();
            let (id, value) = w.split_once('=').unwrap_or((w, ""));
            match settings.get(id) {
                Some(Value::Bool(on)) => match value {
                    "" | "on" => *on,
                    _ => !*on,
                },
                Some(Value::String(s)) => s == value,
                _ => false,
            }
        }
        Value::Object(o) if o.contains_key("all") => {
            (o["all"].as_array().into_iter().flatten()).all(|r| holds(r, settings))
        }
        Value::Object(o) if o.contains_key("any") => {
            (o["any"].as_array().into_iter().flatten()).any(|r| holds(r, settings))
        }
        _ => panic!("{rule} isn't a rule over setting words"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toggle(id: &str) -> Domain {
        Domain {
            id: id.into(),
            values: vec![false.into(), true.into()],
        }
    }

    fn on(pairs: &[(&str, Value)]) -> Settings {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect()
    }

    #[test]
    fn a_row_one_toggle_adds_holds_with_that_toggle() {
        let domains = [toggle("songs")];
        let variants = [Variant {
            overlay: on(&[("songs", true.into())]),
            from: None,
        }];
        let counts = Counts {
            base: 0,
            by_variant: &[1],
        };
        assert_eq!(
            steps(&domains, &variants, &counts),
            vec![(1, json!({ "setting": "songs" }))]
        );
    }

    #[test]
    fn a_base_row_a_toggle_removes_holds_with_it_off() {
        let domains = [toggle("npc-gifts")];
        let variants = [Variant {
            overlay: on(&[("npc-gifts", true.into())]),
            from: None,
        }];
        let counts = Counts {
            base: 1,
            by_variant: &[0],
        };
        let off = json!({ "setting": "npc-gifts=off" });
        assert_eq!(steps(&domains, &variants, &counts), vec![(1, off)]);
    }

    #[test]
    fn counts_add_up_step_by_step() {
        let domains = [toggle("a"), toggle("b")];
        let variants = [
            Variant {
                overlay: on(&[("a", true.into())]),
                from: None,
            },
            Variant {
                overlay: on(&[("b", true.into())]),
                from: None,
            },
        ];
        let counts = Counts {
            base: 2,
            by_variant: &[3, 4],
        };
        let rows = steps(&domains, &variants, &counts);
        assert_eq!(rows[0], (2, Value::Bool(true)));
        let at = |a: bool, b: bool| {
            let s = on(&[("a", a.into()), ("b", b.into())]);
            rows.iter()
                .filter(|r| holds(&r.1, &s))
                .map(|r| r.0)
                .sum::<u32>()
        };
        assert_eq!(
            [
                at(false, false),
                at(true, false),
                at(false, true),
                at(true, true)
            ],
            [2, 3, 4, 5]
        );
    }

    #[test]
    fn a_choice_reads_as_the_other_options() {
        let fortress = Domain {
            id: "gerudo-fortress".into(),
            values: vec!["normal".into(), "fast".into(), "freed".into()],
        };
        let domains = [toggle("npc-gifts"), fortress];
        let joint = |gifts: bool, f: &str| Variant {
            overlay: on(&[("npc-gifts", gifts.into()), ("gerudo-fortress", f.into())]),
            from: None,
        };
        let variants = [
            joint(true, "normal"),
            joint(true, "fast"),
            joint(true, "freed"),
            joint(false, "fast"),
            joint(false, "freed"),
        ];
        let counts = Counts {
            base: 0,
            by_variant: &[1, 1, 0, 0, 0],
        };
        let rows = steps(&domains, &variants, &counts);
        assert_eq!(rows.len(), 1);
        let rule = &rows[0].1;
        for (gifts, f, want) in [
            (true, "normal", true),
            (true, "fast", true),
            (true, "freed", false),
            (false, "normal", false),
        ] {
            let s = on(&[("npc-gifts", gifts.into()), ("gerudo-fortress", f.into())]);
            assert_eq!(holds(rule, &s), want, "{gifts} {f}");
        }
        assert_eq!(rule["all"].as_array().unwrap().len(), 2, "{rule}");
    }
}
