//! The Archipelago client: sends the checks Link makes, gives him what
//! other worlds find for him, and reports the goal. One core for the app
//! and `zootr ap connect`, which only start it and show its notes.
//!
//! The game side is the [`Game`] trait, so the core runs against a fake in
//! tests. [`Live`] is the real one: the Mac game or a native build's bridge.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use archipelago_rs::{self as ap, ClientStatus, ConnectionOptions, ConnectionState, ItemHandling};
use ounce_formats::Romfs;

use super::ids::Ids;
use super::seed::Dealt;
use super::{ApSeed, GAME, RECEIVED};
use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::known::flow;
use crate::live::{self, Game as Target};
use crate::steps;
use crate::tracker::Tracker;

/// What the core needs of the running game.
pub trait Game {
    /// Link is in play: no load and no event.
    fn ready(&mut self) -> Result<bool>;
    /// The spots Link has checked.
    fn checked(&mut self) -> Result<BTreeSet<u64>>;
    /// Link has reached the goal.
    fn goal(&mut self) -> Result<bool>;
    /// How many received items Link has been given, from the save.
    fn given(&mut self) -> Result<usize>;
    /// Give Link `actor`, then count `given` items given.
    fn give(&mut self, actor: &str, given: usize) -> Result<()>;
}

/// The seed's AP ids: each free check's location, each pool item's actor.
pub struct Book {
    locations: HashMap<u64, i64>,
    actors: HashMap<i64, String>,
}

impl Book {
    pub fn new(romfs: &Romfs, seed: &ApSeed) -> Result<Self> {
        let dealt = Dealt::new(romfs, &seed.full_settings())?;
        let ids = Ids::load()?;
        let id = |n: u64| i64::try_from(n).context("an AP id past i64");
        let locations = (seed.placements.keys())
            .filter_map(|&h| Some((h, ids.location_id(h)?)))
            .map(|(h, l)| Ok((h, id(l)?)))
            .collect::<Result<_>>()?;
        let actors = (ids.actors(dealt.pool.iter().map(String::as_str)))
            .into_iter()
            .map(|(i, a)| Ok((id(i)?, a.to_owned())))
            .collect::<Result<_>>()?;
        Ok(Self { locations, actors })
    }

    /// The AP locations of `spots`, skipping any AP didn't place.
    fn locations<'a>(&self, spots: impl IntoIterator<Item = &'a u64>) -> Vec<i64> {
        spots
            .into_iter()
            .filter_map(|h| self.locations.get(h).copied())
            .collect()
    }
}

/// What the core tells its front end.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    Connecting,
    Connected,
    /// The server refused the slot, with its reasons. No retry.
    Refused(String),
    /// Lost, retrying after the wait.
    Lost(String, Duration),
    /// Locations sent, by name.
    Sent(Vec<String>),
    Received {
        item: String,
        from: String,
    },
    Goal,
    /// Something went wrong that the core keeps going past.
    Trouble(String),
}

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Refused(why) => write!(f, "refused: {why}"),
            Self::Lost(why, wait) => write!(f, "lost ({why}), retrying in {} s", wait.as_secs()),
            Self::Sent(names) => write!(f, "sent {}", names.join(", ")),
            Self::Received { item, from } => write!(f, "received {item} from {from}"),
            Self::Goal => write!(f, "goal sent"),
            Self::Trouble(why) => write!(f, "{why}"),
        }
    }
}

/// One look at the game, without the server: what to send, whether the goal
/// is new, and the one give to make.
#[derive(Default)]
struct Ledger {
    /// Spots the server has from us this connection.
    sent: BTreeSet<u64>,
    goal: bool,
}

#[derive(Debug, Default, PartialEq)]
struct Look {
    send: Vec<u64>,
    goal: bool,
    /// The received item's index and actor, given.
    gave: Option<(usize, String)>,
}

impl Ledger {
    /// After a connect: the server may have lost what it had.
    fn reset(&mut self) {
        *self = Self::default();
    }

