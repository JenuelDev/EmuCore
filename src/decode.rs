//! Decode through iced-x86, then lower only explicitly supported instructions.
#![forbid(unsafe_code)]
use std::collections::{BTreeMap, BTreeSet};

pub use crate::cpu::Register;
use crate::{
    Error, ErrorKind, Result,
    cpu::{Address, Width},
    elf::ElfImage,
    flags::{Alu, Condition},
};
use iced_x86::{
    Decoder, DecoderOptions, Instruction as X86Instruction, MemorySize, Mnemonic, OpKind,
    Register as X86Register,
};

pub const MAX_INSTRUCTIONS: usize = 1024;
const MAX_CODE_SIZE: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Destination {
    Register(Register),
    Memory(Address),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    Register(Register),
    Immediate(u64),
    Memory(Address),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    Move {
        destination: Destination,
        source: Source,
        width: Width,
    },
    Lea {
        register: Register,
        address: Address,
        width: Width,
    },
    Alu {
        operation: Alu,
        destination: Destination,
        source: Source,
        width: Width,
    },
    Branch {
        condition: Condition,
        target: u64,
    },
    Syscall {
        next_ip: u64,
    },
    Push {
        source: Source,
    },
    Pop {
        register: Register,
    },
    Call {
        target: u64,
        return_address: u64,
    },
    Return {
        stack_adjust: u16,
    },
    Trap {
        address: u64,
    },
    Nop,
}
#[derive(Debug)]
pub struct Program {
    instructions: Vec<Instruction>,
    addresses: Vec<u64>,
    fallthrough: Vec<Option<usize>>,
    entry_index: usize,
}

struct DecodedNode {
    instruction: Instruction,
    length: usize,
    fallthrough: Option<u64>,
}
fn error(at: u64, message: &str) -> Error {
    Error::new(
        ErrorKind::Decode,
        format!("guest address 0x{at:x}: {message}"),
    )
}

fn register(raw: X86Register, at: u64) -> Result<(Register, Width)> {
    use X86Register::*;
    const FAMILIES: [[X86Register; 4]; 16] = [
        [RAX, EAX, AX, AL],
        [RCX, ECX, CX, CL],
        [RDX, EDX, DX, DL],
        [RBX, EBX, BX, BL],
        [RSP, ESP, SP, SPL],
        [RBP, EBP, BP, BPL],
        [RSI, ESI, SI, SIL],
        [RDI, EDI, DI, DIL],
        [R8, R8D, R8W, R8L],
        [R9, R9D, R9W, R9L],
        [R10, R10D, R10W, R10L],
        [R11, R11D, R11W, R11L],
        [R12, R12D, R12W, R12L],
        [R13, R13D, R13W, R13L],
        [R14, R14D, R14W, R14L],
        [R15, R15D, R15W, R15L],
    ];
    for (index, family) in FAMILIES.iter().enumerate() {
        if let Some(width) = family.iter().position(|&candidate| candidate == raw) {
            return Ok((
                Register::ALL[index],
                [Width::Qword, Width::Dword, Width::Word, Width::Byte][width],
            ));
        }
    }
    Err(error(
        at,
        "unsupported register; high-byte, vector, and segment registers are not modeled",
    ))
}

