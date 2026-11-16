//! Settings saved by older versions, read as the current ones.

use super::{SHUFFLE_ITEMS, Settings};

/// The toggle that left every item in place, before it became
/// [`SHUFFLE_ITEMS`] with the opposite sense.
const VANILLA_CHESTS: &str = "vanilla-chests";

/// Brings saved `settings` (the config, a history entry, a share code) up
/// to date, so an old seed builds as it did.
pub(crate) fn upgrade(settings: &mut Settings) {
    if let Some(vanilla) = settings.remove(VANILLA_CHESTS) {
        let shuffled = vanilla != true;
        settings
            .entry(SHUFFLE_ITEMS.into())
            .or_insert(shuffled.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_chests_become_shuffle_items_the_other_way_round() {
        let up = |vanilla: bool| {
            let mut s = Settings::from([(VANILLA_CHESTS.into(), vanilla.into())]);
            upgrade(&mut s);
            s
        };
        assert_eq!(
            up(true),
            Settings::from([(SHUFFLE_ITEMS.into(), false.into())])
        );
        assert_eq!(
            up(false),
            Settings::from([(SHUFFLE_ITEMS.into(), true.into())])
        );
        let mut current = Settings::from([(SHUFFLE_ITEMS.into(), true.into())]);
        upgrade(&mut current);
        assert_eq!(
            current,
            Settings::from([(SHUFFLE_ITEMS.into(), true.into())])
        );
    }
}
