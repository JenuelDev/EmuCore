//! An explicit anonymous-data/heap syscall subset. No guest address is a host pointer.
#![forbid(unsafe_code)]

use crate::{
    Error, ErrorKind, Result,
    elf::ElfImage,
    memory::{AddressSpace, MAX_MAPPED_BYTES, Permissions},
};
use std::collections::BTreeMap;

pub const PAGE_SIZE: u64 = 4096;
const MMAP_BASE: u64 = 0x1_0000_0000;
const MMAP_LIMIT: u64 = 0x7000_0000_0000;
const EINVAL: i64 = 22;
const ENOMEM: i64 = 12;
const EACCES: i64 = 13;
const EOPNOTSUPP: i64 = 95;

fn errno(number: i64) -> u64 {
    (-number) as u64
}
fn page_end(value: u64) -> Option<u64> {
    value
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
}
fn mapping_size(length: u64) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let size = page_end(length)?;
    if size > MAX_MAPPED_BYTES as u64 {
        return None;
    }
    usize::try_from(size).ok()
}

fn permissions(protection: u64) -> std::result::Result<Permissions, u64> {
    match protection {
        0 => Ok(Permissions {
            read: false,
            write: false,
            execute: false,
        }),
        1 => Ok(Permissions::READ),
        3 => Ok(Permissions::READ_WRITE),
        value if value & 4 != 0 => Err(errno(EACCES)),
        _ => Err(errno(EOPNOTSUPP)),
    }
}

pub struct MemoryManager {
    anonymous: BTreeMap<u64, usize>,
    next_mapping: u64,
    heap_base: u64,
    program_break: u64,
    heap_backing: usize,
}

impl MemoryManager {
    pub fn new(image: &ElfImage) -> Result<Self> {
        let image_end = image
            .segments()
            .iter()
            .map(|segment| segment.address + segment.memory_size)
            .max()
            .ok_or_else(|| Error::new(ErrorKind::InvalidElf, "missing image segments"))?;
        let heap_base = page_end(image_end).ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "image leaves no representable program break",
            )
        })?;
        Ok(Self {
            anonymous: BTreeMap::new(),
            next_mapping: MMAP_BASE,
            heap_base,
            program_break: heap_base,
            heap_backing: 0,
        })
    }

    pub fn mmap(&mut self, memory: &mut AddressSpace, arguments: [u64; 6]) -> u64 {
        let [address, length, protection, flags, descriptor, offset] = arguments;
        if length == 0 {
            return errno(EINVAL);
        }
        if address != 0 || flags != 0x22 {
            return errno(EOPNOTSUPP);
        }
        if descriptor as u32 != u32::MAX || offset != 0 {
            return errno(EINVAL);
        }
        let permissions = match permissions(protection) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let size = match mapping_size(length) {
            Some(size) => size,
            None => return errno(ENOMEM),
        };
        let address = match memory.find_free(self.next_mapping, size, PAGE_SIZE, MMAP_LIMIT) {
            Some(address) => address,
            None => return errno(ENOMEM),
        };
        if memory.map_zeroed(address, size, permissions).is_err() {
            return errno(ENOMEM);
        }
        self.anonymous.insert(address, size);
        self.next_mapping = address + size as u64;
        address
    }

    fn complete_mapping(&self, address: u64, length: u64) -> std::result::Result<usize, u64> {
        if !address.is_multiple_of(PAGE_SIZE) || length == 0 {
            return Err(errno(EINVAL));
        }
        let size = mapping_size(length).ok_or_else(|| errno(EINVAL))?;
        if self.anonymous.get(&address).copied() == Some(size) {
            return Ok(size);
        }
        if let Some((&base, &existing)) = self.anonymous.range(..=address).next_back()
            && address < base + existing as u64
        {
            return Err(errno(EOPNOTSUPP));
        }
        Err(errno(EINVAL))
    }

    pub fn mprotect(
        &mut self,
        memory: &mut AddressSpace,
        address: u64,
        length: u64,
        protection: u64,
    ) -> u64 {
        let permissions = match permissions(protection) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let size = match self.complete_mapping(address, length) {
            Ok(value) => value,
            Err(error) => return error,
        };
        if memory.protect_region(address, size, permissions).is_err() {
            return errno(ENOMEM);
        }
        0
    }

    pub fn munmap(&mut self, memory: &mut AddressSpace, address: u64, length: u64) -> u64 {
        if let Err(error) = self.complete_mapping(address, length) {
            return error;
        }
        if memory.remove_region(address).is_err() {
            return errno(EINVAL);
        }
        self.anonymous.remove(&address);
        0
    }

    /// Raw Linux brk convention: return the actual break, including on failure.
    pub fn brk(&mut self, memory: &mut AddressSpace, requested: u64) -> u64 {
        if requested == 0 || requested < self.heap_base {
            return self.program_break;
        }
        let relative = requested - self.heap_base;
        let rounded = match page_end(relative) {
            Some(value) => value,
            None => return self.program_break,
        };
        if rounded > MAX_MAPPED_BYTES as u64 {
            return self.program_break;
        }
        let size = rounded as usize;
        let result = if size == self.heap_backing {
            Ok(())
        } else if self.heap_backing == 0 {
            memory.map_zeroed(self.heap_base, size, Permissions::READ_WRITE)
        } else {
            memory.resize_region(self.heap_base, size)
        };
        if result.is_ok() {
            self.heap_backing = size;
            self.program_break = requested;
        }
        self.program_break
    }

    pub fn program_break(&self) -> u64 {
        self.program_break
    }
    pub fn anonymous_count(&self) -> usize {
        self.anonymous.len()
    }
}