    /// `received` is the server's full list of item ids for this slot.
    fn look(&mut self, game: &mut impl Game, book: &Book, received: &[i64]) -> Result<Look> {
        let checked = game.checked()?;
        let send: Vec<u64> = checked.difference(&self.sent).copied().collect();
        self.sent.extend(&send);
        let goal = !self.goal && game.goal()?;
        self.goal |= goal;
        let mut look = Look {
            send,
            goal,
            gave: None,
        };
        if !game.ready()? {
            return Ok(look);
        }
        let given = game.given()?;
        let Some(&id) = received.get(given) else {
            ensure!(
                given <= received.len(),
                "the save has {given} items given, but the server sent {}: is it another room's save?",
                received.len()
            );
            return Ok(look);
        };
        let actor =
            (book.actors.get(&id)).with_context(|| format!("no item of ours has AP id {id}"))?;
        game.give(actor, given + 1)?;
        look.gave = Some((given, actor.clone()));
        Ok(look)
    }
}

/// Where and as whom to connect.
#[derive(Clone, Debug)]
pub struct Room {
    /// `host[:port]` or a `ws://`/`wss://` url.
    pub server: String,
    pub slot: String,
    pub password: Option<String>,
}

/// How often the core looks at the game.
const LOOK: Duration = Duration::from_millis(500);
/// How often it lets the connection answer.
const POLL: Duration = Duration::from_millis(50);
/// The waits before each reconnect, the last repeating.
const BACKOFF: [u64; 4] = [2, 5, 10, 30];

/// The wait before reconnect `tries` (0 first).
fn backoff(tries: usize) -> Duration {
    Duration::from_secs(BACKOFF[tries.min(BACKOFF.len() - 1)])
}

/// Run the client until `stop` sends or closes. Never returns early on a
/// lost connection: it reconnects. A refusal ends it.
pub fn run(
    room: &Room,
    book: &Book,
    game: &mut impl Game,
    stop: &Receiver<()>,
    note: &mut impl FnMut(Note),
) {
    // The app's reqwest brings rustls' ring backend and this crate its
    // aws-lc one. With both, rustls can't pick and panics on connect.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut tries: usize = 0;
    loop {
        note(Note::Connecting);
        match session(room, book, game, stop, note) {
            End::Stopped => return,
            End::Refused(why) => return note(Note::Refused(why)),
            End::Lost(why, connected) => {
                tries = if connected { 0 } else { tries + 1 };
                let wait = backoff(tries.saturating_sub(1));
                note(Note::Lost(why, wait));
                if waited_out(stop, wait) {
                    return;
                }
            }
        }
    }
}

enum End {
    Stopped,
    Refused(String),
    /// Why, and whether it had connected.
    Lost(String, bool),
}

/// Sleep `wait`, or less if `stop` sends: true then.
fn waited_out(stop: &Receiver<()>, wait: Duration) -> bool {
    !matches!(
        stop.recv_timeout(wait),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    )
}

/// One connection, from connecting to its end.
fn session(
    room: &Room,
    book: &Book,
    game: &mut impl Game,
    stop: &Receiver<()>,
    note: &mut impl FnMut(Note),
) -> End {
    // Other worlds' finds alone would be `starting_inventory: false`, but
    // archipelago_rs 3.0.1 then sends no items flag at all, and the server
    // sends nothing. So take the start inventory too, and skip it: the mod
    // already gives it.
    let mut options = ConnectionOptions::new().receive_items(ItemHandling::OtherWorlds {
        own_world: false,
        starting_inventory: true,
    });
    if let Some(p) = &room.password {
        options = options.password(p.clone());
    }
    let mut conn: ap::Connection =
        ap::Connection::new(room.server.clone(), room.slot.clone(), Some(GAME), options);
    let mut ledger = Ledger::default();
    let mut connected = false;
    let mut next_look = Instant::now();
    loop {
        if !matches!(stop.try_recv(), Err(TryRecvError::Empty)) {
            return End::Stopped;
        }
        for event in conn.update() {
            match event {
                ap::Event::Connected => {
                    connected = true;
                    ledger.reset();
                    note(Note::Connected);
                }
                ap::Event::Error(e) if !conn.is_disconnected() => {
                    note(Note::Trouble(e.to_string()))
                }
                _ => {}
            }
        }
        match conn.state_mut() {
            ConnectionState::Disconnected(e) => {
                return match e {
                    ap::Error::ConnectionRefused(_) => End::Refused(e.to_string()),
                    _ => End::Lost(e.to_string(), connected),
                };
            }
            ConnectionState::Connected(client) if Instant::now() >= next_look => {
                next_look = Instant::now() + LOOK;
                tick(client, book, game, &mut ledger, note);
            }
            _ => {}
        }
        thread::sleep(POLL);
    }
}

