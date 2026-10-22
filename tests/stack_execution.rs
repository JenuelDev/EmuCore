#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
use emucore::{
    ErrorKind,
    cpu::Register,
    elf::ElfImage,
    fixture,
    process::ProcessOptions,
    runtime::{self, Backend},
};

fn exit(code: &mut Vec<u8>) {
    code.extend_from_slice(&[0xb8, 60, 0, 0, 0, 0x31, 0xff, 0x0f, 0x05]);
}

#[test]
fn argument_fixture_reads_guest_argv_in_both_backends() {
    let image = ElfImage::parse(&fixture::arguments_demo()).unwrap();
    let options = ProcessOptions {
        arguments: vec!["owned".into(), "Rust".into()],
        environment: vec![],
    };
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_options(&image, backend, &options).unwrap();
        assert_eq!(result.stdout, b"Rust");
        assert_eq!(result.exit_code, 0);
    }
}

#[test]
fn push_pop_use_guest_memory_and_pre_push_rsp() {
    let mut code = vec![0x49, 0x89, 0xe4, 0x54, 0x41, 0x5e, 0x6a, 0xff, 0x5b]; // save rsp; push rsp/pop r14; push -1/pop rbx
    exit(&mut code);
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_backend(&image, backend).unwrap();
        assert_eq!(
            result.registers[Register::Rsp as usize],
            result.registers[Register::R12 as usize]
        );
        assert_eq!(
            result.registers[Register::R14 as usize],
            result.registers[Register::R12 as usize]
        );
        assert_eq!(result.registers[Register::Rbx as usize], u64::MAX);
    }
}

#[test]
fn pop_into_rsp_assigns_the_popped_value() {
    let mut code = vec![0x48, 0x89, 0xe3, 0x53, 0x5c]; // mov rbx,rsp; push rbx; pop rsp
    exit(&mut code);
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    let result = runtime::run(&image).unwrap();
    assert_eq!(
        result.registers[Register::Rsp as usize],
        result.registers[Register::Rbx as usize]
    );
}

#[test]
fn invalid_guest_stack_is_a_runtime_fault() {
    for instruction in [0x50, 0x58] {
        // push/pop rax
        let mut code = vec![0x48, 0xbc, 0, 0, 0, 0, 0, 0, 0, 0, instruction];
        exit(&mut code);
        let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
        let error = runtime::run(&image).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Runtime);
        assert!(error.to_string().contains("fault"));
    }
}
