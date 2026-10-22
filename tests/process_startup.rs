use emucore::{
    elf::ElfImage,
    fixture,
    memory::{Access, AddressSpace, Permissions},
    process::{
        self, MAX_ARGUMENTS, MAX_ENVIRONMENT, MAX_STRINGS_BYTES, METADATA_BASE, ProcessOptions,
        STACK_BASE, STACK_SIZE,
    },
};
use std::collections::BTreeMap;

fn word(memory: &AddressSpace, address: u64) -> u64 {
    let mut bytes = [0; 8];
    memory.read_into(address, &mut bytes).unwrap();
    u64::from_le_bytes(bytes)
}

fn string(memory: &AddressSpace, address: u64) -> Vec<u8> {
    let mut output = Vec::new();
    for offset in 0..256 {
        let mut byte = [0];
        memory.read_into(address + offset, &mut byte).unwrap();
        if byte[0] == 0 {
            return output;
        }
        output.push(byte[0]);
    }
    panic!("test string missing terminator");
}

#[test]
fn builds_aligned_arguments_environment_and_auxiliary_metadata() {
    let image = ElfImage::parse(&fixture::hello()).unwrap();
    let mut memory = AddressSpace::from_image(&image).unwrap();
    let options = ProcessOptions {
        arguments: vec!["owned-program".into(), "hello world".into(), "".into()],
        environment: vec!["LANG=C".into(), "EMPTY=".into()],
    };
    let state = process::initialize(&mut memory, &image, &options, [0xa5; 16]).unwrap();
    assert_eq!(state.stack_pointer % 16, 0);
    assert_eq!(state.argument_count, 3);
    assert_eq!(word(&memory, state.stack_pointer), 3);
    for (index, argument) in options.arguments.iter().enumerate() {
        assert_eq!(
            string(
                &memory,
                word(&memory, state.stack_pointer + 8 + index as u64 * 8)
            ),
            argument.as_bytes()
        );
    }
    let mut cursor = state.stack_pointer + 8 + 3 * 8;
    assert_eq!(word(&memory, cursor), 0);
    cursor += 8;
    for variable in &options.environment {
        assert_eq!(string(&memory, word(&memory, cursor)), variable.as_bytes());
        cursor += 8;
    }
    assert_eq!(word(&memory, cursor), 0);
    cursor += 8;
    let mut auxiliary = BTreeMap::new();
    loop {
        let kind = word(&memory, cursor);
        let value = word(&memory, cursor + 8);
        cursor += 16;
        if kind == 0 {
            assert_eq!(value, 0);
            break;
        }
        assert!(auxiliary.insert(kind, value).is_none());
    }
    assert_eq!(auxiliary[&3], METADATA_BASE);
    assert_eq!(auxiliary[&4], 56);
    assert_eq!(auxiliary[&5], 2);
    assert_eq!(auxiliary[&6], 4096);
    assert_eq!(auxiliary[&7], 0);
    assert_eq!(auxiliary[&9], image.entry());
    assert_eq!(auxiliary[&11], 1000);
    assert_eq!(string(&memory, auxiliary[&15]), b"x86_64");
    assert_eq!(string(&memory, auxiliary[&31]), b"owned-program");
    let mut random = [0; 16];
    memory.read_into(auxiliary[&25], &mut random).unwrap();
    assert_eq!(random, [0xa5; 16]);
    let mut headers = vec![0; image.program_headers().len()];
    memory.read_into(auxiliary[&3], &mut headers).unwrap();
    assert_eq!(headers, image.program_headers());
    assert!(memory.write(METADATA_BASE, &[0]).is_err());
    assert!(
        memory
            .validate(STACK_BASE, STACK_SIZE, Access::Execute)
            .is_err()
    );
    assert!(memory.validate(STACK_BASE - 1, 1, Access::Read).is_err());
    assert!(
        memory
            .validate(STACK_BASE + STACK_SIZE as u64, 1, Access::Read)
            .is_err()
    );
}

#[test]
fn rejects_invalid_startup_options_before_mapping() {
    let image = ElfImage::parse(&fixture::hello()).unwrap();
    let cases = [
        ProcessOptions {
            arguments: vec![],
            environment: vec![],
        },
        ProcessOptions {
            arguments: vec!["x".into(); MAX_ARGUMENTS + 1],
            environment: vec![],
        },
        ProcessOptions {
            arguments: vec!["x".repeat(MAX_STRINGS_BYTES)],
            environment: vec![],
        },
        ProcessOptions {
            arguments: vec!["x\0y".into()],
            environment: vec![],
        },
        ProcessOptions {
            arguments: vec!["x".into()],
            environment: vec!["X=1".into(); MAX_ENVIRONMENT + 1],
        },
        ProcessOptions {
            arguments: vec!["x".into()],
            environment: vec!["=bad".into()],
        },
        ProcessOptions {
            arguments: vec!["x".into()],
            environment: vec!["missing-equals".into()],
        },
    ];
    for options in cases {
        let mut memory = AddressSpace::from_image(&image).unwrap();
        let before = memory.mapped_bytes();
        assert!(process::initialize(&mut memory, &image, &options, [0; 16]).is_err());
        assert_eq!(memory.mapped_bytes(), before);
    }
    assert!(ProcessOptions::default().environment.is_empty());
}

#[test]
fn reuses_a_program_header_table_already_in_a_load_segment() {
    let mut file = fixture::hello();
    for (at, value) in [(72, 0), (80, 0x400000), (96, 0x1021), (104, 0x1021)] {
        file[at..at + 8].copy_from_slice(&(value as u64).to_le_bytes());
    }
    let image = ElfImage::parse(&file).unwrap();
    assert_eq!(image.program_header_address(), Some(0x400040));
    let mut memory = AddressSpace::from_image(&image).unwrap();
    process::initialize(&mut memory, &image, &ProcessOptions::default(), [0; 16]).unwrap();
    assert!(memory.validate(METADATA_BASE, 1, Access::Read).is_err());
    let mut headers = vec![0; 112];
    memory.read_into(0x400040, &mut headers).unwrap();
    assert_eq!(headers, image.program_headers());
}

#[test]
fn reserved_stack_mapping_collisions_are_rejected() {
    let image = ElfImage::parse(&fixture::hello()).unwrap();
    let mut memory = AddressSpace::from_image(&image).unwrap();
    memory.map_zeroed(STACK_BASE, 1, Permissions::READ).unwrap();
    assert!(process::initialize(&mut memory, &image, &ProcessOptions::default(), [0; 16]).is_err());
}
