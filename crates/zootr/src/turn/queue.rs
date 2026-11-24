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
    /// Wants every game at once, granted together or not at all.
    #[serde(default)]
    pub both: bool,
    /// Wants this game alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
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

    /// The games `session` holds.
    pub fn holds<'a>(&'a self, session: &'a str) -> impl Iterator<Item = &'a Game> {
        (self.games.iter()).filter(move |g| g.held_by(session))
    }

    /// The first game `session` holds.
    pub fn held(&self, session: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.held_by(session))
    }

    /// Whether `g` could go to `session` once any sweep on it steps aside.
    fn open_to(g: &Game, session: &str) -> bool {
        g.holder.is_none() && !g.maintainer && g.reserved.as_deref().is_none_or(|r| r == session)
    }

    /// Whether a game of every machine could go to `session` at once.
    fn all_open_to(&self, session: &str) -> bool {
        (self.machines().into_iter()).all(|m| {
            (self.games.iter()).any(|g| machine(&g.name) == m && Self::open_to(g, session))
        })
    }

    /// The machines the games run on, in the games' order.
    fn machines(&self) -> Vec<&str> {
        let mut seen = Vec::new();
        for g in &self.games {
            if !seen.contains(&machine(&g.name)) {
                seen.push(machine(&g.name));
            }
        }
        seen
    }

    /// The first game not `claimed` that `fits`, one with no sweep on it
    /// before one a sweep would have to leave.
    fn pick(&self, claimed: &[bool], fits: impl Fn(&Game) -> bool) -> Option<usize> {
        let fit = |(j, g): &(usize, &Game)| {
            !claimed[*j] && self.room(machine(&g.name), claimed) && fits(g)
        };
        // The first of the least: ties keep the games' order.
        (self.games.iter().enumerate())
            .filter(fit)
            .min_by_key(|(_, g)| g.sweep.is_some())
            .map(|(j, _)| j)
    }

    /// How many of `m`'s games are played or `claimed` this tick: by a
    /// session, or by the maintainer.
    fn taken(&self, m: &str, claimed: &[bool]) -> usize {
        let on_m = |(j, g): &(usize, &Game)| {
            machine(&g.name) == m && (claimed[*j] || g.holder.is_some() || g.maintainer)
        };
        self.games.iter().enumerate().filter(on_m).count()
    }

    /// Whether `m` takes another game: always, but only one beside a
    /// sweep, which shares the machine's memory with the model judging it.
    fn room(&self, m: &str, claimed: &[bool]) -> bool {
        let swept = (self.games.iter()).any(|g| machine(&g.name) == m && g.sweep.is_some());
        !swept || self.taken(m, claimed) == 0
    }

    /// Queue `session`, or update its note, urgency, `both` and `on` if it
    /// waits already. With `both` it waits for a game of every machine at
    /// once, with `on` for that game or one of its slots ([`suits`]).
    /// Returns its place, from 1.
    pub fn join(
        &mut self,
        session: &str,
        note: &str,
        urgent: bool,
        both: bool,
        on: Option<&str>,
        now: i64,
    ) -> Result<usize> {
        if session.is_empty() {
            bail!("name the session with --session");
        }
        if let Some(game) = on {
            if both {
                bail!("--on names one game, --both every machine: pick one");
            }
            self.game(game)?;
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
                both,
                on: on.map(Into::into),
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

    /// `session` played a game: renew the lease on every game it holds.
    /// Refused unless it holds one. Returns `here` when it holds that,
    /// else the first game it holds.
    pub fn renew(&mut self, session: &str, here: &str, now: i64) -> Result<String> {
        self.holding(session, here, Some(now))
    }

    /// `session` only read a game: [`Self::renew`]'s answer, with each
    /// lease left to run out. A hold kept up by reads alone, such as a
    /// loop of `dev state`, lapses like an idle one (maintainer,
    /// 2026-11-17).
    pub fn look(&mut self, session: &str, here: &str) -> Result<String> {
        self.holding(session, here, None)
    }

    /// The game `session` holds, renewing each of its leases to `now`.
    fn holding(&mut self, session: &str, here: &str, now: Option<i64>) -> Result<String> {
        if let Some(g) = (self.games.iter_mut()).find(|g| g.bumped.as_deref() == Some(session)) {
            g.bumped = None;
            bail!("the maintainer took {}: join the queue again", g.name);
        }
        let mut held = Vec::new();
        for g in &mut self.games {
            if let Some(h) = g.holder.as_mut().filter(|h| h.session == session) {
                h.seen = now.unwrap_or(h.seen);
                held.push(g.name.clone());
            }
        }
        if let Some(first) = held.first() {
            return Ok(held.iter().find(|g| *g == here).unwrap_or(first).clone());
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
        g.ask_aside(now);
        Ok(())
    }

    /// The maintainer stopped playing `game`.
    pub fn leave(&mut self, game: &str) -> Result<()> {
        self.game(game)?.maintainer = false;
        Ok(())
    }

    /// Keep `game` for `session` only, or lift that with `None`. While a
    /// game is kept for it, the session waits for that game alone.
    pub fn reserve(&mut self, game: &str, session: Option<&str>) -> Result<()> {
        self.game(game)?.reserved = session.map(Into::into);
        Ok(())
    }

    /// Sweep `name` asks to run on `game`: yes once no one holds it, the
    /// maintainer isn't playing, and no one waits who could have a game
    /// now, and the machine plays one game at most besides ([`room`]). A
    /// `both` waiter counts only once every machine is open to it, an `on`
    /// waiter only for a game it [`suits`]. Another sweep on the game keeps
    /// it; the same sweep asking again starts afresh, since nothing still
    /// wants it to step aside.
    pub fn sweep_start(&mut self, game: &str, name: &str) -> Result<bool> {
        let idle = (self.waiting.iter()).all(|w| {
            (w.both && !self.all_open_to(&w.session))
                || w.on.as_deref().is_some_and(|o| !suits(o, game))
        });
        let unclaimed = vec![false; self.games.len()];
        let crowded = self.taken(machine(game), &unclaimed) > 1;
        let g = self.game(game)?;
        let other = g.sweep.as_ref().is_some_and(|s| s.name != name);
        if g.holder.is_some() || g.maintainer || !idle || crowded || other {
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
    /// suits, asking a sweep on it to step aside first, and only when no
    /// game without one suits. A machine with a sweep takes one game more
    /// at most ([`room`]). A `both` waiter gets a game of every
    /// machine in one tick or none, and asks sweeps to step aside only
    /// once every machine is otherwise open to it.
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
            if self.waiting[i].both {
                let chosen: Option<Vec<usize>> = (self.machines().iter())
                    .map(|&m| {
                        self.pick(&claimed, |g| {
                            machine(&g.name) == m && Self::open_to(g, &session)
                        })
                    })
                    .collect();
                let Some(chosen) = chosen else {
                    i += 1;
                    continue;
                };
                let mut swept = false;
                for &j in &chosen {
                    claimed[j] = true;
                    swept |= self.games[j].ask_aside(now);
                }
                if swept {
                    i += 1;
                    continue;
                }
                let w = self.waiting.remove(i);
                for &j in &chosen {
                    let g = &mut self.games[j];
                    g.holder = Some(w.hold(now));
                    grants.push(Grant {
                        session: w.session.clone(),
                        game: g.name.clone(),
                    });
                }
                continue;
            }
            // A session a game is kept for waits for that game alone.
            let kept = (self.games.iter()).any(|g| g.reserved.as_deref() == Some(&session));
            let on = self.waiting[i].on.clone();
            let free = |g: &Game| {
                g.holder.is_none()
                    && !g.maintainer
                    && g.reserved.as_deref() == kept.then_some(session.as_str())
                    && on.as_deref().is_none_or(|o| suits(o, &g.name))
            };
            let Some(j) = self.pick(&claimed, free) else {
                i += 1;
                continue;
            };
            claimed[j] = true;
            let g = &mut self.games[j];
            if g.ask_aside(now) {
                i += 1;
                continue;
            }
            let w = self.waiting.remove(i);
            g.holder = Some(w.hold(now));
            grants.push(Grant {
                session: w.session,
                game: g.name.clone(),
            });
        }
        grants
    }
}

/// The machine a game runs on: its name without a slot's `-N`, so
/// `other-2` runs on `other`.
pub fn machine(game: &str) -> &str {
    match game.rsplit_once('-') {
        Some((m, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => m,
        _ => game,
    }
}

/// Whether a waiter `on` a game takes `game`: that game, or any slot of
/// its machine when it names the machine.
pub fn suits(on: &str, game: &str) -> bool {
    game == on || machine(game) == on
}

impl Game {
    fn held_by(&self, session: &str) -> bool {
        self.holder.as_ref().is_some_and(|h| h.session == session)
    }

    /// Ask its sweep to step aside from `now`, unless asked already.
    /// Whether it has one.
    fn ask_aside(&mut self, now: i64) -> bool {
        match self.sweep.as_mut() {
            Some(s) => {
                s.yielding.get_or_insert(now);
                true
            }
            None => false,
        }
    }
}

impl Waiter {
    fn hold(&self, now: i64) -> Hold {
        Hold {
            session: self.session.clone(),
            note: self.note.clone(),
            since: now,
            seen: now,
        }
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
            let both = if w.both { " (both)" } else { "" };
            let on =
                w.on.as_ref()
                    .map(|o| format!(" (on {o})"))
                    .unwrap_or_default();
            writeln!(f, "{}. {}{urgent}{both}{on}: {}", i + 1, w.session, w.note)?;
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
        assert_eq!(q.join("a", "", false, false, None, 0).unwrap(), 1);
        assert_eq!(q.join("b", "", false, false, None, 1).unwrap(), 2);
        assert_eq!(q.join("c", "", false, false, None, 2).unwrap(), 3);
        assert_eq!(granted(&mut q, 3), [pair("a", "near"), pair("b", "far")]);
        assert_eq!(q.waiting[0].session, "c");
        assert!(q.free("a"));
        assert_eq!(granted(&mut q, 4), [pair("c", "near")]);
    }

    #[test]
    fn urgent_waiters_go_ahead_of_normal_ones_in_their_own_order() {
        let mut q = queue();
        q.join("a", "", false, false, None, 0).unwrap();
        assert_eq!(q.join("u1", "", true, false, None, 1).unwrap(), 1);
        assert_eq!(q.join("u2", "", true, false, None, 2).unwrap(), 2);
        let order: Vec<_> = q.waiting.iter().map(|w| w.session.as_str()).collect();
        assert_eq!(order, ["u1", "u2", "a"]);
    }

    #[test]
    fn a_holder_can_t_join_and_rejoining_keeps_the_first_time() {
        let mut q = queue();
        q.join("a", "", false, false, None, 5).unwrap();
        q.join("a", "new note", false, false, None, 9).unwrap();
        assert_eq!((q.waiting.len(), q.waiting[0].since), (1, 5));
        granted(&mut q, 10);
        assert!(q.join("a", "", false, false, None, 11).is_err());
    }

    #[test]
    fn only_a_holder_renews_and_an_idle_hold_lapses() {
        let mut q = queue();
        q.join("a", "", false, false, None, 0).unwrap();
        assert!(
            q.renew("a", "", 0)
                .unwrap_err()
                .to_string()
                .contains("number 1 in line")
        );
        granted(&mut q, 0);
        assert_eq!(q.renew("a", "", 600).unwrap(), "near");
        granted(&mut q, 600 + LEASE - 1);
        assert!(q.held("a").is_some(), "renewed at 600");
        granted(&mut q, 600 + LEASE);
        assert!(q.held("a").is_none());
        assert!(q.renew("a", "", 600 + LEASE).is_err());
    }

    #[test]
    fn a_hold_kept_up_by_looks_alone_lapses() {
        // A loop of `dev state` kept games held while nothing played
        // (maintainer, 2026-11-17).
        let mut q = queue();
        q.join("a", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        assert_eq!(q.look("a", "").unwrap(), "near");
        granted(&mut q, LEASE - 1);
        assert_eq!(
            q.look("a", "").unwrap(),
            "near",
            "a look answers like a renewal"
        );
        granted(&mut q, LEASE);
        assert!(q.held("a").is_none(), "looks never renewed it");
        assert!(q.look("a", "").is_err());
    }

    #[test]
    fn the_maintainer_takes_a_game_from_its_holder_and_it_is_told_once() {
        let mut q = queue();
        q.join("a", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        q.take("near", 1).unwrap();
        let told = q.renew("a", "", 2).unwrap_err().to_string();
        assert!(told.contains("the maintainer took near"), "{told}");
        assert!(
            !q.renew("a", "", 3)
                .unwrap_err()
                .to_string()
                .contains("maintainer")
        );
        q.join("b", "", false, false, None, 4).unwrap();
        assert_eq!(granted(&mut q, 5), [pair("b", "far")]);
        q.leave("near").unwrap();
        q.join("c", "", false, false, None, 6).unwrap();
        assert_eq!(granted(&mut q, 7), [pair("c", "near")]);
    }

    #[test]
    fn a_reserved_game_goes_only_to_its_session() {
        let mut q = queue();
        q.reserve("near", Some("r")).unwrap();
        q.join("a", "", false, false, None, 0).unwrap();
        q.join("r", "", false, false, None, 1).unwrap();
        assert_eq!(granted(&mut q, 2), [pair("a", "far"), pair("r", "near")]);
    }

    #[test]
    fn a_session_with_a_kept_game_waits_for_it_while_others_are_free() {
        let mut q = queue();
        q.reserve("far", Some("r")).unwrap();
        q.take("far", 0).unwrap();
        q.join("r", "", false, false, None, 1).unwrap();
        assert_eq!(granted(&mut q, 2), []);
        q.join("a", "", false, false, None, 3).unwrap();
        assert_eq!(granted(&mut q, 4), [pair("a", "near")]);
        q.leave("far").unwrap();
        assert_eq!(granted(&mut q, 5), [pair("r", "far")]);
    }

    #[test]
    fn a_sweep_steps_aside_for_a_waiter_then_resumes_once_no_one_wants_the_game() {
        let mut q = Queue::new(&["far".into()]);
        assert!(q.sweep_start("far", "night").unwrap());
        assert!(!q.sweep_yields("far", "night"));
        q.join("a", "", false, false, None, 10).unwrap();
        assert!(granted(&mut q, 10).is_empty(), "the sweep goes first");
        assert!(q.sweep_yields("far", "night"));
        q.sweep_done("far", "night").unwrap();
        assert_eq!(granted(&mut q, 40), [pair("a", "far")]);
        assert!(!q.sweep_start("far", "night").unwrap(), "held");
        q.free("a");
        assert!(q.sweep_start("far", "night").unwrap());
    }

    #[test]
    fn a_second_sweep_waits_for_the_first_and_leaves_its_step_aside_be() {
        let mut q = Queue::new(&["far".into()]);
        assert!(q.sweep_start("far", "night").unwrap());
        q.take("far", 3).unwrap();
        q.leave("far").unwrap();
        assert!(!q.sweep_start("far", "day").unwrap());
        assert!(q.sweep_yields("far", "night"), "still asked to step aside");
        q.sweep_done("far", "night").unwrap();
        assert!(q.sweep_start("far", "day").unwrap());
    }

    #[test]
    fn a_sweep_still_mid_row_past_the_cap_counts_as_gone() {
        let mut q = Queue::new(&["far".into()]);
        q.sweep_start("far", "night").unwrap();
        q.join("a", "", false, false, None, 0).unwrap();
        assert!(granted(&mut q, 0).is_empty());
        assert!(granted(&mut q, CAP - 1).is_empty());
        assert_eq!(granted(&mut q, CAP), [pair("a", "far")]);
        assert!(q.sweep_yields("far", "night"));
    }

    #[test]
    fn with_the_first_game_held_a_waiter_asks_the_sweep_on_the_next_to_step_aside() {
        let mut q = queue();
        q.join("x", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        assert!(q.sweep_start("far", "night").unwrap());
        q.join("a", "", false, false, None, 1).unwrap();
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
    fn a_waiter_on_one_game_waits_for_it_and_lets_the_line_behind_it_through() {
        let mut q = queue();
        q.join("x", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        q.free("x");
        q.join("y", "", false, false, Some("near"), 1).unwrap();
        q.join("far-only", "", false, false, Some("far"), 1)
            .unwrap();
        q.join("any", "", false, false, None, 1).unwrap();
        assert_eq!(
            granted(&mut q, 1),
            [pair("y", "near"), pair("far-only", "far")]
        );
        q.free("y");
        q.join("far-only-2", "", false, false, Some("far"), 2)
            .unwrap();
        assert_eq!(granted(&mut q, 2), [pair("any", "near")], "never near");
        assert!(q.held("far-only-2").is_none());
    }

    #[test]
    fn a_sweep_starts_on_a_game_no_waiter_wants() {
        let mut q = queue();
        q.join("x", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        q.join("a", "", false, false, Some("near"), 1).unwrap();
        assert!(q.sweep_start("far", "night").unwrap());
        assert!(!q.sweep_yields("far", "night"));
        assert!(granted(&mut q, 2).is_empty());
        assert!(!q.sweep_yields("far", "night"), "a wants near only");
    }

    #[test]
    fn a_join_on_names_one_known_game_and_never_with_both() {
        let mut q = queue();
        assert!(q.join("a", "", false, false, Some("moon"), 0).is_err());
        assert!(q.join("a", "", false, true, Some("near"), 0).is_err());
        assert!(q.waiting.is_empty());
        q.join("a", "note", false, false, Some("far"), 0).unwrap();
        assert!(q.to_string().contains("1. a (on far): note"), "{q}");
    }

    #[test]
    fn a_both_waiter_gets_every_game_in_one_tick_once_all_are_free() {
        let mut q = queue();
        q.join("x", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        q.join("ab", "", false, true, None, 1).unwrap();
        assert_eq!(granted(&mut q, 1), [], "never just far");
        assert!(q.held("ab").is_none());
        q.free("x");
        assert_eq!(granted(&mut q, 2), [pair("ab", "near"), pair("ab", "far")]);
        assert_eq!(q.holds("ab").count(), 2);
        assert!(q.free("ab"));
        assert_eq!(q.holds("ab").count(), 0);
    }

    #[test]
    fn a_both_waiter_that_can_t_be_served_lets_the_line_behind_it_through() {
        let mut q = queue();
        q.join("x", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        q.join("ab", "", false, true, None, 1).unwrap();
        q.join("b", "", false, false, None, 2).unwrap();
        assert_eq!(granted(&mut q, 3), [pair("b", "far")]);
        assert_eq!(q.waiting[0].session, "ab");
    }

    #[test]
    fn a_both_waiter_skips_a_game_kept_for_another_session() {
        let mut q = queue();
        q.reserve("far", Some("r")).unwrap();
        q.join("ab", "", false, true, None, 0).unwrap();
        assert_eq!(granted(&mut q, 1), []);
        q.reserve("far", Some("ab")).unwrap();
        assert_eq!(granted(&mut q, 2), [pair("ab", "near"), pair("ab", "far")]);
    }

    #[test]
    fn a_both_waiter_leaves_a_sweep_be_until_every_other_game_is_free() {
        let mut q = queue();
        q.join("x", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        assert!(q.sweep_start("far", "night").unwrap());
        q.join("ab", "", false, true, None, 1).unwrap();
        assert_eq!(granted(&mut q, 1), []);
        assert!(!q.sweep_yields("far", "night"), "near is still held");
        q.sweep_done("far", "night").unwrap();
        assert!(
            q.sweep_start("far", "night").unwrap(),
            "ab can't be served yet"
        );
        q.free("x");
        assert_eq!(granted(&mut q, 2), []);
        assert!(q.sweep_yields("far", "night"));
        q.join("b", "", false, false, None, 3).unwrap();
        assert_eq!(
            granted(&mut q, 3),
            [],
            "ab claims near while far's sweep steps aside"
        );
        q.sweep_done("far", "night").unwrap();
        assert_eq!(granted(&mut q, 4), [pair("ab", "near"), pair("ab", "far")]);
    }

    #[test]
    fn renewing_renews_every_hold_and_names_this_machine_s_game() {
        let mut q = queue();
        q.join("ab", "", false, true, None, 0).unwrap();
        granted(&mut q, 0);
        assert_eq!(q.renew("ab", "far", 600).unwrap(), "far");
        assert_eq!(q.renew("ab", "", 600).unwrap(), "near");
        granted(&mut q, 600 + LEASE - 1);
        assert_eq!(q.holds("ab").count(), 2, "both renewed at 600");
        granted(&mut q, 600 + LEASE);
        assert_eq!(q.holds("ab").count(), 0);
    }

    #[test]
    fn configure_keeps_known_games_in_the_new_order() {
        let mut q = queue();
        q.join("a", "", false, false, None, 0).unwrap();
        granted(&mut q, 0);
        q.configure(&["far".into(), "near".into(), "new".into()]);
        let names: Vec<_> = q.games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["far", "near", "new"]);
        assert_eq!(q.held("a").unwrap().name, "near");
    }

    /// `near`, and `far` with two more slots.
    fn slots() -> Queue {
        Queue::new(&["near".into(), "far".into(), "far-2".into(), "far-3".into()])
    }

    #[test]
    fn a_slot_runs_on_its_machine() {
        assert_eq!(machine("far-2"), "far");
        assert_eq!(machine("far"), "far");
        assert_eq!(machine("far-x"), "far-x");
        assert!(suits("far", "far-3") && suits("far-2", "far-2"));
        assert!(!suits("far-2", "far") && !suits("far", "near"));
    }

    #[test]
    fn a_waiter_on_a_machine_gets_any_of_its_slots_and_on_a_slot_that_one() {
        let mut q = slots();
        for s in ["a", "b"] {
            q.join(s, "", false, false, Some("far"), 0).unwrap();
        }
        q.join("c", "", false, false, Some("far-2"), 0).unwrap();
        q.join("d", "", false, false, Some("far"), 0).unwrap();
        let got = granted(&mut q, 1);
        assert_eq!(
            got,
            [pair("a", "far"), pair("b", "far-2"), pair("d", "far-3")]
        );
        assert_eq!(q.waiting[0].session, "c", "far-2 alone, held by b");
        q.free("b");
        assert_eq!(granted(&mut q, 2), [pair("c", "far-2")]);
    }

    #[test]
    fn beside_a_sweep_a_machine_plays_one_game_more_and_leaves_the_sweep_be() {
        let mut q = slots();
        assert!(q.sweep_start("far", "night").unwrap());
        for w in ["a", "b"] {
            q.join(w, "", false, false, Some("far"), 1).unwrap();
        }
        assert_eq!(granted(&mut q, 1), [pair("a", "far-2")]);
        assert!(!q.sweep_yields("far", "night"), "b waits, the sweep runs");
        q.join("c", "", false, false, None, 2).unwrap();
        assert_eq!(granted(&mut q, 2), [pair("c", "near")], "near has no sweep");
        q.sweep_done("far", "night").unwrap();
        assert_eq!(granted(&mut q, 3), [pair("b", "far")]);
    }

    #[test]
    fn a_sweep_waits_while_two_slots_of_its_machine_play() {
        let mut q = slots();
        q.join("a", "", false, false, Some("far-2"), 0).unwrap();
        q.join("b", "", false, false, Some("far-3"), 0).unwrap();
        assert_eq!(granted(&mut q, 0), [pair("a", "far-2"), pair("b", "far-3")]);
        assert!(!q.sweep_start("far", "night").unwrap());
        q.free("b");
        assert!(q.sweep_start("far", "night").unwrap());
    }

    #[test]
    fn a_both_waiter_gets_one_game_of_each_machine() {
        let mut q = slots();
        q.join("x", "", false, false, Some("far"), 0).unwrap();
        granted(&mut q, 0);
        q.join("ab", "", false, true, None, 1).unwrap();
        assert_eq!(
            granted(&mut q, 1),
            [pair("ab", "near"), pair("ab", "far-2")]
        );
        assert_eq!(q.holds("ab").count(), 2);
    }
}
