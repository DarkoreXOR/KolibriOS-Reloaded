;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; file_system_is_operation_safe — Rust embed + ABI smoke (Cut AZ)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_file_system_is_operation_safe).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_file_system_is_operation_safe.bin
;;
;; Requires USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE=1 so the public
;; file_system_is_operation_safe hits the trampoline (not the FASM body).
;;
;; Critical ABI: stdcall(inf) → ZF; preserve EAX/EBX/ECX/EDX (REG-001).
;;
;; REG-003: smoke uses only uglobal synthetic inf structs — never mutates
;; live filesystem mounts or boot-initialized tables.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust file_system_is_operation_safe ran.
rust_file_system_is_operation_safe_smoke_result dd 0
endg

uglobal
align 4
; Synthetic sysfn70 info structs (20 bytes each: subfn/enc/count/base).
fs_op_safe_smoke_inf0:  rd 5
fs_op_safe_smoke_inf1:  rd 5
fs_op_safe_smoke_inf_bad: rd 5
fs_op_safe_smoke_inf_unk: rd 5
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline ZF/regs on synthetic inf only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
file_system_is_operation_safe_rust_smoke_test:
        pushad

        ; --- Plant synthetic inf0: subfn0, size=1, base=0 → safe ---
        mov     dword [fs_op_safe_smoke_inf0], 0          ; subfn
        mov     dword [fs_op_safe_smoke_inf0 + 8], 0       ; encoding
        mov     dword [fs_op_safe_smoke_inf0 + 12], 1      ; size
        mov     dword [fs_op_safe_smoke_inf0 + 16], 0      ; base

        ; --- Direct rust_*: safe ---
        stdcall rust_file_system_is_operation_safe, fs_op_safe_smoke_inf0
        cmp     eax, 1
        jne     .fail

        ; --- Plant inf_bad: subfn0, size=1, base=OS_BASE → unsafe ---
        mov     dword [fs_op_safe_smoke_inf_bad], 0
        mov     dword [fs_op_safe_smoke_inf_bad + 8], 0
        mov     dword [fs_op_safe_smoke_inf_bad + 12], 1
        mov     dword [fs_op_safe_smoke_inf_bad + 16], 0x80000000

        stdcall rust_file_system_is_operation_safe, fs_op_safe_smoke_inf_bad
        test    eax, eax
        jnz     .fail

        ; --- Plant inf1: subfn1 unicode, count=1, base=0 → len=592, safe ---
        mov     dword [fs_op_safe_smoke_inf1], 1
        mov     dword [fs_op_safe_smoke_inf1 + 8], 2       ; encoding > 1
        mov     dword [fs_op_safe_smoke_inf1 + 12], 1
        mov     dword [fs_op_safe_smoke_inf1 + 16], 0

        stdcall rust_file_system_is_operation_safe, fs_op_safe_smoke_inf1
        cmp     eax, 1
        jne     .fail

        ; --- Unknown subfn 4 + kernel base → still safe (legacy quirk) ---
        mov     dword [fs_op_safe_smoke_inf_unk], 4
        mov     dword [fs_op_safe_smoke_inf_unk + 8], 0
        mov     dword [fs_op_safe_smoke_inf_unk + 12], 0xFFFFFFFF
        mov     dword [fs_op_safe_smoke_inf_unk + 16], 0x80000000

        stdcall rust_file_system_is_operation_safe, fs_op_safe_smoke_inf_unk
        cmp     eax, 1
        jne     .fail

        ; --- Public trampoline: safe + register canaries ---
        mov     eax, 0xA1A10001
        mov     ebx, 0xB1B10002
        mov     ecx, 0xC1C10003
        mov     edx, 0xD1D10004
        stdcall file_system_is_operation_safe, fs_op_safe_smoke_inf0
        jnz     .fail                         ; expect ZF=1
        cmp     eax, 0xA1A10001
        jne     .fail
        cmp     ebx, 0xB1B10002
        jne     .fail
        cmp     ecx, 0xC1C10003
        jne     .fail
        cmp     edx, 0xD1D10004
        jne     .fail

        ; --- Public trampoline: unsafe + canaries ---
        mov     eax, 0xA2A20011
        mov     ebx, 0xB2B20012
        mov     ecx, 0xC2C20013
        mov     edx, 0xD2D20014
        stdcall file_system_is_operation_safe, fs_op_safe_smoke_inf_bad
        jz      .fail                         ; expect ZF=0
        cmp     eax, 0xA2A20011
        jne     .fail
        cmp     ebx, 0xB2B20012
        jne     .fail
        cmp     ecx, 0xC2C20013
        jne     .fail
        cmp     edx, 0xD2D20014
        jne     .fail

        ; --- Public trampoline: unknown-subfn accept + canaries ---
        mov     eax, 0xA3A30021
        mov     ebx, 0xB3B30022
        mov     ecx, 0xC3C30023
        mov     edx, 0xD3D30024
        stdcall file_system_is_operation_safe, fs_op_safe_smoke_inf_unk
        jnz     .fail                         ; expect ZF=1
        cmp     eax, 0xA3A30021
        jne     .fail
        cmp     ebx, 0xB3B30022
        jne     .fail
        cmp     ecx, 0xC3C30023
        jne     .fail
        cmp     edx, 0xD3D30024
        jne     .fail

        mov     [rust_file_system_is_operation_safe_smoke_result], 0x534F5346 ; 'FSOS'
        popad
        ret

.fail:
        ; Distinctive hang pattern (never reaches desktop).
        mov     [rust_file_system_is_operation_safe_smoke_result], 0x44414544 ; 'DEAD'
@@:
        hlt
        jmp     @b

; Raw reloc-free blob
align 4
rust_file_system_is_operation_safe:
        file '../../rust_kernel/kolibri_utils/out/rust_file_system_is_operation_safe.bin'
