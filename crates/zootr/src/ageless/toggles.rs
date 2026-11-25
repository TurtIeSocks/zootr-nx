//! The Both Ages toggles, one per item: the pause menu rows each opens to
//! the other age, and the shared part it needs. The schema, the seed, the
//! upgrade and Archipelago all read [`TOGGLES`]. See
//! `superpowers/specs/2026-11-24-both-ages-toggles-design.md`.

use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};
use ounce_formats::{BymlDoc, ModBuilder, Romfs};

use crate::ageless_items;
use crate::colors::Colors;
use crate::game::POUCH_TABLE;
use crate::known::{Name, actor, item};
use crate::standins::Tables;

/// What a toggle needs beyond its rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    /// Nothing: a one-handed sword or shield.
    Table,
    /// The other Link's clips, holders and sounds ([`crate::ageless_items`]).
    Clips,
    /// A child's boots, with the pouch row whose D-pad shortcut offers them.
    Boots(&'static Name),
    /// A child's tunic, with the adult armor his copy is made from.
    Tunic(&'static Name),
}

/// One item either Link can use.
pub struct Toggle {
    pub id: &'static str,
    pub label: &'static str,
    pub about: &'static str,
    /// Its rows in the pause menu's table, on whichever page they show.
    pub rows: &'static [&'static Name],
    pub part: Part,
}

/// Every Both Ages toggle, in the order the app shows them. The Hylian
/// Shield has none: the game lets both ages carry it.
pub const TOGGLES: [Toggle; 17] = [
    Toggle {
        id: "both-ages-kokiri-sword",
        label: "Kokiri Sword",
        about: "Adult Link can wield the Kokiri Sword",
        rows: &[actor::WEAPON_SWORD_KOKIRI],
        part: Part::Table,
    },
    Toggle {
        id: "both-ages-master-sword",
        label: "Master Sword",
        about: "Child Link can wield the Master Sword",
        rows: &[actor::WEAPON_SWORD_MASTER],
        part: Part::Table,
    },
    Toggle {
        id: "both-ages-giants-knife",
        label: "Giant's Knife",
        about: "Child Link can wield the Giant's Knife, whole or broken",
        rows: &[actor::WEAPON_SWORD_GIANT, actor::WEAPON_SWORD_BROKEN_GIANT],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-biggoron-sword",
        label: "Biggoron's Sword",
        about: "Child Link can wield Biggoron's Sword",
        rows: &[actor::WEAPON_SWORD_BIGGORON],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-deku-shield",
        label: "Deku Shield",
        about: "Adult Link can carry the Deku Shield",
        rows: &[actor::WEAPON_SHIELD_DEKU],
        part: Part::Table,
    },
    Toggle {
        id: "both-ages-mirror-shield",
        label: "Mirror Shield",
        about: "Child Link can carry the Mirror Shield. Only adult Link aims it at light",
        rows: &[actor::WEAPON_SHIELD_MIRROR],
        part: Part::Table,
    },
    Toggle {
        id: "both-ages-bow",
        label: "Bow",
        about: "Child Link can shoot the Bow, with its fire, ice and light arrows",
        // Each slot's arrow row and loaded bow row.
        rows: &[
            item::ARROW_NORMAL,
            actor::WEAPON_RANGE_BOW,
            item::ARROW_FIRE,
            actor::POUCH_RANGE_BOW_AND_ARROW_FIRE,
            item::ARROW_ICE,
            actor::POUCH_RANGE_BOW_AND_ARROW_ICE,
            item::ARROW_LIGHT,
            actor::POUCH_RANGE_BOW_AND_ARROW_LIGHT,
        ],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-slingshot",
        label: "Slingshot",
        about: "Adult Link can shoot the Slingshot",
        rows: &[item::DEKU_SEED, actor::WEAPON_RANGE_SLING_SHOT],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-hammer",
        label: "Megaton Hammer",
        about: "Child Link can swing the Megaton Hammer",
        rows: &[item::HAMMER],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-hookshot",
        label: "Hookshot",
        about: "Child Link can fire the Hookshot",
        rows: &[item::HOOKSHOT],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-longshot",
        label: "Longshot",
        about: "Child Link can fire the Longshot",
        rows: &[item::LONGSHOT],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-boomerang",
        label: "Boomerang",
        about: "Adult Link can throw the Boomerang",
        rows: &[item::BOOMERANG],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-deku-sticks",
        label: "Deku Sticks",
        about: "Adult Link can swing Deku Sticks",
        rows: &[item::DEKU_STICK],
        part: Part::Clips,
    },
    Toggle {
        id: "both-ages-iron-boots",
        label: "Iron Boots",
        about: "Child Link can wear the Iron Boots",
        rows: &[actor::ARMOR_ADULT_HEAVY_BOOT],
        part: Part::Boots(item::HEAVY_BOOT),
    },
    Toggle {
        id: "both-ages-hover-boots",
        label: "Hover Boots",
        about: "Child Link can wear the Hover Boots",
        rows: &[actor::ARMOR_ADULT_HOVER_BOOT],
        part: Part::Boots(item::HOVER_BOOT),
    },
    Toggle {
        id: "both-ages-zora-tunic",
        label: "Zora Tunic",
        about: "Child Link can wear the Zora Tunic, his own tunic in its blue",
        rows: &[actor::ARMOR_ADULT_ZORA],
        part: Part::Tunic(actor::ARMOR_ADULT_ZORA),
    },
    Toggle {
        id: "both-ages-goron-tunic",
        label: "Goron Tunic",
        about: "Child Link can wear the Goron Tunic, his own tunic in its red",
        rows: &[actor::ARMOR_ADULT_GORON],
        part: Part::Tunic(actor::ARMOR_ADULT_GORON),
    },
];