fn address(instruction: &X86Instruction, at: u64) -> Result<Address> {
    if instruction.is_ip_rel_memory_operand() {
        if instruction.memory_base() != X86Register::RIP {
            return Err(error(at, "32-bit address-size override is unsupported"));
        }
        return Ok(Address {
            base: None,
            index: None,
            scale: 1,
            displacement: instruction.ip_rel_memory_address(),
        });
    }
    let mut result = Address {
        base: None,
        index: None,
        scale: instruction.memory_index_scale() as u8,
        displacement: instruction.memory_displacement64(),
    };
    for (raw, slot) in [
        (instruction.memory_base(), &mut result.base),
        (instruction.memory_index(), &mut result.index),
    ] {
        if raw != X86Register::None {
            let (register, width) = register(raw, at)?;
            if width != Width::Qword {
                return Err(error(at, "only 64-bit addressing is supported"));
            }
            *slot = Some(register);
        }
    }
    Ok(result)
}
fn memory_width(instruction: &X86Instruction, at: u64) -> Result<Width> {
    match instruction.memory_size() {
        MemorySize::UInt8 => Ok(Width::Byte),
        MemorySize::UInt16 => Ok(Width::Word),
        MemorySize::UInt32 => Ok(Width::Dword),
        MemorySize::UInt64 => Ok(Width::Qword),
        _ => Err(error(at, "unsupported memory operand width")),
    }
}
fn is_immediate(kind: OpKind) -> bool {
    matches!(
        kind,
        OpKind::Immediate8
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64
    )
}
fn has_address_override(bytes: &[u8]) -> bool {
    for &byte in bytes {
        match byte {
            0x67 => return true,
            0x40..=0x4f | 0x66 | 0xf0 | 0xf2 | 0xf3 | 0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 => (),
            _ => return false,
        }
    }
    false
}
fn lower_alu(instruction: &X86Instruction, at: u64, operation: Alu) -> Result<Instruction> {
    let (destination, width) = match instruction.op0_kind() {
        OpKind::Register => {
            let (register, width) = register(instruction.op0_register(), at)?;
            (Destination::Register(register), width)
        }
        OpKind::Memory => (
            Destination::Memory(address(instruction, at)?),
            memory_width(instruction, at)?,
        ),
        _ => return Err(error(at, "unsupported arithmetic destination")),
    };
    let source = if matches!(operation, Alu::Increment | Alu::Decrement) {
        Source::Immediate(1)
    } else {
        match instruction.op1_kind() {
            OpKind::Register => {
                let (register, source_width) = register(instruction.op1_register(), at)?;
                if source_width != width {
                    return Err(error(at, "arithmetic operand width mismatch"));
                }
                Source::Register(register)
            }
            OpKind::Memory => Source::Memory(address(instruction, at)?),
            kind if is_immediate(kind) => Source::Immediate(instruction.immediate(1)),
            _ => return Err(error(at, "unsupported arithmetic source")),
        }
    };
    Ok(Instruction::Alu {
        operation,
        destination,
        source,
        width,
    })
}

fn branch_condition(mnemonic: Mnemonic) -> Option<Condition> {
    Some(match mnemonic {
        Mnemonic::Jmp => Condition::Always,
        Mnemonic::Je => Condition::Equal,
        Mnemonic::Jne => Condition::NotEqual,
        Mnemonic::Jb => Condition::Below,
        Mnemonic::Jbe => Condition::BelowOrEqual,
        Mnemonic::Ja => Condition::Above,
        Mnemonic::Jae => Condition::AboveOrEqual,
        Mnemonic::Jl => Condition::Less,
        Mnemonic::Jle => Condition::LessOrEqual,
        Mnemonic::Jg => Condition::Greater,
        Mnemonic::Jge => Condition::GreaterOrEqual,
        Mnemonic::Jo => Condition::Overflow,
        Mnemonic::Jno => Condition::NotOverflow,
        Mnemonic::Js => Condition::Negative,
        Mnemonic::Jns => Condition::NonNegative,
        Mnemonic::Jp => Condition::Parity,
        Mnemonic::Jnp => Condition::NotParity,
        _ => return None,
    })
}

