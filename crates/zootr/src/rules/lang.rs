//! The rule language itself: a rule parsed into an [`Expr`] of words, and
//! resolved for a seed into a [`Need`] that [`Have`] meets or doesn't.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};

use crate::world::Era;

/// A parsed rule, words unresolved.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Word(String),
    All(Vec<Expr>),
    Any(Vec<Expr>),
}

/// The rule as a file writes it, with brackets only where they're needed.
impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let join = |parts: &[Expr], word: &str, f: &mut std::fmt::Formatter| {
            let parts: Vec<String> = (parts.iter())
                .map(|p| match p {
                    Expr::Word(_) => p.to_string(),
                    _ => format!("({p})"),
                })
                .collect();
            write!(f, "{}", parts.join(word))
        };
        match self {
            Expr::Word(w) => write!(f, "{w}"),
            Expr::All(parts) => join(parts, " and ", f),
            Expr::Any(parts) => join(parts, " or ", f),
        }
    }
}

/// A rule resolved for one seed's settings.
#[derive(Clone, Debug, PartialEq)]
pub enum Need {
    Always,
    Never,
    /// At least `count` of these actors between them.
    Items {
        actors: Vec<String>,
        count: u32,
    },
    Era(Era),
    Flag(String),
    /// A toggle the Archipelago export leaves open, answered per player.
    /// Unmet here: no seed of ours opens one.
    Setting(String),
    All(Vec<Need>),
    Any(Vec<Need>),
}

/// What Link has collected so far.
#[derive(Clone, Debug, Default)]
pub struct Have {
    pub items: HashMap<String, u32>,
    pub flags: HashSet<String>,
}

impl Have {
    pub fn add(&mut self, actor: &str) {
        *self.items.entry(actor.to_owned()).or_default() += 1;
    }

    /// Whether Link holds `flag`, named or as `#hash`, either way round.
    pub fn flag(&self, flag: &str) -> bool {
        self.flags.contains(flag)
            || flag_hash(flag).is_some_and(|h| self.flags.iter().any(|f| flag_hash(f) == Some(h)))
    }
}

/// A flag's GameData hash, from its name or its `#hash`.
pub fn flag_hash(flag: &str) -> Option<u32> {
    match flag.strip_prefix('#') {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => Some(ounce_formats::murmur3(flag.as_bytes())),
    }
}

impl Need {
    pub fn met(&self, have: &Have, era: Era) -> bool {
        match self {
            Need::Always => true,
            Need::Never => false,
            Need::Items { actors, count } => {
                actors
                    .iter()
                    .map(|a| have.items.get(a).copied().unwrap_or(0))
                    .sum::<u32>()
                    >= *count
            }
            Need::Era(e) => *e == era,
            Need::Flag(f) => have.flag(f),
            Need::Setting(_) => false,
            Need::All(needs) => needs.iter().all(|n| n.met(have, era)),
            Need::Any(needs) => needs.iter().any(|n| n.met(have, era)),
        }
    }

    /// Whether anything could meet this in `era`: every item, no flags.
    pub fn possible(&self, era: Era) -> bool {
        self.could(era, false)
    }

    /// Whether anything could ever meet this in `era`: every item, and
    /// every flag set.
    pub fn ever(&self, era: Era) -> bool {
        self.could(era, true)
    }

    fn could(&self, era: Era, flags: bool) -> bool {
        match self {
            Need::Always | Need::Items { .. } | Need::Setting(_) => true,
            Need::Never => false,
            Need::Flag(_) => flags,
            Need::Era(e) => *e == era,
            Need::All(needs) => needs.iter().all(|n| n.could(era, flags)),
            Need::Any(needs) => needs.iter().any(|n| n.could(era, flags)),
        }
    }

    /// This as it reads in `era`: `child` and `adult` settled, nested
    /// groups flattened, what's always met and repeats dropped, and an
    /// `or` that another part of its `and` already meets dropped.
    pub fn in_era(&self, era: Era) -> Need {
        match self {
            Need::Era(e) if *e == era => Need::Always,
            Need::Era(_) => Need::Never,
            Need::All(needs) => group(needs, era, true),
            Need::Any(needs) => group(needs, era, false),
            other => other.clone(),
        }
    }

    /// Every item actor this mentions.
    pub fn items<'a>(&'a self, out: &mut BTreeSet<&'a str>) {
        match self {
            Need::Items { actors, .. } => out.extend(actors.iter().map(String::as_str)),
            Need::All(needs) | Need::Any(needs) => needs.iter().for_each(|n| n.items(out)),
            _ => {}
        }
    }
}

/// An `and` (`all`) or `or` group of `needs` in `era`, simplified.
fn group(needs: &[Need], era: Era, all: bool) -> Need {
    let (unit, zero) = match all {
        true => (Need::Always, Need::Never),
        false => (Need::Never, Need::Always),
    };
    let mut parts: Vec<Need> = Vec::new();
    for n in needs.iter().map(|n| n.in_era(era)) {
        let inner = match (&n, all) {
            (Need::All(v), true) | (Need::Any(v), false) => v.clone(),
            _ => vec![n],
        };
        for n in inner {
            if n == zero {
                return zero;
            }
            if n != unit && !parts.contains(&n) {
                parts.push(n);
            }
        }
    }
    // `a and (a or b)` is `a`, and `a or (a and b)` is `a`.
    let covered = |n: &Need, parts: &[Need]| match (n, all) {
        (Need::Any(v), true) | (Need::All(v), false) => v.iter().any(|m| parts.contains(m)),
        _ => false,
    };
    let kept: Vec<Need> = (parts.iter())
        .filter(|n| !covered(n, &parts))
        .cloned()
        .collect();
    match kept.len() {
        0 => unit,
        1 => kept.into_iter().next().unwrap_or(unit),
        _ if all => Need::All(kept),
        _ => Need::Any(kept),
    }
}