/// One look, and what it sends.
fn tick(
    client: &mut ap::Client,
    book: &Book,
    game: &mut impl Game,
    ledger: &mut Ledger,
    note: &mut impl FnMut(Note),
) {
    let received: Vec<i64> = found(client).map(|r| r.item().id()).collect();
    let look = match ledger.look(game, book, &received) {
        Ok(look) => look,
        Err(e) => return note(Note::Trouble(format!("{e:#}"))),
    };
    let ids = book.locations(&look.send);
    if !ids.is_empty() {
        // The server may have lost them while we were away: always resend.
        let names = ids.iter().map(|&id| location_name(client, id)).collect();
        match client.mark_checked(ids) {
            Ok(()) => note(Note::Sent(names)),
            Err(e) => {
                ledger.sent.retain(|s| !look.send.contains(s));
                note(Note::Trouble(e.to_string()));
            }
        }
    }
    if look.goal {
        match client.set_status(ClientStatus::Goal) {
            Ok(()) => note(Note::Goal),
            Err(e) => {
                ledger.goal = false;
                note(Note::Trouble(e.to_string()));
            }
        }
    }
    if let Some((index, _)) = look.gave
        && let Some(r) = found(client).nth(index)
    {
        note(Note::Received {
            item: r.item().name().to_string(),
            from: r.sender().name().to_string(),
        });
    }
}

/// AP's location for an item in the start inventory.
const START: i64 = -2;

/// What other worlds found for Link, in the server's order.
fn found(client: &ap::Client) -> impl Iterator<Item = &ap::ReceivedItem> {
    (client.received_items().iter()).filter(|r| r.location().id() != START)
}

fn location_name(client: &ap::Client, id: i64) -> String {
    (client.this_game().location(id)).map_or_else(|| id.to_string(), |l| l.name().to_string())
}

/// The installed seed's client parts, on the game `target` runs.
pub struct Installed {
    pub book: Book,
    pub game: Live,
    /// The seed's own slot.
    pub slot: String,
}

/// What the client needs of the installed seed. `gd` is the installed
/// seed's, which names the Int it adds.
pub fn installed(romfs: &Romfs, gd: Arc<GameData>, target: Target) -> Result<Installed> {
    let history = crate::history::History::load()?;
    let entry = history.get(history.installed.context("no seed is installed")?)?;
    let seed =
        (entry.extras.ap.as_ref()).context("the installed seed isn't one from Archipelago")?;
    Ok(Installed {
        book: Book::new(romfs, seed)?,
        game: Live::new(target, gd, &entry.report.spots)?,
        slot: seed.slot.clone(),
    })
}

/// The running game, through the mailbox.
pub struct Live {
    dev: Dev<Target>,
    gd: Arc<GameData>,
    tracker: Tracker,
    goal: u32,
    received: u32,
}

impl Live {
    pub fn new(game: Target, gd: Arc<GameData>, spots: &[Spot]) -> Result<Self> {
        let tracker = Tracker::new(&gd, spots)?;
        Ok(Self {
            dev: Dev::new(game)?,
            gd,
            tracker,
            goal: live::hash(&steps::saved(flow::DEFEAT_GANON))?,
            received: live::hash(RECEIVED)?,
        })
    }

    fn get(&self, hash: u32) -> Result<String> {
        self.dev.game().get(&self.gd, hash)
    }
}

/// The step Ganon's defeat reaches once his last fight is over.
const GOAL_DONE: &str = "Complete";

impl Game for Live {
    fn ready(&mut self) -> Result<bool> {
        let s = self.dev.state()?;
        Ok(!s.loading && !s.event)
    }

