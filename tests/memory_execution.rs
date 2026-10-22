#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use emucore::{
    ErrorKind,
    cpu::Register,
    elf::ElfImage,
    fixture,
    runtime::{self, Backend, RunResult},
};

fn mov64(code: &mut Vec<u8>, register: u8, value: u64) {
    code.extend_from_slice(&[if register >= 8 { 0x49 } else { 0x48 }, 0xb8 + register % 8]);
    code.extend_from_slice(&value.to_le_bytes());
}

fn exit(code: &mut Vec<u8>) {
    code.extend_from_slice(&[0xb8, 60, 0, 0, 0, 0x31, 0xff, 0x0f, 0x05]);
}

fn differential(file: &[u8]) -> RunResult {
    let image = ElfImage::parse(file).unwrap();
    let native = runtime::run(&image).unwrap();
    let reference = runtime::run_with_backend(&image, Backend::Reference).unwrap();
    assert_eq!(native.stdout, reference.stdout);
    assert_eq!(native.stderr, reference.stderr);
    assert_eq!(native.registers, reference.registers);
    assert_eq!(native.exit_code, reference.exit_code);
    native
}

#[test]
fn writable_fixture_mutates_data_before_printing() {
    let result = differential(&fixture::memory_demo());
    assert_eq!(result.stdout, fixture::MEMORY_MESSAGE);
    assert_eq!(result.exit_code, 0);
}

#[test]
fn native_register_file_covers_all_sixteen_registers() {
    let mut code = Vec::new();
    for register in 0..16 {
        mov64(
            &mut code,
            register,
            0x1234_5678_0000_0000 + u64::from(register),
        );
    }
    exit(&mut code);
    let result = differential(&fixture::elf(&code, b"x"));
    for register in Register::ALL {
        if !matches!(
            register,
            Register::Rax | Register::Rdi | Register::Rcx | Register::R11
        ) {
            assert_eq!(
                result.registers[register as usize],
                0x1234_5678_0000_0000 + register as u64
            );
        }
    }
}

#[test]
fn partial_register_writes_preserve_or_zero_upper_bits() {
    for (instruction, expected) in [
        (&[0x41, 0xb0, 0x7e][..], 0xfedc_ba98_7654_327e),
        (&[0x66, 0x41, 0xb8, 0x34, 0x12][..], 0xfedc_ba98_7654_1234),
        (&[0x41, 0xb8, 0x34, 0x12, 0, 0][..], 0x1234),
    ] {
        let mut code = Vec::new();
        mov64(&mut code, 8, 0xfedc_ba98_7654_3210);
        code.extend_from_slice(instruction);
        exit(&mut code);
        let result = differential(&fixture::elf(&code, b"x"));
        assert_eq!(result.registers[Register::R8 as usize], expected);
    }
}

#[test]
fn extended_indexed_addressing_and_negative_displacements() {
    let mut code = Vec::new();
    mov64(&mut code, 13, fixture::DATA_ADDRESS);
    mov64(&mut code, 1, 2);
    mov64(&mut code, 10, u64::from_le_bytes(*b"indexed\n"));
    code.extend_from_slice(&[0x4d, 0x89, 0x54, 0x8d, 0xf8]); // mov [r13+rcx*4-8],r10
    code.extend_from_slice(&[0x49, 0x8d, 0x74, 0x8d, 0xf8]); // lea rsi,[r13+rcx*4-8]
    code.extend_from_slice(&[
        0xb8, 1, 0, 0, 0, 0xbf, 1, 0, 0, 0, 0xba, 8, 0, 0, 0, 0x0f, 0x05,
    ]);
    exit(&mut code);
    let result = differential(&fixture::writable_elf(&code, &[0; 8]));
    assert_eq!(result.stdout, b"indexed\n");
}

#[test]
fn read_only_and_unmapped_memory_faults_are_reported() {
    for address in [
        fixture::DATA_ADDRESS,
        fixture::CODE_ADDRESS,
        u64::MAX,
        fixture::DATA_ADDRESS - 1,
    ] {
        let mut code = Vec::new();
        mov64(&mut code, 6, address);
        code.extend_from_slice(&[0xc6, 0x06, 0x41]);
        exit(&mut code);
        let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
        for backend in [Backend::Native, Backend::Reference] {
            let error = runtime::run_with_backend(&image, backend).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Runtime);
            assert!(error.to_string().contains("Write fault"), "{error}");
        }
    }
}

#[test]
fn loads_preserve_partial_register_semantics() {
    for (load, expected) in [
        (&[0x44, 0x8a, 0x06][..], 0xfedc_ba98_7654_3211),
        (&[0x66, 0x44, 0x8b, 0x06][..], 0xfedc_ba98_7654_2211),
        (&[0x44, 0x8b, 0x06][..], 0x4433_2211),
        (&[0x4c, 0x8b, 0x06][..], 0x8877_6655_4433_2211),
    ] {
        let mut code = Vec::new();
        mov64(&mut code, 6, fixture::DATA_ADDRESS);
        mov64(&mut code, 8, 0xfedc_ba98_7654_3210);
        code.extend_from_slice(load);
        exit(&mut code);
        let result = differential(&fixture::elf(
            &code,
            &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        ));
        assert_eq!(result.registers[Register::R8 as usize], expected);
    }
}