impl std::fmt::Display for Need {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let join = |f: &mut std::fmt::Formatter, needs: &[Need], op: &str| {
            let parts: Vec<String> = needs
                .iter()
                .filter(|n| op == "or" || **n != Need::Always)
                .map(|n| match n {
                    Need::All(v) | Need::Any(v) if v.len() > 1 => format!("({n})"),
                    _ => n.to_string(),
                })
                .collect();
            match parts.is_empty() {
                true => write!(f, "always"),
                false => write!(f, "{}", parts.join(&format!(" {op} "))),
            }
        };
        match self {
            Need::Always => write!(f, "always"),
            Need::Never => write!(f, "never"),
            Need::Items { actors, count: 1 } => write!(f, "{}", actors.join("|")),
            Need::Items { actors, count } => write!(f, "{}*{count}", actors.join("|")),
            Need::Era(e) => write!(f, "{e}"),
            Need::Flag(name) => write!(f, "flag:{name}"),
            Need::Setting(id) => write!(f, "setting:{id}"),
            Need::All(v) => join(f, v, "and"),
            Need::Any(v) => join(f, v, "or"),
        }
    }
}

pub fn parse(text: &str) -> Result<Expr> {
    let spaced = text.replace('(', " ( ").replace(')', " ) ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut at = 0;
    let expr = or(&tokens, &mut at).with_context(|| format!("rule `{text}`"))?;
    ensure!(
        at == tokens.len(),
        "rule `{text}`: unexpected `{}`",
        tokens[at]
    );
    Ok(expr)
}

fn or(tokens: &[&str], at: &mut usize) -> Result<Expr> {
    joined(tokens, at, "or", and, Expr::Any)
}

fn and(tokens: &[&str], at: &mut usize) -> Result<Expr> {
    joined(tokens, at, "and", atom, Expr::All)
}

/// Terms parsed by `term`, separated by `op`. `and` binds tighter than `or`.
fn joined(
    tokens: &[&str],
    at: &mut usize,
    op: &str,
    term: fn(&[&str], &mut usize) -> Result<Expr>,
    group: fn(Vec<Expr>) -> Expr,
) -> Result<Expr> {
    let mut terms = vec![term(tokens, at)?];
    while tokens.get(*at) == Some(&op) {
        *at += 1;
        terms.push(term(tokens, at)?);
    }
    Ok(if terms.len() == 1 {
        terms.remove(0)
    } else {
        group(terms)
    })
}

fn atom(tokens: &[&str], at: &mut usize) -> Result<Expr> {
    let Some(&token) = tokens.get(*at) else {
        bail!("ends early");
    };
    *at += 1;
    match token {
        "(" => {
            let inner = or(tokens, at)?;
            ensure!(tokens.get(*at) == Some(&")"), "missing `)`");
            *at += 1;
            Ok(inner)
        }
        ")" | "and" | "or" => bail!("unexpected `{token}`"),
        word => Ok(Expr::Word(word.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_need_in_one_era_settles_the_ages_and_drops_what_repeats() {
        let item = |a: &str| Need::Items {
            actors: vec![a.into()],
            count: 1,
        };
        let adult = |n: Need| Need::All(vec![Need::Era(Era::Adult), n]);
        let child = |n: Need| Need::All(vec![Need::Era(Era::Child), n]);
        let hook = adult(item("Hook"));
        let need = Need::All(vec![
            hook.clone(),
            Need::All(vec![hook.clone(), item("Ocarina")]),
            Need::Any(vec![child(item("Sling")), adult(item("Bow")), hook.clone()]),
        ]);
        assert_eq!(need.in_era(Era::Child), Need::Never);
        assert_eq!(need.in_era(Era::Adult).to_string(), "Hook and Ocarina");
        let either = Need::Any(vec![child(item("Sling")), adult(item("Bow"))]);
        assert_eq!(either.in_era(Era::Child).to_string(), "Sling");
        assert_eq!(
            Need::All(vec![Need::Era(Era::Adult)]).in_era(Era::Adult),
            Need::Always
        );
    }

    fn w(s: &str) -> Expr {
        Expr::Word(s.into())
    }

    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(
            parse("a or b and c").unwrap(),
            Expr::Any(vec![w("a"), Expr::All(vec![w("b"), w("c")])])
        );
        assert_eq!(
            parse("(a or b) and c").unwrap(),
            Expr::All(vec![Expr::Any(vec![w("a"), w("b")]), w("c")])
        );
        assert_eq!(parse("  a  ").unwrap(), w("a"));
    }

    #[test]
    fn bad_rules_fail() {
        for bad in ["", "a or", "(a", "a)", "a b", "and a", "()"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
