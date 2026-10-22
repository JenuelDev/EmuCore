#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use emucore::{
    ErrorKind,
    elf::ElfImage,
    fixture,
    runtime::{self, MAX_OUTPUT},
};

fn set32(code: &mut Vec<u8>, opcode: u8, value: u32) {
    code.push(opcode);
    code.extend_from_slice(&value.to_le_bytes());
}

fn set64(code: &mut Vec<u8>, opcode: u8, value: u64) {
    code.extend_from_slice(&[0x48, opcode]);
    code.extend_from_slice(&value.to_le_bytes());
}

fn write(code: &mut Vec<u8>, fd: u32, address: u64, length: u32) {
    set32(code, 0xb8, 1);
    set32(code, 0xbf, fd);
    set64(code, 0xbe, address);
    set32(code, 0xba, length);
    code.extend_from_slice(&[0x0f, 0x05]);
}

fn exit_with_write_result(code: &mut Vec<u8>) {
    code.extend_from_slice(&[0x89, 0xc7]); // mov edi,eax, testing the syscall return register
    set32(code, 0xb8, 60);
    code.extend_from_slice(&[0x0f, 0x05]);
}

#[test]
fn translates_linux_fixture_and_returns_cleanly() {
    let image = ElfImage::parse(&fixture::hello()).unwrap();
    for _ in 0..8 {
        let result = runtime::run(&image).unwrap();
        assert_eq!(result.stdout, fixture::MESSAGE);
        assert!(result.stderr.is_empty());
        assert_eq!(result.exit_code, 0);
    }
}

#[test]
fn preserves_syscall_results_and_routes_stderr() {
    let mut code = Vec::new();
    write(&mut code, 2, fixture::DATA_ADDRESS, 3);
    exit_with_write_result(&mut code);
    let result = runtime::run(&ElfImage::parse(&fixture::elf(&code, b"abc")).unwrap()).unwrap();
    assert_eq!(result.stderr, b"abc");
    assert!(result.stdout.is_empty());
    assert_eq!(result.exit_code, 3);
}

#[test]
fn invalid_guest_pointers_and_descriptors_return_linux_errors() {
    for (fd, address, length, expected) in [
        (1, u64::MAX, 2, 242), // -EFAULT, low eight exit bits
        (1, fixture::DATA_ADDRESS - 1, 2, 242),
        (99, fixture::DATA_ADDRESS, 1, 247), // -EBADF
        (1, u64::MAX, 0, 0),                 // zero-length write must not dereference a pointer
        (1, fixture::DATA_ADDRESS, MAX_OUTPUT as u32 + 1, 229), // -EFBIG
    ] {
        let mut code = Vec::new();
        write(&mut code, fd, address, length);
        exit_with_write_result(&mut code);
        let result = runtime::run(&ElfImage::parse(&fixture::elf(&code, b"abc")).unwrap()).unwrap();
        assert_eq!(result.exit_code, expected);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn enforces_combined_output_budget_and_stops_on_exit_group() {
    let mut code = Vec::new();
    write(&mut code, 1, fixture::DATA_ADDRESS, MAX_OUTPUT as u32);
    write(&mut code, 2, fixture::DATA_ADDRESS, 1);
    code.extend_from_slice(&[0x89, 0xc7]);
    set32(&mut code, 0xb8, 231);
    code.extend_from_slice(&[0x0f, 0x05]);
    // This is valid code, but must never execute after exit_group.
    set32(&mut code, 0xb8, 999);
    code.extend_from_slice(&[0x0f, 0x05]);
    let data = vec![b'x'; MAX_OUTPUT];
    let result = runtime::run(&ElfImage::parse(&fixture::elf(&code, &data)).unwrap()).unwrap();
    assert_eq!(result.stdout, data);
    assert!(result.stderr.is_empty());
    assert_eq!(result.exit_code, 229);
}

#[test]
fn reports_unknown_syscalls_and_missing_exit() {
    let mut code = Vec::new();
    set32(&mut code, 0xb8, 999);
    code.extend_from_slice(&[0x0f, 0x05]);
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    assert_eq!(
        runtime::run(&image).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    let image = ElfImage::parse(&fixture::elf(&[0x31, 0xc0], b"x")).unwrap();
    assert_eq!(runtime::run(&image).unwrap_err().kind, ErrorKind::Runtime);
}

#[test]
fn zero_extends_32_bit_register_assignments() {
    let mut code = Vec::new();
    set64(&mut code, 0xb8, 0x1234_5678_0000_0001);
    code.extend_from_slice(&[0x89, 0xc7]); // mov edi,eax must discard the high half
    set32(&mut code, 0xb8, 1);
    set64(&mut code, 0xbe, fixture::DATA_ADDRESS);
    set32(&mut code, 0xba, 3);
    code.extend_from_slice(&[0x0f, 0x05]);
    exit_with_write_result(&mut code);
    let result = runtime::run(&ElfImage::parse(&fixture::elf(&code, b"abc")).unwrap()).unwrap();
    assert_eq!(result.stdout, b"abc"); // a non-zero high half would instead produce EBADF
    assert_eq!(result.exit_code, 3);
}
