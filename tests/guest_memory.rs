use emucore::memory::{Access, AddressSpace, MAX_MAPPED_BYTES, MAX_REGIONS, Permissions};

#[test]
fn reads_and_writes_across_adjacent_regions() {
    let mut memory = AddressSpace::default();
    memory
        .map_bytes(0x1000, b"abcd", Permissions::READ_WRITE)
        .unwrap();
    memory
        .map_bytes(0x1004, b"efgh", Permissions::READ_WRITE)
        .unwrap();
    memory.write(0x1002, b"WXYZ").unwrap();
    let mut bytes = [0; 8];
    memory.read_into(0x1000, &mut bytes).unwrap();
    assert_eq!(&bytes, b"abWXYZgh");
    assert_eq!(memory.mapped_bytes(), 8);
}

#[test]
fn a_later_permission_fault_never_partially_writes() {
    let mut memory = AddressSpace::default();
    memory
        .map_bytes(0x1000, b"abcd", Permissions::READ_WRITE)
        .unwrap();
    memory
        .map_bytes(0x1004, b"efgh", Permissions::READ)
        .unwrap();
    let fault = memory.write(0x1002, b"WXYZ").unwrap_err();
    assert_eq!(fault.address, 0x1004);
    assert_eq!(fault.access, Access::Write);
    let mut bytes = [0; 8];
    memory.read_into(0x1000, &mut bytes).unwrap();
    assert_eq!(&bytes, b"abcdefgh");
}

#[test]
fn validates_gaps_overflow_and_execute_permissions() {
    let mut memory = AddressSpace::default();
    memory
        .map_zeroed(0x1000, 4, Permissions::READ_EXECUTE)
        .unwrap();
    memory
        .map_zeroed(0x1008, 4, Permissions::READ_WRITE)
        .unwrap();
    assert!(memory.validate(0x1000, 4, Access::Execute).is_ok());
    assert_eq!(
        memory
            .validate(0x1008, 1, Access::Execute)
            .unwrap_err()
            .address,
        0x1008
    );
    assert_eq!(
        memory
            .validate(0x1002, 8, Access::Read)
            .unwrap_err()
            .address,
        0x1004
    );
    assert!(memory.validate(u64::MAX, 2, Access::Read).is_err());
    assert!(memory.read_into(u64::MAX, &mut []).is_ok());
    assert!(memory.write(u64::MAX, &[]).is_ok());
    assert!(memory.map_zeroed(u64::MAX, 1, Permissions::READ).is_err());
    assert!(memory.map_zeroed(0x1002, 1, Permissions::READ).is_err());
    assert!(memory.map_zeroed(0x0fff, 10, Permissions::READ).is_err());
}

#[test]
fn rejects_writable_executable_mappings_and_caps_regions() {
    let mut memory = AddressSpace::default();
    assert!(
        memory
            .map_zeroed(
                0,
                1,
                Permissions {
                    read: true,
                    write: true,
                    execute: true
                }
            )
            .is_err()
    );
    for index in 0..MAX_REGIONS {
        memory
            .map_zeroed(index as u64 * 2, 1, Permissions::READ)
            .unwrap();
    }
    assert!(memory.map_zeroed(0x10000, 1, Permissions::READ).is_err());
    assert_eq!(memory.mapped_bytes(), MAX_REGIONS);
}

#[test]
fn rejects_oversized_mapping_before_allocating() {
    let mut memory = AddressSpace::default();
    assert!(
        memory
            .map_zeroed(0, MAX_MAPPED_BYTES + 1, Permissions::READ)
            .is_err()
    );
    assert!(memory.map_zeroed(0, 0, Permissions::READ).is_err());
    assert_eq!(memory.mapped_bytes(), 0);
}
