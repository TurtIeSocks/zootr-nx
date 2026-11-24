//! What the game lets touch Link, read from its collision filter: the
//! Phive config in the Bootup pack. A shape touches Link on foot when
//! its layer mask holds his layer and its sub-layer mask his sub-layer
//! (room reach spec, rules 1 to 4). Each mask is its rigid body's
//! `Enable` mask (none means all) ANDed with its material's, and a mask
//! is named in the config's mask lists, bit n being entry n of its layer
//! list. A shape tagged unridable pushes Link off: a wall, never floor.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use ounce_formats::{ActorPack, Byml, BymlMap, Romfs};

use super::shape::{array, maps, text};
use crate::known::{Name, field};

/// The pack holding the config, by the stem of its file (named for its
/// platform), and the config in it.
pub const BOOTUP: &str = "Pack/Bootup.";
const CONFIG: &str = "Phive/Config/PhiveConfig.byml";

/// What a shape does to Link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Touch {
    /// It stops him walking, jumping or falling.
    pub on_foot: bool,
    /// It stops him climbing.
    pub climbing: bool,
    /// He stands on it, where it faces up: no unridable tag.
    pub ridable: bool,
}

/// A material preset's masks (`None` leaves the other side's alone) and
/// its shape tags, by bit.
#[derive(Clone, Debug, Default)]
struct Preset {
    layers: Option<u64>,
    sub_layers: Option<u64>,
    tags: u64,
}

/// The config's layers, masks, presets and names.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    layers: Vec<String>,
    sub_layers: Vec<String>,
    layer_masks: HashMap<String, u64>,
    sub_layer_masks: HashMap<String, u64>,
    presets: HashMap<String, Preset>,
    tags: Vec<String>,
    pub materials: Vec<String>,
    pub sub_materials: Vec<String>,
}

/// A list of names, each entry n of the list.
fn names(config: &BymlMap, key: &Name) -> Vec<String> {
    array(config, key.get())
        .map(|v| match v {
            Byml::String(s) => s.to_string(),
            v => v
                .as_map()
                .ok()
                .and_then(|m| text(m, field::COMPONENT_NAME.get()))
                .unwrap_or_default()
                .to_owned(),
        })
        .collect()
}

/// A mask list: each entry's name and value.
fn masks(config: &BymlMap, key: &Name) -> HashMap<String, u64> {
    maps(config, key.get())
        .filter_map(|m| {
            let name = text(m, field::COMPONENT_NAME.get())?;
            let value = match m.get(field::MASK_VALUE.get())? {
                Byml::U64(v) => *v,
                Byml::I64(v) => *v as u64,
                Byml::U32(v) => u64::from(*v),
                Byml::I32(v) => *v as u64,
                _ => return None,
            };
            Some((name.to_owned(), value))
        })
        .collect()
}

/// The bit of `name` in `list`, or 0 when it isn't there.
fn bit(list: &[String], name: &str) -> u64 {
    list.iter().position(|n| n == name).map_or(0, |i| 1 << i)
}

/// A mask by its name: `None` for no name or an empty one, all ones for
/// a name the config doesn't list.
fn named(masks: &HashMap<String, u64>, name: Option<&str>) -> Option<u64> {
    name.filter(|n| !n.is_empty())
        .map(|n| masks.get(n).copied().unwrap_or(u64::MAX))
}

impl Filter {
    /// The filter read from the dump `romfs` holds.
    pub fn read(romfs: &Romfs) -> Result<Self> {
        let file = (crate::known::starting(romfs, BOOTUP)?.into_iter().next())
            .with_context(|| format!("no {BOOTUP}* pack"))?;
        let rel = file
            .strip_prefix(romfs.root())?
            .to_string_lossy()
            .into_owned();
        let pack = ActorPack::from_bytes("Bootup", romfs.read(&rel)?)?;
        let raw = pack.raw(CONFIG).with_context(|| format!("no {CONFIG}"))?;
        let doc = Byml::from_binary(raw)?;
        Ok(Self::of(doc.as_map()?))
    }

