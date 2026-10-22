#![forbid(unsafe_code)]

use crate::{
    Error, ErrorKind, Result,
    cpu::Width,
    decode::{Destination, Instruction, Program, Register, Source},
    elf::ElfImage,
    flags::Condition,
    memory::AddressSpace,
    process::{self, ProcessOptions},
    vm::MemoryManager,
};

pub const MAX_OUTPUT: usize = 32 * 1024;
pub const MAX_EXECUTION_STEPS: u64 = 100_000;

use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub(crate) struct ExecutionMetrics {
    pub translation: Duration,
    pub executable_setup: Duration,
    pub execution: Duration,
    pub native_code_bytes: usize,
}

/// Wall-clock stages and logical guest memory, separate from deterministic guest state.
#[derive(Debug)]
pub struct RunMetrics {
    pub decode: Duration,
    pub process_setup: Duration,
    pub translation: Duration,
    pub executable_setup: Duration,
    pub execution: Duration,
    pub total: Duration,
    pub decoded_instructions: usize,
    pub native_code_bytes: usize,
    pub guest_mapped_bytes_final: usize,
    pub guest_mapped_bytes_peak: usize,
}

#[derive(Debug)]
pub struct ProfiledRun {
    pub result: RunResult,
    pub metrics: RunMetrics,
}

#[derive(Debug, Eq, PartialEq)]
pub struct RunResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: u8,
    pub registers: [u64; 16],
    pub flags: u64,
    pub instructions_executed: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum Backend {
    Native,
    Reference,
}

// The register file must remain first: generated code addresses its sixteen u64 fields.
#[repr(C)]
pub(crate) struct Context {
    pub registers: [u64; 16],
    flags: u64,
    pub(crate) steps_remaining: u64,
    pub(crate) return_target_index: u64,
    code_targets: Vec<u64>,
    memory: AddressSpace,
    vm: MemoryManager,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<u8>,
    failure: Option<Error>,
}

impl Context {
    fn new(image: &ElfImage, options: &ProcessOptions) -> Result<Self> {
        let mut memory = AddressSpace::from_image(image)?;
        let mut random = [0; 16];
        crate::native::fill_random(&mut random)?;
        let startup = process::initialize(&mut memory, image, options, random)?;
        let mut registers = [0; 16];
        registers[Register::Rsp as usize] = startup.stack_pointer;
        Ok(Self {
            registers,
            flags: 0x202,
            steps_remaining: MAX_EXECUTION_STEPS,
            return_target_index: 0,
            code_targets: Vec::new(),
            memory,
            vm: MemoryManager::new(image)?,
            // Reserve the bounded output storage before crossing the FFI boundary.
            stdout: Vec::with_capacity(MAX_OUTPUT),
            stderr: Vec::with_capacity(MAX_OUTPUT),
            exit_code: None,
            failure: None,
        })
    }

    /// Returns true to stop the native block. No panics may cross the native boundary.
    fn syscall(&mut self, next_ip: u64) -> bool {
        let number = self.registers[Register::Rax as usize];
        let argument0 = self.registers[Register::Rdi as usize];
        let argument1 = self.registers[Register::Rsi as usize];
        let argument2 = self.registers[Register::Rdx as usize];
        self.registers[Register::Rcx as usize] = next_ip;
        self.registers[Register::R11 as usize] = self.flags;
        match number {
            1 => {
                let result = if !matches!(argument0, 1 | 2) {
                    errno(9) // EBADF
                } else if argument2 > (MAX_OUTPUT - self.stdout.len() - self.stderr.len()) as u64 {
                    errno(27) // EFBIG: the experiment's declared output limit
                } else if argument2 == 0 {
                    0
                } else {
                    let output = if argument0 == 1 {
                        &mut self.stdout
                    } else {
                        &mut self.stderr
                    };
                    let start = output.len();
                    output.resize(start + argument2 as usize, 0);
                    if self
                        .memory
                        .read_into(argument1, &mut output[start..])
                        .is_ok()
                    {
                        argument2
                    } else {
                        output.truncate(start);
                        errno(14) // EFAULT
                    }
                };
                self.registers[Register::Rax as usize] = result;
                false
            }
            9..=12 => {
                let result = match number {
                    9 => self.vm.mmap(
                        &mut self.memory,
                        [
                            argument0,
                            argument1,
                            argument2,
                            self.registers[Register::R10 as usize],
                            self.registers[Register::R8 as usize],
                            self.registers[Register::R9 as usize],
                        ],
                    ),
                    10 => self
                        .vm
                        .mprotect(&mut self.memory, argument0, argument1, argument2),
                    11 => self.vm.munmap(&mut self.memory, argument0, argument1),
                    _ => self.vm.brk(&mut self.memory, argument0),
                };
                self.registers[Register::Rax as usize] = result;
                false
            }
            60 | 231 => {
                self.exit_code = Some((argument0 & 0xff) as u8);
                true
            }
            _ => {
                self.failure = Some(Error::new(
                    ErrorKind::Unsupported,
                    format!("Linux syscall {number} is not implemented"),
                ));
                true
            }
        }
    }

