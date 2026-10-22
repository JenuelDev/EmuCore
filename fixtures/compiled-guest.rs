//! Owned freestanding Rust guest using the Linux x86-64 syscall ABI.
//! Build separately with scripts/build-guest.ps1; this is not part of the host crate.
#![no_std]
#![no_main]

use core::arch::{asm, naked_asm};

static INPUT: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const MESSAGE: &[u8] = b"Compiled Rust guest executed successfully!\n";

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // A process starts with a 16-byte-aligned stack. CALL establishes the C function-entry alignment.
    naked_asm!("call {main}", "ud2", main = sym guest_main);
}

#[inline(never)]
fn checksum() -> u64 {
    let mut sum = 0_u64;
    for index in 0..INPUT.len() {
        // SAFETY: index is bounded by INPUT.len(), and the immutable static lives for the whole run.
        sum = sum.wrapping_add(unsafe { core::ptr::read_volatile(INPUT.as_ptr().add(index)) });
    }
    sum
}

extern "C" fn guest_main() -> ! {
    if checksum() != 36 {
        exit(1);
    }
    write(MESSAGE);
    exit(0)
}

#[inline(never)]
fn write(message: &[u8]) {
    // SAFETY: Linux write reads this live borrowed buffer. RCX/R11 and RAX clobbers are declared.
    unsafe {
        asm!("syscall", inlateout("rax") 1_u64 => _, in("rdi") 1_u64, in("rsi") message.as_ptr(), in("rdx") message.len(), lateout("rcx") _, lateout("r11") _, options(nostack));
    }
}

#[inline(always)]
fn exit(status: u64) -> ! {
    // SAFETY: the Linux exit ABI terminates guest execution and does not return.
    unsafe {
        asm!("syscall", in("rax") 60_u64, in("rdi") status, options(noreturn, nostack));
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: an unexpected guest panic is an intentional guest trap, never a host call.
    unsafe {
        asm!("ud2", options(noreturn, nostack));
    }
}
