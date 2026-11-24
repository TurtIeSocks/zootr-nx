//! The Archipelago client: sends the checks Link makes, gives him what
//! other worlds find for him, and reports the goal. One core for the app
//! and `zootr ap connect`, which only start it and show its notes.
//!
//! The game side is the [`Game`] trait, so the core runs against a fake in
//! tests. [`Live`] is the real one: the target's game, read directly or through its bridge.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use archipelago_rs::{self as ap, ClientStatus, ConnectionOptions, ConnectionState, ItemHandling};
use ounce_formats::Romfs;

use super::death::{Death, Deaths, Done};
use super::ids::Ids;
use super::notice::Notices;
use super::seed::Dealt;
use super::{ApSeed, GAME, RECEIVED};
use crate::dev::Dev;
use crate::gamedata::GameData;
use crate::generate::Spot;
use crate::known::flow;
use crate::live::{self, Game as Target};
use crate::notice::Shown;
use crate::steps;
use crate::tracker::{Flag, Tracker};

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
    /// Give Link `actor`, then count `given` items given. Says what he
    /// got: a progressive item's level.
    fn give(&mut self, actor: &str, given: usize) -> Result<String>;
    /// Show a received item's notice.
    fn notice(&mut self, shown: &Shown) -> Result<()>;
    /// Link died since the last look.
    fn died(&mut self) -> Result<bool>;
    /// Drop Link's hearts to zero, as a last hit would.
    fn kill(&mut self) -> Result<()>;
}

/// The seed's AP ids: each free check's location, each pool item's actor.
/// And whether the slot plays DeathLink.
pub struct Book {
    locations: HashMap<u64, i64>,
    actors: HashMap<i64, String>,
    death_link: bool,
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
        let actors = (dealt.by_id(&ids)?)
            .into_iter()
            .map(|(i, a)| Ok((id(i)?, a.to_owned())))
            .collect::<Result<_>>()?;
        Ok(Self {
            locations,
            actors,
            death_link: seed.death_link,
        })
    }

    /// The AP location of `spot`, if AP placed it.
    pub fn location(&self, spot: u64) -> Option<i64> {
        self.locations.get(&spot).copied()
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
    /// Locations the server has taken from us, by name: its item-send
    /// message for each, the only reply a check gets.
    Confirmed(Vec<String>),
    Received {
        item: String,
        from: String,
    },
    Goal,
    /// Link died, and the room was told.
    Died,
    /// Link was killed for a death in the room.
    Killed {
        from: String,
        cause: Option<String>,
    },
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
            Self::Confirmed(names) => write!(f, "server has {}", names.join(", ")),
            Self::Received { item, from } => write!(f, "received {item} from {from}"),
            Self::Goal => write!(f, "goal sent"),
            Self::Died => write!(f, "Link died, DeathLink sent"),
            Self::Killed { from, cause } => match cause {
                Some(cause) => write!(f, "DeathLink: {cause}"),
                None => write!(f, "DeathLink: {from} died"),
            },
            Self::Trouble(why) => write!(f, "{why}"),
        }
    }
}

/// What the server already has from us this connection.
#[derive(Default)]
struct Ledger {
    /// Spots the server has from us this connection.
    sent: BTreeSet<u64>,
    goal: bool,
}

/// One look at the game, without the server: what to send, and whether the
/// goal is new.
#[derive(Debug, Default, PartialEq)]
struct Look {
    send: Vec<u64>,
    goal: bool,
}

impl Ledger {
    /// After a connect: the server may have lost what it had.
    fn reset(&mut self) {
        *self = Self::default();
    }

    /// What to send. Both reads come before anything is marked, so a
    /// failed read loses nothing.
    fn look(&mut self, game: &mut impl Game) -> Result<Look> {
        let checked = game.checked()?;
        let goal = !self.goal && game.goal()?;
        let send: Vec<u64> = checked.difference(&self.sent).copied().collect();
        self.sent.extend(&send);
        self.goal |= goal;
        Ok(Look { send, goal })
    }
}

/// Give Link the next received item, if he's in play and one is left:
/// its index and what he got. `received` is the server's full list of item ids
/// for this slot. Apart from [`Ledger::look`], so a give that fails never
/// holds up a send.
fn give_next(
    game: &mut impl Game,
    book: &Book,
    received: &[i64],
) -> Result<Option<(usize, String)>> {
    if !game.ready()? {
        return Ok(None);
    }
    let given = game.given()?;
    let Some(&id) = received.get(given) else {
        ensure!(
            given <= received.len(),
            "the save has {given} items given, but the server sent {}: is it another room's save?",
            received.len()
        );
        return Ok(None);
    };
    let actor =
        (book.actors.get(&id)).with_context(|| format!("no item of ours has AP id {id}"))?;
    let got = game.give(actor, given + 1)?;
    Ok(Some((given, got)))
}

