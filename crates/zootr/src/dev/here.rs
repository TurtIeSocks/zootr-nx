//! `zootr dev here`: where Link stands, as a room file names a spot.

use std::fmt;

use anyhow::{Result, ensure};
use serde::Serialize;

use super::Dev;

/// `DungeonRoom`: the dungeon room Link stands in.
const DUNGEON_ROOM: u32 = 0x9b529f63;

/// Link's spot, map and, in a dungeon, its id and his room.
#[derive(Serialize)]
pub struct Here {
    /// `x,y,z`, to 0.1 m.
    pub at: String,
    pub map: String,
    pub dungeon: Option<&'static str>,
    pub room: Option<i32>,
}

impl fmt::Display for Here {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} in {}", self.at, self.map)?;
        if let (Some(d), Some(r)) = (self.dungeon, self.room) {
            write!(f, ", {d} room {r}")?;
        }
        Ok(())
    }
}

impl Dev {
    pub fn here(&self) -> Result<Here> {
        let [x, y, z] = self.link()?;
        let map = self.state()?.map;
        let dungeon = (crate::entrances::DUNGEONS.iter()).find(|d| d.node.get() == map);
        let room = match dungeon {
            Some(_) => {
                let bytes = self.value_of(DUNGEON_ROOM)?;
                ensure!(bytes.len() >= 4, "DungeonRoom is {} bytes", bytes.len());
                Some(i32::from_le_bytes(bytes[..4].try_into()?))
            }
            None => None,
        };
        Ok(Here {
            at: format!("{x:.1},{y:.1},{z:.1}"),
            map,
            dungeon: dungeon.map(|d| d.id),
            room,
        })
    }
}
