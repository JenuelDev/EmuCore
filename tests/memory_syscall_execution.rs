#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
use emucore::{
    cpu::Register,
    elf::ElfImage,
    fixture,
    runtime::{self, Backend},
};

fn immediate(code: &mut Vec<u8>, register: u8, value: u64) {
    code.extend_from_slice(&[if register >= 8 { 0x49 } else { 0x48 }, 0xb8 + register % 8]);
    code.extend_from_slice(&value.to_le_bytes());
}
fn exit(code: &mut Vec<u8>) {
    code.extend_from_slice(&[0xb8, 60, 0, 0, 0, 0x31, 0xff, 0x0f, 0x05]);
}

#[test]
fn mmap_uses_r10_r8_r9_for_arguments_four_five_and_six() {
    for (flags, descriptor, offset, error) in [
        (0x22, u64::MAX, 0, None),
        (2, u64::MAX, 0, Some(95)),
        (0x22, 0, 0, Some(22)),
        (0x22, u64::MAX, 4096, Some(22)),
    ] {
        let mut code = Vec::new();
        for (register, value) in [
            (0, 9),
            (7, 0),
            (6, 4096),
            (2, 3),
            (10, flags),
            (8, descriptor),
            (9, offset),
        ] {
            immediate(&mut code, register, value);
        }
        code.extend_from_slice(&[0x0f, 0x05, 0x48, 0x89, 0xc3]); // save result in rbx
        exit(&mut code);
        let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
        for backend in [Backend::Native, Backend::Reference] {
            let result = runtime::run_with_backend(&image, backend).unwrap();
            let address = result.registers[Register::Rbx as usize];
            if let Some(error) = error {
                assert_eq!(address, (-i64::from(error)) as u64);
            } else {
                assert_eq!(address, 0x1_0000_0000);
            }
        }
    }
}

#[test]
fn guest_brk_returns_the_actual_break_instead_of_a_libc_status() {
    let mut code = vec![0xb8, 12, 0, 0, 0, 0x31, 0xff, 0x0f, 0x05, 0x49, 0x89, 0xc4];
    code.extend_from_slice(&[
        0x48, 0x8d, 0x78, 16, 0xb8, 12, 0, 0, 0, 0x0f, 0x05, 0x49, 0x89, 0xc5,
    ]);
    code.extend_from_slice(&[
        0x4c, 0x89, 0xe7, 0xb8, 12, 0, 0, 0, 0x0f, 0x05, 0x48, 0x89, 0xc3,
    ]);
    exit(&mut code);
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_backend(&image, backend).unwrap();
        let base = result.registers[Register::R12 as usize];
        assert_eq!(result.registers[Register::R13 as usize], base + 16);
        assert_eq!(result.registers[Register::Rbx as usize], base);
    }
}
