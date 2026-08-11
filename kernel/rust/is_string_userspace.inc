;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; is_string_userspace — Rust embed + in-kernel smoke (Cut BJ)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_is_string_userspace).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_is_string_userspace.bin
;;
;; Requires USE_RUST_IS_STRING_USERSPACE=1 so public is_string_userspace
;; hits: FASM trampoline → rust_is_string_userspace (not the FASM body).
;;
;; Critical ABI: stdcall ZF-out; preserve EAX/ECX/EDI (+ EDX canary);
;; ret 4. REG-001: Rust stdcall clobbers ECX/EDX — trampoline restores.
;; REG-003: reject-only smoke (base≥OS_BASE) — userspace PDT cleared
;; before early-init smokes; no live process/FS/net mutation.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust is_string_userspace path ran.
rust_is_string_userspace_smoke_result dd 0
endg

; Called once from high_code after Cut P / AZ smokes.
; Exercises: public symbol → trampoline → Rust → ZF + reg preserve.
; Accept/NUL-scan covered by host differential (userspace unmapped here).
; On mismatch: distinctive hang (does not reach desktop).
align 4
is_string_userspace_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        push    ebx
        push    esi
        push    edi

        ; ========== base=OS_BASE → ZF=0 + regs ==========
        mov     eax, 0xA0000001
        mov     ecx, 0xC0000002
        mov     edx, 0xD0000003
        mov     ebx, 0xB0000004
        mov     esi, 0x50000005
        mov     edi, 0xE0000006
        stdcall is_string_userspace, OS_BASE
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xA0000001
        jne     .fail
        cmp     ecx, 0xC0000002
        jne     .fail
        cmp     edx, 0xD0000003
        jne     .fail
        cmp     ebx, 0xB0000004
        jne     .fail
        cmp     esi, 0x50000005
        jne     .fail
        cmp     edi, 0xE0000006
        jne     .fail

        ; ========== base=OS_BASE+1 → ZF=0 ==========
        mov     eax, 0xA1000001
        mov     ecx, 0xC1000002
        mov     edx, 0xD1000003
        mov     edi, 0xE1000006
        stdcall is_string_userspace, OS_BASE+1
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xA1000001
        jne     .fail
        cmp     ecx, 0xC1000002
        jne     .fail
        cmp     edx, 0xD1000003
        jne     .fail
        cmp     edi, 0xE1000006
        jne     .fail

        ; ========== base=0xFFFFFFFF → ZF=0 ==========
        mov     eax, 0xA2000001
        mov     ecx, 0xC2000002
        mov     edx, 0xD2000003
        mov     edi, 0xE2000006
        stdcall is_string_userspace, 0xFFFFFFFF
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xA2000001
        jne     .fail
        cmp     ecx, 0xC2000002
        jne     .fail
        cmp     edx, 0xD2000003
        jne     .fail
        cmp     edi, 0xE2000006
        jne     .fail

        ; ========== Direct rust_*: kernel iglobal ptr → 0 (base≥OS_BASE) ==========
        stdcall rust_is_string_userspace, rust_is_string_userspace_smoke_result
        test    eax, eax
        jnz     .fail

        ; Extra ZF capture via setz (after regs already proven); clobbers AL only.
        stdcall is_string_userspace, OS_BASE
        setz    al
        test    al, al
        jnz     .fail

        mov     [rust_is_string_userspace_smoke_result], 0x53555349 ; 'ISUS'
        pop     edi
        pop     esi
        pop     ebx
        mov     esp, ebp
        pop     ebp
        ret
.fail:
        mov     eax, 0xDEAD0C51         ; Cut BJ fail marker
        mov     ebx, 0x53555349         ; 'ISUS'
        mov     ecx, 0x4641494C         ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_is_string_userspace(base) -> EAX ∈ {0,1}; ret 4
align 16
rust_is_string_userspace:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_is_string_userspace.bin'
