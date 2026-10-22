use emucore::{
    elf::ElfImage,
    fixture,
    memory::{AddressSpace, MAX_MAPPED_BYTES, MAX_REGIONS, Permissions},
    vm::{MemoryManager, PAGE_SIZE},
};

fn setup() -> (AddressSpace, MemoryManager) {
    let image = ElfImage::parse(&fixture::hello()).unwrap();
    (
        AddressSpace::from_image(&image).unwrap(),
        MemoryManager::new(&image).unwrap(),
    )
}
fn anonymous(length: u64, protection: u64) -> [u64; 6] {
    [0, length, protection, 0x22, u64::MAX, 0]
}
fn errno(number: i64) -> u64 {
    (-number) as u64
}

#[test]
fn anonymous_pages_are_zeroed_protected_and_released() {
    let (mut memory, mut manager) = setup();
    let before = memory.mapped_bytes();
    let address = manager.mmap(&mut memory, anonymous(1, 3));
    assert_eq!(address % PAGE_SIZE, 0);
    assert_eq!(manager.anonymous_count(), 1);
    assert_eq!(memory.mapped_bytes(), before + PAGE_SIZE as usize);
    let mut bytes = [0xff; 8];
    memory.read_into(address, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 8]);
    memory.write(address, b"owned").unwrap();
    assert_eq!(manager.mprotect(&mut memory, address, 1, 1), 0);
    assert!(memory.write(address, b"x").is_err());
    let mut content = [0; 5];
    memory.read_into(address, &mut content).unwrap();
    assert_eq!(&content, b"owned");
    assert_eq!(manager.mprotect(&mut memory, address, PAGE_SIZE, 0), 0);
    assert!(memory.read_into(address, &mut content).is_err());
    assert_eq!(manager.mprotect(&mut memory, address, PAGE_SIZE, 3), 0);
    assert_eq!(manager.munmap(&mut memory, address, 1), 0);
    assert_eq!(memory.mapped_bytes(), before);
    assert_eq!(manager.anonymous_count(), 0);
    assert!(memory.read_into(address, &mut content).is_err());
}

#[test]
fn invalid_or_unsupported_mapping_requests_preserve_state() {
    let (mut memory, mut manager) = setup();
    let before = memory.mapped_bytes();
    let mut cases = vec![
        (anonymous(0, 3), errno(22)),
        (anonymous(u64::MAX, 3), errno(12)),
        (anonymous(MAX_MAPPED_BYTES as u64 + 1, 3), errno(12)),
        (anonymous(1, 7), errno(13)),
        (anonymous(1, 2), errno(95)),
    ];
    for (index, value, expected) in [(0, 0x1000, 95), (3, 2, 95), (4, 0, 22), (5, PAGE_SIZE, 22)] {
        let mut arguments = anonymous(PAGE_SIZE, 3);
        arguments[index] = value;
        cases.push((arguments, errno(expected)));
    }
    for (arguments, expected) in cases {
        assert_eq!(manager.mmap(&mut memory, arguments), expected);
        assert_eq!(memory.mapped_bytes(), before);
        assert_eq!(manager.anonymous_count(), 0);
    }
    let mut arguments = anonymous(PAGE_SIZE, 3);
    arguments[4] = u32::MAX as u64; // fd is a 32-bit integer, including with zero-extended argument bits.
    assert!(manager.mmap(&mut memory, arguments) < i64::MAX as u64);
}

#[test]
fn partial_range_operations_are_explicitly_unsupported_and_atomic() {
    let (mut memory, mut manager) = setup();
    let address = manager.mmap(&mut memory, anonymous(PAGE_SIZE + 1, 3));
    memory.write(address + PAGE_SIZE, b"persist").unwrap();
    assert_eq!(
        manager.mprotect(&mut memory, address, PAGE_SIZE, 1),
        errno(95)
    );
    assert_eq!(
        manager.munmap(&mut memory, address + PAGE_SIZE, PAGE_SIZE),
        errno(95)
    );
    assert_eq!(
        manager.munmap(&mut memory, address + 1, PAGE_SIZE),
        errno(22)
    );
    memory.write(address, b"writable").unwrap();
    let mut bytes = [0; 7];
    memory.read_into(address + PAGE_SIZE, &mut bytes).unwrap();
    assert_eq!(&bytes, b"persist");
    assert_eq!(manager.munmap(&mut memory, address, PAGE_SIZE + 1), 0);
}

#[test]
fn region_limit_failure_is_recoverable_after_release() {
    let (mut memory, mut manager) = setup();
    let mut first = 0;
    for index in 0..MAX_REGIONS - 2 {
        let address = manager.mmap(&mut memory, anonymous(1, 3));
        assert!(address < i64::MAX as u64);
        if index == 0 {
            first = address;
        }
    }
    let before = memory.mapped_bytes();
    assert_eq!(manager.mmap(&mut memory, anonymous(1, 3)), errno(12));
    assert_eq!(memory.mapped_bytes(), before);
    assert_eq!(manager.munmap(&mut memory, first, 1), 0);
    assert!(manager.mmap(&mut memory, anonymous(1, 3)) < i64::MAX as u64);
}

#[test]
fn raw_brk_grows_shrinks_preserves_data_and_zeroes_regrown_pages() {
    let (mut memory, mut manager) = setup();
    let base = manager.brk(&mut memory, 0);
    assert_eq!(base % PAGE_SIZE, 0);
    assert_eq!(
        manager.brk(&mut memory, base + PAGE_SIZE + 16),
        base + PAGE_SIZE + 16
    );
    memory.write(base, b"keep").unwrap();
    memory.write(base + 200, b"tail").unwrap();
    memory.write(base + PAGE_SIZE, b"discard").unwrap();
    assert_eq!(manager.brk(&mut memory, base + 100), base + 100);
    let mut tail = [0; 4];
    memory.read_into(base + 200, &mut tail).unwrap();
    assert_eq!(&tail, b"tail"); // The retained partial page remains backed beyond the logical break.
    assert!(memory.read_into(base + PAGE_SIZE, &mut [0]).is_err());
    assert_eq!(
        manager.brk(&mut memory, base + PAGE_SIZE + 16),
        base + PAGE_SIZE + 16
    );
    let mut bytes = [0xff; 7];
    memory.read_into(base + PAGE_SIZE, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 7]);
    let mut kept = [0; 4];
    memory.read_into(base, &mut kept).unwrap();
    assert_eq!(&kept, b"keep");
    let current = manager.program_break();
    assert_eq!(manager.brk(&mut memory, u64::MAX), current);
    assert_eq!(manager.brk(&mut memory, base - 1), current);
    assert_eq!(manager.brk(&mut memory, base), base);
    assert!(memory.read_into(base, &mut kept).is_err());
}

#[test]
fn failed_heap_growth_does_not_change_the_break_or_existing_mappings() {
    let (mut memory, mut manager) = setup();
    let base = manager.program_break();
    memory
        .map_bytes(base + PAGE_SIZE, b"neighbor", Permissions::READ)
        .unwrap();
    let before = memory.mapped_bytes();
    assert_eq!(manager.brk(&mut memory, base + PAGE_SIZE + 1), base);
    assert_eq!(manager.program_break(), base);
    assert_eq!(memory.mapped_bytes(), before);
    assert_eq!(manager.brk(&mut memory, base + 1), base + 1);
    assert_eq!(manager.brk(&mut memory, base), base);
    let mut bytes = [0; 8];
    memory.read_into(base + PAGE_SIZE, &mut bytes).unwrap();
    assert_eq!(&bytes, b"neighbor");
}
