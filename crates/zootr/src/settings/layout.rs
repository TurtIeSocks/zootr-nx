//! Where each setting shows: the app's panels and their sections.

/// The settings panels, by name.
pub mod group {
    /// How items are placed, and what stands open from the start.
    pub const LOGIC: &str = "Logic & Access";
    /// Which places become checks, and what the pool holds.
    pub const SHUFFLES: &str = "Shuffles";
    /// What a dungeon or its boss gives, and its keys.
    pub const DUNGEONS: &str = "Dungeons";
    /// Changes to how Link plays and what plays out: nothing shuffled,
    /// nothing opened.
    pub const ENHANCEMENTS: &str = "Enhancements";
    /// What a new save starts holding or having done.
    pub const INVENTORY: &str = "Inventory";
}

/// Every panel, in the order the app lists them.
pub const GROUPS: [&str; 5] = [
    group::LOGIC,
    group::SHUFFLES,
    group::DUNGEONS,
    group::ENHANCEMENTS,
    group::INVENTORY,
];

/// Where a setting shows: its panel, and the section of that panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub group: &'static str,
    pub section: &'static str,
    /// A line under the section's title.
    pub about: &'static str,
    /// The section's toggles show as one checklist, not a row each.
    pub list: bool,
}

/// Every section, by panel.
pub mod place {
    use super::{Place, group};

    const fn rows(group: &'static str, section: &'static str, about: &'static str) -> Place {
        Place {
            group,
            section,
            about,
            list: false,
        }
    }

    const fn list(group: &'static str, section: &'static str, about: &'static str) -> Place {
        Place {
            group,
            section,
            about,
            list: true,
        }
    }

    pub const RULES: Place = rows(
        group::LOGIC,
        "Rules",
        "How items are placed and how the game is won",
    );
    pub const WIN_CONDITION: Place = rows(
        group::LOGIC,
        "Win Condition",
        "What stands between Link and Ganondorf",
    );
    pub const OPEN_AREAS: Place = rows(
        group::LOGIC,
        "Open Areas",
        "What stands open from the start",
    );
    pub const QUEST: Place = list(
        group::LOGIC,
        "Quest Progress",
        "Story steps a new game starts with done",
    );
    pub const CHECKS: Place = rows(
        group::SHUFFLES,
        "Checks",
        "Which places and people give a check",
    );
    pub const BREAKABLES: Place = rows(
        group::SHUFFLES,
        "Breakables",
        "Things Link breaks or cuts that give a check once",
    );
    pub const ITEM_POOL: Place = rows(
        group::SHUFFLES,
        "Item Pool",
        "Items that join the pool or change how it gives them",
    );
    pub const HINTS: Place = rows(group::SHUFFLES, "Hints", "What gossip stones tell Link");
    pub const REWARDS: Place = rows(group::DUNGEONS, "Rewards", "What bosses and dungeons give");
    pub const KEYS: Place = rows(
        group::DUNGEONS,
        "Keys",
        "Where small keys, boss keys and the fortress keys can land",
    );
    pub const ENTRANCES: Place = rows(
        group::DUNGEONS,
        "Entrances",
        "Where each dungeon's door leads",
    );
    pub const COMFORT: Place = rows(
        group::ENHANCEMENTS,
        "Comfort",
        "Fewer stops, pop-ups and waits",
    );
    pub const GAMEPLAY: Place = rows(
        group::ENHANCEMENTS,
        "Gameplay",
        "Changes to what Link can do",
    );
    pub const CUTSCENES: Place = rows(
        group::ENHANCEMENTS,
        "Cutscenes",
        "Skip the cutscenes that only play, all at once or by name",
    );
    pub const CUTSCENE_AREAS: Place = list(
        group::ENHANCEMENTS,
        "Cutscenes by Area",
        "Every cutscene in an area that only plays, changing nothing",
    );
    /// The grid, which draws on its own rather than as a section.
    pub const START: Place = rows(
        group::INVENTORY,
        "Starting Inventory",
        "What a new game starts with",
    );

    /// Every section, in the order the app shows them.
    pub const ALL: [Place; 16] = [
        RULES,
        WIN_CONDITION,
        OPEN_AREAS,
        QUEST,
        CHECKS,
        BREAKABLES,
        ITEM_POOL,
        HINTS,
        REWARDS,
        KEYS,
        ENTRANCES,
        COMFORT,
        GAMEPLAY,
        CUTSCENES,
        CUTSCENE_AREAS,
        START,
    ];
}
