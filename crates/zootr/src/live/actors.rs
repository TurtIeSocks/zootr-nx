//! The live actors, walked from the game's actor table (offsets as of
//! 1.1.1, into `main`), the Life an enemy has left, and hiding an actor's
//! model.

use anyhow::{Context, Result};
use serde::Serialize;

use super::Memory;

/// Where the table's header pointer and its slot count are.
const TABLE: u64 = 0x4e9_0da8;
const CAPACITY: u64 = 0x4e9_0da0;
/// Where the header's slots start, and how far apart they are. A slot
/// holds its actor's pointer first.
const SLOTS: u64 = 0x40;
const SLOT: usize = 0x60;
/// An actor's vtable, and a Life component's.
const ACTOR_VTABLE: u64 = 0x4b2_5e18;
const LIFE_VTABLE: u64 = 0x4b4_86a0;
/// Where an actor keeps its name pointer, its component block, its
/// position and its placement's record.
const NAME: usize = 0x260;
const BLOCK: usize = 0x270;
const POS: usize = 0x31c;
const PLACEMENT: usize = 0x3d8;
/// How much of an actor the walk reads.
const BODY: usize = PLACEMENT + 8;
/// Where the placement record keeps its hash, and the block its model.
const HASH: u64 = 0x88;
const MODEL: u64 = 0x260;
/// A model's hide mask: any bit set hides it. Bit 7 is no AI node's, so
/// it's ours. The dirty bit tells the game the mask crossed zero.
const MASK: u64 = 0xe4;
const OURS: u8 = 0x80;
const DIRTY: u64 = 0x120;
/// How far into the block a Life's pointer may be.
const BLOCK_LEN: usize = 0x8000;
/// Where a Life keeps its actor, what's left and its most.
const OWNER: u64 = 0x8;
const LEFT: u64 = 0x18;
const MOST: u64 = 0x28;

#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    pub at: u64,
    pub name: String,
    pub pos: [f32; 3],
    /// Its placement's hash. None for an actor no placement made (an
    /// effect, Navi).
    pub hash: Option<u64>,
    /// Its model component, if it has one.
    pub model: Option<u64>,
    pub hidden: Hidden,
}

/// Whether an actor's model is hidden, and by whom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Hidden {
    #[default]
    Shown,
    /// By [`hide`].
    Ours,
    /// By the game itself (an actor waiting on a scene).
    Game,
}

