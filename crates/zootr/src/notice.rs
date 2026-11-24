//! Received-item notices. When another world's item reaches Link, the
//! game's SimpleInfo banner names it and its sender ("Hookshot from
//! Saria"), and for a pouch resource the pickup popup shows its icon over
//! him. Neither stops play.
//!
//! The banner's words come from GameData: the app writes the item, the
//! sender's stand-in and a count, then asks the hook for the notice, which
//! the dev branch of a seed's map change flow shows. So the seed writes
//! three banner lines over the amiibo ones, and a name for each slot's
//! stand-in.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use ounce_formats::{EventKind, Flowchart, Romfs, Value};
use zootr_hook::{ARG_ACTOR, ARG_MIXED, ARG_SUMMARY, Notice};

use crate::flows::{self, Model, running, set};
use crate::foreign::clean;
use crate::key_tints::{MADE, POUCH_TEXT, label};
use crate::keys::SMALL;
use crate::known::{Name, field, flag, flow, scene};
use crate::text::{Copies, CopyLine, Piece, Says, Tagged};

/// The GameData the banner reads: the item's actor, the sender's
/// stand-in actor, and a summary's count.
pub const ITEM: &Name = flag::UI_SIMPLE_INFO_ACTOR_NAME;
pub const SENDER: &Name = flag::SHOP_SELECT_ITEM_NAME;
pub const COUNT: &Name = flag::SIMPLE_INT_FOR_MESSAGE;

/// The banner's message file.
const BANNERS: &str = "LayoutMsg/SimpleInfo_00";
/// Tags a GameData string reads as an actor's name (3), and an Int (1).
const NAMED: u16 = 3;
const INT: u16 = 1;
/// Where each tag is found in every language, to copy.
const ITEM_TAG: Tagged = Tagged {
    file: BANNERS,
    name: ITEM,
    kind: NAMED,
};
const SENDER_TAG: Tagged = Tagged {
    file: "LayoutMsg/ShopBuy_00",
    name: SENDER,
    kind: NAMED,
};
const COUNT_TAG: Tagged = Tagged {
    file: "LayoutMsg/Pointer_00",
    name: COUNT,
    kind: INT,
};

/// One banner: its SimpleInfo type, the label the game reads for it, and
/// its words.
pub(crate) struct Banner {
    pub(crate) kind: i32,
    label: &'static Name,
    says: &'static [Piece],
}

/// [`Notice::Item`], [`Notice::Summary`] and [`Notice::Mixed`]'s banners,
/// over the amiibo banners. The game's unused banner types would leave
/// those alone, but their names are only in `main`, which a dump's romfs
/// lacks: the maintainer's call (received notice spec). An amiibo used
/// in a seed shows a notice's last words instead of its own.
const LINES: [Banner; 3] = [
    Banner {
        kind: 28,
        label: field::CANNOT_USE_AMIIBO_HERE,
        says: &[
            Piece::Tag(ITEM_TAG),
            Piece::Words(" from "),
            Piece::Tag(SENDER_TAG),
        ],
    },
    Banner {
        kind: 26,
        label: field::CANNOT_USE_AMIIBO_TODAY,
        says: &[
            Piece::Words("+"),
            Piece::Tag(COUNT_TAG),
            Piece::Words(" more from "),
            Piece::Tag(SENDER_TAG),
        ],
    },
    Banner {
        kind: 27,
        label: field::CANNOT_USE_UNSUPPORTED_AMIIBO,
        says: &[
            Piece::Words("+"),
            Piece::Tag(COUNT_TAG),
            Piece::Words(" more"),
        ],
    },
];

/// The banner `notice` shows.
pub(crate) fn banner(notice: Notice) -> &'static Banner {
    &LINES[notice as usize]
}

/// The server's slot, which `!getitem` and a slot's start items come from.
pub const SERVER: u32 = 0;
const SERVER_NAME: &str = "Archipelago";

/// The stand-in actor naming slot `slot`. It has no row: only a name.
pub fn sender(slot: u32) -> String {
    format!("{MADE}From{slot}")
}

/// One notice: which banner, the item actor Link got (for an item), who
/// sent it (a slot, [`SERVER`] for the server) and a summary's count.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub kind: Notice,
    pub item: String,
    pub sender: u32,
    pub count: usize,
}

impl Shown {
    pub fn item(item: &str, sender: u32) -> Self {
        Self {
            kind: Notice::Item,
            item: item.to_owned(),
            sender,
            count: 1,
        }
    }

    /// `count` more, from `sender`, or from several when `None`.
    pub fn summary(sender: Option<u32>, count: usize) -> Self {
        Self {
            kind: sender.map_or(Notice::Mixed, |_| Notice::Summary),
            item: String::new(),
            sender: sender.unwrap_or(SERVER),
            count,
        }
    }

    /// The GameData the banner reads, by name, and each value.
    pub(crate) fn gamedata(&self) -> [(&'static str, String); 3] {
        [
            (ITEM.get(), self.item.clone()),
            (SENDER.get(), sender(self.sender)),
            (COUNT.get(), self.count.to_string()),
        ]
    }
}

/// The three banner lines, in every language that has the banner file.
pub(crate) fn banners() -> Copies<'static> {
    let new = LINES.iter().map(|b| CopyLine {
        label: b.label.get().to_owned(),
        sources: vec![b.label.get().to_owned()],
        says: Says::Tagged(b.says),
    });
    Copies {
        file: BANNERS,
        new: new.collect(),
    }
}