    fn checked(&mut self) -> Result<BTreeSet<u64>> {
        let set = |f: u32| self.dev.game().get(&self.gd, f).is_ok_and(|v| v == "true");
        let on: Vec<u32> = self.tracker.flags().filter(|&f| set(f)).collect();
        self.tracker.load(|f| on.contains(&f));
        Ok(self.tracker.checked().clone())
    }

    fn goal(&mut self) -> Result<bool> {
        Ok(self.get(self.goal)? == GOAL_DONE)
    }

    fn given(&mut self) -> Result<usize> {
        let n: i64 = self.get(self.received)?.parse()?;
        Ok(usize::try_from(n)?)
    }

    fn give(&mut self, actor: &str, given: usize) -> Result<()> {
        self.dev.give_level(actor)?;
        live::set(
            self.dev.game(),
            self.dev.game().global(),
            self.received,
            &given.to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        busy: bool,
        checked: BTreeSet<u64>,
        goal: bool,
        given: Vec<String>,
    }

    impl Game for Fake {
        fn ready(&mut self) -> Result<bool> {
            Ok(!self.busy)
        }
        fn checked(&mut self) -> Result<BTreeSet<u64>> {
            Ok(self.checked.clone())
        }
        fn goal(&mut self) -> Result<bool> {
            Ok(self.goal)
        }
        fn given(&mut self) -> Result<usize> {
            Ok(self.given.len())
        }
        fn give(&mut self, actor: &str, given: usize) -> Result<()> {
            self.given.push(actor.into());
            assert_eq!(self.given.len(), given);
            Ok(())
        }
    }

    fn book() -> Book {
        Book {
            locations: [(10, 100), (20, 200)].into(),
            actors: [(1, "Item_FakeBow".into()), (2, "Item_FakeHook".into())].into(),
        }
    }

    #[test]
    fn a_check_is_sent_once_per_connection() {
        let (mut game, book, mut ledger) = (Fake::default(), book(), Ledger::default());
        game.checked = [10].into();
        assert_eq!(ledger.look(&mut game, &book, &[]).unwrap().send, [10]);
        game.checked.insert(20);
        assert_eq!(ledger.look(&mut game, &book, &[]).unwrap().send, [20]);
        assert!(ledger.look(&mut game, &book, &[]).unwrap().send.is_empty());
        ledger.reset();
        assert_eq!(ledger.look(&mut game, &book, &[]).unwrap().send, [10, 20]);
    }

    #[test]
    fn a_spot_ap_didnt_place_has_no_location() {
        assert_eq!(book().locations(&[10, 30, 20]), [100, 200]);
    }

    #[test]
    fn items_are_given_once_each_in_order_and_only_in_play() {
        let (mut game, book, mut ledger) = (Fake::default(), book(), Ledger::default());
        let received = [2, 1, 2];
        game.busy = true;
        assert_eq!(ledger.look(&mut game, &book, &received).unwrap().gave, None);
        game.busy = false;
        for _ in 0..5 {
            ledger.look(&mut game, &book, &received).unwrap();
        }
        assert_eq!(
            game.given,
            ["Item_FakeHook", "Item_FakeBow", "Item_FakeHook"]
        );
    }

    #[test]
    fn a_reconnect_resending_the_list_gives_nothing_twice() {
        let (mut game, book, mut ledger) = (Fake::default(), book(), Ledger::default());
        ledger.look(&mut game, &book, &[1]).unwrap();
        ledger.reset();
        let look = ledger.look(&mut game, &book, &[1, 2]).unwrap();
        assert_eq!(look.gave, Some((1, "Item_FakeHook".into())));
        assert_eq!(game.given.len(), 2);
    }

    #[test]
    fn a_save_ahead_of_the_server_gives_nothing_and_says_so() {
        let (mut game, book, mut ledger) = (Fake::default(), book(), Ledger::default());
        game.given = vec!["Item_FakeBow".into(); 3];
        let err = ledger.look(&mut game, &book, &[1]).unwrap_err();
        assert!(err.to_string().contains("another room"), "{err}");
        assert_eq!(game.given.len(), 3);
    }

    #[test]
    fn an_unknown_item_stops_the_queue_on_it() {
        let (mut game, book, mut ledger) = (Fake::default(), book(), Ledger::default());
        assert!(ledger.look(&mut game, &book, &[9, 1]).is_err());
        assert!(game.given.is_empty());
    }

    #[test]
    fn the_goal_is_sent_once() {
        let (mut game, book, mut ledger) = (Fake::default(), book(), Ledger::default());
        assert!(!ledger.look(&mut game, &book, &[]).unwrap().goal);
        game.goal = true;
        assert!(ledger.look(&mut game, &book, &[]).unwrap().goal);
        assert!(!ledger.look(&mut game, &book, &[]).unwrap().goal);
    }

    #[test]
    fn reconnects_back_off_then_hold() {
        let secs: Vec<u64> = (0..6).map(|t| backoff(t).as_secs()).collect();
        assert_eq!(secs, [2, 5, 10, 30, 30, 30]);
    }

    #[test]
    fn every_item_the_seed_places_for_us_has_an_actor() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let seed = ApSeed::from_zip(include_bytes!("testdata/two_slots.apznx")).unwrap();
        let book = Book::new(&romfs, &seed).unwrap();
        let ours: Vec<i64> = (seed.placements.values())
            .filter_map(|p| match p {
                super::super::Placed::Own(id) => Some(*id as i64),
                super::super::Placed::Foreign(_) => None,
            })
            .collect();
        assert!(!ours.is_empty());
        let missing: Vec<_> = ours
            .iter()
            .filter(|id| !book.actors.contains_key(id))
            .collect();
        assert!(missing.is_empty(), "no actor for {missing:?}");
        assert!(
            seed.placements
                .keys()
                .all(|h| book.locations.contains_key(h))
        );
    }

