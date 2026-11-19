//! The live actors, walked from the game's actor table (offsets as of
//! 1.1.1, into `main`), and the Life an enemy has left.

use anyhow::Result;

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
/// Where an actor keeps its name pointer, its component block and its
/// position.
const NAME: usize = 0x260;
const BLOCK: usize = 0x270;
const POS: usize = 0x31c;
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
    let bodies = each(mem, ptrs.iter().map(|&p| (p, POS + 12)).collect());
    let bodies: Vec<_> = (ptrs.into_iter().zip(bodies))
        .filter_map(|(at, b)| Some((at, b?)))
        .filter(|(_, b)| quad(b, 0) == main + ACTOR_VTABLE)
        .collect();
    let names = each(
        mem,
        bodies.iter().map(|(_, b)| (quad(b, NAME), 64)).collect(),
    );
    let actors = bodies.into_iter().zip(names).filter_map(|((at, b), name)| {
        let name = name?;
        let name = name.split(|&c| c == 0).next().unwrap_or_default();
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        Some(Actor {
            at,
            name: String::from_utf8_lossy(name).into_owned(),
            pos: [f(POS), f(POS + 4), f(POS + 8)],
        })
    });
    Ok(actors.collect())
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

    #[test]
    fn walks_the_table_and_finds_an_enemys_life() {
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
        let mem = Fake(vec![(main + CAPACITY, statics), (header, table), (heap, h)]);
        let found = actors(&mem, main).unwrap();
        let named: Vec<_> = found.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(named, ["Enemy_Fake", "Obj_FakeRock"]);
        assert_eq!(found[0].pos[0], 1.5);
        let got = found[0].life(&mem, main).unwrap().unwrap();
        assert_eq!((got.at, got.left, got.most), (life, 18, 18));
        assert_eq!(got.left_at(), life + LEFT);
        assert_eq!(found[1].life(&mem, main).unwrap(), None);
    }
}
