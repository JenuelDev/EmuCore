//! Integer flags and branch conditions, independent from host CPU flags.
#![forbid(unsafe_code)]

use crate::cpu::Width;

pub const CARRY: u64 = 1;
pub const PARITY: u64 = 1 << 2;
pub const AUXILIARY: u64 = 1 << 4;
pub const ZERO: u64 = 1 << 6;
pub const SIGN: u64 = 1 << 7;
pub const OVERFLOW: u64 = 1 << 11;
const ARITHMETIC: u64 = CARRY | PARITY | AUXILIARY | ZERO | SIGN | OVERFLOW;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Alu {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Compare,
    Test,
    Increment,
    Decrement,
}

impl Alu {
    pub fn writes_result(self) -> bool {
        !matches!(self, Self::Compare | Self::Test)
    }

    pub fn evaluate(self, left: u64, right: u64, width: Width, previous: u64) -> (u64, u64) {
        let left = left & width.mask();
        let right = right & width.mask();
        let sign = 1_u64 << (width.bytes() * 8 - 1);
        let (result, carry, overflow, auxiliary) = match self {
            Self::Add | Self::Increment => {
                let right = if self == Self::Increment { 1 } else { right };
                let result = left.wrapping_add(right) & width.mask();
                (
                    result,
                    (left as u128 + right as u128) > width.mask() as u128,
                    (!(left ^ right) & (left ^ result) & sign) != 0,
                    ((left ^ right ^ result) & 0x10) != 0,
                )
            }
            Self::Sub | Self::Compare | Self::Decrement => {
                let right = if self == Self::Decrement { 1 } else { right };
                let result = left.wrapping_sub(right) & width.mask();
                (
                    result,
                    left < right,
                    ((left ^ right) & (left ^ result) & sign) != 0,
                    ((left ^ right ^ result) & 0x10) != 0,
                )
            }
            Self::And | Self::Test => (left & right, false, false, false),
            Self::Or => (left | right, false, false, false),
            Self::Xor => (left ^ right, false, false, false),
        };
        let mut flags = previous & !ARITHMETIC;
        if carry {
            flags |= CARRY;
        }
        if overflow {
            flags |= OVERFLOW;
        }
        if auxiliary {
            flags |= AUXILIARY;
        }
        if result == 0 {
            flags |= ZERO;
        }
        if result & sign != 0 {
            flags |= SIGN;
        }
        if (result as u8).count_ones().is_multiple_of(2) {
            flags |= PARITY;
        }
        if matches!(self, Self::Increment | Self::Decrement) {
            flags = (flags & !CARRY) | (previous & CARRY);
        }
        (result, flags)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Condition {
    Always,
    Equal,
    NotEqual,
    Below,
    BelowOrEqual,
    Above,
    AboveOrEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Overflow,
    NotOverflow,
    Negative,
    NonNegative,
    Parity,
    NotParity,
}

impl Condition {
    pub fn test(self, flags: u64) -> bool {
        let carry = flags & CARRY != 0;
        let zero = flags & ZERO != 0;
        let negative = flags & SIGN != 0;
        let overflow = flags & OVERFLOW != 0;
        match self {
            Self::Always => true,
            Self::Equal => zero,
            Self::NotEqual => !zero,
            Self::Below => carry,
            Self::BelowOrEqual => carry || zero,
            Self::Above => !carry && !zero,
            Self::AboveOrEqual => !carry,
            Self::Less => negative != overflow,
            Self::LessOrEqual => zero || negative != overflow,
            Self::Greater => !zero && negative == overflow,
            Self::GreaterOrEqual => negative == overflow,
            Self::Overflow => overflow,
            Self::NotOverflow => !overflow,
            Self::Negative => negative,
            Self::NonNegative => !negative,
            Self::Parity => flags & PARITY != 0,
            Self::NotParity => flags & PARITY == 0,
        }
    }
}
