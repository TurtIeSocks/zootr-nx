use super::*;
use crate::known::{actor, item};

#[test]
fn a_loose_quiver_updates_a_closed_chest_after_the_pickup_event() {
    let bow = item::RANGE_BOW.get();
    let quiver = item::QUIVER_MEDIUM.get();
    let larger = item::QUIVER_LARGE.get();
    let line = &[
        (bow, actor::WEAPON_RANGE_BOW.get()),
        (quiver, quiver),
        (larger, larger),
    ];
    let seed = encode(0, &[line], &[], &[], &[], &[], None);
    let name = |s: &str| {
        seed.windows(s.len() + 1)
            .position(|b| b.ends_with(&[0]) && b.starts_with(s.as_bytes()))
            .unwrap() as u64
    };
    for mut m in machines() {
        let b = m.h.build;
        let table = at(b.regions.table);
        let events = SCRATCH + 0x2000;
        let cell = SCRATCH + 0x3000;
        let pouch = SCRATCH + 0x4000;
        let holder = SCRATCH + 0x5000;
        let entry = SCRATCH + 0x6000;
        let slots = SCRATCH + 0x7000;
        let slot = SCRATCH + 0x8000;
        let gd = SCRATCH + 0x9000;
        // One closed chest still holds the Quiver model from the first get.
        let list = at(zootr_hook::list(&b)) + 8;
        let chest = list + 8;
        let write64 = |m: &mut Machine, address, value: u64| {
            m.uc.mem_write(address, &value.to_le_bytes()).unwrap();
        };
        let read64 = |m: &mut Machine, address| {
            let mut bytes = [0; 8];
            m.uc.mem_read(address, &mut bytes).unwrap();
            u64::from_le_bytes(bytes)
        };
        m.uc.mem_write(table, &seed).unwrap();
        write64(&mut m, at(b.got.events), cell);
        write64(&mut m, cell, events);
        write64(&mut m, at(b.got.gamedata), cell + 8);
        write64(&mut m, cell + 8, gd);
        write64(&mut m, at(b.bss.manager), pouch);
        write64(&mut m, list, 1);
        write64(&mut m, chest + 8, holder);
        write64(&mut m, chest + 16, table + name(quiver));
        write64(&mut m, holder + 0x18, holder);
        write64(&mut m, holder + 0x28, entry);
        write64(&mut m, holder + 0x34, 1);
        write64(&mut m, entry, 42);
        write64(&mut m, entry + 0x10, 1);
        write64(&mut m, entry + 0x18, slots);
        write64(&mut m, slots, slot);
        write64(&mut m, slot, at(b.vtables.slot));
        m.set_registers();
        let take = b.entries.taken;
        assert_eq!(m.next_call(at(take)), Some(take + 4));

        let update = b.entries.link_update;
        write64(&mut m, events + 0x68, 1);
        m.set_registers();
        assert_eq!(m.next_call(at(update)), Some(update + 4));
        assert_eq!(read64(&mut m, chest + 16), table + name(quiver));

        write64(&mut m, events + 0x68, 0);
        m.set_registers();
        let mut resume = at(update);
        let mut created = 0;
        loop {
            let call = m.next_call(resume).expect("link update's own body");
            if call == update + 4 {
                break;
            }
            resume = m.uc.reg_read(R::X30).unwrap();
            let result = match call {
                call if call == b.funcs.count => {
                    let arg = m.uc.reg_read(R::X1).unwrap();
                    u64::from(read64(&mut m, arg) == table + name(actor::WEAPON_RANGE_BOW.get()))
                }
                call if call == b.funcs.hash => {
                    let arg = m.uc.reg_read(R::X0).unwrap();
                    let owned = format!("IsGet.{quiver}\0");
                    let mut text = vec![0; owned.len()];
                    m.uc.mem_read(arg, &mut text).unwrap();
                    if text == owned.as_bytes() { 123 } else { 42 }
                }
                call if call == b.funcs.get_bool => {
                    let out = m.uc.reg_read(R::X1).unwrap();
                    let hash = m.uc.reg_read(R::X2).unwrap();
                    m.uc.mem_write(out, &[u8::from(hash == 123)]).unwrap();
                    1
                }
                call if call == b.funcs.delete => 1,
                call if call == b.funcs.create => {
                    let arg = m.uc.reg_read(R::X2).unwrap();
                    assert_eq!(read64(&mut m, arg), table + name(larger));
                    created += 1;
                    1
                }
                _ => panic!("unexpected game call at {call:#x}"),
            };
            m.uc.reg_write(R::X0, result).unwrap();
        }
        assert_eq!(created, 1, "{}", m.h.release.name());
        m.set_registers();
        assert_eq!(m.next_call(at(update)), Some(update + 4));
    }
}
