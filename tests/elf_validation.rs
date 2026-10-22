use emucore::{
    ErrorKind,
    elf::{ElfImage, MAX_FILE_SIZE},
    fixture,
};

fn set64(file: &mut [u8], at: usize, value: u64) {
    file[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn set32(file: &mut [u8], at: usize, value: u32) {
    file[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn loads_fixture_segments_and_zero_fills_bss() {
    let mut file = fixture::hello();
    set64(&mut file, 120 + 40, fixture::MESSAGE.len() as u64 + 32);
    let image = ElfImage::parse(&file).unwrap();
    assert_eq!(image.entry(), fixture::CODE_ADDRESS);
    assert_eq!(image.segments().len(), 2);
    assert_eq!(
        image.read(fixture::DATA_ADDRESS, fixture::MESSAGE.len() as u64),
        Some(fixture::MESSAGE)
    );
    assert_eq!(
        image.read(fixture::DATA_ADDRESS + fixture::MESSAGE.len() as u64, 32),
        Some(&[0; 32][..])
    );
    assert!(image.read(fixture::DATA_ADDRESS - 1, 2).is_none());
    assert!(image.read(u64::MAX, 2).is_none());
}

#[test]
fn rejects_every_truncated_fixture_without_panicking() {
    let file = fixture::hello();
    for end in 0..file.len() {
        assert!(
            ElfImage::parse(&file[..end]).is_err(),
            "accepted truncated length {end}"
        );
    }
}

#[test]
fn rejects_header_and_architecture_mismatches() {
    for (at, value, kind) in [
        (0, 0, ErrorKind::InvalidElf),
        (4, 1, ErrorKind::Unsupported),
        (5, 2, ErrorKind::Unsupported),
        (6, 0, ErrorKind::InvalidElf),
        (7, 9, ErrorKind::Unsupported),
        (8, 1, ErrorKind::Unsupported),
        (16, 3, ErrorKind::Unsupported),
        (18, 40, ErrorKind::Unsupported),
        (20, 0, ErrorKind::InvalidElf),
        (48, 1, ErrorKind::Unsupported),
        (52, 0, ErrorKind::InvalidElf),
        (54, 0, ErrorKind::InvalidElf),
        (56, 0, ErrorKind::InvalidElf),
        (56, 17, ErrorKind::InvalidElf),
    ] {
        let mut file = fixture::hello();
        file[at] = value;
        assert_eq!(
            ElfImage::parse(&file).unwrap_err().kind,
            kind,
            "offset {at}"
        );
    }
}

#[test]
fn rejects_overflows_alignment_overlap_and_unbacked_entry() {
    let cases: &[(usize, u64)] = &[
        (32, u64::MAX),                       // program-header range overflow
        (64 + 8, u64::MAX),                   // segment file range overflow
        (64 + 16, u64::MAX - 1),              // segment virtual address overflow
        (64 + 40, 1),                         // filesz > memsz
        (64 + 48, 3),                         // invalid p_align
        (64 + 16, fixture::CODE_ADDRESS + 1), // offset/address misalignment
        (120 + 16, fixture::CODE_ADDRESS),    // overlapping PT_LOAD ranges
        (120 + 16, 0x80000000),               // huge sparse image
        (120 + 40, 32 * 1024 * 1024),         // excessive allocation
        (24, fixture::DATA_ADDRESS),          // non-executable entry
        (24, fixture::CODE_ADDRESS - 1),      // unmapped entry
    ];
    for &(at, value) in cases {
        let mut file = fixture::hello();
        set64(&mut file, at, value);
        assert!(
            ElfImage::parse(&file).is_err(),
            "accepted offset {at} value {value:x}"
        );
    }
    let mut file = fixture::hello();
    let code_size = file.len() as u64; // point entry past code into declared BSS
    set64(&mut file, 64 + 40, code_size);
    set64(&mut file, 24, fixture::CODE_ADDRESS + 100);
    assert!(ElfImage::parse(&file).is_err());
}

#[test]
fn rejects_unsupported_program_headers_and_permissions() {
    for header_type in [2, 3, 7, 0x6474e551] {
        let mut file = fixture::hello();
        set32(&mut file, 64, header_type);
        assert_eq!(
            ElfImage::parse(&file).unwrap_err().kind,
            ErrorKind::Unsupported
        );
    }
    for flags in [0, 1, 7, 0x100] {
        let mut file = fixture::hello();
        set32(&mut file, 64 + 4, flags);
        assert_eq!(
            ElfImage::parse(&file).unwrap_err().kind,
            ErrorKind::Unsupported
        );
    }
    assert!(ElfImage::parse(&vec![0; MAX_FILE_SIZE + 1]).is_err());
}

#[test]
fn program_header_address_translation_does_not_overflow_intermediate_arithmetic() {
    let original = fixture::hello();
    let mut file = vec![0; 0x3000];
    file[..64].copy_from_slice(&original[..64]);
    file[0x2000..0x2070].copy_from_slice(&original[64..176]);
    let high = u64::MAX - 4095;
    set64(&mut file, 32, 0x2000);
    set64(&mut file, 24, high + 0x100);
    set64(&mut file, 0x2000 + 8, 0x2000);
    set64(&mut file, 0x2000 + 16, high);
    set64(&mut file, 0x2000 + 32, 512);
    set64(&mut file, 0x2000 + 40, 512);
    set32(&mut file, 0x2000 + 56, 0);
    let image = ElfImage::parse(&file).unwrap();
    assert_eq!(image.program_header_address(), Some(high));
}

fn linker_style_metadata() -> Vec<u8> {
    let original = fixture::hello();
    let mut file = original.clone();
    file[56..58].copy_from_slice(&6_u16.to_le_bytes());
    file[64..400].fill(0);
    // PT_PHDR, a read-only headers load, original code/data loads, GNU_STACK, GNU_EH_FRAME.
    file[176..232].copy_from_slice(&original[64..120]);
    file[232..288].copy_from_slice(&original[120..176]);
    for (at, kind, flags, offset, address, size, align) in [
        (64, 6, 4, 64, 0x400040, 336, 8),
        (120, 1, 4, 0, 0x400000, 400, 4096),
        (288, 0x6474e551, 6, 0, 0, 0, 16),
        (344, 0x6474e550, 4, 0x2000, fixture::DATA_ADDRESS, 8, 8),
    ] {
        set32(&mut file, at, kind);
        set32(&mut file, at + 4, flags);
        set64(&mut file, at + 8, offset);
        set64(&mut file, at + 16, address);
        set64(&mut file, at + 32, size);
        set64(&mut file, at + 40, size);
        set64(&mut file, at + 48, align);
    }
    file
}

#[test]
fn accepts_valid_linker_metadata_without_extra_load_mappings() {
    let image = ElfImage::parse(&linker_style_metadata()).unwrap();
    assert_eq!(image.segments().len(), 3);
    assert_eq!(image.program_header_address(), Some(0x400040));
    assert_eq!(image.program_headers().len(), 336);
}

#[test]
fn rejects_mismatched_metadata_duplicate_headers_and_executable_stack() {
    for (at, value) in [
        (64 + 8, 65),
        (64 + 32, 1),
        (344 + 16, fixture::DATA_ADDRESS + 4096),
    ] {
        let mut file = linker_style_metadata();
        set64(&mut file, at, value);
        assert_eq!(
            ElfImage::parse(&file).unwrap_err().kind,
            ErrorKind::InvalidElf
        );
    }
    let mut file = linker_style_metadata();
    let duplicate = file[64..120].to_vec();
    file[344..400].copy_from_slice(&duplicate);
    assert_eq!(
        ElfImage::parse(&file).unwrap_err().kind,
        ErrorKind::InvalidElf
    );
    let mut file = linker_style_metadata();
    set32(&mut file, 288 + 4, 7);
    assert_eq!(
        ElfImage::parse(&file).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    let mut file = linker_style_metadata();
    set64(&mut file, 288 + 40, 4096);
    assert_eq!(
        ElfImage::parse(&file).unwrap_err().kind,
        ErrorKind::Unsupported
    );
}