    /// The filter for the dump `romfs` holds, read once a process.
    pub fn get(romfs: &Romfs) -> Result<&'static Self> {
        static FILTER: OnceLock<Filter> = OnceLock::new();
        if let Some(f) = FILTER.get() {
            return Ok(f);
        }
        let read = Self::read(romfs)?;
        Ok(FILTER.get_or_init(|| read))
    }

    /// The filter a parsed config holds.
    pub fn of(config: &BymlMap) -> Self {
        let tags = names(config, field::USER_SHAPE_TAG_COLLECTION);
        let layer_masks = masks(config, field::LAYER_HIT_MASK_ENTITY_COLLECTION);
        let sub_layer_masks = masks(config, field::SUB_LAYER_HIT_MASK_ENTITY_COLLECTION);
        let presets = maps(config, field::MATERIAL_PRESET_COLLECTION.get())
            .filter_map(|m| {
                let name = text(m, field::COMPONENT_NAME.get())?;
                let tag_names =
                    array(m, field::USER_SHAPE_TAG_MASK.get()).filter_map(|v| v.as_string().ok());
                let preset = Preset {
                    layers: named(&layer_masks, text(m, field::LAYER_HIT_MASK_ENTITY.get())),
                    sub_layers: named(
                        &sub_layer_masks,
                        text(m, field::SUB_LAYER_HIT_MASK_ENTITY.get()),
                    ),
                    tags: tag_names.map(|t| bit(&tags, t)).fold(0, |a, b| a | b),
                };
                Some((name.to_owned(), preset))
            })
            .collect();
        Self {
            layers: names(config, field::LAYER_ENTITY_COLLECTION),
            sub_layers: names(config, field::SUB_LAYER_ENTITY_COLLECTION),
            layer_masks,
            sub_layer_masks,
            presets,
            tags,
            materials: names(config, field::MATERIAL_COLLECTION),
            sub_materials: names(config, field::SUB_MATERIAL_COLLECTION),
        }
    }

    /// A layer mask by its name: all ones for none.
    pub fn layer_mask(&self, name: Option<&str>) -> u64 {
        named(&self.layer_masks, name).unwrap_or(u64::MAX)
    }

    /// A sub-layer mask by its name: all ones for none.
    pub fn sub_layer_mask(&self, name: Option<&str>) -> u64 {
        named(&self.sub_layer_masks, name).unwrap_or(u64::MAX)
    }

    /// What a shape with these masks and tags does to Link: on foot he is
    /// layer `Player`, sub-layer `Unspecified`; climbing, `ClimbingPlayer`.
    pub fn touch(&self, layers: u64, sub_layers: u64, tags: u64) -> Touch {
        let player = layers & bit(&self.layers, field::PLAYER.get()) != 0;
        let has = |sub: &Name| sub_layers & bit(&self.sub_layers, sub.get()) != 0;
        Touch {
            on_foot: player && has(field::UNSPECIFIED),
            climbing: player && has(field::CLIMBING_PLAYER),
            ridable: tags & bit(&self.tags, field::UNRIDABLE.get()) == 0,
        }
    }

    /// What a primitive with these material presets does to Link, under a
    /// rigid body with these `Enable` masks.
    pub fn presets<'p>(&self, body: [u64; 2], presets: impl IntoIterator<Item = &'p str>) -> Touch {
        let [mut layers, mut sub_layers] = body;
        let mut tags = 0;
        for p in presets.into_iter().filter_map(|p| self.presets.get(p)) {
            layers &= p.layers.unwrap_or(u64::MAX);
            sub_layers &= p.sub_layers.unwrap_or(u64::MAX);
            tags |= p.tags;
        }
        self.touch(layers, sub_layers, tags)
    }

    /// The names of the tags set in `tags`.
    pub fn tag_names(&self, tags: u64) -> Vec<&str> {
        (self.tags.iter().enumerate())
            .filter(|(i, _)| tags >> i & 1 != 0)
            .map(|(_, t)| t.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's own filter: an airwall's preset touches Link on foot,
    /// the climbing airwall's only while he climbs, and the unridable
    /// preset's shape is a wall he can't stand on.
    #[test]
    fn the_game_filter_sorts_what_touches_link() {
        let Some(romfs) = crate::game::dev_dump() else {
            return;
        };
        let f = Filter::get(&romfs).unwrap();
        let all = [u64::MAX; 2];
        assert!(!f.presets.is_empty() && !f.layers.is_empty());
        let unridable = field::UNRIDABLE.get();
        let wall = f.presets(all, [unridable]);
        assert!(wall.on_foot && !wall.ridable, "{wall:?}");
        let climb = field::CLIMBING_PLAYER.get();
        // The climbing airwall's preset is named for it: the one preset
        // whose sub-layer mask holds only the climbing bit.
        let only_climb = (f.presets.iter())
            .find(|(_, p)| p.sub_layers == Some(bit(&f.sub_layers, climb)))
            .map(|(n, _)| n.as_str())
            .unwrap();
        let t = f.presets(all, [only_climb]);
        assert!(t.climbing && !t.on_foot, "{only_climb}: {t:?}");
        // A preset that names no masks leaves the body's alone.
        let plain = (f.presets.iter())
            .find(|(_, p)| p.layers.is_none() && p.sub_layers.is_none() && p.tags == 0)
            .map(|(n, _)| n.as_str())
            .unwrap();
        assert!(f.presets(all, [plain]).on_foot);
    }

    fn fake() -> Filter {
        let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        Filter {
            layers: s(&["FakeGround", "FakeCamera", "FakeHand", "Player"]),
            sub_layers: s(&["Unspecified", "FakeAnimal", "ClimbingPlayer"]),
            tags: s(&["FakeSlip", "Unridable"]),
            ..Filter::default()
        }
    }

    #[test]
    fn a_shape_touches_link_by_both_masks() {
        let Some(_) = crate::game::dev_dump() else {
            return;
        };
        let f = fake();
        let t = f.touch(0b1000, 0b001, 0);
        assert_eq!((t.on_foot, t.climbing, t.ridable), (true, false, true));
        assert!(!f.touch(0b0111, u64::MAX, 0).on_foot, "no player bit");
        let t = f.touch(u64::MAX, 0b100, 0b10);
        assert_eq!((t.on_foot, t.climbing, t.ridable), (false, true, false));
    }
}