/// Give the next item owed, if any, then show the notice due: between a
/// burst's gives only its first few, so the rest aren't held up. Its index
/// and what Link got. `sender` says who sent an index, as it's given.
///
/// One give a pass, so the room is answered between gives: the server
/// drops a client that leaves its keepalive unanswered for 20 s, and a
/// burst over a bridge takes longer than that.
fn give_one<G: Game>(
    game: &mut G,
    book: &Book,
    received: &[i64],
    notices: &mut Notices,
    mut sender: impl FnMut(usize) -> Option<u32>,
    now: Instant,
) -> Result<Option<(usize, String)>> {
    let gave = give_next(game, book, received)?;
    match &gave {
        Some((index, got)) => {
            if let Some(slot) = sender(*index) {
                notices.got(got, slot);
            }
            notices.look_giving(game, now)?;
        }
        None => drop(notices.look(game, now)?),
    }
    Ok(gave)
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
    if book.death_link {
        options = options.tags([ap::tags::DEATH_LINK]);
    }
    let mut conn: ap::Connection =
        ap::Connection::new(room.server.clone(), room.slot.clone(), Some(GAME), options);
    let mut ledger = Ledger::default();
    let mut deaths = Deaths::default();
    let mut notices = Notices::default();
    let mut heard = Vec::new();
    let mut printed = Vec::new();
    let mut connected = false;
    let mut next_look = Instant::now();
    // An item may still be owed: give it on the next pass, unslept.
    let mut owed = false;
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
                // Something for Link: look now rather than at the next tick.
                ap::Event::ReceivedItems(_) => next_look = Instant::now(),
                ap::Event::Print(ap::Print::ItemSend { item, .. }) => printed.push(item),
                ap::Event::DeathLink { source, cause, .. } if book.death_link => {
                    heard.push(Death { source, cause });
                    next_look = Instant::now();
                }
                _ => {}
            }
        }
        if let ConnectionState::Connected(client) = conn.state_mut()
            && let Some(names) = ours(client, &mut printed)
        {
            note(Note::Confirmed(names));
        }
        match conn.state_mut() {
            ConnectionState::Disconnected(e) => {
                return match e {
                    ap::Error::ConnectionRefused(_) => End::Refused(e.to_string()),
                    _ => End::Lost(e.to_string(), connected),
                };
            }
            ConnectionState::Connected(client) if owed || Instant::now() >= next_look => {
                next_look = Instant::now() + LOOK;
                owed = tick(client, book, game, &mut ledger, &mut notices, note);
                if book.death_link {
                    // The room's own bounce comes back to us too.
                    let me = client.this_player().alias().to_string();
                    (heard.drain(..).filter(|d| d.source != me)).for_each(|d| deaths.hold(d));
                    death_link(client, game, &mut deaths, &me, note);
                }
            }
            _ => owed = false,
        }
        if !owed {
            thread::sleep(POLL);
        }
    }
}

