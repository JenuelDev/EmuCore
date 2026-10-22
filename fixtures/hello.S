# Reference source for fixture::hello(), using GNU assembler Intel syntax.
# The Rust generator emits the ELF directly, so no assembler is required to build it.
.intel_syntax noprefix
.global _start
.section .text
_start:
    mov eax, 1
    mov edi, 1
    lea rsi, [rip + message]
    mov edx, message_end - message
    syscall
    mov eax, 60
    xor edi, edi
    syscall
.section .rodata
message:
    .ascii "Hello from EmuCore's Linux ELF fixture!\n"
message_end:
