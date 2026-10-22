//! Logical guest address space. All backing storage is ordinary Rust-owned memory.
#![forbid(unsafe_code)]

use crate::{Error, ErrorKind, Result, elf::ElfImage};
use std::{collections::BTreeMap, fmt};

pub const MAX_MAPPED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_REGIONS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permissions {
    pub const READ: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const READ_EXECUTE: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
    Execute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryFault {
    pub address: u64,
    pub access: Access,
}

impl fmt::Display for MemoryFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "guest {:?} fault at 0x{:x}",
            self.access, self.address
        )
    }
}
impl std::error::Error for MemoryFault {}

struct Region {
    bytes: Vec<u8>,
    permissions: Permissions,
}

fn zeroed(size: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| Error::new(ErrorKind::Runtime, "guest backing allocation failed"))?;
    bytes.resize(size, 0);
    Ok(bytes)
}

#[derive(Default)]
pub struct AddressSpace {
    regions: BTreeMap<u64, Region>,
    mapped_bytes: usize,
    peak_mapped_bytes: usize,
}

impl AddressSpace {
    pub fn from_image(image: &ElfImage) -> Result<Self> {
        let mut memory = Self::default();
        for segment in image.segments() {
            let bytes = image
                .read(segment.address, segment.memory_size)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidElf,
                        "validated image segment cannot be read",
                    )
                })?;
            let permissions = Permissions {
                read: segment.readable(),
                write: segment.flags & 2 != 0,
                execute: segment.executable(),
            };
            memory.map_bytes(segment.address, bytes, permissions)?;
        }
        Ok(memory)
    }

    fn check_mapping(&self, address: u64, size: usize, permissions: Permissions) -> Result<()> {
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, "guest mapping address overflow"))?;
        if size == 0
            || self.regions.len() >= MAX_REGIONS
            || size > MAX_MAPPED_BYTES - self.mapped_bytes
        {
            return Err(Error::new(
                ErrorKind::Runtime,
                "guest mapping resource limit exceeded",
            ));
        }
        if permissions.write && permissions.execute {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "writable executable guest mappings are rejected",
            ));
        }
        if let Some((&base, region)) = self.regions.range(..=address).next_back()
            && base + region.bytes.len() as u64 > address
        {
            return Err(Error::new(ErrorKind::Runtime, "overlapping guest mapping"));
        }
        if self.regions.range(address..end).next().is_some() {
            return Err(Error::new(ErrorKind::Runtime, "overlapping guest mapping"));
        }
        Ok(())
    }

    pub fn map_zeroed(
        &mut self,
        address: u64,
        size: usize,
        permissions: Permissions,
    ) -> Result<()> {
        self.check_mapping(address, size, permissions)?;
        self.regions.insert(
            address,
            Region {
                bytes: zeroed(size)?,
                permissions,
            },
        );
        self.mapped_bytes += size;
        self.peak_mapped_bytes = self.peak_mapped_bytes.max(self.mapped_bytes);
        Ok(())
    }

    pub fn map_bytes(
        &mut self,
        address: u64,
        bytes: &[u8],
        permissions: Permissions,
    ) -> Result<()> {
        self.check_mapping(address, bytes.len(), permissions)?;
        self.regions.insert(
            address,
            Region {
                bytes: {
                    let mut owned = Vec::new();
                    owned.try_reserve_exact(bytes.len()).map_err(|_| {
                        Error::new(ErrorKind::Runtime, "guest backing allocation failed")
                    })?;
                    owned.extend_from_slice(bytes);
                    owned
                },
                permissions,
            },
        );
        self.mapped_bytes += bytes.len();
        self.peak_mapped_bytes = self.peak_mapped_bytes.max(self.mapped_bytes);
        Ok(())
    }

    /// Validates the entire range first, so a later fault cannot partially mutate memory.
    pub fn validate(
        &self,
        address: u64,
        size: usize,
        access: Access,
    ) -> std::result::Result<(), MemoryFault> {
        let fault = |address| MemoryFault { address, access };
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| fault(address))?;
        let mut cursor = address;
        while cursor < end {
            let (&base, region) = self
                .regions
                .range(..=cursor)
                .next_back()
                .ok_or_else(|| fault(cursor))?;
            let region_end = base + region.bytes.len() as u64;
            let allowed = match access {
                Access::Read => region.permissions.read,
                Access::Write => region.permissions.write,
                Access::Execute => region.permissions.execute,
            };
            if cursor >= region_end || !allowed {
                return Err(fault(cursor));
            }
            cursor = end.min(region_end);
        }
        Ok(())
    }

    pub fn read_into(
        &self,
        address: u64,
        destination: &mut [u8],
    ) -> std::result::Result<(), MemoryFault> {
        self.validate(address, destination.len(), Access::Read)?;
        let mut copied = 0;
        while copied < destination.len() {
            // Validation establishes the containing region at every step.
            let cursor = address + copied as u64;
            let (&base, region) = self
                .regions
                .range(..=cursor)
                .next_back()
                .ok_or(MemoryFault {
                    address: cursor,
                    access: Access::Read,
                })?;
            let offset = (cursor - base) as usize;
            let count = (destination.len() - copied).min(region.bytes.len() - offset);
            destination[copied..copied + count]
                .copy_from_slice(&region.bytes[offset..offset + count]);
            copied += count;
        }
        Ok(())
    }

    pub fn write(&mut self, address: u64, source: &[u8]) -> std::result::Result<(), MemoryFault> {
        self.validate(address, source.len(), Access::Write)?;
        let mut copied = 0;
        while copied < source.len() {
            let cursor = address + copied as u64;
            let (&base, region) =
                self.regions
                    .range_mut(..=cursor)
                    .next_back()
                    .ok_or(MemoryFault {
                        address: cursor,
                        access: Access::Write,
                    })?;
            let offset = (cursor - base) as usize;
            let count = (source.len() - copied).min(region.bytes.len() - offset);
            region.bytes[offset..offset + count].copy_from_slice(&source[copied..copied + count]);
            copied += count;
        }
        Ok(())
    }

    pub fn mapped_bytes(&self) -> usize {
        self.mapped_bytes
    }

    /// High-water mark of logical guest backing, excluding host allocation overhead.
    pub fn peak_mapped_bytes(&self) -> usize {
        self.peak_mapped_bytes
    }

    pub fn region_size(&self, address: u64) -> Option<usize> {
        self.regions.get(&address).map(|region| region.bytes.len())
    }

    pub fn remove_region(&mut self, address: u64) -> Result<()> {
        let region = self
            .regions
            .remove(&address)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, "no guest mapping at base address"))?;
        self.mapped_bytes -= region.bytes.len();
        Ok(())
    }

    pub fn protect_region(
        &mut self,
        address: u64,
        size: usize,
        permissions: Permissions,
    ) -> Result<()> {
        if permissions.write && permissions.execute {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "writable executable guest mappings are rejected",
            ));
        }
        let region = self
            .regions
            .get_mut(&address)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, "no guest mapping at base address"))?;
        if size != region.bytes.len() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "protection changes require one complete guest mapping",
            ));
        }
        region.permissions = permissions;
        Ok(())
    }

    pub fn resize_region(&mut self, address: u64, size: usize) -> Result<()> {
        let old_size = self
            .region_size(address)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, "no guest mapping at base address"))?;
        if size == 0 {
            return self.remove_region(address);
        }
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, "guest resize address overflow"))?;
        if size > MAX_MAPPED_BYTES - (self.mapped_bytes - old_size) {
            return Err(Error::new(
                ErrorKind::Runtime,
                "guest resize memory limit exceeded",
            ));
        }
        if let Some((&next, _)) = self
            .regions
            .range((
                std::ops::Bound::Excluded(address),
                std::ops::Bound::Unbounded,
            ))
            .next()
            && end > next
        {
            return Err(Error::new(
                ErrorKind::Runtime,
                "guest resize overlaps another mapping",
            ));
        }
        let region = self
            .regions
            .get_mut(&address)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, "guest resize mapping disappeared"))?;
        // Replace backing storage so shrinking does not keep a large Vec capacity resident.
        let mut bytes = zeroed(size)?;
        let preserved = old_size.min(size);
        bytes[..preserved].copy_from_slice(&region.bytes[..preserved]);
        region.bytes = bytes;
        self.mapped_bytes = self.mapped_bytes - old_size + size;
        self.peak_mapped_bytes = self.peak_mapped_bytes.max(self.mapped_bytes);
        Ok(())
    }

    pub fn find_free(&self, start: u64, size: usize, alignment: u64, limit: u64) -> Option<u64> {
        if size == 0 || !alignment.is_power_of_two() {
            return None;
        }
        let aligned = |value: u64| {
            value
                .checked_add(alignment - 1)
                .map(|value| value & !(alignment - 1))
        };
        let mut candidate = aligned(start)?;
        for (&base, region) in &self.regions {
            let end = candidate.checked_add(size as u64)?;
            if end > limit {
                return None;
            }
            if end <= base {
                return Some(candidate);
            }
            let region_end = base + region.bytes.len() as u64;
            if candidate < region_end {
                candidate = aligned(region_end)?;
            }
        }
        if candidate.checked_add(size as u64)? <= limit {
            Some(candidate)
        } else {
            None
        }
    }
}