    fn set_register(&mut self, register: Register, width: Width, value: u64) {
        let old = self.registers[register as usize];
        self.registers[register as usize] = width.merge(old, value);
    }

    fn read_source(&self, source: Source, width: Width) -> Result<u64> {
        match source {
            Source::Register(register) => Ok(self.registers[register as usize] & width.mask()),
            Source::Immediate(value) => Ok(value & width.mask()),
            Source::Memory(address) => {
                let mut bytes = [0; 8];
                self.memory
                    .read_into(
                        address.resolve(&self.registers),
                        &mut bytes[..width.bytes()],
                    )
                    .map_err(|fault| Error::new(ErrorKind::Runtime, fault.to_string()))?;
                Ok(u64::from_le_bytes(bytes))
            }
        }
    }

    fn write_destination(
        &mut self,
        destination: Destination,
        width: Width,
        value: u64,
    ) -> Result<()> {
        match destination {
            Destination::Register(register) => self.set_register(register, width, value),
            Destination::Memory(address) => self
                .memory
                .write(
                    address.resolve(&self.registers),
                    &value.to_le_bytes()[..width.bytes()],
                )
                .map_err(|fault| Error::new(ErrorKind::Runtime, fault.to_string()))?,
        }
        Ok(())
    }

    fn apply(&mut self, instruction: &Instruction) -> Result<()> {
        match *instruction {
            Instruction::Move {
                destination,
                source,
                width,
            } => {
                let value = self.read_source(source, width)?;
                match destination {
                    Destination::Register(register) => self.set_register(register, width, value),
                    Destination::Memory(address) => self
                        .memory
                        .write(
                            address.resolve(&self.registers),
                            &value.to_le_bytes()[..width.bytes()],
                        )
                        .map_err(|fault| Error::new(ErrorKind::Runtime, fault.to_string()))?,
                }
            }
            Instruction::Lea {
                register,
                address,
                width,
            } => self.set_register(register, width, address.resolve(&self.registers)),
            Instruction::Alu {
                operation,
                destination,
                source,
                width,
            } => {
                let left_source = match destination {
                    Destination::Register(register) => Source::Register(register),
                    Destination::Memory(address) => Source::Memory(address),
                };
                let left = self.read_source(left_source, width)?;
                let right = self.read_source(source, width)?;
                let (value, flags) = operation.evaluate(left, right, width, self.flags);
                if operation.writes_result() {
                    self.write_destination(destination, width, value)?;
                }
                self.flags = flags;
            }
            Instruction::Branch { .. } => {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    "internal branch dispatch mismatch",
                ));
            }
            Instruction::Nop => (),
            Instruction::Push { source } => {
                let value = self.read_source(source, Width::Qword)?;
                let next_stack = self.registers[Register::Rsp as usize].wrapping_sub(8);
                self.memory
                    .write(next_stack, &value.to_le_bytes())
                    .map_err(|fault| Error::new(ErrorKind::Runtime, fault.to_string()))?;
                self.registers[Register::Rsp as usize] = next_stack;
            }
            Instruction::Pop { register } => {
                let stack = self.registers[Register::Rsp as usize];
                let mut bytes = [0; 8];
                self.memory
                    .read_into(stack, &mut bytes)
                    .map_err(|fault| Error::new(ErrorKind::Runtime, fault.to_string()))?;
                self.registers[Register::Rsp as usize] = stack.wrapping_add(8);
                self.registers[register as usize] = u64::from_le_bytes(bytes);
            }
            Instruction::Call { return_address, .. } => {
                let stack = self.registers[Register::Rsp as usize].wrapping_sub(8);
                self.memory
                    .write(stack, &return_address.to_le_bytes())
                    .map_err(|fault| Error::new(ErrorKind::Runtime, fault.to_string()))?;
                self.registers[Register::Rsp as usize] = stack;
            }
            Instruction::Return { stack_adjust } => {
                let stack = self.registers[Register::Rsp as usize];
                let mut bytes = [0; 8];
                self.memory
                    .read_into(stack, &mut bytes)
                    .map_err(|fault| Error::new(ErrorKind::Runtime, fault.to_string()))?;
                let target = u64::from_le_bytes(bytes);
                let index = self.code_targets.binary_search(&target).map_err(|_| {
                    Error::new(
                        ErrorKind::Runtime,
                        format!(
                            "guest return target 0x{target:x} is not a decoded instruction boundary"
                        ),
                    )
                })?;
                self.registers[Register::Rsp as usize] =
                    stack.wrapping_add(8 + u64::from(stack_adjust));
                self.return_target_index = index as u64;
            }
            Instruction::Trap { address } => {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    format!("guest trap at 0x{address:x}"),
                ));
            }
            Instruction::Syscall { .. } => {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    "internal syscall dispatch mismatch",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn operation(&mut self, instruction: &Instruction) -> bool {
        if let Instruction::Syscall { next_ip } = *instruction {
            return self.syscall(next_ip);
        }
        match self.apply(instruction) {
            Ok(()) => false,
            Err(error) => {
                self.failure = Some(error);
                true
            }
        }
    }

    pub(crate) fn condition(&self, condition: Condition) -> bool {
        condition.test(self.flags)
    }

    pub(crate) fn exhaust_budget(&mut self) {
        self.failure = Some(Error::new(
            ErrorKind::Runtime,
            format!("guest execution budget exhausted after {MAX_EXECUTION_STEPS} instructions"),
        ));
    }

    fn tick(&mut self) -> bool {
        if self.steps_remaining == 0 {
            self.exhaust_budget();
            false
        } else {
            self.steps_remaining -= 1;
            true
        }
    }
}

