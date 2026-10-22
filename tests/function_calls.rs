#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
use emucore::{
    ErrorKind,
    cpu::Register,
    elf::ElfImage,
    fixture,
    runtime::{self, Backend},
};

#[test]
fn nested_guest_calls_preserve_frames_and_return_values() {
    let code = [
        0x49, 0x89, 0xe4, // save original rsp in r12
        0xe8, 9, 0, 0, 0, 0x89, 0xc7, 0xb8, 60, 0, 0, 0, 0x0f, 0x05, 0x53, 0xbb, 7, 0, 0, 0, 0xe8,
        4, 0, 0, 0, 0x01, 0xd8, 0x5b, 0xc3, 0xb8, 5, 0, 0, 0, 0xc3,
    ];
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_backend(&image, backend).unwrap();
        assert_eq!(result.exit_code, 12);
        assert_eq!(result.registers[Register::Rbx as usize], 0);
        assert_eq!(
            result.registers[Register::Rsp as usize],
            result.registers[Register::R12 as usize]
        );
    }
}

#[test]
fn return_with_stack_adjustment_discards_guest_arguments() {
    let code = [
        0x49, 0x89, 0xe4, 0x48, 0x83, 0xec, 16, // save rsp; reserve two qwords
        0xe8, 9, 0, 0, 0, // callee follows the nine-byte exit sequence
        0xb8, 60, 0, 0, 0, 0x31, 0xff, 0x0f, 0x05, 0xc2, 16, 0, // ret 16
    ];
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_backend(&image, backend).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(
            result.registers[Register::Rsp as usize],
            result.registers[Register::R12 as usize]
        );
    }
}

#[test]
fn return_targets_cannot_reach_unmapped_memory_or_instruction_interiors() {
    for target in [1_u32, fixture::CODE_ADDRESS as u32 + 1, 0x7fff_ffff] {
        let mut code = vec![0x68];
        code.extend_from_slice(&target.to_le_bytes());
        code.push(0xc3);
        let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
        for backend in [Backend::Native, Backend::Reference] {
            let error = runtime::run_with_backend(&image, backend).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Runtime);
            assert!(error.to_string().contains("return target"));
        }
    }
}

#[test]
fn explicit_guest_traps_stop_with_a_guest_address_diagnostic() {
    for code in [&[0xcc][..], &[0x0f, 0x0b]] {
        let image = ElfImage::parse(&fixture::elf(code, b"x")).unwrap();
        for backend in [Backend::Native, Backend::Reference] {
            let error = runtime::run_with_backend(&image, backend).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Runtime);
            assert!(error.to_string().contains("guest trap at 0x401000"));
        }
    }
}
