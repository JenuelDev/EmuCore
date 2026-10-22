//! The only unsafe module. Guest bytes are decoded, never copied into executable memory.

#[cfg(windows)]
pub(crate) mod client;
#[cfg(windows)]
pub(crate) mod process;

#[cfg(windows)]
pub(crate) fn fill_random(bytes: &mut [u8]) -> crate::Result<()> {
    use std::ffi::c_void;
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, length: u32, flags: u32)
        -> i32;
    }
    let length = u32::try_from(bytes.len())
        .map_err(|_| crate::Error::new(crate::ErrorKind::Host, "random buffer exceeds OS limit"))?;
    // SAFETY: the OS receives the live writable slice, its exact length, and a null
    // algorithm handle as required by BCRYPT_USE_SYSTEM_PREFERRED_RNG.
    let status = unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), length, 2) };
    if status < 0 {
        return Err(crate::Error::new(
            crate::ErrorKind::Host,
            format!("BCryptGenRandom failed: NTSTATUS {status:#x}"),
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn fill_random(bytes: &mut [u8]) -> crate::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(bytes))
        .map_err(|error| crate::Error::io("read OS entropy", error))
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn fill_random(_: &mut [u8]) -> crate::Result<()> {
    Err(crate::Error::new(
        crate::ErrorKind::Unsupported,
        "OS entropy is unavailable on this platform",
    ))
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod windows {
    use crate::{
        Error, ErrorKind, Result,
        cpu::{Register, Width},
        decode::{Destination, Instruction, Program, Source},
        flags::Condition,
        runtime::Context,
    };
    use std::{
        ffi::c_void,
        ptr::{self, NonNull},
    };

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualAlloc(
            address: *mut c_void,
            size: usize,
            allocation: u32,
            protection: u32,
        ) -> *mut c_void;
        fn VirtualProtect(
            address: *mut c_void,
            size: usize,
            protection: u32,
            previous: *mut u32,
        ) -> i32;
        fn VirtualFree(address: *mut c_void, size: usize, free_type: u32) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
        fn FlushInstructionCache(process: *mut c_void, address: *const c_void, size: usize) -> i32;
    }

    struct Executable {
        address: NonNull<c_void>,
    }

    impl Executable {
        fn new(code: &[u8]) -> Result<Self> {
            // SAFETY: OS chooses the address; size is bounded by the validated instruction budget.
            let allocation = unsafe { VirtualAlloc(ptr::null_mut(), code.len(), 0x3000, 0x04) };
            let address = NonNull::new(allocation).ok_or_else(|| host_error("VirtualAlloc"))?;
            let executable = Self { address };
            // SAFETY: the unique allocation is writable, at least code.len() bytes, and disjoint from code.
            unsafe {
                ptr::copy_nonoverlapping(code.as_ptr(), address.as_ptr().cast::<u8>(), code.len());
            }
            let mut previous = 0;
            // SAFETY: the allocation is live and committed; previous is a valid writable u32.
            if unsafe { VirtualProtect(address.as_ptr(), code.len(), 0x20, &mut previous) } == 0 {
                return Err(host_error("VirtualProtect"));
            }
            // SAFETY: this is the current process and the entire live generated-code region.
            if unsafe { FlushInstructionCache(GetCurrentProcess(), address.as_ptr(), code.len()) }
                == 0
            {
                return Err(host_error("FlushInstructionCache"));
            }
            Ok(executable)
        }

        fn invoke(&self, context: &mut Context) {
            // SAFETY: emit produces a Windows x64 function taking exactly one context pointer.
            // Direct stores target the register file and instruction budget. All calls target
            // the known Rust bridges; all guest branch targets are validated instruction entries.
            // The allocation is RX, remains live through the call, and context is exclusively borrowed.
            let entry: unsafe extern "system" fn(*mut Context) =
                unsafe { std::mem::transmute(self.address.as_ptr()) };
            // SAFETY: the generated entry's ABI, stack alignment, and context lifetime are established above.
            unsafe {
                entry(context);
            }
        }
    }

    impl Drop for Executable {
        fn drop(&mut self) {
            // SAFETY: this wrapper owns the original allocation and frees it once with MEM_RELEASE.
            unsafe {
                VirtualFree(self.address.as_ptr(), 0, 0x8000);
            }
        }
    }

    fn host_error(operation: &str) -> Error {
        Error::new(
            ErrorKind::Host,
            format!("{operation}: {}", std::io::Error::last_os_error()),
        )
    }

    unsafe extern "system" fn operation_bridge(
        context: *mut Context,
        instruction: *const Instruction,
    ) -> u32 {
        // SAFETY: only emitted code calls this with the live exclusive context and a pointer
        // into the immutable Program kept alive by execute until the generated call returns.
        let context = unsafe { &mut *context };
        // SAFETY: the caller-provided instruction belongs to that same live, immutable Program.
        let instruction = unsafe { &*instruction };
        u32::from(context.operation(instruction))
    }

    unsafe extern "system" fn condition_bridge(
        context: *const Context,
        instruction: *const Instruction,
    ) -> u32 {
        // SAFETY: emitted code passes a live context and an immutable instruction from Program.
        let context = unsafe { &*context };
        // SAFETY: Program remains live throughout execution and no guest write can reach it.
        let instruction = unsafe { &*instruction };
        if let Instruction::Branch { condition, .. } = *instruction {
            u32::from(context.condition(condition))
        } else {
            0
        }
    }

    unsafe extern "system" fn budget_bridge(context: *mut Context) {
        // SAFETY: emitted code passes the same live, exclusively borrowed context as every bridge.
        unsafe { &mut *context }.exhaust_budget();
    }

    fn call_bridge(code: &mut Vec<u8>, argument: u64, target: usize) {
        code.extend_from_slice(&[0x4c, 0x89, 0xe1, 0x48, 0xba]); // mov rcx,r12; mov rdx,argument
        code.extend_from_slice(&argument.to_le_bytes());
        code.extend_from_slice(&[0x48, 0xb8]);
        code.extend_from_slice(&(target as u64).to_le_bytes());
        code.extend_from_slice(&[0xff, 0xd0]);
    }

    fn patch(code: &mut [u8], offset: usize, target: usize) -> Result<()> {
        let displacement = i32::try_from(target as i64 - (offset + 4) as i64)
            .map_err(|_| Error::new(ErrorKind::Host, "native branch displacement exceeds limit"))?;
        code.get_mut(offset..offset + 4)
            .ok_or_else(|| Error::new(ErrorKind::Host, "invalid native patch offset"))?
            .copy_from_slice(&displacement.to_le_bytes());
        Ok(())
    }

    fn store(code: &mut Vec<u8>, register: Register, width: Width) {
        // mov [r12 + register-file offset], rax
        match width {
            Width::Byte => code.extend_from_slice(&[0x41, 0x88, 0x84, 0x24]),
            Width::Word => code.extend_from_slice(&[0x66, 0x41, 0x89, 0x84, 0x24]),
            Width::Dword | Width::Qword => code.extend_from_slice(&[0x49, 0x89, 0x84, 0x24]),
        }
        code.extend_from_slice(&(u32::from(register as u8) * 8).to_le_bytes());
    }

    fn load(code: &mut Vec<u8>, register: Register, width: Width) {
        match width {
            Width::Byte => code.extend_from_slice(&[0x41, 0x0f, 0xb6, 0x84, 0x24]),
            Width::Word => code.extend_from_slice(&[0x41, 0x0f, 0xb7, 0x84, 0x24]),
            Width::Dword => code.extend_from_slice(&[0x41, 0x8b, 0x84, 0x24]),
            Width::Qword => code.extend_from_slice(&[0x49, 0x8b, 0x84, 0x24]),
        }
        code.extend_from_slice(&(u32::from(register as u8) * 8).to_le_bytes());
    }

    fn emit(program: &Program) -> Result<Vec<u8>> {
        // push r12; sub rsp, 32; mov r12, rcx.
        // Entry RSP is 8 mod 16. Saving R12 aligns it; 32 bytes reserve the shadow store.
        let mut code = vec![0x41, 0x54, 0x48, 0x83, 0xec, 0x20, 0x49, 0x89, 0xcc];
        code.push(0xe9);
        let entry_jump = code.len();
        code.extend_from_slice(&[0; 4]);
        let mut exits = Vec::new();
        let mut budget_exits = Vec::new();
        let mut branches = Vec::new();
        let mut returns = Vec::new();
        let mut offsets = Vec::new();
        for (instruction_index, instruction) in program.instructions().iter().enumerate() {
            offsets.push(code.len());
            // cmp qword [r12+budget],0; je exhausted; dec qword [r12+budget].
            let budget_offset = std::mem::offset_of!(Context, steps_remaining) as u32;
            code.extend_from_slice(&[0x49, 0x83, 0xbc, 0x24]);
            code.extend_from_slice(&budget_offset.to_le_bytes());
            code.extend_from_slice(&[0, 0x0f, 0x84]);
            budget_exits.push(code.len());
            code.extend_from_slice(&[0; 4]);
            code.extend_from_slice(&[0x49, 0xff, 0x8c, 0x24]);
            code.extend_from_slice(&budget_offset.to_le_bytes());
            match *instruction {
                Instruction::Move {
                    destination: Destination::Register(register),
                    source: Source::Immediate(value),
                    width,
                } => {
                    code.extend_from_slice(&[0x48, 0xb8]); // mov rax, imm64
                    code.extend_from_slice(&(value & width.mask()).to_le_bytes());
                    store(&mut code, register, width);
                }
                Instruction::Move {
                    destination: Destination::Register(destination),
                    source: Source::Register(source),
                    width,
                } => {
                    load(&mut code, source, width);
                    store(&mut code, destination, width);
                }
                Instruction::Nop => (),
                Instruction::Call { target, .. } => {
                    call_bridge(
                        &mut code,
                        instruction as *const Instruction as usize as u64,
                        operation_bridge as *const () as usize,
                    );
                    code.extend_from_slice(&[0x85, 0xc0, 0x0f, 0x85]);
                    exits.push(code.len());
                    code.extend_from_slice(&[0; 4]);
                    code.push(0xe9);
                    branches.push((code.len(), target));
                    code.extend_from_slice(&[0; 4]);
                }
                Instruction::Return { .. } => {
                    call_bridge(
                        &mut code,
                        instruction as *const Instruction as usize as u64,
                        operation_bridge as *const () as usize,
                    );
                    code.extend_from_slice(&[0x85, 0xc0, 0x0f, 0x85]);
                    exits.push(code.len());
                    code.extend_from_slice(&[0; 4]);
                    code.push(0xe9);
                    returns.push(code.len());
                    code.extend_from_slice(&[0; 4]);
                }
                Instruction::Branch { condition, target } => {
                    if condition == Condition::Always {
                        code.push(0xe9);
                    } else {
                        call_bridge(
                            &mut code,
                            instruction as *const Instruction as usize as u64,
                            condition_bridge as *const () as usize,
                        );
                        code.extend_from_slice(&[0x85, 0xc0, 0x0f, 0x85]);
                    }
                    branches.push((code.len(), target));
                    code.extend_from_slice(&[0; 4]);
                }
                _ => {
                    call_bridge(
                        &mut code,
                        instruction as *const Instruction as usize as u64,
                        operation_bridge as *const () as usize,
                    );
                    code.extend_from_slice(&[0x85, 0xc0, 0x0f, 0x85]); // test eax,eax; jne epilogue
                    exits.push(code.len());
                    code.extend_from_slice(&[0; 4]);
                }
            }
            if !matches!(
                *instruction,
                Instruction::Call { .. }
                    | Instruction::Return { .. }
                    | Instruction::Branch {
                        condition: Condition::Always,
                        ..
                    }
            ) && program.fallthrough(instruction_index) != Some(instruction_index + 1)
            {
                code.push(0xe9);
                if let Some(next) = program.fallthrough(instruction_index) {
                    branches.push((code.len(), program.addresses()[next]));
                } else {
                    exits.push(code.len());
                }
                code.extend_from_slice(&[0; 4]);
            }
        }
        code.push(0xe9); // Normal fallthrough skips the exhaustion diagnostic.
        exits.push(code.len());
        code.extend_from_slice(&[0; 4]);
        let dispatch = code.len();
        // RET's helper proves the index belongs to Program. Dispatch only through this
        // immutable table of validated instruction entries, never through a guest pointer.
        code.extend_from_slice(&[0x48, 0x8d, 0x05]); // lea rax,[rip+table]
        let table_pointer = code.len();
        code.extend_from_slice(&[0; 4]);
        code.extend_from_slice(&[0x49, 0x8b, 0x94, 0x24]); // mov rdx,[r12+return index]
        code.extend_from_slice(
            &(std::mem::offset_of!(Context, return_target_index) as u32).to_le_bytes(),
        );
        code.extend_from_slice(&[0x48, 0x63, 0x14, 0x90, 0x48, 0x01, 0xd0, 0xff, 0xe0]); // signed relative table offset; add; jmp rax
        let exhausted = code.len();
        call_bridge(&mut code, 0, budget_bridge as *const () as usize);
        let epilogue = code.len();
        code.extend_from_slice(&[0x48, 0x83, 0xc4, 0x20, 0x41, 0x5c, 0xc3]); // restore stack and R12; ret
        let table = code.len();
        for &offset in &offsets {
            let relative = i32::try_from(offset as i64 - table as i64).map_err(|_| {
                Error::new(
                    ErrorKind::Host,
                    "native return table exceeds displacement limit",
                )
            })?;
            code.extend_from_slice(&relative.to_le_bytes());
        }
        patch(&mut code, entry_jump, offsets[program.entry_index()])?;
        patch(&mut code, table_pointer, table)?;
        for offset in returns {
            patch(&mut code, offset, dispatch)?;
        }
        for offset in exits {
            patch(&mut code, offset, epilogue)?;
        }
        for offset in budget_exits {
            patch(&mut code, offset, exhausted)?;
        }
        for (offset, target) in branches {
            let index = program
                .index_of(target)
                .ok_or_else(|| Error::new(ErrorKind::Host, "unvalidated guest branch target"))?;
            patch(&mut code, offset, offsets[index])?;
        }
        Ok(code)
    }

    pub(super) fn execute(
        program: &Program,
        context: &mut Context,
    ) -> Result<crate::runtime::ExecutionMetrics> {
        let started = std::time::Instant::now();
        let code = emit(program)?;
        let translation = started.elapsed();
        let started = std::time::Instant::now();
        let executable = Executable::new(&code)?;
        let executable_setup = started.elapsed();
        let started = std::time::Instant::now();
        executable.invoke(context);
        let execution = started.elapsed();
        Ok(crate::runtime::ExecutionMetrics {
            translation,
            executable_setup,
            execution,
            native_code_bytes: code.len(),
        })
    }
}

pub(crate) fn execute(
    program: &crate::decode::Program,
    context: &mut crate::runtime::Context,
) -> crate::Result<crate::runtime::ExecutionMetrics> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        windows::execute(program, context)
    }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    {
        let _ = (program, context);
        Err(crate::Error::new(
            crate::ErrorKind::Unsupported,
            "native execution requires Windows x86-64",
        ))
    }
}
