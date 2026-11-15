//! What a `no_std` binary leaves for its own code to supply: `memset`,
//! `memcpy` and the panic handler.

/// A small `memset`, for the same reason as `memcpy`.
///
/// # Safety
/// As `memset`.
#[unsafe(no_mangle)]
pub(super) unsafe extern "C" fn memset(dst: *mut u8, value: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe { dst.add(i).write_volatile(value as u8) };
    }
    dst
}

/// A small `memcpy` in place of the builtin, which is too big for the
/// space. Volatile, so it isn't turned back into a `memcpy` call.
///
/// # Safety
/// As `memcpy`.
#[unsafe(no_mangle)]
pub(super) unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe { dst.add(i).write_volatile(src.add(i).read_volatile()) };
    }
    dst
}

/// Leave `mark` and the caller's return address at `PANIC`, for reading
/// from outside when the game hangs, then stop this thread for good.
#[inline(never)]
pub(super) fn stop(mark: &[u8; 8]) -> ! {
    let lr: u64;
    // SAFETY: reads a register.
    unsafe { core::arch::asm!("mov {}, x30", out(reg) lr, options(nomem, nostack)) };
    let to = (super::main() + crate::PANIC as usize) as *mut u64;
    // SAFETY: the last 16 bytes of the list page, kept for this.
    unsafe {
        to.write_volatile(u64::from_le_bytes(*mark));
        to.add(1).write_volatile(lr);
    }
    loop {}
}

/// A panic's location would need relocating, which the hook can't be, so
/// it leaves only its mark.
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    stop(b"PANICKED")
}
