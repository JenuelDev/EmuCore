//! Guest registers are independent from native host register allocation.
#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Register {
    Rax,
    Rcx,
    Rdx,
    Rbx,
    Rsp,
    Rbp,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl Register {
    pub const ALL: [Self; 16] = [
        Self::Rax,
        Self::Rcx,
        Self::Rdx,
        Self::Rbx,
        Self::Rsp,
        Self::Rbp,
        Self::Rsi,
        Self::Rdi,
        Self::R8,
        Self::R9,
        Self::R10,
        Self::R11,
        Self::R12,
        Self::R13,
        Self::R14,
        Self::R15,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Width {
    Byte,
    Word,
    Dword,
    Qword,
}

impl Width {
    pub const fn bytes(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Word => 2,
            Self::Dword => 4,
            Self::Qword => 8,
        }
    }
    pub const fn mask(self) -> u64 {
        match self {
            Self::Byte => 0xff,
            Self::Word => 0xffff,
            Self::Dword => 0xffff_ffff,
            Self::Qword => u64::MAX,
        }
    }
    pub fn merge(self, old: u64, value: u64) -> u64 {
        match self {
            Self::Byte | Self::Word => (old & !self.mask()) | (value & self.mask()),
            Self::Dword | Self::Qword => value & self.mask(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Address {
    pub base: Option<Register>,
    pub index: Option<Register>,
    pub scale: u8,
    pub displacement: u64,
}

impl Address {
    /// Effective-address arithmetic wraps at 64 bits; actual accesses remain checked.
    pub fn resolve(self, registers: &[u64; 16]) -> u64 {
        let base = self.base.map_or(0, |register| registers[register as usize]);
        let index = self
            .index
            .map_or(0, |register| registers[register as usize]);
        base.wrapping_add(index.wrapping_mul(u64::from(self.scale)))
            .wrapping_add(self.displacement)
    }
}