/// The toggles `on` holds.
pub struct Enabled(Vec<&'static Toggle>);

impl Enabled {
    pub fn of(on: &BTreeSet<String>) -> Self {
        Self(TOGGLES.iter().filter(|t| on.contains(t.id)).collect())
    }

    /// Whether any toggle on needs the other Link's clips.
    pub fn clips(&self) -> bool {
        self.0.iter().any(|t| t.part == Part::Clips)
    }

    /// The pouch rows of the boots on.
    pub fn boots(&self) -> Vec<&'static Name> {
        (self.0.iter())
            .filter_map(|t| match t.part {
                Part::Boots(row) => Some(row),
                _ => None,
            })
            .collect()
    }

    /// The adult armor of the tunics on.
    pub fn tunics(&self) -> Vec<&'static Name> {
        (self.0.iter())
            .filter_map(|t| match t.part {
                Part::Tunic(armor) => Some(armor),
                _ => None,
            })
            .collect()
    }

    /// Whether a child wears any boots or tunic: the hook's `CHILD_GEAR`.
    pub fn gear(&self) -> bool {
        !self.boots().is_empty() || !self.tunics().is_empty()
    }

    /// The pause menu's table at `table` with every row of the toggles on
    /// open to both ages, or `None` with none on. Each row must be there,
    /// and each toggle must open one that wasn't.
    pub fn opened(&self, table: &[u8]) -> Result<Option<Vec<u8>>> {
        if self.0.is_empty() {
            return Ok(None);
        }
        let mut doc = BymlDoc::parse(table)?;
        let mut found = BTreeSet::new();
        let mut changed = BTreeSet::new();
        for actor in super::rows(&mut doc.root)? {
            let Some(name) = super::name(actor).map(str::to_owned) else {
                continue;
            };
            let Some(toggle) = (self.0.iter()).find(|t| t.rows.iter().any(|r| r.get() == name))
            else {
                continue;
            };
            if super::open(actor) {
                changed.insert(toggle.id);
            }
            found.insert(name);
        }
        for toggle in &self.0 {
            if let Some(row) = toggle.rows.iter().find(|r| !found.contains(r.get())) {
                bail!("the pause menu lacks {row}");
            }
            ensure!(
                changed.contains(toggle.id),
                "{} already opens to both ages",
                toggle.label
            );
        }
        Ok(Some(doc.to_binary()))
    }

    /// Write the toggles on: the pause menu's rows, then each shared part
    /// once, whichever toggles need it.
    pub fn write(
        &self,
        romfs: &Romfs,
        builder: &mut ModBuilder,
        tables: &mut Tables,
        colors: &Colors,
    ) -> Result<()> {
        if let Some(table) = self.opened(&builder.read(POUCH_TABLE)?)? {
            builder.write(POUCH_TABLE, &table)?;
        }
        if self.gear() {
            super::gear::write(romfs, builder, tables, &self.tunics(), colors)?;
        }
        if !self.boots().is_empty() {
            // The boots' clips and effects are the adult's, which the items
            // lend too. Their sounds don't cover the boots.
            if !self.clips() {
                ageless_items::lend_child_clips(romfs, builder)?;
            }
            ageless_items::lend_boot_sounds(builder)?;
        }
        if self.clips() {
            ageless_items::write(romfs, builder)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row of the table at `bytes`, on every page: its name and ages.
    fn ages(bytes: &[u8]) -> Vec<(String, [bool; 2])> {
        let mut root = BymlDoc::parse(bytes).unwrap().root;
        let rows = super::super::rows(&mut root).unwrap();
        (rows.into_iter())
            .map(|a| {
                let name = super::super::name(a).unwrap_or_default().to_owned();
                (name, super::super::ages(a))
            })
            .collect()
    }

    fn on(ids: &[&str]) -> Enabled {
        Enabled::of(&ids.iter().map(|id| (*id).to_owned()).collect())
    }

    #[test]
    fn each_toggle_opens_its_rows_and_nothing_else() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let before = ages(&vanilla);
        for toggle in &TOGGLES {
            let table = on(&[toggle.id]).opened(&vanilla).unwrap().unwrap();
            let after = ages(&table);
            assert_eq!(before.len(), after.len());
            for ((name, was), (_, now)) in before.iter().zip(&after) {
                match toggle.rows.iter().any(|r| r.get() == name) {
                    true => assert_eq!(*now, [true, true], "{}: {name}", toggle.id),
                    false => assert_eq!(was, now, "{}: {name} isn't its", toggle.id),
                }
            }
        }
    }

    #[test]
    fn every_toggle_together_opens_as_each_alone() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let vanilla = romfs.read(POUCH_TABLE).unwrap();
        let all = on(&TOGGLES.map(|t| t.id))
            .opened(&vanilla)
            .unwrap()
            .unwrap();
        let mut each = vanilla.clone();
        for toggle in &TOGGLES {
            each = on(&[toggle.id]).opened(&each).unwrap().unwrap();
        }
        assert_eq!(ages(&all), ages(&each));
    }

    #[test]
    fn the_check_counts_rows_by_name_not_how_many_match() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        // Every Normal Arrow row renamed to the Bow's: as many rows match
        // the Bow's as before, but its arrow row is gone.
        let mut doc = BymlDoc::parse(&romfs.read(POUCH_TABLE).unwrap()).unwrap();
        let mut renamed = 0;
        for actor in super::super::rows(&mut doc.root).unwrap() {
            if super::super::name(actor) == Some(item::ARROW_NORMAL.get()) {
                let bow = ounce_formats::Byml::String(actor::WEAPON_RANGE_BOW.get().into());
                actor.insert(crate::known::field::ACTOR_SHORT.get().into(), bow);
                renamed += 1;
            }
        }
        assert!(renamed > 0);
        let error = on(&["both-ages-bow"]).opened(&doc.to_binary()).unwrap_err();
        assert!(
            error.to_string().contains(item::ARROW_NORMAL.get()),
            "{error}"
        );
    }

    #[test]
    fn no_toggle_leaves_the_table_alone() {
        assert_eq!(on(&[]).opened(b"not a table").unwrap(), None);
    }

    #[test]
    fn the_parts_follow_the_toggles_on() {
        let none = on(&[]);
        assert!(!none.clips() && !none.gear());
        let zora = on(&["both-ages-zora-tunic"]);
        assert!(zora.gear() && !zora.clips() && zora.boots().is_empty());
        assert_eq!(zora.tunics(), [actor::ARMOR_ADULT_ZORA]);
        let iron = on(&["both-ages-iron-boots", "both-ages-master-sword"]);
        assert!(iron.gear() && !iron.clips());
        assert_eq!(iron.boots(), [item::HEAVY_BOOT]);
        assert!(on(&["both-ages-hammer"]).clips());
    }

    #[test]
    fn ids_and_labels_are_unique() {
        let ids: BTreeSet<_> = TOGGLES.iter().map(|t| t.id).collect();
        let labels: BTreeSet<_> = TOGGLES.iter().map(|t| t.label).collect();
        assert_eq!((ids.len(), labels.len()), (TOGGLES.len(), TOGGLES.len()));
    }
}
