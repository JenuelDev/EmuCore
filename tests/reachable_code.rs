#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
use emucore::{
    ErrorKind,
    decode::Program,
    elf::ElfImage,
    fixture,
    runtime::{self, Backend},
};

fn set64(file: &mut [u8], at: usize, value: u64) {
    file[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn set32(file: &mut [u8], at: usize, value: u32) {
    file[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn relative_call(from: u64, to: u64) -> [u8; 5] {
    let displacement = i32::try_from(to as i64 - (from + 5) as i64).unwrap();
    let mut bytes = [0xe8, 0, 0, 0, 0];
    bytes[1..].copy_from_slice(&displacement.to_le_bytes());
    bytes
}

fn three_segments(first: &[u8], last: &[u8], entry: u64) -> ElfImage {
    let mut file = fixture::elf(first, b"x");
    file.resize(0x3000 + last.len(), 0);
    file[0x3000..].copy_from_slice(last);
    file[56..58].copy_from_slice(&3_u16.to_le_bytes());
    set64(&mut file, 24, entry);
    set32(&mut file, 176, 1);
    set32(&mut file, 180, 5);
    for (offset, value) in [
        (8, 0x3000),
        (16, 0x403000),
        (32, last.len() as u64),
        (40, last.len() as u64),
        (48, 4096),
    ] {
        set64(&mut file, 176 + offset, value);
    }
    ElfImage::parse(&file).unwrap()
}

#[test]
fn calls_a_function_before_entry_and_skips_unreachable_invalid_padding() {
    let mut code = vec![0xb8, 42, 0, 0, 0, 0xc3];
    code.resize(16, 0x06); // Invalid legacy instructions in 64-bit mode, never reachable.
    code.extend_from_slice(&relative_call(
        fixture::CODE_ADDRESS + 16,
        fixture::CODE_ADDRESS,
    ));
    code.extend_from_slice(&[0x89, 0xc7, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
    let mut file = fixture::elf(&code, b"x");
    set64(&mut file, 24, fixture::CODE_ADDRESS + 16);
    let image = ElfImage::parse(&file).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        let result = runtime::run_with_backend(&image, backend).unwrap();
        assert_eq!(result.exit_code, 42);
        assert_eq!(result.instructions_executed, 6);
    }
}

#[test]
fn calls_across_distinct_executable_segments_and_returns_to_the_entry_segment() {
    let helper = [0xb8, 37, 0, 0, 0, 0xc3];
    let mut entry = relative_call(0x403000, fixture::CODE_ADDRESS).to_vec();
    entry.extend_from_slice(&[0x89, 0xc7, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
    let image = three_segments(&helper, &entry, 0x403000);
    for backend in [Backend::Native, Backend::Reference] {
        assert_eq!(
            runtime::run_with_backend(&image, backend)
                .unwrap()
                .exit_code,
            37
        );
    }
}

#[test]
fn unreachable_invalid_bytes_after_an_unconditional_branch_are_not_decoded() {
    let mut code = vec![0xeb, 6];
    code.extend_from_slice(&[0x06; 6]);
    code.extend_from_slice(&[0xb8, 60, 0, 0, 0, 0xbf, 0, 0, 0, 0, 0x0f, 0x05]);
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    let program = Program::decode(&image).unwrap();
    assert_eq!(program.instructions().len(), 4);
    for backend in [Backend::Native, Backend::Reference] {
        assert_eq!(
            runtime::run_with_backend(&image, backend)
                .unwrap()
                .exit_code,
            0
        );
    }
}

#[test]
fn missing_fallthrough_does_not_enter_a_different_sorted_code_region() {
    let mut first = vec![0x31, 0xc0, 0x0f, 0x85];
    first.extend_from_slice(
        &i32::try_from(0x403000_i64 - (fixture::CODE_ADDRESS + 8) as i64)
            .unwrap()
            .to_le_bytes(),
    );
    first.extend_from_slice(&[0xb8, 60, 0, 0, 0]); // No syscall and no executable byte following this instruction.
    let other = [0xbf, 0, 0, 0, 0, 0xb8, 60, 0, 0, 0, 0x0f, 0x05];
    let image = three_segments(&first, &other, fixture::CODE_ADDRESS);
    for backend in [Backend::Native, Backend::Reference] {
        let error = runtime::run_with_backend(&image, backend).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Runtime);
        assert!(error.to_string().contains("without exit"));
    }
}

#[test]
fn ordinary_fallthrough_can_cross_adjacent_executable_segments() {
    let mut file = fixture::elf(&[0xb8, 60, 0, 0, 0], b"x");
    let continuation = [0xbf, 0, 0, 0, 0, 0x0f, 0x05];
    file.resize(0x2005 + continuation.len(), 0);
    file[0x2005..].copy_from_slice(&continuation);
    set32(&mut file, 124, 5);
    for (offset, value) in [
        (8, 0x2005),
        (16, fixture::CODE_ADDRESS + 5),
        (32, 7),
        (40, 7),
    ] {
        set64(&mut file, 120 + offset, value);
    }
    let image = ElfImage::parse(&file).unwrap();
    for backend in [Backend::Native, Backend::Reference] {
        assert_eq!(
            runtime::run_with_backend(&image, backend)
                .unwrap()
                .exit_code,
            0
        );
    }
}

#[test]
fn overlapping_reachable_decodes_and_non_executable_targets_are_rejected() {
    let image = ElfImage::parse(&fixture::elf(&[0x74, 1, 0xb8, 0, 0, 0, 0], b"x")).unwrap();
    assert!(
        Program::decode(&image)
            .unwrap_err()
            .to_string()
            .contains("overlapping")
    );
    let mut code = vec![0xe9];
    code.extend_from_slice(
        &i32::try_from(fixture::DATA_ADDRESS as i64 - (fixture::CODE_ADDRESS + 5) as i64)
            .unwrap()
            .to_le_bytes(),
    );
    let image = ElfImage::parse(&fixture::elf(&code, b"x")).unwrap();
    assert!(
        Program::decode(&image)
            .unwrap_err()
            .to_string()
            .contains("not file-backed executable")
    );
}
