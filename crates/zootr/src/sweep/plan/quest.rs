//! The quest steps a spot needs first: read from a gate's expressions, or
//! named for a gift's flow.

use super::Steps;
use crate::gamedata::GameData;
use crate::gifts::Gift;
use crate::known::{Name, actor, flow};
use crate::{gates, steps};

/// Quest steps a gift's flow needs first, as `(flow, challenge, step)`,
/// where a test preset already records them.
const STEPS: [(&Name, &Name, &str); 1] = [(
    actor::NPC_GORON_BIGGORON_ADULT,
    flow::SUB_EVENT_GET_SWORD_BIG_GORON,
    "Step11",
)];

/// The `(challenge, step)` a BoolExp rendered as `Step_<challenge> ==
/// <step>` asks for, and `None` for any other expression.
pub(super) fn step_of(expr: &str) -> Option<(String, String)> {
    let (saved, step) = compared(expr, "==")?;
    Some((saved.strip_prefix(steps::SAVED)?.into(), step.into()))
}

/// The enum and value a BoolExp rendered as `NAME OP VALUE` compares.
fn compared<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let (name, value) = expr.split_once(&format!(" {op} "))?;
    let one = |s: &str| !s.is_empty() && !s.contains(' ');
    (one(name) && one(value)).then_some((name, value))
}

/// The quest steps that open `expr`: for each quest it reads past a step,
/// directly or through a BoolExp like the Spirit sage's "after step 9" one,
/// the step after it. A boss's heart container appears once its quest is
/// past the boss.
pub(super) fn quests(gd: &GameData, expr: &gates::Expr) -> Steps {
    let past = |atom: &gates::Atom| match atom {
        gates::Atom::After { name, value } => Some((name.clone(), value.clone())),
        gates::Atom::Exp(exp) => {
            let rendered = gd.expr(exp).ok()??;
            let (name, value) = compared(&rendered, ">")?;
            Some((name.to_owned(), value.to_owned()))
        }
        _ => None,
    };
    let next = |(name, value): (String, String)| {
        let order = gd.enum_values(&name)?;
        let after = order.into_iter().skip_while(|s| *s != value).nth(1)?;
        Some((name.strip_prefix(steps::SAVED)?.to_owned(), after))
    };
    expr.quests()
        .into_iter()
        .filter_map(past)
        .filter_map(next)
        .collect()
}

pub(super) fn steps(gift: &Gift) -> Steps {
    let of = STEPS.iter().filter(|(flow, ..)| *flow == gift.flow);
    of.map(|&(_, c, s)| (c.to_string(), s.into())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_steps_are_steps_the_game_has() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let gd = GameData::load(&romfs).unwrap();
        for (_, challenge, step) in STEPS {
            steps::check(&gd, challenge, step).unwrap();
        }
    }

    #[test]
    fn a_step_is_read_from_its_expression() {
        needs_names!();
        let ruby = Some((flow::GET_STONE_OF_FIRE.to_string(), "Complete".into()));
        assert_eq!(
            step_of(&format!(
                "{} == Complete",
                crate::steps::saved(flow::GET_STONE_OF_FIRE)
            )),
            ruby
        );
        assert_eq!(
            step_of(&format!(
                "{} > Step02",
                crate::steps::saved(flow::MEET_DEKU_TREE)
            )),
            None
        );
        assert_eq!(step_of("Step_A == B && Step_C == D"), None);
        assert_eq!(step_of("IsGet.Item_A"), None);
    }
}
