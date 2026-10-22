.intel_syntax noprefix
.global _start
.section .text
_start:
    cmp qword ptr [rsp], 2
    jb missing
    mov rsi, [rsp + 16]
    xor edx, edx
measure:
    cmp byte ptr [rsi + rdx], 0
    je print_argument
    inc rdx
    jmp measure
print_argument:
    mov eax, 1
    mov edi, 1
    syscall
    xor edi, edi
    mov eax, 60
    syscall
missing:
    mov edi, 64
    mov eax, 60
    syscall
