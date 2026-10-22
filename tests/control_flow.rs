#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
use emucore::{
    ErrorKind,
    cpu::Register,
    decode::Program,
    elf::ElfImage,
    fixture,
    process::ProcessOptions,
    runtime::{self, Backend},
};

#[test]
fn backwards_loop_has_matching_native_reference_results_and_step_counts() {
    let code = [
        0x31, 0xdb, 0xb9, 10, 0, 0, 0, 0x01, 0xcb, 0xff, 0xc9, 0x75, 0xfa, 0x89, 0xdf, 0xb8, 60, 0,
        0, 0, 0x0f, 0x05,
    ];
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_backend(&image, backend).unwrap();
        assert_eq!(result.exit_code, 55);
        assert_eq!(result.registers[Register::Rbx as usize], 55);
        assert_eq!(result.instructions_executed, 35);
    }
}

#[test]
fn signed_and_unsigned_conditions_take_different_correct_paths() {
    for (left, right, opcode, expected) in [
        (0x80, 1, 0x7c, 1),
        (0x80, 1, 0x72, 0),
        (0, 1, 0x72, 1),
        (5, 5, 0x74, 1),
        (5, 5, 0x75, 0),
        (0x7f, 0xff, 0x7f, 1),
    ] {
        let mut code = vec![0xb0, left, 0x3c, right, opcode, 12];
        code.extend_from_slice(&[0xbf, 0, 0, 0, 0, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
        code.extend_from_slice(&[0xbf, 1, 0, 0, 0, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
        let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
        for backend in [Backend::Native, Backend::Reference] {
            assert_eq!(
                runtime::run_with_backend(&image, backend)
                    .unwrap()
                    .exit_code,
                expected
            );
        }
    }
}

#[test]
fn rejects_non_boundary_or_out_of_code_branch_targets() {
    for code in [
        &[0x74, 1, 0xb8, 0, 0, 0, 0][..],
        &[0xeb, 0x7f],
        &[0xeb, 0xfd],
    ] {
        let image = ElfImage::parse(&fixture::elf(code, b"x")).unwrap();
        assert_eq!(Program::decode(&image).unwrap_err().kind, ErrorKind::Decode);
    }
}

#[test]
fn reference_infinite_loop_exhausts_the_declared_budget() {
    let image = ElfImage::parse(&fixture::elf(&[0xeb, 0xfe], b"x")).unwrap();
    let error = runtime::run_with_backend(&image, Backend::Reference).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("execution budget exhausted after 100000")
    );
}

#[test]
fn sign_extended_immediates_work_for_memory_and_arithmetic() {
    let mut code = vec![0x48, 0xbe];
    code.extend_from_slice(&fixture::DATA_ADDRESS.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xc7, 0x06, 0xff, 0xff, 0xff, 0xff]); // mov qword [rsi],-1
    code.extend_from_slice(&[0x4c, 0x8b, 0x06]); // mov r8,[rsi]
    code.extend_from_slice(&[0xbb, 1, 0, 0, 0, 0x48, 0x83, 0xc3, 0xff]); // add rbx,-1
    code.extend_from_slice(&[0xbf, 0, 0, 0, 0, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
    let image = ElfImage::parse(&fixture::writable_elf(&code, &[0; 8])).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_backend(&image, backend).unwrap();
        assert_eq!(result.registers[Register::R8 as usize], u64::MAX);
        assert_eq!(result.registers[Register::Rbx as usize], 0);
        assert_ne!(result.flags & emucore::flags::CARRY, 0);
        assert_ne!(result.flags & emucore::flags::ZERO, 0);
    }
}

#[test]
fn argument_loop_handles_variable_lengths_empty_strings_and_missing_arguments() {
    let image = ElfImage::parse(&fixture::arguments_demo()).unwrap();
    for argument in [
        "".to_owned(),
        "not a fixed width".to_owned(),
        "Unicode 🦀 text".to_owned(),
        "x".repeat(8192),
    ] {
        let options = ProcessOptions {
            arguments: vec!["owned".into(), argument.clone()],
            environment: vec![],
        };
        for backend in [Backend::Native, Backend::Reference] {
            let result = runtime::run_with_options(&image, backend, &options).unwrap();
            assert_eq!(result.stdout, argument.as_bytes());
            assert_eq!(result.exit_code, 0);
        }
    }
    assert_eq!(runtime::run(&image).unwrap().exit_code, 64);
}
