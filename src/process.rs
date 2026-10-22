//! Bounded initial state for owned static, libc-free x86-64 guest programs.
#![forbid(unsafe_code)]

use crate::{
    Error, ErrorKind, Result,
    elf::ElfImage,
    memory::{AddressSpace, Permissions},
};

pub const STACK_BASE: u64 = 0x7fff_ff00_0000;
pub const STACK_SIZE: usize = 64 * 1024;
pub const METADATA_BASE: u64 = 0x7fff_fef0_0000;
pub const MAX_STRINGS_BYTES: usize = 16 * 1024;
pub const MAX_ARGUMENTS: usize = 64;
pub const MAX_ENVIRONMENT: usize = 64;

#[derive(Clone, Debug)]
pub struct ProcessOptions {
    pub arguments: Vec<String>,
    pub environment: Vec<String>,
}

impl Default for ProcessOptions {
    fn default() -> Self {
        Self {
            arguments: vec!["emucore-guest".into()],
            environment: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InitialState {
    pub stack_pointer: u64,
    pub argument_count: usize,
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::Runtime, message)
}

struct Stack {
    bytes: Vec<u8>,
    cursor: usize,
}

impl Stack {
    fn push_bytes(&mut self, bytes: &[u8]) -> Result<u64> {
        self.cursor = self
            .cursor
            .checked_sub(bytes.len())
            .ok_or_else(|| invalid("initial guest stack overflow"))?;
        self.bytes[self.cursor..self.cursor + bytes.len()].copy_from_slice(bytes);
        Ok(STACK_BASE + self.cursor as u64)
    }

    fn push_string(&mut self, string: &str) -> Result<u64> {
        self.push_bytes(&[0])?;
        self.push_bytes(string.as_bytes())
    }
}

fn validate_options(options: &ProcessOptions) -> Result<()> {
    if options.arguments.is_empty()
        || options.arguments.len() > MAX_ARGUMENTS
        || options.environment.len() > MAX_ENVIRONMENT
    {
        return Err(invalid(
            "guest argument or environment count outside limits",
        ));
    }
    let mut total = 0_usize;
    for string in options.arguments.iter().chain(&options.environment) {
        if string.contains('\0') {
            return Err(invalid("guest startup strings cannot contain NUL"));
        }
        total = total
            .checked_add(string.len() + 1)
            .ok_or_else(|| invalid("guest startup strings overflow"))?;
        if total > MAX_STRINGS_BYTES {
            return Err(invalid("guest startup strings exceed 16 KiB"));
        }
    }
    for variable in &options.environment {
        if !variable
            .split_once('=')
            .is_some_and(|(name, _)| !name.is_empty())
        {
            return Err(invalid(
                "guest environment entries require a non-empty NAME=value",
            ));
        }
    }
    Ok(())
}

/// The seed is supplied by the caller so production can use OS entropy and tests stay reproducible.
pub fn initialize(
    memory: &mut AddressSpace,
    image: &ElfImage,
    options: &ProcessOptions,
    random: [u8; 16],
) -> Result<InitialState> {
    validate_options(options)?;
    let mut stack = Stack {
        bytes: vec![0; STACK_SIZE],
        cursor: STACK_SIZE,
    };
    let mut arguments = Vec::with_capacity(options.arguments.len());
    for argument in &options.arguments {
        arguments.push(stack.push_string(argument)?);
    }
    let mut environment = Vec::with_capacity(options.environment.len());
    for variable in &options.environment {
        environment.push(stack.push_string(variable)?);
    }
    let platform = stack.push_string("x86_64")?;
    let random_pointer = stack.push_bytes(&random)?;
    let program_headers = if let Some(address) = image.program_header_address() {
        address
    } else {
        memory.map_bytes(METADATA_BASE, image.program_headers(), Permissions::READ)?;
        METADATA_BASE
    };
    let auxiliary = [
        (3, program_headers),
        (4, 56),
        (5, (image.program_headers().len() / 56) as u64),
        (6, 4096),
        (7, 0),
        (8, 0),
        (9, image.entry()),
        (11, 1000),
        (12, 1000),
        (13, 1000),
        (14, 1000),
        (15, platform),
        (23, 0),
        (25, random_pointer),
        (31, arguments[0]),
        (0, 0),
    ];
    let mut words = vec![arguments.len() as u64];
    words.extend(arguments);
    words.push(0);
    words.extend(environment);
    words.push(0);
    for (kind, value) in auxiliary {
        words.extend_from_slice(&[kind, value]);
    }
    stack.cursor = stack
        .cursor
        .checked_sub(words.len() * 8)
        .ok_or_else(|| invalid("initial guest stack overflow"))?
        & !15;
    for (index, word) in words.iter().enumerate() {
        let offset = stack.cursor + index * 8;
        stack.bytes[offset..offset + 8].copy_from_slice(&word.to_le_bytes());
    }
    let stack_pointer = STACK_BASE + stack.cursor as u64;
    memory.map_bytes(STACK_BASE, &stack.bytes, Permissions::READ_WRITE)?;
    Ok(InitialState {
        stack_pointer,
        argument_count: options.arguments.len(),
    })
}