fn lower(instruction: &X86Instruction, at: u64) -> Result<Instruction> {
    match instruction.mnemonic() {
        Mnemonic::Mov => {
            let (destination, width) = match instruction.op0_kind() {
                OpKind::Register => {
                    let (register, width) = register(instruction.op0_register(), at)?;
                    (Destination::Register(register), width)
                }
                OpKind::Memory => (
                    Destination::Memory(address(instruction, at)?),
                    memory_width(instruction, at)?,
                ),
                _ => return Err(error(at, "unsupported MOV destination")),
            };
            let source = match instruction.op1_kind() {
                OpKind::Register => {
                    let (register, source_width) = register(instruction.op1_register(), at)?;
                    if source_width != width {
                        return Err(error(at, "MOV operand width mismatch"));
                    }
                    Source::Register(register)
                }
                OpKind::Memory => Source::Memory(address(instruction, at)?),
                kind if is_immediate(kind) => Source::Immediate(instruction.immediate(1)),
                _ => return Err(error(at, "unsupported MOV source")),
            };
            Ok(Instruction::Move {
                destination,
                source,
                width,
            })
        }
        Mnemonic::Lea
            if instruction.op0_kind() == OpKind::Register
                && instruction.op1_kind() == OpKind::Memory =>
        {
            let (register, width) = register(instruction.op0_register(), at)?;
            Ok(Instruction::Lea {
                register,
                address: address(instruction, at)?,
                width,
            })
        }
        Mnemonic::Add => lower_alu(instruction, at, Alu::Add),
        Mnemonic::Sub => lower_alu(instruction, at, Alu::Sub),
        Mnemonic::And => lower_alu(instruction, at, Alu::And),
        Mnemonic::Or => lower_alu(instruction, at, Alu::Or),
        Mnemonic::Xor => lower_alu(instruction, at, Alu::Xor),
        Mnemonic::Cmp => lower_alu(instruction, at, Alu::Compare),
        Mnemonic::Test => lower_alu(instruction, at, Alu::Test),
        Mnemonic::Inc => lower_alu(instruction, at, Alu::Increment),
        Mnemonic::Dec => lower_alu(instruction, at, Alu::Decrement),
        mnemonic
            if instruction.op0_kind() == OpKind::NearBranch64
                && branch_condition(mnemonic).is_some() =>
        {
            let condition =
                branch_condition(mnemonic).ok_or_else(|| error(at, "unknown branch condition"))?;
            Ok(Instruction::Branch {
                condition,
                target: instruction.near_branch_target(),
            })
        }
        Mnemonic::Push => {
            let source = match instruction.op0_kind() {
                OpKind::Register => {
                    let (register, width) = register(instruction.op0_register(), at)?;
                    if width != Width::Qword {
                        return Err(error(
                            at,
                            "only 64-bit guest stack operations are supported",
                        ));
                    }
                    Source::Register(register)
                }
                OpKind::Memory if memory_width(instruction, at)? == Width::Qword => {
                    Source::Memory(address(instruction, at)?)
                }
                OpKind::Immediate8to64 | OpKind::Immediate32to64 => {
                    Source::Immediate(instruction.immediate(0))
                }
                _ => return Err(error(at, "unsupported PUSH operand")),
            };
            Ok(Instruction::Push { source })
        }
        Mnemonic::Pop if instruction.op0_kind() == OpKind::Register => {
            let (register, width) = register(instruction.op0_register(), at)?;
            if width != Width::Qword {
                return Err(error(
                    at,
                    "only 64-bit guest stack operations are supported",
                ));
            }
            Ok(Instruction::Pop { register })
        }
        Mnemonic::Call if instruction.op0_kind() == OpKind::NearBranch64 => Ok(Instruction::Call {
            target: instruction.near_branch_target(),
            return_address: instruction.next_ip(),
        }),
        Mnemonic::Ret => {
            let stack_adjust = if instruction.op_count() == 0 {
                0
            } else if instruction.op0_kind() == OpKind::Immediate16 {
                instruction.immediate16()
            } else {
                return Err(error(at, "unsupported RET operand"));
            };
            Ok(Instruction::Return { stack_adjust })
        }
        Mnemonic::Int3 | Mnemonic::Ud2 => Ok(Instruction::Trap {
            address: instruction.ip(),
        }),
        Mnemonic::Syscall => Ok(Instruction::Syscall {
            next_ip: instruction.next_ip(),
        }),
        Mnemonic::Nop => Ok(Instruction::Nop),
        _ => Err(error(
            at,
            &format!(
                "unsupported instruction {:?} ({:?})",
                instruction.mnemonic(),
                instruction.code()
            ),
        )),
    }
}
impl Program {
    pub fn decode(image: &ElfImage) -> Result<Self> {
        Self::decode_graph(image.entry(), |address| image.code_at(address))
    }

    #[cfg(test)]
    fn decode_bytes(code: &[u8], entry: u64) -> Result<Self> {
        if code.is_empty()
            || code.len() > MAX_CODE_SIZE
            || entry.checked_add(code.len() as u64).is_none()
        {
            return Err(error(0, "code size or address outside limits"));
        }
        Self::decode_graph(entry, |address| {
            code.get(usize::try_from(address.checked_sub(entry)?).ok()?..)
                .filter(|bytes| !bytes.is_empty())
        })
    }

