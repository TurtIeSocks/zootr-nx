//! `zootr turn serve`: the queue as a keyed service. Each connection asks
//! one thing. A `wait` or a sweep's start is answered once it's granted,
//! and a ticker lapses idle holds and hands out freed games meanwhile.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::queue::Queue;
use super::remote::{Answer, Ask, Request, ServeConfig, TurnConfig};

/// How often the ticker looks for lapsed holds and freed games, and how
/// long a waiter sleeps between looks when nothing wakes it.
const TICK: Duration = Duration::from_secs(5);

struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
    serve: ServeConfig,
    key: String,
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

impl Shared {
    fn save(&self, q: &Queue) {
        let json = serde_json::to_vec_pretty(q).expect("a queue serializes");
        if let Err(e) = std::fs::write(&self.serve.state, json) {
            eprintln!("saving the queue to {}: {e}", self.serve.state.display());
        }
    }

    /// Lapse, grant and save, then wake every waiter to look again.
    fn tick(&self, q: &mut Queue) {
        for g in q.tick(now(), self.serve.lease, self.serve.yield_cap) {
            println!("{} gets {}", g.session, g.game);
        }
        self.save(q);
        self.changed.notify_all();
    }

    fn answer(&self, ask: Ask) -> Result<Answer> {
        let mut q = self.queue.lock().expect("the queue lock");
        let mut answer = Answer::default();
        match ask {
            Ask::Status => answer.queue = Some(q.clone()),
            Ask::Join {
                session,
                note,
                urgent,
                both,
                on,
            } => {
                answer.place = Some(q.join(&session, &note, urgent, both, on.as_deref(), now())?)
            }
            Ask::Wait { session } => loop {
                let held: Vec<_> = q.holds(&session).map(|g| g.name.as_str()).collect();
                if !held.is_empty() {
                    answer.game = Some(held.join(", "));
                    break;
                }
                if !q.waiting.iter().any(|w| w.session == session) {
                    bail!("{session} isn't in line: `zootr turn join` first");
                }
                q = self
                    .changed
                    .wait_timeout(q, TICK)
                    .expect("the queue lock")
                    .0;
            },
            Ask::Free { session } => answer.yes = Some(q.free(&session)),
            Ask::Renew {
                session,
                here,
                look,
            } => {
                answer.game = Some(match look {
                    true => q.look(&session, &here)?,
                    false => q.renew(&session, &here, now())?,
                })
            }
            Ask::Take { game } => q.take(&game, now())?,
            Ask::Leave { game } => q.leave(&game)?,
            Ask::Reserve { game, session } => q.reserve(&game, session.as_deref())?,
            Ask::SweepStart { game, name } => {
                while !q.sweep_start(&game, &name)? {
                    q = self
                        .changed
                        .wait_timeout(q, TICK)
                        .expect("the queue lock")
                        .0;
                }
                answer.yes = Some(true);
            }
            Ask::SweepYields { game, name } => answer.yes = Some(q.sweep_yields(&game, &name)),
            Ask::SweepDone { game, name } => q.sweep_done(&game, &name)?,
        }
        self.tick(&mut q);
        Ok(answer)
    }

    fn handle(&self, stream: TcpStream) -> Result<()> {
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        let answer = match serde_json::from_str::<Request>(&line) {
            Err(e) => Answer::refused(format!("not a request: {e}")),
            Ok(r) if r.key != self.key => Answer::refused("wrong key".into()),
            Ok(r) => self
                .answer(r.ask)
                .unwrap_or_else(|e| Answer::refused(format!("{e:#}"))),
        };
        let mut reply = serde_json::to_string(&answer)?;
        reply.push('\n');
        (&stream).write_all(reply.as_bytes())?;
        Ok(())
    }
}

impl Answer {
    fn refused(error: String) -> Self {
        Self {
            error: Some(error),
            ..Self::default()
        }
    }
}

/// Run the queue for `turn`'s games until killed, keeping it in the
/// state file between restarts.
pub fn serve(turn: &TurnConfig) -> Result<()> {
    let serve = turn
        .serve
        .as_ref()
        .context("no `serve` in the config's `turn`")?;
    let listener = TcpListener::bind(&serve.listen)
        .with_context(|| format!("listening on {}", serve.listen))?;
    println!(
        "turn queue on {} for {}",
        serve.listen,
        turn.games.join(", ")
    );
    serve_on(listener, turn)
}