    /// Against a real room: `ZOOTR_AP_SERVER` hosting the `.apznx` at
    /// `ZOOTR_AP_SEED`, whose other slot is ChecksFinder's `Finder`. The core
    /// sends checks and the goal, and gives what Finder's checks find for us.
    #[test]
    #[ignore = "needs a running Archipelago room"]
    fn a_room_takes_checks_and_the_goal_and_sends_items() {
        let (Ok(server), Ok(path)) = (
            std::env::var("ZOOTR_AP_SERVER"),
            std::env::var("ZOOTR_AP_SEED"),
        ) else {
            panic!("set ZOOTR_AP_SERVER and ZOOTR_AP_SEED");
        };
        let romfs = crate::game::dev_dump().expect("the dump");
        let seed = ApSeed::read(std::path::Path::new(&path)).unwrap();
        let book = Book::new(&romfs, &seed).unwrap();
        let room = Room {
            server: server.clone(),
            slot: seed.slot.clone(),
            password: None,
        };
        let finder = thread::spawn(move || {
            let mut conn: ap::Connection = ap::Connection::new(
                server,
                "Finder",
                Some("ChecksFinder"),
                ConnectionOptions::new(),
            );
            let until = Instant::now() + Duration::from_secs(8);
            let mut done = false;
            while Instant::now() < until {
                conn.update();
                if let ConnectionState::Connected(c) = conn.state_mut()
                    && !done
                {
                    let all: Vec<_> = c.unchecked_locations().collect();
                    c.mark_checked(all).unwrap();
                    done = true;
                }
                thread::sleep(POLL);
            }
            assert!(done, "Finder never connected");
        });
        let mut game = Fake {
            checked: seed.placements.keys().take(3).copied().collect(),
            goal: true,
            ..Fake::default()
        };
        let (send, stop) = std::sync::mpsc::channel();
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_secs(10));
            drop(send);
        });
        let mut notes = vec![];
        run(&room, &book, &mut game, &stop, &mut |n| notes.push(n));
        finder.join().unwrap();
        stopper.join().unwrap();
        let said = |f: fn(&Note) -> bool| notes.iter().any(f);
        assert!(said(|n| matches!(n, Note::Connected)), "{notes:?}");
        assert!(
            said(|n| matches!(n, Note::Sent(s) if s.len() == 3)),
            "{notes:?}"
        );
        assert!(said(|n| matches!(n, Note::Goal)), "{notes:?}");
        assert!(!game.given.is_empty(), "nothing given: {notes:?}");
        assert!(!said(|n| matches!(n, Note::Trouble(_))), "{notes:?}");
    }
}