/// A name for each slot's stand-in, and the server's.
pub(crate) fn senders(players: &BTreeMap<u32, String>) -> Copies<'static> {
    let server = (!players.is_empty()).then_some((&SERVER, SERVER_NAME));
    let all = server
        .into_iter()
        .chain(players.iter().map(|(s, n)| (s, n.as_str())));
    let new = all.map(|(&slot, name)| CopyLine {
        label: label(&sender(slot)),
        sources: vec![label(SMALL)],
        says: Says::Text(clean(name)),
    });
    Copies {
        file: POUCH_TEXT,
        new: new.collect(),
    }
}

const POPUP: (&Name, &Name) = (
    field::UI_SYSTEM_ACTOR,
    flow::EVENT_TRIGGER_REQUEST_OPEN_PICK_UP_WINDOW,
);
const BANNER: (&Name, &Name) = (
    field::UI_SYSTEM_ACTOR,
    flow::EVENT_TRIGGER_REQUEST_OPEN_SIMPLE_INFO,
);

/// The game's own popup and banner calls, to copy: a get's popup, and the
/// ocarina's banner.
pub(crate) struct Shows {
    popup: Model,
    banner: Model,
}

impl Shows {
    pub(crate) fn load(romfs: &Romfs) -> Result<Self> {
        let first = |name: &Name, call: (&Name, &Name)| -> Result<Model> {
            let flow = flows::timeline(romfs, name)?;
            let at = *running(&flow, call)
                .first()
                .with_context(|| format!("{name} never runs {call:?}"))?;
            flows::model(&flow, call, at)
        };
        Ok(Self {
            popup: first(scene::SY_GET, POPUP)?,
            banner: first(scene::SY_OCARINA, BANNER)?,
        })
    }

    /// The notice's events, by the hook's arguments: a summary's banner
    /// alone, or the popup for `ZootrActor` then the item's banner. `when`
    /// switches on a bool argument. Returns the first.
    pub(crate) fn branch(
        &self,
        flow: &mut Flowchart,
        mut when: impl FnMut(&mut Flowchart, &[u8], u16, u16) -> Result<u16>,
    ) -> Result<u16> {
        let item = self.banner(flow, Notice::Item)?;
        let (actor, action) = flow.action(&self.popup.actor, self.popup.name);
        let mut params = self.popup.params.clone();
        set(
            &mut params,
            flag::ACTOR_NAME,
            Value::Argument(arg(ARG_ACTOR)),
        )?;
        let popup = flow.push(EventKind::Action {
            next: Some(item),
            actor,
            action,
            params,
        });
        let summary = self.banner(flow, Notice::Summary)?;
        let mixed = self.banner(flow, Notice::Mixed)?;
        let summary = when(flow, ARG_MIXED, mixed, summary)?;
        when(flow, ARG_SUMMARY, summary, popup)
    }

    /// `notice`'s banner, which ends the notice.
    fn banner(&self, flow: &mut Flowchart, notice: Notice) -> Result<u16> {
        let (actor, action) = flow.action(&self.banner.actor, self.banner.name);
        let mut params = self.banner.params.clone();
        set(
            &mut params,
            field::TYPE,
            Value::Int(vec![banner(notice).kind]),
        )?;
        Ok(flow.push(EventKind::Action {
            next: None,
            actor,
            action,
            params,
        }))
    }
}

/// A hook argument's name, as a flow reads it.
fn arg(name: &[u8]) -> String {
    crate::steps::arg(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ounce_formats::Part;

    #[test]
    fn each_notice_has_its_banner() {
        assert_eq!(banner(Notice::Item).kind, 28);
        assert_eq!(banner(Notice::Summary).kind, 26);
        assert_eq!(banner(Notice::Mixed).kind, 27);
    }

    #[test]
    fn senders_name_every_slot_and_the_server() {
        let players = BTreeMap::from([(1, "Link".to_string()), (2, "Saria".to_string())]);
        let copies = senders(&players);
        let said: Vec<(&str, &str)> = (copies.new.iter())
            .map(|c| match &c.says {
                Says::Text(t) => (c.label.as_str(), t.as_str()),
                _ => panic!("a name is plain text"),
            })
            .collect();
        let named = |slot| label(&sender(slot));
        let want = [(0, "Archipelago"), (1, "Link"), (2, "Saria")].map(|(s, n)| (named(s), n));
        let said: Vec<_> = said.into_iter().map(|(l, n)| (l.to_owned(), n)).collect();
        assert_eq!(said, want);
        assert!(senders(&BTreeMap::new()).new.is_empty());
    }

    #[test]
    fn every_language_gets_the_three_banners() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let path = format!("{BANNERS}.msbt");
        let banners = banners();
        let rels = crate::text::archives(&romfs).unwrap();
        assert!(rels.len() > 10, "every language: {rels:?}");
        for rel in rels {
            let data = romfs.read(&rel).unwrap();
            let data = crate::text::copied(&data, &rel, &banners).unwrap();
            let sarc = roead::sarc::Sarc::new(&data).unwrap();
            let msbt = ounce_formats::Msbt::parse(sarc.get_data(&path).unwrap()).unwrap();
            for b in &LINES {
                let line = msbt.get(b.label.get()).unwrap();
                let line = line.unwrap_or_else(|| panic!("{rel} has no {}", b.label));
                assert_eq!(line.len(), b.says.len(), "{rel} {}", b.label);
                for (part, piece) in line.iter().zip(b.says) {
                    match (part, piece) {
                        (Part::Text(t), Piece::Words(w)) => assert_eq!(t, w),
                        (Part::Tag(raw), Piece::Tag(tag)) => assert!(crate::text::reads(raw, tag)),
                        _ => panic!("{rel} {}: {part:?}", b.label),
                    }
                }
            }
        }
    }

    #[test]
    fn a_stand_in_is_never_a_foreign_copy() {
        assert!(!crate::foreign::is_foreign(&sender(3)));
    }
}
