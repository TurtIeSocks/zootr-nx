//! A gate's expression in the logic files' rule language: words joined by
//! `and` and `or`, with what needs nothing dropped.

use std::collections::BTreeSet;
use std::fmt;

use super::{Atom, Expr};
use crate::known::field;

/// A gate in the rule language, or `None` when it needs nothing.
pub fn rule(expr: &Expr) -> Option<String> {
    match Rule::of(expr) {
        Rule::Const(true) => None,
        Rule::Const(false) => Some("todo:never".into()),
        r => Some(r.to_string()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Rule {
    Const(bool),
    Word(String),
    And(BTreeSet<Rule>),
    Or(BTreeSet<Rule>),
}

impl Rule {
    fn of(expr: &Expr) -> Rule {
        match expr {
            Expr::Const(b) => Rule::Const(*b),
            Expr::Atom(a) => atom(a),
            Expr::SetFlag(_, drives) => Rule::any(drives.iter().map(Rule::of)),
            Expr::And(v) => Rule::all(v.iter().map(Rule::of)),
            Expr::Or(v) => Rule::any(v.iter().map(Rule::of)),
            Expr::Not(e) => Rule::of(e).negated(),
        }
    }

    /// The rule language has no `not`, so a negated word stays a `todo:`.
    fn negated(self) -> Rule {
        match self {
            Rule::Const(b) => Rule::Const(!b),
            Rule::Word(w) => Rule::Word(format!("todo:not:{}", w.trim_start_matches("todo:"))),
            Rule::And(parts) => Rule::any(parts.into_iter().map(Rule::negated)),
            Rule::Or(parts) => Rule::all(parts.into_iter().map(Rule::negated)),
        }
    }

    fn all(parts: impl Iterator<Item = Rule>) -> Rule {
        Rule::join(parts, true)
    }

    fn any(parts: impl Iterator<Item = Rule>) -> Rule {
        Rule::join(parts, false)
    }

    /// `and` when `all`, else `or`: flattened, deduplicated, and with the
    /// constants folded.
    fn join(parts: impl Iterator<Item = Rule>, all: bool) -> Rule {
        let mut set = BTreeSet::new();
        for p in parts {
            match p {
                Rule::Const(b) if b == all => {}
                Rule::Const(_) => return Rule::Const(!all),
                Rule::And(inner) if all => set.extend(inner),
                Rule::Or(inner) if !all => set.extend(inner),
                other => {
                    set.insert(other);
                }
            }
        }
        let absorbed: Vec<_> = set
            .iter()
            .filter(|r| set.iter().any(|other| other != *r && other.absorbs(r, all)))
            .cloned()
            .collect();
        for r in absorbed {
            set.remove(&r);
        }
        match set.len() {
            0 => Rule::Const(all),
            1 => set.pop_first().unwrap_or(Rule::Const(all)),
            _ if all => Rule::And(set),
            _ => Rule::Or(set),
        }
    }

    /// Whether `other` is redundant beside `self`: under `or`, `a` makes
    /// `a and b` redundant, and under `and`, `a` makes `a or b` redundant.
    fn absorbs(&self, other: &Rule, all: bool) -> bool {
        let inner = match (other, all) {
            (Rule::And(inner), false) | (Rule::Or(inner), true) => inner,
            _ => return false,
        };
        match (self, all) {
            (Rule::And(mine), false) | (Rule::Or(mine), true) => mine.is_subset(inner),
            _ => inner.contains(self),
        }
    }
}

pub(super) fn atom(a: &Atom) -> Rule {
    let word = |w: &str| Rule::Word(w.to_owned());
    match a {
        Atom::Child => word("child"),
        Atom::Adult => word("adult"),
        Atom::Flag(f) => Rule::Word(format!("flag:{f}")),
        Atom::Enum { name, value } => Rule::Word(format!("enum:{name}={value}")),
        Atom::Todo(t) => Rule::Word(format!("todo:{t}")),
        Atom::After { name, value } => Rule::Word(format!("todo:{name}>{value}")),
        Atom::Exp(exp) => Rule::Word(format!("todo:exp:{exp}")),
        Atom::Actor { name, output, .. } => match output.as_str() {
            "Dead" if name.starts_with(crate::enemies::ENEMY) => Rule::Const(true),
            // A step switch reports its first press as `FirstOn`.
            _ if output == field::ON_STATE || output == field::FIRST_ON => {
                Rule::Word(format!("switch:{name}"))
            }
            "Burning" => word("fire"),
            _ if output == field::IS_HIT_BLUE_FIRE => word("blue-fire"),
            _ if output == field::IS_LIGHT_MAX => word("light"),
            _ => Rule::Word(format!("todo:{name}.{output}")),
        },
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (parts, sep) = match self {
            Rule::Const(b) => return write!(f, "{b}"),
            Rule::Word(w) => return f.write_str(w),
            Rule::And(parts) => (parts, " and "),
            Rule::Or(parts) => (parts, " or "),
        };
        let shown: Vec<_> = parts
            .iter()
            .map(|p| match p {
                Rule::And(_) | Rule::Or(_) => format!("({p})"),
                _ => p.to_string(),
            })
            .collect();
        f.write_str(&shown.join(sep))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{actor, flag};
    use super::*;
    use crate::known::actor;

    #[test]
    fn rules_drop_what_needs_nothing() {
        needs_names!();
        let stingers = Expr::Or(vec![
            actor(actor::ENEMY_STINGER, "Dead"),
            Expr::SetFlag("Cleared".into(), vec![actor(actor::ENEMY_STINGER, "Dead")]),
        ]);
        assert_eq!(rule(&stingers), None);
        assert_eq!(
            rule(&Expr::And(vec![Expr::Const(true), flag("F")])).unwrap(),
            "flag:F"
        );
        assert_eq!(rule(&Expr::Const(false)).unwrap(), "todo:never");
    }

    #[test]
    fn a_flag_the_graphs_set_reads_as_its_setter() {
        needs_names!();
        let hit = actor(actor::OBJ_SWITCH_HIT_A_05, field::ON_STATE);
        let latch = Expr::SetFlag("Fake_On".into(), vec![hit.clone()]);
        assert_eq!(
            rule(&Expr::Or(vec![hit, latch])).unwrap(),
            format!("switch:{}", actor::OBJ_SWITCH_HIT_A_05)
        );
    }

    #[test]
    fn rules_absorb_and_bracket() {
        needs_names!();
        let switch = actor(actor::OBJ_STEP_SWITCH_A_01, field::ON_STATE);
        let adult = Expr::Atom(Atom::Adult);
        let absorbed = Expr::Or(vec![switch.clone(), Expr::And(vec![adult.clone(), switch])]);
        assert_eq!(
            rule(&absorbed).unwrap(),
            format!("switch:{}", actor::OBJ_STEP_SWITCH_A_01)
        );
        let either = Expr::Or(vec![
            actor(actor::OBJ_CANDLE_STICK_A_02, "Burning"),
            actor(actor::OBJ_SOLAR_SWITCH_A_01, field::IS_LIGHT_MAX),
        ]);
        let both = Expr::And(vec![either, adult, Expr::Atom(Atom::Child)]);
        assert_eq!(rule(&both).unwrap(), "adult and child and (fire or light)");
    }

    #[test]
    fn negations_fold_or_stay_todo() {
        let unset = Expr::SetFlag("Cleared".into(), vec![Expr::Const(false)]);
        assert_eq!(rule(&Expr::Not(Box::new(unset))), None);
        let not_either = Expr::Not(Box::new(Expr::Or(vec![flag("A"), flag("B")])));
        assert_eq!(
            rule(&not_either).unwrap(),
            "todo:not:flag:A and todo:not:flag:B"
        );
    }
}
