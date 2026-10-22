//! Deterministic, project-owned static ELF fixture. No assembler or downloaded game code.
#![forbid(unsafe_code)]

pub const CODE_ADDRESS: u64 = 0x401000;
pub const DATA_ADDRESS: u64 = 0x402000;
pub const MESSAGE: &[u8] = b"Hello from EmuCore's Linux ELF fixture!\n";
pub const MEMORY_MESSAGE: &[u8] = b"Rust memory works\n";

pub fn hello() -> Vec<u8> {
    let mut code = vec![0xb8, 1, 0, 0, 0, 0xbf, 1, 0, 0, 0];
    // lea rsi, [rip + message], relative to the end of this seven-byte instruction.
    code.extend_from_slice(&[0x48, 0x8d, 0x35]);
    code.extend_from_slice(&((DATA_ADDRESS - (CODE_ADDRESS + 17)) as i32).to_le_bytes());
    code.push(0xba);
    code.extend_from_slice(&(MESSAGE.len() as u32).to_le_bytes());
    code.extend_from_slice(&[0x0f, 0x05, 0xb8, 60, 0, 0, 0, 0x31, 0xff, 0x0f, 0x05]);
    elf(&code, MESSAGE)
}

/// A guest program writes its own data using 64-bit and 16-bit memory stores.
pub fn memory_demo() -> Vec<u8> {
    let mut code = vec![0x48, 0xbe]; // mov rsi, data
    code.extend_from_slice(&DATA_ADDRESS.to_le_bytes());
    for (index, chunk) in MEMORY_MESSAGE.chunks(8).enumerate() {
        let mut value = [0; 8];
        value[..chunk.len()].copy_from_slice(chunk);
        code.extend_from_slice(&[0x48, 0xb8]);
        code.extend_from_slice(&value);
        match chunk.len() {
            8 => code.extend_from_slice(&[0x48, 0x89, 0x46, (index * 8) as u8]),
            2 => code.extend_from_slice(&[0x66, 0x89, 0x46, (index * 8) as u8]),
            _ => unreachable!("fixed fixture message uses qword and word chunks"),
        }
    }
    code.extend_from_slice(&[0xb8, 1, 0, 0, 0, 0xbf, 1, 0, 0, 0, 0xba]);
    code.extend_from_slice(&(MEMORY_MESSAGE.len() as u32).to_le_bytes());
    code.extend_from_slice(&[0x0f, 0x05, 0xb8, 60, 0, 0, 0, 0x31, 0xff, 0x0f, 0x05]);
    writable_elf(&code, &vec![0; MEMORY_MESSAGE.len()])
}

/// A bounded guest loop echoes argv[1]. Missing argv[1] exits 64.
pub fn arguments_demo() -> Vec<u8> {
    elf(
        &[
            0x48, 0x83, 0x3c, 0x24, 2, // cmp qword [rsp],2
            0x72, 0x27, // jb missing
            0x48, 0x8b, 0x74, 0x24, 0x10, // mov rsi,[rsp+16]
            0x31, 0xd2, // xor edx,edx
            0x80, 0x3c, 0x16, 0, // loop: cmp byte [rsi+rdx],0
            0x74, 5, // je print
            0x48, 0xff, 0xc2, // inc rdx
            0xeb, 0xf5, // jmp loop
            0xb8, 1, 0, 0, 0, 0xbf, 1, 0, 0, 0, 0x0f, 0x05, // print: write(1,rsi,rdx)
            0x31, 0xff, 0xb8, 60, 0, 0, 0, 0x0f, 0x05, // exit(0)
            0xbf, 64, 0, 0, 0, 0xb8, 60, 0, 0, 0, 0x0f, 0x05, // missing: exit(64)
        ],
        b"x",
    )
}

pub fn writable_elf(code: &[u8], data: &[u8]) -> Vec<u8> {
    let mut file = elf(code, data);
    put32(&mut file, 124, 6);
    file
}

/// Builds the same documented two-segment layout for small owned test fixtures.
pub fn elf(code: &[u8], data: &[u8]) -> Vec<u8> {
    let mut file = vec![0; 0x2000 + data.len().max(1)];
    file[..9].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0]);
    put16(&mut file, 16, 2);
    put16(&mut file, 18, 62);
    put32(&mut file, 20, 1);
    put64(&mut file, 24, CODE_ADDRESS);
    put64(&mut file, 32, 64);
    put16(&mut file, 52, 64);
    put16(&mut file, 54, 56);
    put16(&mut file, 56, 2);
    for (at, flags, offset, address, size) in [
        (64, 5, 0x1000, CODE_ADDRESS, code.len()),
        (120, 4, 0x2000, DATA_ADDRESS, data.len().max(1)),
    ] {
        put32(&mut file, at, 1);
        put32(&mut file, at + 4, flags);
        put64(&mut file, at + 8, offset);
        put64(&mut file, at + 16, address);
        put64(&mut file, at + 32, size as u64);
        put64(&mut file, at + 40, size as u64);
        put64(&mut file, at + 48, 4096);
    }
    // The owned fixture generator has a deliberately small, fixed code slot.
    assert!(
        code.len() <= 4096,
        "fixture code must fit in its segment slot"
    );
    file[0x1000..0x1000 + code.len()].copy_from_slice(code);
    file[0x2000..0x2000 + data.len()].copy_from_slice(data);
    file
}

fn put16(file: &mut [u8], at: usize, value: u16) {
    file[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(file: &mut [u8], at: usize, value: u32) {
    file[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(file: &mut [u8], at: usize, value: u64) {
    file[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
