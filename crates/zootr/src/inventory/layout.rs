//! How the tracker lays the grid out: its sections, where each sits and
//! how wide it draws.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use super::{GRID, KEPT, ROWS};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Section {
    Inventory,
    Trade,
    Masks,
    Equipment,
    Upgrades,
    Quest,
    Songs,
    Stones,
    Medallions,
    Dungeons,
}

impl Section {
    pub const ALL: [Section; 10] = [
        Section::Inventory,
        Section::Trade,
        Section::Masks,
        Section::Equipment,
        Section::Upgrades,
        Section::Quest,
        Section::Songs,
        Section::Stones,
        Section::Medallions,
        Section::Dungeons,
    ];

    /// Icons per row until the user changes it.
    fn width(self) -> u8 {
        match self {
            Section::Equipment | Section::Stones => 3,
            Section::Upgrades | Section::Masks | Section::Dungeons => 4,
            Section::Quest | Section::Trade => 5,
            Section::Inventory | Section::Songs | Section::Medallions => 6,
        }
    }

    /// The widths it can take. A row of Dungeons is one dungeon, so it
    /// stays as wide as a row.
    fn widths(self) -> RangeInclusive<u8> {
        match self {
            Section::Dungeons => KEPT.len() as u8..=KEPT.len() as u8,
            _ => 1..=self.slots() as u8,
        }
    }

    fn slots(self) -> usize {
        match self {
            Section::Dungeons => ROWS * KEPT.len(),
            _ => GRID.iter().filter(|s| s.section == self).count(),
        }
    }
}

/// How many columns the grid's sections can sit in.
pub const COLUMNS: u8 = 3;

/// Where a section sits in the grid and how wide it draws.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Placement {
    pub section: Section,
    /// 1 to `COLUMNS`.
    pub column: u8,
    /// Icons per row.
    pub width: u8,
    pub shown: bool,
}

impl Placement {
    fn new(section: Section) -> Self {
        Self {
            section,
            column: 1,
            width: section.width(),
            shown: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum IconSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// How the grid looks, in the tab and the pop out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct TrackerOptions {
    pub size: IconSize,
    /// A CSS color.
    pub background: String,
    /// Every section once. The order within a column is the draw order.
    pub sections: Vec<Placement>,
}

impl Default for TrackerOptions {
    fn default() -> Self {
        Self {
            size: IconSize::default(),
            background: "#18181b".into(),
            sections: Section::ALL.map(Placement::new).into(),
        }
    }
}

impl TrackerOptions {
    /// Every section exactly once and in range, so an older config or a
    /// section added later still draws.
    pub fn tidy(mut self) -> Self {
        let mut seen = vec![];
        self.sections.retain(|p| {
            let new = !seen.contains(&p.section);
            seen.push(p.section);
            new
        });
        let missing = Section::ALL.into_iter().filter(|s| !seen.contains(s));
        self.sections.extend(missing.map(Placement::new));
        for p in &mut self.sections {
            p.column = p.column.clamp(1, COLUMNS);
            let widths = p.section.widths();
            p.width = p.width.clamp(*widths.start(), *widths.end());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_keeps_order_and_fills_what_is_missing() {
        let songs = Placement {
            column: 9,
            width: 0,
            ..Placement::new(Section::Songs)
        };
        let options = TrackerOptions {
            sections: vec![songs, songs],
            ..TrackerOptions::default()
        }
        .tidy();
        assert_eq!(options.sections.len(), Section::ALL.len());
        assert_eq!(options.sections[0].section, Section::Songs);
        assert_eq!(
            (options.sections[0].column, options.sections[0].width),
            (3, 1)
        );
        assert_eq!(options.sections[1], Placement::new(Section::Inventory));
    }

    #[test]
    fn widths_never_pass_a_sections_slots() {
        let wide = Placement {
            width: 99,
            ..Placement::new(Section::Stones)
        };
        let options = TrackerOptions {
            sections: vec![wide],
            ..TrackerOptions::default()
        }
        .tidy();
        assert_eq!(options.sections[0].width, 3);
    }

    #[test]
    fn dungeons_stay_one_row_per_dungeon_wide() {
        let widths = [1, 9].map(|width| Placement {
            width,
            ..Placement::new(Section::Dungeons)
        });
        for p in widths {
            let options = TrackerOptions {
                sections: vec![p],
                ..TrackerOptions::default()
            }
            .tidy();
            assert_eq!(options.sections[0].width as usize, KEPT.len());
        }
    }

    #[test]
    fn default_widths_fit() {
        for s in Section::ALL {
            assert!(s.widths().contains(&s.width()), "{s:?}");
        }
    }
}