fn serve_on(listener: TcpListener, turn: &TurnConfig) -> Result<()> {
    let serve = turn
        .serve
        .clone()
        .context("no `serve` in the config's `turn`")?;
    let mut queue = match std::fs::read(&serve.state) {
        Ok(b) => serde_json::from_slice(&b)
            .with_context(|| format!("reading {}", serve.state.display()))?,
        Err(_) => Queue::default(),
    };
    queue.configure(&turn.games);
    let shared = Arc::new(Shared {
        queue: Mutex::new(queue),
        changed: Condvar::new(),
        serve,
        key: turn.key()?,
    });
    let ticker = Arc::clone(&shared);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(TICK);
            let mut q = ticker.queue.lock().expect("the queue lock");
            ticker.tick(&mut q);
        }
    });
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            if let Err(e) = shared.handle(stream) {
                eprintln!("a turn request: {e:#}");
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::remote::Ask;

    /// A service on a free local port, and a config that reaches it,
    /// keeping its files in a fresh folder named for the test.
    fn started(test: &str, games: &[&str]) -> (TurnConfig, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("zootr-test-{}-turn-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let key_file = dir.join("key");
        std::fs::write(&key_file, "sesame\n").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let turn = TurnConfig {
            url: format!("127.0.0.1:{port}"),
            key_file,
            games: games.iter().map(|g| g.to_string()).collect(),
            here: games[0].into(),
            serve: Some(ServeConfig {
                listen: format!("127.0.0.1:{port}"),
                state: dir.join("queue.json"),
                ..ServeConfig::default()
            }),
        };
        let run = turn.clone();
        std::thread::spawn(move || serve_on(listener, &run));
        (turn, dir)
    }

    fn join(turn: &TurnConfig, session: &str) -> Answer {
        let ask = Ask::Join {
            session: session.into(),
            note: "".into(),
            urgent: false,
            both: false,
            on: None,
        };
        turn.ask(ask).unwrap()
    }

    #[test]
    fn a_session_joins_waits_is_granted_renews_and_frees() {
        let (turn, dir) = started("grant", &["near"]);
        assert_eq!(join(&turn, "a").place, Some(1));
        let wait = turn
            .ask(Ask::Wait {
                session: "a".into(),
            })
            .unwrap();
        assert_eq!(wait.game.as_deref(), Some("near"));
        let renew = turn
            .ask(Ask::Renew {
                session: "a".into(),
                here: "".into(),
                look: false,
            })
            .unwrap();
        assert_eq!(renew.game.as_deref(), Some("near"));
        assert_eq!(
            turn.ask(Ask::Free {
                session: "a".into()
            })
            .unwrap()
            .yes,
            Some(true)
        );
        let refused = turn
            .ask(Ask::Renew {
                session: "a".into(),
                here: "".into(),
                look: false,
            })
            .unwrap_err();
        assert!(refused.to_string().contains("isn't holding"), "{refused}");
        let kept: Queue =
            serde_json::from_slice(&std::fs::read(dir.join("queue.json")).unwrap()).unwrap();
        assert_eq!(kept.games[0].name, "near");
    }

    #[test]
    fn a_wait_returns_once_the_holder_frees_the_game() {
        let (turn, _dir) = started("wait", &["near"]);
        join(&turn, "a");
        turn.ask(Ask::Wait {
            session: "a".into(),
        })
        .unwrap();
        join(&turn, "b");
        let waiter = turn.clone();
        let wait = std::thread::spawn(move || {
            waiter.ask(Ask::Wait {
                session: "b".into(),
            })
        });
        std::thread::sleep(Duration::from_millis(200));
        assert!(!wait.is_finished());
        turn.ask(Ask::Free {
            session: "a".into(),
        })
        .unwrap();
        assert_eq!(wait.join().unwrap().unwrap().game.as_deref(), Some("near"));
    }

    #[test]
    fn a_both_wait_names_every_game_and_renew_names_this_machine_s() {
        let (turn, _dir) = started("both", &["near", "far"]);
        let ask = Ask::Join {
            session: "ab".into(),
            note: "".into(),
            urgent: false,
            both: true,
            on: None,
        };
        turn.ask(ask).unwrap();
        let wait = turn.ask(Ask::Wait {
            session: "ab".into(),
        });
        assert_eq!(wait.unwrap().game.as_deref(), Some("near, far"));
        let renew = turn.ask(Ask::Renew {
            session: "ab".into(),
            here: "far".into(),
            look: false,
        });
        assert_eq!(renew.unwrap().game.as_deref(), Some("far"));
    }

    #[test]
    fn requests_and_a_queue_from_before_both_still_load() {
        let join = r#"{"op":"join","session":"a","note":"","urgent":false}"#;
        assert!(matches!(
            serde_json::from_str(join).unwrap(),
            Ask::Join {
                both: false,
                on: None,
                ..
            }
        ));
        let renew: Ask = serde_json::from_str(r#"{"op":"renew","session":"a"}"#).unwrap();
        assert!(matches!(renew, Ask::Renew { here, .. } if here.is_empty()));
        let q = r#"{"games":[],"waiting":[{"session":"a","note":"","urgent":false,"since":0}]}"#;
        let waiter = &serde_json::from_str::<Queue>(q).unwrap().waiting[0];
        assert!(!waiter.both && waiter.on.is_none());
    }

    #[test]
    fn a_wrong_key_is_refused() {
        let (mut turn, dir) = started("key", &["near"]);
        turn.key_file = dir.join("other");
        std::fs::write(&turn.key_file, "guess").unwrap();
        assert!(
            turn.ask(Ask::Status)
                .unwrap_err()
                .to_string()
                .contains("wrong key")
        );
    }

    #[test]
    fn a_refusal_counts_as_the_service_answering_and_a_closed_port_as_down() {
        let (turn, dir) = started("down", &["near"]);
        let refused = turn
            .ask(Ask::Renew {
                session: "a".into(),
                here: "".into(),
                look: false,
            })
            .unwrap_err();
        assert!(!crate::turn::unreachable(&refused), "{refused:#}");
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let down = TurnConfig {
            url: format!("127.0.0.1:{port}"),
            ..turn
        };
        let _ = dir;
        assert!(crate::turn::unreachable(
            &down.ask(Ask::Status).unwrap_err()
        ));
    }
}
