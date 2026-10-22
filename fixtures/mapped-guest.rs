//! Owned allocation/protection/release guest. Built separately from the host crate.
#![no_std]
#![no_main]
use core::arch::{asm, naked_asm};
const MESSAGE: &[u8] = b"Guest mmap works!\n";

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    naked_asm!("call {main}","ud2",main=sym guest_main);
}

#[inline(never)]
fn map() -> u64 {
    let result;
    // SAFETY: this invokes anonymous Linux mmap with six explicit argument registers.
    unsafe {
        asm!("syscall",inlateout("rax") 9_u64=>result,in("rdi") 0_u64,in("rsi") 4096_u64,in("rdx") 3_u64,in("r10") 0x22_u64,in("r8") u64::MAX,in("r9") 0_u64,lateout("rcx") _,lateout("r11") _,options(nostack));
    }
    result
}

#[inline(never)]
fn protect(address: u64) -> u64 {
    let result;
    // SAFETY: the address belongs to the successful anonymous mapping; request read-only data.
    unsafe {
        asm!("syscall",inlateout("rax") 10_u64=>result,in("rdi") address,in("rsi") 4096_u64,in("rdx") 1_u64,lateout("rcx") _,lateout("r11") _,options(nostack));
    }
    result
}

#[inline(never)]
fn output(address: u64) -> u64 {
    let result;
    // SAFETY: the mapped buffer is readable and contains MESSAGE.len() initialized bytes.
    unsafe {
        asm!("syscall",inlateout("rax") 1_u64=>result,in("rdi") 1_u64,in("rsi") address,in("rdx") MESSAGE.len(),lateout("rcx") _,lateout("r11") _,options(nostack));
    }
    result
}

#[inline(never)]
fn unmap(address: u64) -> u64 {
    let result;
    // SAFETY: remove the complete mapping after the last read of its contents.
    unsafe {
        asm!("syscall",inlateout("rax") 11_u64=>result,in("rdi") address,in("rsi") 4096_u64,lateout("rcx") _,lateout("r11") _,options(nostack));
    }
    result
}

extern "C" fn guest_main() -> ! {
    let address = map();
    if (address as i64) < 0 {
        exit(2);
    }
    for (index, &byte) in MESSAGE.iter().enumerate() {
        // SAFETY: a complete writable page was allocated and index is bounded by the short message.
        unsafe {
            core::ptr::write_volatile((address as *mut u8).add(index), byte);
        }
    }
    if protect(address) != 0 {
        exit(3);
    }
    if output(address) != MESSAGE.len() as u64 {
        exit(4);
    }
    if unmap(address) != 0 {
        exit(5);
    }
    exit(0)
}

fn exit(status: u64) -> ! {
    // SAFETY: the Linux exit ABI does not return.
    unsafe {
        asm!("syscall",in("rax") 60_u64,in("rdi") status,options(noreturn,nostack));
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: an unexpected guest panic becomes an explicit guest trap.
    unsafe {
        asm!("ud2", options(noreturn, nostack));
    }
}
