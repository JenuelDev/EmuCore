#![forbid(unsafe_code)]

use crate::{Error, ErrorKind, Result};
use std::{fs::File, io::Read, path::Path};

pub const MAX_FILE_SIZE: usize = 16 * 1024 * 1024;
const MAX_IMAGE_SIZE: u64 = 16 * 1024 * 1024;
const MAX_SEGMENTS: u16 = 16;
const PAGE_SIZE: u64 = 4096;

#[derive(Clone, Debug)]
pub struct Segment {
    pub address: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub flags: u32,
    file_offset: usize,
}

impl Segment {
    pub fn readable(&self) -> bool {
        self.flags & 4 != 0
    }
    pub fn executable(&self) -> bool {
        self.flags & 1 != 0
    }
}

#[derive(Debug)]
pub struct ElfImage {
    entry: u64,
    base: u64,
    memory: Vec<u8>,
    segments: Vec<Segment>,
    program_headers: Vec<u8>,
    program_header_address: Option<u64>,
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidElf, message)
}
fn unsupported(message: &str) -> Error {
    Error::new(ErrorKind::Unsupported, message)
}

fn bytes<const N: usize>(file: &[u8], at: usize) -> Result<[u8; N]> {
    let end = at
        .checked_add(N)
        .ok_or_else(|| invalid("file offset overflow"))?;
    file.get(at..end)
        .ok_or_else(|| invalid("truncated ELF structure"))?
        .try_into()
        .map_err(|_| invalid("invalid field width"))
}

fn u16_at(file: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(file, at)?))
}
fn u32_at(file: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(file, at)?))
}
fn u64_at(file: &[u8], at: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(bytes(file, at)?))
}

impl ElfImage {
    /// Reads at most the configured file limit plus one byte, including for growing files.
    pub fn from_path(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| Error::io("open ELF", e))?;
        let mut data = Vec::new();
        file.take((MAX_FILE_SIZE + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|e| Error::io("read ELF", e))?;
        Self::parse(&data)
    }