impl Hidden {
    fn of(mask: u8) -> Self {
        match mask {
            0 => Self::Shown,
            m if m & OURS != 0 => Self::Ours,
            _ => Self::Game,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Life {
    /// The component. What's left is the i32 at `at + 0x18`: write 0 there
    /// and the enemy falls as if beaten.
    pub at: u64,
    pub left: i32,
    pub most: i32,
}

impl Life {
    /// Where what's left is kept.
    pub fn left_at(&self) -> u64 {
        self.at + LEFT
    }
}

/// Every live actor, `main` being where `main` loads.
pub fn actors(mem: &impl Memory, main: u64) -> Result<Vec<Actor>> {
    let header = mem.u64(main + TABLE)?;
    let capacity = mem.u64(main + CAPACITY)? as usize;
    let slots = mem.read(header + SLOTS, capacity * SLOT)?;
    let ptrs: Vec<u64> = (slots.chunks(SLOT).map(|s| quad(s, 0)))
        .filter(|&p| p != 0)
        .collect();
    let bodies = each(mem, ptrs.iter().map(|&p| (p, BODY)).collect());
    let bodies: Vec<_> = (ptrs.into_iter().zip(bodies))
        .filter_map(|(at, b)| Some((at, b?)))
        .filter(|(_, b)| quad(b, 0) == main + ACTOR_VTABLE)
        .collect();
    // Each actor's name, placement hash and model pointer, in one round.
    let behind = |p: u64, o: u64, len| (p != 0).then_some((p + o, len));
    let reads = (bodies.iter()).flat_map(|(_, b)| {
        [
            Some((quad(b, NAME), 64)),
            behind(quad(b, PLACEMENT), HASH, 8),
            behind(quad(b, BLOCK), MODEL, 8),
        ]
    });
    let got = each_some(mem, reads.collect());
    let mut got = got.chunks(3);
    let bodies: Vec<_> = (bodies.into_iter())
        .filter_map(|(at, b)| {
            let [name, hash, model] = got.next()? else {
                return None;
            };
            let name = name.as_ref()?;
            let name = name.split(|&c| c == 0).next().unwrap_or_default();
            let ptr = |r: &Option<Vec<u8>>| r.as_ref().map(|v| quad(v, 0)).filter(|&p| p != 0);
            Some((
                at,
                b,
                String::from_utf8_lossy(name).into_owned(),
                // All ones marks a record no placement made.
                ptr(hash).filter(|&h| h != u64::MAX),
                ptr(model),
            ))
        })
        .collect();
    let masks = each_some(
        mem,
        (bodies.iter())
            .map(|(.., model)| model.map(|m| (m + MASK, 1)))
            .collect(),
    );
    let actors = bodies
        .into_iter()
        .zip(masks)
        .map(|((at, b, name, hash, model), mask)| {
            let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
            Actor {
                at,
                name,
                pos: [f(POS), f(POS + 4), f(POS + 8)],
                hash,
                model,
                hidden: mask.map_or(Hidden::Shown, |m| Hidden::of(m[0])),
            }
        });
    Ok(actors.collect())
}

/// Hide `actor`'s model, or show it again: our bit in its hide mask, so a
/// hide of the game's own stays as it is.
pub fn hide(mem: &impl Memory, actor: &Actor, on: bool) -> Result<()> {
    let model = (actor.model).with_context(|| format!("{} has no model to hide", actor.name))?;
    let mask = mem.read(model + MASK, 1)?[0];
    let now = if on { mask | OURS } else { mask & !OURS };
    if now == mask {
        return Ok(());
    }
    mem.write(model + MASK, &[now])?;
    if (mask == 0) != (now == 0) {
        let dirty = mem.read(model + DIRTY, 1)?[0];
        mem.write(model + DIRTY, &[dirty | 1])?;
    }
    Ok(())
}

impl Actor {
    /// The actor's Life, if it has one: the pointer in its component block
    /// that follows the actor's own and leads to a Life of the actor's.
    pub fn life(&self, mem: &impl Memory, main: u64) -> Result<Option<Life>> {
        let block = mem.u64(self.at + BLOCK as u64)?;
        let Ok(raw) = mem.read(block, BLOCK_LEN) else {
            return Ok(None);
        };
        let words: Vec<u64> = raw
            .as_chunks::<8>()
            .0
            .iter()
            .map(|w| u64::from_le_bytes(*w))
            .collect();
        let picks: Vec<u64> = (words.windows(2))
            .filter(|w| w[0] == self.at && w[1] != 0)
            .map(|w| w[1])
            .collect();
        let heads = each(mem, picks.iter().map(|&p| (p, MOST as usize + 4)).collect());
        let life = picks.into_iter().zip(heads).find_map(|(at, head)| {
            let head = head?;
            let ours =
                quad(&head, 0) == main + LIFE_VTABLE && quad(&head, OWNER as usize) == self.at;
            let i =
                |o: u64| i32::from_le_bytes(head[o as usize..o as usize + 4].try_into().unwrap());
            ours.then(|| Life {
                at,
                left: i(LEFT),
                most: i(MOST),
            })
        });
        Ok(life)
    }
}

/// Each range's bytes, or `None` for one that can't be read: in one
/// request when all can.
fn each(mem: &impl Memory, ranges: Vec<(u64, usize)>) -> Vec<Option<Vec<u8>>> {
    match mem.read_many(&ranges) {
        Ok(all) => all.into_iter().map(Some).collect(),
        Err(_) => (ranges.iter())
            .map(|&(at, len)| mem.read(at, len).ok())
            .collect(),
    }
}

/// [`each`] over the ranges there are, `None` where there's none.
fn each_some(mem: &impl Memory, ranges: Vec<Option<(u64, usize)>>) -> Vec<Option<Vec<u8>>> {
    let mut got = each(mem, ranges.iter().flatten().copied().collect()).into_iter();
    (ranges.iter())
        .map(|r| r.and_then(|_| got.next().flatten()))
        .collect()
}

fn quad(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::tests::Fake;

    fn put(b: &mut [u8], at: u64, v: &[u8]) {
        b[at as usize..at as usize + v.len()].copy_from_slice(v);
    }

    /// The game it walked, the enemy it found, and the enemy's model.
    fn walked() -> (Fake, Actor, u64) {
        let main = 0x1000_0000;
        let (header, heap) = (0x2000_0000_u64, 0x3000_0000_u64);
        let (thief, rock, gone) = (heap, heap + 0x1000, heap + 0x2000);
        let (block, life, decoy) = (heap + 0x3000, heap + 0xb000, heap + 0xb100);
        let mut statics = vec![0; 0x10];
        put(&mut statics, 0, &3_u64.to_le_bytes());
        put(&mut statics, 8, &header.to_le_bytes());
        let mut table = vec![0; SLOTS as usize + 3 * SLOT];
        for (k, a) in [thief, gone, rock].into_iter().enumerate() {
            put(&mut table, SLOTS + (k * SLOT) as u64, &a.to_le_bytes());
        }
        let mut h = vec![0; 0xc000];
        let names = [(thief, "Enemy_Fake"), (rock, "Obj_FakeRock")];
        for (k, (a, name)) in names.into_iter().enumerate() {
            let at = a - heap;
            put(&mut h, at, &(main + ACTOR_VTABLE).to_le_bytes());
            let name_at = heap + 0x800 + k as u64 * 0x40;
            put(&mut h, at + NAME as u64, &name_at.to_le_bytes());
            put(&mut h, name_at - heap, name.as_bytes());
            put(&mut h, at + POS as u64, &1.5_f32.to_le_bytes());
            let own = if a == thief { block } else { 0 };
            put(&mut h, at + BLOCK as u64, &own.to_le_bytes());
        }
        // A freed slot keeps its pointer but not its vtable.
        put(&mut h, gone - heap, &7_u64.to_le_bytes());
        // The block names a decoy (another actor's Life) before the real one.
        for (o, p) in [(0x10, decoy), (0x40, life)] {
            put(&mut h, block - heap + o, &thief.to_le_bytes());
            put(&mut h, block - heap + o + 8, &p.to_le_bytes());
        }
        for (p, owner) in [(life, thief), (decoy, rock)] {
            put(&mut h, p - heap, &(main + LIFE_VTABLE).to_le_bytes());
            put(&mut h, p - heap + OWNER, &owner.to_le_bytes());
            put(&mut h, p - heap + LEFT, &18_i32.to_le_bytes());
            put(&mut h, p - heap + MOST, &18_i32.to_le_bytes());
        }
        // The enemy came from a placement and has a model. The rock has
        // neither: its record holds no placement's hash, as a system
        // actor's or a held item's does.
        let (record, unplaced, model) = (heap + 0xb200, heap + 0xb300, heap + 0xb400);
        for (a, r, hash) in [(thief, record, HASHED), (rock, unplaced, u64::MAX)] {
            put(&mut h, a - heap + PLACEMENT as u64, &r.to_le_bytes());
            put(&mut h, r - heap + HASH, &hash.to_le_bytes());
        }
        put(&mut h, block - heap + MODEL, &model.to_le_bytes());
        let mem = Fake::new(vec![(main + CAPACITY, statics), (header, table), (heap, h)]);
        let mut found = actors(&mem, main).unwrap();
        let named: Vec<_> = found.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(named, ["Enemy_Fake", "Obj_FakeRock"]);
        assert_eq!(found[0].pos[0], 1.5);
        let got = found[0].life(&mem, main).unwrap().unwrap();
        assert_eq!((got.at, got.left, got.most), (life, 18, 18));
        assert_eq!(got.left_at(), life + LEFT);
        assert_eq!(found[1].life(&mem, main).unwrap(), None);
        assert_eq!((found[0].hash, found[0].model), (Some(HASHED), Some(model)));
        assert_eq!((found[1].hash, found[1].model), (None, None));
        assert_eq!(found[0].hidden, Hidden::Shown);
        assert!(hide(&mem, &found[1], true).is_err());
        (mem, found.remove(0), model)
    }

    const HASHED: u64 = 16_850_179_431_221_442_802;

    fn byte(mem: &Fake, at: u64) -> u8 {
        mem.read(at, 1).unwrap()[0]
    }

    #[test]
    fn walks_the_table_and_finds_an_enemys_life() {
        walked();
    }

    #[test]
    fn a_hide_is_our_bit_and_marks_the_mask_dirty_as_it_crosses_zero() {
        let (mem, thief, model) = walked();
        let main = 0x1000_0000;
        let hidden = || actors(&mem, main).unwrap()[0].hidden;
        hide(&mem, &thief, true).unwrap();
        assert_eq!(
            (byte(&mem, model + MASK), byte(&mem, model + DIRTY)),
            (OURS, 1)
        );
        assert_eq!(hidden(), Hidden::Ours);
        // The game hides it too: showing it leaves the game's bit, and the
        // mask never crossed zero, so nothing is marked.
        mem.write(model + MASK, &[OURS | 4]).unwrap();
        mem.write(model + DIRTY, &[0]).unwrap();
        hide(&mem, &thief, false).unwrap();
        assert_eq!(
            (byte(&mem, model + MASK), byte(&mem, model + DIRTY)),
            (4, 0)
        );
        assert_eq!(hidden(), Hidden::Game);
        mem.write(model + MASK, &[0]).unwrap();
        hide(&mem, &thief, false).unwrap();
        assert_eq!(byte(&mem, model + DIRTY), 0);
    }
}
