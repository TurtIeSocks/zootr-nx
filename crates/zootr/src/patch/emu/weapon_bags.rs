use super::*;
use crate::known::{actor, item};
use ounce_formats::murmur3;
use zootr_hook::switches::{COUNT, ENTRIES, MAP, MAP_SIZE, STRING64, TABLE};

const POUCH: u64 = SCRATCH + 0x2000;
const GD: u64 = SCRATCH + 0x3000;
const MAP_AT: u64 = SCRATCH + 0x4000;
const ENTRIES_AT: u64 = SCRATCH + 0x5000;
const CELL: u64 = SCRATCH + 0x6000;

/// Run through the hook until it returns or calls the game.
fn next_call(m: &mut Machine, start: u64) -> Option<u32> {
    *m.uc.get_data_mut() = Run {
        start,
        until: SENTINEL,
        left: None,
    };
    m.uc.emu_start(start, SENTINEL, 0, BUDGET).unwrap();
    let left = m.uc.get_data().left;
    assert_eq!(m.uc.reg_read(R::PC).unwrap(), left.unwrap_or(SENTINEL));
    left.map(|pc| (pc - BASE) as u32)
}

#[test]
fn a_first_bow_or_slingshot_grant_adds_its_missing_starter_bag() {
    let gifts = [
        (
            actor::WEAPON_RANGE_SLING_SHOT,
            item::RANGE_SLING_SHOT,
            "DekuSeed",
            item::BULLET_BAG_SMALL,
        ),
        (
            actor::WEAPON_RANGE_BOW,
            item::RANGE_BOW,
            "Arrow",
            item::QUIVER_SMALL,
        ),
    ];
    for mut m in machines() {
        let b = m.h.build;
        let write64 = |m: &mut Machine, address, value: u64| {
            m.uc.mem_write(address, &value.to_le_bytes()).unwrap();
        };
        let (slot, stride, value) = STRING64;
        let table = GD + 8 + (slot * TABLE) as u64;
        write64(&mut m, at(b.got.gamedata), CELL);
        write64(&mut m, CELL, GD);
        write64(&mut m, table + COUNT as u64, gifts.len() as u64);
        write64(&mut m, table + MAP_SIZE as u64, gifts.len() as u64);
        write64(&mut m, table + MAP as u64, MAP_AT);
        write64(&mut m, table + ENTRIES as u64, ENTRIES_AT);
        m.uc.mem_write(
            at(b.regions.table),
            &encode(0, &[], &[], &[], &[], &[], None),
        )
        .unwrap();
        for (i, &(_, _, kind, _)) in gifts.iter().enumerate() {
            let hash = murmur3(format!("CurrentExpansionActor_{kind}").as_bytes());
            write64(
                &mut m,
                MAP_AT + 8 * i as u64,
                ((i as u64) << 32) | u64::from(hash),
            );
        }

        for (i, &(weapon, pickup, _, bag)) in gifts.iter().enumerate() {
            let owned = murmur3(format!("IsGet.{weapon}").as_bytes());
            for (added, category, result, has_weapon, has_bag, grants_bag) in [
                (pickup.get(), 1, 1, false, false, true),
                (weapon.get(), 1, 1, false, false, true),
                ("Alias", 1, 1, true, false, true),
                (pickup.get(), 1, 1, true, true, false),
                (pickup.get(), 1, 0, false, false, false),
                ("OtherItem", 6, 1, false, false, false),
                ("OtherItem", 6, 1, true, false, true),
            ] {
                for index in 0..gifts.len() {
                    let text = if index == i && has_bag { b'I' } else { 0 };
                    m.uc.mem_write(ENTRIES_AT + (index * stride + value) as u64, &[text])
                        .unwrap();
                }
                let set = m.set_registers();
                let argument = m.uc.reg_read(R::X1).unwrap();
                let name_at = SCRATCH + 0x1000;
                m.uc.mem_write(name_at, format!("{added}\0").as_bytes())
                    .unwrap();
                write64(&mut m, argument, name_at);
                m.uc.reg_write(R::X0, POUCH).unwrap();
                m.uc.reg_write(R::X2, category).unwrap();
                m.uc.reg_write(R::X3, 1).unwrap();
                let stack = m.uc.reg_read(R::SP).unwrap();
                write64(&mut m, stack, 1);
                let original = b.entries.granted + 4;
                assert_eq!(next_call(&mut m, at(b.entries.granted)), Some(original));
                let mut stack_arg = [0; 8];
                m.uc.mem_read(m.uc.reg_read(R::SP).unwrap(), &mut stack_arg)
                    .unwrap();
                assert_eq!(u64::from_le_bytes(stack_arg), 1, "ninth grant argument");
                let mut resume = m.uc.reg_read(R::X30).unwrap();
                m.uc.reg_write(R::X0, result).unwrap();
                let mut bags = Vec::new();
                while let Some(call) = next_call(&mut m, resume) {
                    resume = m.uc.reg_read(R::X30).unwrap();
                    match call {
                        call if call == b.funcs.get_bool => {
                            let out = m.uc.reg_read(R::X1).unwrap();
                            let hash = m.uc.reg_read(R::X2).unwrap() as u32;
                            m.uc.mem_write(out, &[u8::from(has_weapon && hash == owned)])
                                .unwrap();
                        }
                        call if call == original => {
                            assert_eq!(m.uc.reg_read(R::X0).unwrap(), POUCH);
                            assert_eq!(m.uc.reg_read(R::X2).unwrap(), 6, "key item category");
                            assert_eq!(m.uc.reg_read(R::X3).unwrap(), 1, "one bag");
                            let mut ptr = [0; 8];
                            m.uc.mem_read(m.uc.reg_read(R::X1).unwrap(), &mut ptr)
                                .unwrap();
                            let mut name = vec![0; bag.get().len() + 1];
                            m.uc.mem_read(u64::from_le_bytes(ptr), &mut name).unwrap();
                            assert_eq!(name.pop(), Some(0));
                            bags.push(String::from_utf8(name).unwrap());
                        }
                        _ => panic!("unexpected game call at {call:#x}"),
                    }
                    m.uc.reg_write(R::X0, 1).unwrap();
                }
                let expected = if grants_bag { vec![bag.get()] } else { vec![] };
                assert_eq!(bags, expected, "{} {weapon}", m.h.release.name());
                assert_eq!(m.uc.reg_read(R::X0).unwrap(), result);
                assert!(m.changed(&set, &KEPT).is_empty());
            }
        }
    }
}