/// One look, what it sends, and the next give. True when it gave one, so
/// another may be owed.
fn tick(
    client: &mut ap::Client,
    book: &Book,
    game: &mut impl Game,
    ledger: &mut Ledger,
    notices: &mut Notices,
    note: &mut impl FnMut(Note),
) -> bool {
    let look = match ledger.look(game) {
        Ok(look) => look,
        Err(e) => {
            note(Note::Trouble(format!("{e:#}")));
            return false;
        }
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
    let received: Vec<i64> = found(client).map(|r| r.item().id()).collect();
    let sender = |index| {
        let r = found(client).nth(index)?;
        note(Note::Received {
            item: r.item().name().to_string(),
            from: r.sender().name().to_string(),
        });
        Some(r.sender().slot())
    };
    match give_one(game, book, &received, notices, sender, Instant::now()) {
        Ok(gave) => gave.is_some(),
        Err(e) => {
            note(Note::Trouble(format!("{e:#}")));
            false
        }
    }
}

/// Send Link's death to the room, or kill him for one of the room's.
fn death_link(
    client: &mut ap::Client,
    game: &mut impl Game,
    deaths: &mut Deaths,
    me: &str,
    note: &mut impl FnMut(Note),
) {
    let sent = match deaths.look(game) {
        Ok(Some(Done::Died)) => {
            let options = ap::DeathLinkOptions::new().cause(format!("{me} died."));
            client.death_link(options).map(|()| Note::Died)
        }
        Ok(Some(Done::Killed(d))) => Ok(Note::Killed {
            from: d.source,
            cause: d.cause,
        }),
        Ok(None) => return,
        Err(e) => return note(Note::Trouble(format!("{e:#}"))),
    };
    note(sent.unwrap_or_else(|e| Note::Trouble(e.to_string())));
}

/// The locations of ours among `printed`'s item sends, which it empties.
fn ours(client: &ap::Client, printed: &mut Vec<ap::LocatedItem>) -> Option<Vec<String>> {
    let me = client.this_player().slot();
    let names: Vec<String> = (printed.drain(..))
        .filter(|i| i.sender().slot() == me)
        .map(|i| i.location().name().to_string())
        .collect();
    (!names.is_empty()).then_some(names)
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
    /// Checks the seed hands over at the start, as Met Zelda's: their
    /// scenes never play, so no flag says so.
    given: BTreeSet<u64>,
    goal: u32,
    received: u32,
    /// [`DEATHS`] at the last look.
    deaths: Option<i64>,
}

impl Live {
    pub fn new(game: Target, gd: Arc<GameData>, spots: &[Spot]) -> Result<Self> {
        let tracker = Tracker::new(&gd, spots)?;
        Ok(Self {
            dev: Dev::new(game)?,
            gd,
            tracker,
            given: spots.iter().filter(|s| s.given).map(|s| s.hash).collect(),
            goal: live::hash(&steps::saved(flow::DEFEAT_GANON))?,
            received: live::hash(RECEIVED)?,
            deaths: None,
        })
    }

    fn get(&self, hash: u32) -> Result<String> {
        self.dev.game().get(&self.gd, hash)
    }
}

/// Link's deaths in the save (GameData, unnamed). It counts the moment his
/// hearts run out, and a fairy's save doesn't count.
const DEATHS: u32 = 0xd30c_8e29;

/// The step Ganon's defeat reaches once his last fight is over.
const GOAL_DONE: &str = "Complete";

impl Game for Live {
    fn ready(&mut self) -> Result<bool> {
        Ok(!self.dev.busy()?)
    }

    /// One read for every Bool, and the per-placement flags only when a
    /// spot goes by one.
    fn checked(&mut self) -> Result<BTreeSet<u64>> {
        let game = self.dev.game();
        let (mut bools, mut keyed) = (Vec::new(), false);
        for flag in self.tracker.flags() {
            match flag {
                Flag::Bool(h) => bools.push(h),
                Flag::Key(_) => keyed = true,
            }
        }
        let read = live::bools(game, game.global(), &bools)?;
        let on: HashSet<u32> = (bools.iter().zip(read))
            .filter_map(|(&h, on)| on.then_some(h))
            .collect();
        let keys = match keyed {
            true => live::keys(game, game.global())?,
            false => BTreeSet::new(),
        };
        self.tracker.load(|f| match f {
            Flag::Bool(h) => on.contains(&h),
            Flag::Key(k) => keys.contains(&k),
        });
        Ok(self.tracker.checked().union(&self.given).copied().collect())
    }

    fn goal(&mut self) -> Result<bool> {
        Ok(self.get(self.goal)? == GOAL_DONE)
    }

    fn given(&mut self) -> Result<usize> {
        let n: i64 = self.get(self.received)?.parse()?;
        Ok(usize::try_from(n)?)
    }

    fn died(&mut self) -> Result<bool> {
        let n: i64 = self.get(DEATHS)?.parse()?;
        let died = self.deaths.is_some_and(|last| n > last);
        self.deaths = Some(n);
        Ok(died)
    }

    fn kill(&mut self) -> Result<()> {
        self.dev.kill()
    }

    fn give(&mut self, actor: &str, given: usize) -> Result<String> {
        let got = self.dev.give_level(actor)?;
        live::set(
            self.dev.game(),
            self.dev.game().global(),
            self.received,
            &given.to_string(),
        )?;
        Ok(got)
    }

    fn notice(&mut self, shown: &Shown) -> Result<()> {
        self.dev.notice(shown)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    #[derive(Default)]
    pub(in crate::ap) struct Fake {
        pub busy: bool,
        pub checked: BTreeSet<u64>,
        pub goal: bool,
        pub given: Vec<String>,
        /// Link died since the last look.
        pub died: bool,
        pub kills: usize,
        pub notices: Vec<Shown>,
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
        fn give(&mut self, actor: &str, given: usize) -> Result<String> {
            self.given.push(actor.into());
            assert_eq!(self.given.len(), given);
            Ok(actor.into())
        }
        fn notice(&mut self, shown: &Shown) -> Result<()> {
            self.notices.push(shown.clone());
            Ok(())
        }
        fn died(&mut self) -> Result<bool> {
            Ok(std::mem::take(&mut self.died))
        }
        fn kill(&mut self) -> Result<()> {
            self.kills += 1;
            Ok(())
        }
    }

    fn book() -> Book {
        Book {
            locations: [(10, 100), (20, 200)].into(),
            actors: [(1, "Item_FakeBow".into()), (2, "Item_FakeHook".into())].into(),
            death_link: false,
        }
    }

    #[test]
    fn a_check_is_sent_once_per_connection() {
        let (mut game, mut ledger) = (Fake::default(), Ledger::default());
        game.checked = [10].into();
        assert_eq!(ledger.look(&mut game).unwrap().send, [10]);
        game.checked.insert(20);
        assert_eq!(ledger.look(&mut game).unwrap().send, [20]);
        assert!(ledger.look(&mut game).unwrap().send.is_empty());
        ledger.reset();
        assert_eq!(ledger.look(&mut game).unwrap().send, [10, 20]);
    }

    #[test]
    fn a_spot_ap_didnt_place_has_no_location() {
        assert_eq!(book().locations(&[10, 30, 20]), [100, 200]);
    }

    #[test]
    fn items_are_given_once_each_in_order_and_only_in_play() {
        let (mut game, book) = (Fake::default(), book());
        let received = [2, 1, 2];
        game.busy = true;
        assert_eq!(give_next(&mut game, &book, &received).unwrap(), None);
        game.busy = false;
        for _ in 0..5 {
            give_next(&mut game, &book, &received).unwrap();
        }
        assert_eq!(
            game.given,
            ["Item_FakeHook", "Item_FakeBow", "Item_FakeHook"]
        );
    }

    /// [`give_one`] until nothing is owed: each index given, and how many
    /// notices had shown after each give.
    fn give_all(game: &mut Fake, notices: &mut Notices, received: &[i64]) -> Vec<(usize, usize)> {
        let (book, now, mut out) = (book(), Instant::now(), vec![]);
        while let Some((i, _)) = give_one(game, &book, received, notices, |_| Some(0), now).unwrap()
        {
            out.push((i, game.notices.len()));
        }
        out
    }

    #[test]
    fn a_pass_gives_one_item_so_the_room_is_answered_between() {
        let (mut game, book, mut notices) = (Fake::default(), book(), Notices::default());
        let now = Instant::now();
        let gave = give_one(&mut game, &book, &[2, 1, 2], &mut notices, |_| Some(0), now);
        assert_eq!(gave.unwrap().map(|(i, _)| i), Some(0));
        assert_eq!(game.given.len(), 1);
        let given: Vec<_> = give_all(&mut game, &mut notices, &[2, 1, 2])
            .iter()
            .map(|g| g.0)
            .collect();
        assert_eq!(given, [1, 2]);
    }

    #[test]
    fn a_burst_shows_its_first_notice_before_the_rest_are_given() {
        let (mut game, mut notices) = (Fake::default(), Notices::default());
        let shown_at: Vec<_> = give_all(&mut game, &mut notices, &[2, 1, 2])
            .iter()
            .map(|g| g.1)
            .collect();
        assert_eq!(
            shown_at,
            [1, 1, 1],
            "one shown at the first give, paced after"
        );
    }

    #[test]
    fn a_reconnect_resending_the_list_gives_nothing_twice() {
        let (mut game, book) = (Fake::default(), book());
        give_next(&mut game, &book, &[1]).unwrap();
        let gave = give_next(&mut game, &book, &[1, 2]).unwrap();
        assert_eq!(gave, Some((1, "Item_FakeHook".into())));
        assert_eq!(game.given.len(), 2);
    }

    #[test]
    fn a_save_ahead_of_the_server_gives_nothing_and_says_so() {
        let (mut game, book) = (Fake::default(), book());
        game.given = vec!["Item_FakeBow".into(); 3];
        let err = give_next(&mut game, &book, &[1]).unwrap_err();
        assert!(err.to_string().contains("another room"), "{err}");
        assert_eq!(game.given.len(), 3);
    }

    #[test]
    fn an_unknown_item_stops_the_queue_on_it() {
        let (mut game, book) = (Fake::default(), book());
        assert!(give_next(&mut game, &book, &[9, 1]).is_err());
        assert!(game.given.is_empty());
    }

    #[test]
    fn the_goal_is_sent_once() {
        let (mut game, mut ledger) = (Fake::default(), Ledger::default());
        assert!(!ledger.look(&mut game).unwrap().goal);
        game.goal = true;
        assert!(ledger.look(&mut game).unwrap().goal);
        assert!(!ledger.look(&mut game).unwrap().goal);
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

    /// The core on `game` as `seed`'s slot, against the room at `server`,
    /// for `secs`.
    fn played(server: &str, seed: &ApSeed, mut game: Fake, secs: u64) -> (Fake, Vec<Note>) {
        let romfs = crate::game::dev_dump().expect("the dump");
        let book = Book::new(&romfs, seed).unwrap();
        let room = Room {
            server: server.into(),
            slot: seed.slot.clone(),
            password: None,
        };
        let (send, stop) = std::sync::mpsc::channel();
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_secs(secs));
            drop(send);
        });
        let mut notes = vec![];
        run(&room, &book, &mut game, &stop, &mut |n| notes.push(n));
        stopper.join().unwrap();
        (game, notes)
    }

    /// The room's server and the `.apznx` files in `ZOOTR_AP_SEED`
    /// (comma-separated).
    fn room() -> (String, Vec<ApSeed>) {
        let (Ok(server), Ok(paths)) = (
            std::env::var("ZOOTR_AP_SERVER"),
            std::env::var("ZOOTR_AP_SEED"),
        ) else {
            panic!("set ZOOTR_AP_SERVER and ZOOTR_AP_SEED");
        };
        let seeds = (paths.split(','))
            .map(|p| ApSeed::read(std::path::Path::new(p)).unwrap())
            .collect();
        (server, seeds)
    }

    /// The notes say it connected, sent, reached the goal, and never had
    /// trouble.
    fn went_well(notes: &[Note]) {
        let said = |f: fn(&Note) -> bool| notes.iter().any(f);
        assert!(said(|n| matches!(n, Note::Connected)), "{notes:?}");
        assert!(
            said(|n| matches!(n, Note::Sent(s) if !s.is_empty())),
            "{notes:?}"
        );
        assert!(said(|n| matches!(n, Note::Goal)), "{notes:?}");
        assert!(!said(|n| matches!(n, Note::Trouble(_))), "{notes:?}");
    }

    /// Against a real room: `ZOOTR_AP_SERVER` hosting the `.apznx` at
    /// `ZOOTR_AP_SEED`, whose other slot is ChecksFinder's `Finder`. The core
    /// sends checks and the goal, and gives what Finder's checks find for us.
    #[test]
    #[ignore = "needs a running Archipelago room"]
    fn a_room_takes_checks_and_the_goal_and_sends_items() {
        let (server, seeds) = room();
        let seed = &seeds[0];
        let finder_server = server.clone();
        let finder = thread::spawn(move || {
            let mut conn: ap::Connection = ap::Connection::new(
                finder_server,
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
        let game = Fake {
            checked: seed.placements.keys().take(3).copied().collect(),
            goal: true,
            ..Fake::default()
        };
        let (game, notes) = played(&server, seed, game, 10);
        finder.join().unwrap();
        went_well(&notes);
        assert!(
            notes
                .iter()
                .any(|n| matches!(n, Note::Sent(s) if s.len() == 3)),
            "{notes:?}"
        );
        assert!(!game.given.is_empty(), "nothing given: {notes:?}");
    }

    /// M4 without the games: two of our slots in one room
    /// (`ZOOTR_AP_SEED=a.apznx,b.apznx`), each on a fake game that has
    /// made every check. Each gets the items the other found for it, once
    /// each, and both reach the goal.
    #[test]
    #[ignore = "needs a running Archipelago room"]
    fn two_of_ours_trade_items_both_ways() {
        let (server, seeds) = room();
        assert_eq!(seeds.len(), 2, "two .apznx in ZOOTR_AP_SEED");
        let runs: Vec<_> = (seeds.iter().cloned())
            .map(|seed| {
                let server = server.clone();
                thread::spawn(move || {
                    let game = Fake {
                        checked: seed.placements.keys().copied().collect(),
                        goal: true,
                        ..Fake::default()
                    };
                    played(&server, &seed, game, 15)
                })
            })
            .collect();
        for (seed, run) in seeds.iter().zip(runs) {
            let (game, notes) = run.join().unwrap();
            went_well(&notes);
            let foreign = (seed.placements.values())
                .filter(|p| matches!(p, super::super::Placed::Foreign(_)))
                .count();
            assert!(foreign > 0, "{} places nothing for the other", seed.slot);
            assert!(
                !game.given.is_empty(),
                "{} got nothing: {notes:?}",
                seed.slot
            );
        }
    }
}
