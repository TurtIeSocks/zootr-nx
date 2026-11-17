//! The turn queue's rules, apart from any socket: who holds each game, who
//! waits, and when a sweep running on a game must step aside. Times are
//! Unix seconds, passed in, so every rule is tested without a clock.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Queue {
    /// In preference order: a waiter gets the first one free.
    pub games: Vec<Game>,
    /// Urgent waiters first, then first come, first served.
    pub waiting: Vec<Waiter>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<Hold>,
    /// The maintainer is playing it: no one else gets it.
    #[serde(default)]
    pub maintainer: bool,
    /// Only this session is granted it, until the maintainer lifts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved: Option<String>,
    /// A sweep running on it whenever no one else wants it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep: Option<Sweep>,
    /// The session the maintainer last took it from, told so once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bumped: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hold {
    pub session: String,
    pub note: String,
    pub since: i64,
    /// Its last game command: the lease runs from here.
    pub seen: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Waiter {
    pub session: String,
    pub note: String,
    pub urgent: bool,
    pub since: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sweep {
    pub name: String,
    /// Asked to step aside at its next row boundary, since then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yielding: Option<i64>,
}

/// A hold handed out by [`Queue::tick`].
#[derive(Clone, Debug, PartialEq)]
pub struct Grant {
    pub session: String,
    pub game: String,
}

impl Queue {
    pub fn new(games: &[String]) -> Self {
        Self {
            games: (games.iter())
                .map(|name| Game {
                    name: name.clone(),
                    ..Game::default()
                })
                .collect(),
            waiting: Vec::new(),
        }
    }

    /// Keep the state of games still configured, in the config's order,
    /// and add new ones.
    pub fn configure(&mut self, games: &[String]) {
        let mut kept = std::mem::take(&mut self.games);
        self.games = (games.iter())
            .map(|name| match kept.iter().position(|g| g.name == *name) {
                Some(i) => kept.swap_remove(i),
                None => Game {
                    name: name.clone(),
                    ..Game::default()
                },
            })
            .collect();
    }

    fn game(&mut self, name: &str) -> Result<&mut Game> {
        match self.games.iter_mut().find(|g| g.name == name) {
            Some(g) => Ok(g),
            None => bail!("no game named {name}"),
        }
    }

    /// The game `session` holds.
    pub fn held(&self, session: &str) -> Option<&Game> {
        (self.games.iter()).find(|g| g.holder.as_ref().is_some_and(|h| h.session == session))
    }

    /// Queue `session`, or update its note and urgency if it waits
    /// already. Returns its place, from 1.
    pub fn join(&mut self, session: &str, note: &str, urgent: bool, now: i64) -> Result<usize> {
        if session.is_empty() {
            bail!("name the session with --session");
        }
        if let Some(g) = self.held(session) {
            bail!("{session} holds {} already", g.name);
        }
        let since = match self.waiting.iter().position(|w| w.session == session) {
            Some(i) => self.waiting.remove(i).since,
            None => now,
        };
        let at = match urgent {
            true => self.waiting.iter().take_while(|w| w.urgent).count(),
            false => self.waiting.len(),
        };
        self.waiting.insert(
            at,
            Waiter {
                session: session.into(),
                note: note.into(),
                urgent,
                since,
            },
        );
        Ok(at + 1)
    }

    /// Let go of `session`'s hold and its place in line. Whether it had
    /// either.
    pub fn free(&mut self, session: &str) -> bool {
        let waited = self.waiting.len();
        self.waiting.retain(|w| w.session != session);
        let mut held = false;
        for g in &mut self.games {
            if g.holder.as_ref().is_some_and(|h| h.session == session) {
                g.holder = None;
                held = true;
            }
        }
        held || waited != self.waiting.len()
    }

    /// `session` used its game: renew its lease. Refused unless it holds
    /// one. Returns the game's name.
    pub fn renew(&mut self, session: &str, now: i64) -> Result<String> {
        if let Some(g) = (self.games.iter_mut()).find(|g| g.bumped.as_deref() == Some(session)) {
            g.bumped = None;
            bail!("the maintainer took {}: join the queue again", g.name);
        }
        for g in &mut self.games {
            if let Some(h) = g.holder.as_mut().filter(|h| h.session == session) {
                h.seen = now;
                return Ok(g.name.clone());
            }
        }
        match self.waiting.iter().position(|w| w.session == session) {
            Some(i) => bail!(
                "{session} isn't holding a game yet: it's number {} in line",
                i + 1
            ),
            None => {
                bail!("{session} isn't holding a game: `zootr turn join`, then `zootr turn wait`")
            }
        }
    }

    /// The maintainer plays `game`: its holder loses it at once, and its
    /// sweep steps aside at its next row.
    pub fn take(&mut self, game: &str, now: i64) -> Result<()> {
        let g = self.game(game)?;
        g.maintainer = true;
        if let Some(h) = g.holder.take() {
            g.bumped = Some(h.session);
        }
        if let Some(s) = g.sweep.as_mut() {
            s.yielding.get_or_insert(now);
        }
        Ok(())
    }

    /// The maintainer stopped playing `game`.
    pub fn leave(&mut self, game: &str) -> Result<()> {
        self.game(game)?.maintainer = false;
        Ok(())
    }

    /// Keep `game` for `session` only, or lift that with `None`.
    pub fn reserve(&mut self, game: &str, session: Option<&str>) -> Result<()> {
        self.game(game)?.reserved = session.map(Into::into);
        Ok(())
    }

    /// Sweep `name` asks to run on `game`: yes once no one holds it, the
    /// maintainer isn't playing, and no one waits.
    pub fn sweep_start(&mut self, game: &str, name: &str) -> Result<bool> {
        let idle = self.waiting.is_empty();
        let g = self.game(game)?;
        if g.holder.is_some() || g.maintainer || !idle {
            return Ok(false);
        }
        g.sweep = Some(Sweep {
            name: name.into(),
            yielding: None,
        });
        Ok(true)
    }

    /// Whether sweep `name` must step aside from `game` at this row
    /// boundary: asked to, or already counted as gone.
    pub fn sweep_yields(&self, game: &str, name: &str) -> bool {
        let sweep = (self.games.iter())
            .find(|g| g.name == game)
            .and_then(|g| g.sweep.as_ref());
        sweep.is_none_or(|s| s.name != name || s.yielding.is_some())
    }

    /// Sweep `name` stopped on `game`.
    pub fn sweep_done(&mut self, game: &str, name: &str) -> Result<()> {
        let g = self.game(game)?;
        if g.sweep.as_ref().is_some_and(|s| s.name == name) {
            g.sweep = None;
        }
        Ok(())
    }

    /// Lapse holds idle past `lease`, count sweeps still mid-row past
    /// `cap` as gone, then hand each free game to the first waiter it
    /// suits, asking a sweep on it to step aside first.
    pub fn tick(&mut self, now: i64, lease: i64, cap: i64) -> Vec<Grant> {
        for g in &mut self.games {
            if g.holder.as_ref().is_some_and(|h| now - h.seen >= lease) {
                g.holder = None;
            }
            if g.sweep
                .as_ref()
                .and_then(|s| s.yielding)
                .is_some_and(|t| now - t >= cap)
            {
                g.sweep = None;
            }
        }
        let mut grants = Vec::new();
        let mut claimed = vec![false; self.games.len()];
        let mut i = 0;
        while i < self.waiting.len() {
            let session = self.waiting[i].session.clone();
            let free = |(j, g): &(usize, &Game)| {
                !claimed[*j]
                    && g.holder.is_none()
                    && !g.maintainer
                    && g.reserved.as_ref().is_none_or(|r| *r == session)
            };
            let Some(j) = self.games.iter().enumerate().find(free).map(|(j, _)| j) else {
                i += 1;
                continue;
            };
            claimed[j] = true;
            let g = &mut self.games[j];
            if let Some(s) = g.sweep.as_mut() {
                s.yielding.get_or_insert(now);
                i += 1;
                continue;
            }
            let w = self.waiting.remove(i);
            g.holder = Some(Hold {
                session: w.session.clone(),
                note: w.note,
                since: now,
                seen: now,
            });
            grants.push(Grant {
                session: w.session,
                game: g.name.clone(),
            });
        }
        grants
    }
}

impl std::fmt::Display for Queue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for g in &self.games {
            write!(f, "{}: ", g.name)?;
            match (&g.holder, g.maintainer, &g.sweep) {
                (_, true, _) => write!(f, "the maintainer's")?,
                (Some(h), ..) => write!(f, "{} ({})", h.session, h.note)?,
                (None, _, Some(s)) if s.yielding.is_some() => {
                    write!(f, "{} stepping aside", s.name)?
                }
                (None, _, Some(s)) => write!(f, "{} sweeping", s.name)?,
                (None, _, None) => write!(f, "free")?,
            }
            if let Some(r) = &g.reserved {
                write!(f, ", kept for {r}")?;
            }
            writeln!(f)?;
        }
        for (i, w) in self.waiting.iter().enumerate() {
            let urgent = if w.urgent { " (urgent)" } else { "" };
            writeln!(f, "{}. {}{urgent}: {}", i + 1, w.session, w.note)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEASE: i64 = 900;
    const CAP: i64 = 180;

    fn queue() -> Queue {
        Queue::new(&["near".into(), "far".into()])
    }

    fn granted(q: &mut Queue, now: i64) -> Vec<(String, String)> {
        let grants = q.tick(now, LEASE, CAP);
        grants.into_iter().map(|g| (g.session, g.game)).collect()
    }

    fn pair(session: &str, game: &str) -> (String, String) {
        (session.into(), game.into())
    }

    #[test]
    fn waiters_get_the_first_free_game_in_order() {
        let mut q = queue();
        assert_eq!(q.join("a", "", false, 0).unwrap(), 1);
        assert_eq!(q.join("b", "", false, 1).unwrap(), 2);
        assert_eq!(q.join("c", "", false, 2).unwrap(), 3);
        assert_eq!(granted(&mut q, 3), [pair("a", "near"), pair("b", "far")]);
        assert_eq!(q.waiting[0].session, "c");
        assert!(q.free("a"));
        assert_eq!(granted(&mut q, 4), [pair("c", "near")]);
    }

    #[test]
    fn urgent_waiters_go_ahead_of_normal_ones_in_their_own_order() {
        let mut q = queue();
        q.join("a", "", false, 0).unwrap();
        assert_eq!(q.join("u1", "", true, 1).unwrap(), 1);
        assert_eq!(q.join("u2", "", true, 2).unwrap(), 2);
        let order: Vec<_> = q.waiting.iter().map(|w| w.session.as_str()).collect();
        assert_eq!(order, ["u1", "u2", "a"]);
    }

    #[test]
    fn a_holder_can_t_join_and_rejoining_keeps_the_first_time() {
        let mut q = queue();
        q.join("a", "", false, 5).unwrap();
        q.join("a", "new note", false, 9).unwrap();
        assert_eq!((q.waiting.len(), q.waiting[0].since), (1, 5));
        granted(&mut q, 10);
        assert!(q.join("a", "", false, 11).is_err());
    }

    #[test]
    fn only_a_holder_renews_and_an_idle_hold_lapses() {
        let mut q = queue();
        q.join("a", "", false, 0).unwrap();
        assert!(
            q.renew("a", 0)
                .unwrap_err()
                .to_string()
                .contains("number 1 in line")
        );
        granted(&mut q, 0);
        assert_eq!(q.renew("a", 600).unwrap(), "near");
        granted(&mut q, 600 + LEASE - 1);
        assert!(q.held("a").is_some(), "renewed at 600");
        granted(&mut q, 600 + LEASE);
        assert!(q.held("a").is_none());
        assert!(q.renew("a", 600 + LEASE).is_err());
    }

    #[test]
    fn the_maintainer_takes_a_game_from_its_holder_and_it_is_told_once() {
        let mut q = queue();
        q.join("a", "", false, 0).unwrap();
        granted(&mut q, 0);
        q.take("near", 1).unwrap();
        let told = q.renew("a", 2).unwrap_err().to_string();
        assert!(told.contains("the maintainer took near"), "{told}");
        assert!(
            !q.renew("a", 3)
                .unwrap_err()
                .to_string()
                .contains("maintainer")
        );
        q.join("b", "", false, 4).unwrap();
        assert_eq!(granted(&mut q, 5), [pair("b", "far")]);
        q.leave("near").unwrap();
        q.join("c", "", false, 6).unwrap();
        assert_eq!(granted(&mut q, 7), [pair("c", "near")]);
    }

    #[test]
    fn a_reserved_game_goes_only_to_its_session() {
        let mut q = queue();
        q.reserve("near", Some("r")).unwrap();
        q.join("a", "", false, 0).unwrap();
        q.join("r", "", false, 1).unwrap();
        assert_eq!(granted(&mut q, 2), [pair("a", "far"), pair("r", "near")]);
    }

    #[test]
    fn a_sweep_steps_aside_for_a_waiter_then_resumes_once_no_one_wants_the_game() {
        let mut q = Queue::new(&["far".into()]);
        assert!(q.sweep_start("far", "night").unwrap());
        assert!(!q.sweep_yields("far", "night"));
        q.join("a", "", false, 10).unwrap();
        assert!(granted(&mut q, 10).is_empty(), "the sweep goes first");
        assert!(q.sweep_yields("far", "night"));
        q.sweep_done("far", "night").unwrap();
        assert_eq!(granted(&mut q, 40), [pair("a", "far")]);
        assert!(!q.sweep_start("far", "night").unwrap(), "held");
        q.free("a");
        assert!(q.sweep_start("far", "night").unwrap());
    }

    #[test]
    fn a_sweep_still_mid_row_past_the_cap_counts_as_gone() {
        let mut q = Queue::new(&["far".into()]);
        q.sweep_start("far", "night").unwrap();
        q.join("a", "", false, 0).unwrap();
        assert!(granted(&mut q, 0).is_empty());
        assert!(granted(&mut q, CAP - 1).is_empty());
        assert_eq!(granted(&mut q, CAP), [pair("a", "far")]);
        assert!(q.sweep_yields("far", "night"));
    }

    #[test]
    fn with_the_first_game_held_a_waiter_asks_the_sweep_on_the_next_to_step_aside() {
        let mut q = queue();
        q.join("x", "", false, 0).unwrap();
        granted(&mut q, 0);
        assert!(q.sweep_start("far", "night").unwrap());
        q.join("a", "", false, 1).unwrap();
        assert!(granted(&mut q, 1).is_empty());
        assert!(q.sweep_yields("far", "night"));
    }

    #[test]
    fn the_maintainer_playing_asks_the_sweep_to_step_aside() {
        let mut q = Queue::new(&["far".into()]);
        q.sweep_start("far", "night").unwrap();
        q.take("far", 3).unwrap();
        assert!(q.sweep_yields("far", "night"));
        assert!(!q.sweep_start("far", "night").unwrap());
    }

    #[test]
    fn configure_keeps_known_games_in_the_new_order() {
        let mut q = queue();
        q.join("a", "", false, 0).unwrap();
        granted(&mut q, 0);
        q.configure(&["far".into(), "near".into(), "new".into()]);
        let names: Vec<_> = q.games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["far", "near", "new"]);
        assert_eq!(q.held("a").unwrap().name, "near");
    }
}
