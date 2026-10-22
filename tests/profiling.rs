use emucore::{
    elf::ElfImage,
    fixture,
    memory::{AddressSpace, Permissions},
    process::ProcessOptions,
    runtime::{self, Backend},
};

#[test]
fn high_water_mark_retains_released_and_resized_mapping_history() {
    let mut memory = AddressSpace::default();
    assert_eq!(memory.peak_mapped_bytes(), 0);
    memory
        .map_zeroed(0x1000, 16, Permissions::READ_WRITE)
        .unwrap();
    memory
        .map_bytes(0x2000, &[1; 24], Permissions::READ)
        .unwrap();
    assert_eq!(memory.peak_mapped_bytes(), 40);
    memory.resize_region(0x1000, 32).unwrap();
    assert_eq!(memory.peak_mapped_bytes(), 56);
    memory.resize_region(0x1000, 8).unwrap();
    memory.remove_region(0x2000).unwrap();
    assert_eq!(memory.mapped_bytes(), 8);
    assert_eq!(memory.peak_mapped_bytes(), 56);
    assert!(memory.map_zeroed(0x1000, 128, Permissions::READ).is_err());
    assert_eq!(memory.peak_mapped_bytes(), 56);
}

#[test]
fn reference_profile_keeps_timing_out_of_observable_guest_state() {
    let image = ElfImage::parse(&fixture::hello()).unwrap();
    let profile =
        runtime::run_profiled(&image, Backend::Reference, &ProcessOptions::default()).unwrap();
    let normal = runtime::run_with_backend(&image, Backend::Reference).unwrap();
    assert_eq!(profile.result, normal);
    assert_eq!(profile.metrics.native_code_bytes, 0);
    assert!(profile.metrics.translation.is_zero());
    assert!(profile.metrics.executable_setup.is_zero());
    assert_eq!(
        profile.metrics.decoded_instructions as u64,
        normal.instructions_executed
    );
    assert_eq!(
        profile.metrics.guest_mapped_bytes_final,
        profile.metrics.guest_mapped_bytes_peak
    );
    let stages = profile.metrics.decode + profile.metrics.process_setup + profile.metrics.execution;
    assert!(profile.metrics.total >= stages);
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn native_profile_measures_generated_code_and_preserves_results() {
    let image = ElfImage::parse(&fixture::hello()).unwrap();
    let profile =
        runtime::run_profiled(&image, Backend::Native, &ProcessOptions::default()).unwrap();
    let reference = runtime::run_with_backend(&image, Backend::Reference).unwrap();
    assert_eq!(profile.result, reference);
    assert!(profile.metrics.native_code_bytes > 0);
    let stages = profile.metrics.decode
        + profile.metrics.process_setup
        + profile.metrics.translation
        + profile.metrics.executable_setup
        + profile.metrics.execution;
    assert!(profile.metrics.total >= stages);
}