fn errno(number: i64) -> u64 {
    (-number) as u64
}

/// Executes a fully validated block. The CLI calls this only in a disposable worker.
/// This is a narrow native translator, not an isolation boundary or Android sandbox.
pub fn run(image: &ElfImage) -> Result<RunResult> {
    run_with_backend(image, Backend::Native)
}

/// The reference backend is a correctness oracle, not a performance configuration.
pub fn run_with_backend(image: &ElfImage, backend: Backend) -> Result<RunResult> {
    run_with_options(image, backend, &ProcessOptions::default())
}

pub fn run_with_options(
    image: &ElfImage,
    backend: Backend,
    options: &ProcessOptions,
) -> Result<RunResult> {
    run_profiled(image, backend, options).map(|profile| profile.result)
}

/// Measures successful runs. The total includes guest-memory and executable teardown,
/// but excludes ELF loading, CLI output, process startup, and worker supervision.
pub fn run_profiled(
    image: &ElfImage,
    backend: Backend,
    options: &ProcessOptions,
) -> Result<ProfiledRun> {
    let started = Instant::now();
    let mut profile = execute_profiled(image, backend, options)?;
    profile.metrics.total = started.elapsed();
    Ok(profile)
}

fn execute_profiled(
    image: &ElfImage,
    backend: Backend,
    options: &ProcessOptions,
) -> Result<ProfiledRun> {
    let started = Instant::now();
    let program = Program::decode(image)?;
    let decode = started.elapsed();
    let started = Instant::now();
    let mut context = Context::new(image, options)?;
    context.code_targets = program.addresses().to_vec();
    let process_setup = started.elapsed();
    let execution = match backend {
        Backend::Native => crate::native::execute(&program, &mut context)?,
        Backend::Reference => {
            let started = Instant::now();
            let mut index = program.entry_index();
            while index < program.instructions().len() {
                if !context.tick() {
                    break;
                }
                let instruction = &program.instructions()[index];
                if let Instruction::Branch { condition, target } = *instruction {
                    if context.condition(condition) {
                        index = program.index_of(target).ok_or_else(|| {
                            Error::new(ErrorKind::Runtime, "internal invalid branch target")
                        })?;
                        continue;
                    }
                } else {
                    if context.operation(instruction) {
                        break;
                    }
                    match *instruction {
                        Instruction::Call { target, .. } => {
                            index = program.index_of(target).ok_or_else(|| {
                                Error::new(ErrorKind::Runtime, "internal invalid call target")
                            })?;
                            continue;
                        }
                        Instruction::Return { .. } => {
                            index = context.return_target_index as usize;
                            continue;
                        }
                        _ => (),
                    }
                }
                index = program
                    .fallthrough(index)
                    .unwrap_or(program.instructions().len());
            }
            ExecutionMetrics {
                execution: started.elapsed(),
                ..ExecutionMetrics::default()
            }
        }
    };
    if let Some(error) = context.failure {
        return Err(error);
    }
    let exit_code = context.exit_code.ok_or_else(|| {
        Error::new(
            ErrorKind::Runtime,
            "program reached end of block without exit or exit_group",
        )
    })?;
    let metrics = RunMetrics {
        decode,
        process_setup,
        translation: execution.translation,
        executable_setup: execution.executable_setup,
        execution: execution.execution,
        total: Duration::ZERO,
        decoded_instructions: program.instructions().len(),
        native_code_bytes: execution.native_code_bytes,
        guest_mapped_bytes_final: context.memory.mapped_bytes(),
        guest_mapped_bytes_peak: context.memory.peak_mapped_bytes(),
    };
    let result = RunResult {
        stdout: context.stdout,
        stderr: context.stderr,
        exit_code,
        registers: context.registers,
        flags: context.flags,
        instructions_executed: MAX_EXECUTION_STEPS - context.steps_remaining,
    };
    Ok(ProfiledRun { result, metrics })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn failed_stack_operations_preserve_rsp_and_destination() {
        let image = ElfImage::parse(&fixture::hello()).unwrap();
        for instruction in [
            Instruction::Push {
                source: Source::Immediate(42),
            },
            Instruction::Pop {
                register: Register::Rbx,
            },
            Instruction::Call {
                target: fixture::CODE_ADDRESS,
                return_address: fixture::CODE_ADDRESS,
            },
            Instruction::Return { stack_adjust: 0 },
        ] {
            let mut context = Context::new(&image, &ProcessOptions::default()).unwrap();
            context.registers[Register::Rsp as usize] = 0;
            context.registers[Register::Rbx as usize] = 123;
            assert!(context.operation(&instruction));
            assert!(context.failure.is_some());
            assert_eq!(context.registers[Register::Rsp as usize], 0);
            assert_eq!(context.registers[Register::Rbx as usize], 123);
        }
    }

    #[test]
    fn failed_memory_arithmetic_preserves_flags_and_memory() {
        let image = ElfImage::parse(&fixture::hello()).unwrap();
        let mut context = Context::new(&image, &ProcessOptions::default()).unwrap();
        let instruction = Instruction::Alu {
            operation: crate::flags::Alu::Add,
            destination: Destination::Memory(crate::cpu::Address {
                base: None,
                index: None,
                scale: 1,
                displacement: fixture::DATA_ADDRESS,
            }),
            source: Source::Immediate(255),
            width: Width::Byte,
        };
        let previous = context.flags;
        assert!(context.operation(&instruction));
        assert_eq!(context.flags, previous);
        let mut byte = [0];
        context
            .memory
            .read_into(fixture::DATA_ADDRESS, &mut byte)
            .unwrap();
        assert_eq!(byte[0], fixture::MESSAGE[0]);
    }

    #[test]
    fn invalid_return_targets_do_not_pop_guest_stack_state() {
        let image = ElfImage::parse(&fixture::hello()).unwrap();
        let mut context = Context::new(&image, &ProcessOptions::default()).unwrap();
        context.code_targets = vec![fixture::CODE_ADDRESS];
        let stack = context.registers[Register::Rsp as usize];
        context
            .memory
            .write(stack, &(fixture::CODE_ADDRESS + 1).to_le_bytes())
            .unwrap();
        assert!(context.operation(&Instruction::Return { stack_adjust: 16 }));
        assert_eq!(context.registers[Register::Rsp as usize], stack);
        assert_eq!(context.return_target_index, 0);
    }
}