    fn decode_graph<'a>(entry: u64, source: impl Fn(u64) -> Option<&'a [u8]>) -> Result<Self> {
        let mut pending = BTreeSet::from([entry]);
        let mut nodes = BTreeMap::<u64, DecodedNode>::new();
        let mut decoded_bytes = 0;
        while let Some(address) = pending.pop_first() {
            if nodes.contains_key(&address) {
                continue;
            }
            if nodes.len() == MAX_INSTRUCTIONS {
                return Err(error(address, "instruction limit exceeded"));
            }
            if let Some((&previous, node)) = nodes.range(..address).next_back()
                && previous + node.length as u64 > address
            {
                return Err(error(address, "overlapping reachable instruction decodes"));
            }
            let bytes = source(address).ok_or_else(|| {
                error(
                    address,
                    "control-flow target is not file-backed executable memory",
                )
            })?;
            let mut decoder = Decoder::with_ip(64, bytes, address, DecoderOptions::NONE);
            let raw = decoder.decode();
            if raw.is_invalid() {
                return Err(error(address, "invalid or truncated x86-64 instruction"));
            }
            let next = address
                .checked_add(raw.len() as u64)
                .ok_or_else(|| error(address, "instruction address overflow"))?;
            if nodes.range(address..next).next().is_some() {
                return Err(error(address, "overlapping reachable instruction decodes"));
            }
            if raw.has_lock_prefix()
                || raw.has_rep_prefix()
                || raw.has_repne_prefix()
                || raw.segment_prefix() != X86Register::None
                || has_address_override(&bytes[..raw.len()])
            {
                return Err(error(
                    address,
                    "lock, repeat, segment, and address-size prefixes are not supported",
                ));
            }
            decoded_bytes += raw.len();
            if decoded_bytes > MAX_CODE_SIZE {
                return Err(error(address, "decoded code size exceeds limit"));
            }
            let instruction = lower(&raw, address)?;
            let can_fallthrough = !matches!(
                instruction,
                Instruction::Return { .. }
                    | Instruction::Trap { .. }
                    | Instruction::Branch {
                        condition: Condition::Always,
                        ..
                    }
            );
            let fallthrough = if can_fallthrough && source(next).is_some() {
                pending.insert(next);
                Some(next)
            } else {
                None
            };
            if let Instruction::Branch { target, .. } | Instruction::Call { target, .. } =
                instruction
            {
                pending.insert(target);
            }
            nodes.insert(
                address,
                DecodedNode {
                    instruction,
                    length: raw.len(),
                    fallthrough,
                },
            );
        }
        let addresses: Vec<u64> = nodes.keys().copied().collect();
        let entry_index = addresses
            .binary_search(&entry)
            .map_err(|_| error(entry, "entry point was not decoded"))?;
        let mut fallthrough = Vec::with_capacity(nodes.len());
        for node in nodes.values() {
            fallthrough.push(
                node.fallthrough
                    .map(|address| {
                        addresses
                            .binary_search(&address)
                            .map_err(|_| error(address, "missing fallthrough instruction"))
                    })
                    .transpose()?,
            );
        }
        let instructions = nodes.into_values().map(|node| node.instruction).collect();
        Ok(Self {
            instructions,
            addresses,
            fallthrough,
            entry_index,
        })
    }
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }
    pub fn index_of(&self, address: u64) -> Option<usize> {
        self.addresses.binary_search(&address).ok()
    }
    pub(crate) fn addresses(&self) -> &[u64] {
        &self.addresses
    }
    pub(crate) fn entry_index(&self) -> usize {
        self.entry_index
    }
    pub(crate) fn fallthrough(&self, index: usize) -> Option<usize> {
        self.fallthrough.get(index).copied().flatten()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsupported_control_flow_registers_and_prefixes() {
        for code in [
            &[0xff, 0xe0][..],
            &[0xb4, 1],
            &[0x64, 0x48, 0x8b, 0x00],
            &[0x67, 0x8b, 0x00],
            &[0xf0, 0x01, 0x00],
            &[0x0f, 0x28, 0xc0],
        ] {
            assert!(Program::decode_bytes(code, 0x401000).is_err(), "{code:x?}");
        }
    }
    #[test]
    fn rejects_truncation_and_code_address_overflow() {
        for code in [
            &[0xb8, 1][..],
            &[0x48],
            &[0x0f],
            &[0x48, 0x8d, 0x35, 0],
            &[0x48, 0x8b],
        ] {
            assert!(Program::decode_bytes(code, 0).is_err());
        }
        assert!(Program::decode_bytes(&[0x48, 0x8d, 0x35, 0, 0, 0, 0], u64::MAX - 3).is_err());
    }
    #[test]
    fn decodes_extended_registers_and_indexed_memory() {
        let program = Program::decode_bytes(&[0x4d, 0x89, 0x54, 0x8d, 0xf8], 0x401000).unwrap();
        assert_eq!(
            program.instructions(),
            &[Instruction::Move {
                destination: Destination::Memory(Address {
                    base: Some(Register::R13),
                    index: Some(Register::Rcx),
                    scale: 4,
                    displacement: (-8_i64) as u64
                }),
                source: Source::Register(Register::R10),
                width: Width::Qword,
            }]
        );
    }
    #[test]
    fn enforces_instruction_budget() {
        assert!(Program::decode_bytes(&[0x0f, 0x05].repeat(MAX_INSTRUCTIONS + 1), 0).is_err());
    }
}