    /// Accepts only a bounded static, little-endian x86-64 ET_EXEC subset.
    pub fn parse(file: &[u8]) -> Result<Self> {
        if file.len() < 64 || file.len() > MAX_FILE_SIZE {
            return Err(invalid("ELF file size outside limits"));
        }
        if &file[..4] != b"\x7fELF" {
            return Err(invalid("invalid ELF magic"));
        }
        if file[4] != 2 || file[5] != 1 {
            return Err(unsupported("requires ELF64 little-endian"));
        }
        if file[6] != 1 || u32_at(file, 20)? != 1 {
            return Err(invalid("invalid ELF version"));
        }
        if !matches!(file[7], 0 | 3) || file[8] != 0 {
            return Err(unsupported("unsupported ELF OS ABI or ABI version"));
        }
        if u16_at(file, 16)? != 2 || u16_at(file, 18)? != 62 {
            return Err(unsupported("requires static x86-64 ET_EXEC"));
        }
        if u32_at(file, 48)? != 0 {
            return Err(unsupported("unsupported ELF processor flags"));
        }
        if u16_at(file, 52)? != 64 || u16_at(file, 54)? != 56 {
            return Err(invalid("invalid ELF or program header size"));
        }
        let count = u16_at(file, 56)?;
        if count == 0 || count > MAX_SEGMENTS {
            return Err(invalid("program header count outside limits"));
        }
        let phoff = u64_at(file, 32)?;
        let phend = phoff
            .checked_add(u64::from(count) * 56)
            .ok_or_else(|| invalid("program header range overflow"))?;
        if phoff < 64 || phend > file.len() as u64 {
            return Err(invalid("program header table outside file"));
        }
        let mut segments = Vec::new();
        let mut metadata = Vec::new();
        let mut stack_header = false;
        for index in 0..count {
            let at = phoff as usize + usize::from(index) * 56;
            let kind = u32_at(file, at)?;
            match kind {
                0 => continue,
                1 | 6 | 0x6474_e550 => (),
                0x6474_e551 => {
                    if stack_header {
                        return Err(invalid("duplicate GNU stack header"));
                    }
                    stack_header = true;
                    if u32_at(file, at + 4)? & !6 != 0 {
                        return Err(unsupported(
                            "executable or unknown GNU stack permissions are rejected",
                        ));
                    }
                    if u64_at(file, at + 32)? != 0 || u64_at(file, at + 40)? != 0 {
                        return Err(unsupported("GNU stack size hints are not implemented"));
                    }
                    continue;
                }
                _ => {
                    return Err(unsupported(
                        "unsupported ELF program header; interpreter, dynamic linking, and TLS are not implemented",
                    ));
                }
            }
            let flags = u32_at(file, at + 4)?;
            let offset = u64_at(file, at + 8)?;
            let address = u64_at(file, at + 16)?;
            let file_size = u64_at(file, at + 32)?;
            let memory_size = u64_at(file, at + 40)?;
            let align = u64_at(file, at + 48)?;
            if memory_size == 0 || file_size > memory_size {
                return Err(invalid("invalid segment file or memory size"));
            }
            let file_end = offset
                .checked_add(file_size)
                .ok_or_else(|| invalid("segment file range overflow"))?;
            address
                .checked_add(memory_size)
                .ok_or_else(|| invalid("segment virtual address overflow"))?;
            if file_end > file.len() as u64 {
                return Err(invalid("segment extends beyond file"));
            }
            if flags & !7 != 0 || flags & 4 == 0 {
                return Err(unsupported(
                    "segments must be readable with known permission flags",
                ));
            }
            if flags & 3 == 3 {
                return Err(unsupported("writable executable segments are rejected"));
            }
            if (align > 1 && (!align.is_power_of_two() || address % align != offset % align))
                || address % PAGE_SIZE != offset % PAGE_SIZE
            {
                return Err(invalid("invalid segment alignment"));
            }
            let segment = Segment {
                address,
                file_size,
                memory_size,
                flags,
                file_offset: offset as usize,
            };
            if kind == 1 {
                segments.push(segment);
            } else {
                if flags != 4 {
                    return Err(invalid("ELF metadata must have read-only permissions"));
                }
                if metadata
                    .iter()
                    .any(|(previous, _): &(u32, Segment)| *previous == kind)
                {
                    return Err(invalid("duplicate ELF metadata header"));
                }
                if kind == 6
                    && (offset != phoff
                        || file_size != u64::from(count) * 56
                        || memory_size != file_size)
                {
                    return Err(invalid(
                        "PT_PHDR does not describe the program-header table",
                    ));
                }
                metadata.push((kind, segment));
            }
        }
        segments.sort_by_key(|segment| segment.address);
        let first = segments
            .first()
            .ok_or_else(|| invalid("no loadable segments"))?;
        let base = first.address;
        let mut end = base;
        for segment in &segments {
            if segment.address < end {
                return Err(invalid("overlapping loadable segments"));
            }
            end = segment.address + segment.memory_size;
        }
        for (_, item) in &metadata {
            let covered = segments.iter().any(|load| {
                let offset = load.file_offset as u64;
                item.file_offset >= load.file_offset
                    && (item.file_offset as u64 + item.file_size) <= offset + load.file_size
                    && load.address + (item.file_offset as u64 - offset) == item.address
                    && item.memory_size == item.file_size
            });
            if !covered {
                return Err(invalid(
                    "ELF metadata must match a file-backed load segment",
                ));
            }
        }
        let span = end - base;
        if span > MAX_IMAGE_SIZE {
            return Err(invalid("guest image exceeds memory limit"));
        }
        let entry = u64_at(file, 24)?;
        if !segments
            .iter()
            .any(|s| s.executable() && entry >= s.address && entry - s.address < s.file_size)
        {
            return Err(invalid("entry must be in file-backed executable memory"));
        }
        let mut memory = vec![0; span as usize];
        let program_header_address = segments.iter().find_map(|segment| {
            let offset = segment.file_offset as u64;
            if phoff >= offset && phend <= offset + segment.file_size {
                Some(segment.address + (phoff - offset))
            } else {
                None
            }
        });
        for segment in &segments {
            let start = (segment.address - base) as usize;
            let size = segment.file_size as usize;
            memory[start..start + size]
                .copy_from_slice(&file[segment.file_offset..segment.file_offset + size]);
        }
        Ok(Self {
            entry,
            base,
            memory,
            segments,
            program_headers: file[phoff as usize..phend as usize].to_vec(),
            program_header_address,
        })
    }

    pub fn entry(&self) -> u64 {
        self.entry
    }
    pub fn image_size(&self) -> usize {
        self.memory.len()
    }
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn program_headers(&self) -> &[u8] {
        &self.program_headers
    }
    pub fn program_header_address(&self) -> Option<u64> {
        self.program_header_address
    }

    /// Checks guest permissions and bounds, never treating a guest address as a host pointer.
    pub fn read(&self, address: u64, count: u64) -> Option<&[u8]> {
        let end = address.checked_add(count)?;
        self.segments
            .iter()
            .find(|s| s.readable() && address >= s.address && end <= s.address + s.memory_size)?;
        let start = usize::try_from(address.checked_sub(self.base)?).ok()?;
        self.memory
            .get(start..start.checked_add(usize::try_from(count).ok()?)?)
    }

    pub fn code_at(&self, address: u64) -> Option<&[u8]> {
        let segment = self.segments.iter().find(|segment| {
            segment.executable()
                && address >= segment.address
                && address - segment.address < segment.file_size
        })?;
        let start = (address - self.base) as usize;
        let end = (segment.address - self.base + segment.file_size) as usize;
        self.memory.get(start..end)
    }

    pub fn entry_code(&self) -> &[u8] {
        // parse establishes this invariant, and neither metadata nor memory is publicly mutable.
        for segment in &self.segments {
            if segment.executable()
                && self.entry >= segment.address
                && self.entry - segment.address < segment.file_size
            {
                let start = (self.entry - self.base) as usize;
                let end = (segment.address - self.base + segment.file_size) as usize;
                return &self.memory[start..end];
            }
        }
        unreachable!("validated entry segment must exist")
    }
}
