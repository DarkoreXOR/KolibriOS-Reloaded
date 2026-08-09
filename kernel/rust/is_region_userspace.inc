;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; is_region_userspace — Rust embed + in-kernel smoke (Cut P Step 4)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_is_region_userspace).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-is-region-userspace.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_is_region_userspace.bin
;;
;; Requires USE_RUST_IS_REGION_USERSPACE=1 so public is_region_userspace
;; hits: FASM trampoline → rust_is_region_userspace (not the FASM body).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust is_region_userspace path ran.
rust_is_region_userspace_smoke_result dd 0
endg

; Called once from high_code after Cut O smoke.
; Exercises: public symbol → trampoline → Rust → ZF + EAX/ECX/EDX preserve.
; On mismatch: distinctive hang (does not reach desktop).
;
; Flag rule: observe ZF via jz/jnz immediately after stdcall (before any
; flag-clobbering insn). Do NOT use setz al before comparing EAX sentinels
; (setz al destroys AL).
align 4
is_region_userspace_rust_smoke_test:
        push    ebp
        mov     ebp, esp

        ; ========== base=0, len=0 → ZF=1 + regs ==========
        mov     eax, 0xA0000001
        mov     ecx, 0xC0000002
        mov     edx, 0xD0000003
        mov     ebx, 0xB0000004
        mov     esi, 0x50000005
        mov     edi, 0xE0000006
        stdcall is_region_userspace, 0, 0
        jz      @f
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

        ; ========== base=0, len=1 → ZF=1 + regs ==========
        mov     eax, 0xA1000001
        mov     ecx, 0xC1000002
        mov     edx, 0xD1000003
        stdcall is_region_userspace, 0, 1
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0xA1000001
        jne     .fail
        cmp     ecx, 0xC1000002
        jne     .fail
        cmp     edx, 0xD1000003
        jne     .fail

        ; ========== base=0x7FFFFFFF, len=0 → ZF=1 ==========
        mov     eax, 0xA2000001
        mov     ecx, 0xC2000002
        mov     edx, 0xD2000003
        stdcall is_region_userspace, 0x7FFFFFFF, 0
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0xA2000001
        jne     .fail
        cmp     ecx, 0xC2000002
        jne     .fail
        cmp     edx, 0xD2000003
        jne     .fail

        ; ========== base=0x7FFFFFFF, len=1 → end==OS_BASE → ZF=1 ==========
        mov     eax, 0xA3000001
        mov     ecx, 0xC3000002
        mov     edx, 0xD3000003
        stdcall is_region_userspace, 0x7FFFFFFF, 1
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0xA3000001
        jne     .fail
        cmp     ecx, 0xC3000002
        jne     .fail
        cmp     edx, 0xD3000003
        jne     .fail

        ; ========== base=0x7FFFFFFF, len=2 → end>OS_BASE → ZF=0 ==========
        mov     eax, 0xA4000001
        mov     ecx, 0xC4000002
        mov     edx, 0xD4000003
        stdcall is_region_userspace, 0x7FFFFFFF, 2
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xA4000001
        jne     .fail
        cmp     ecx, 0xC4000002
        jne     .fail
        cmp     edx, 0xD4000003
        jne     .fail

        ; ========== base=0x80000000, len=0 → ZF=0 ==========
        mov     eax, 0xA5000001
        mov     ecx, 0xC5000002
        mov     edx, 0xD5000003
        stdcall is_region_userspace, 0x80000000, 0
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xA5000001
        jne     .fail
        cmp     ecx, 0xC5000002
        jne     .fail
        cmp     edx, 0xD5000003
        jne     .fail

        ; ========== base=0x80000000, len=1 → ZF=0 ==========
        mov     eax, 0xA6000001
        mov     ecx, 0xC6000002
        mov     edx, 0xD6000003
        stdcall is_region_userspace, 0x80000000, 1
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xA6000001
        jne     .fail
        cmp     ecx, 0xC6000002
        jne     .fail
        cmp     edx, 0xD6000003
        jne     .fail

        ; ========== overflow-to-zero: base=1, len=0xFFFFFFFF → ZF=1 ==========
        mov     eax, 0xA7000001
        mov     ecx, 0xC7000002
        mov     edx, 0xD7000003
        stdcall is_region_userspace, 1, 0xFFFFFFFF
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0xA7000001
        jne     .fail
        cmp     ecx, 0xC7000002
        jne     .fail
        cmp     edx, 0xD7000003
        jne     .fail

        ; ========== overflow-to-zero: 0x40000000+0xC0000000 → ZF=1 ==========
        mov     eax, 0xA8000001
        mov     ecx, 0xC8000002
        mov     edx, 0xD8000003
        stdcall is_region_userspace, 0x40000000, 0xC0000000
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0xA8000001
        jne     .fail
        cmp     ecx, 0xC8000002
        jne     .fail
        cmp     edx, 0xD8000003
        jne     .fail

        ; ========== overflow non-zero: base=2, len=0xFFFFFFFF → ZF=0 ==========
        mov     eax, 0xA9000001
        mov     ecx, 0xC9000002
        mov     edx, 0xD9000003
        stdcall is_region_userspace, 2, 0xFFFFFFFF
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xA9000001
        jne     .fail
        cmp     ecx, 0xC9000002
        jne     .fail
        cmp     edx, 0xD9000003
        jne     .fail

        ; ========== overflow non-zero: 0x40000000+0xC0000001 → ZF=0 ==========
        mov     eax, 0xAA000001
        mov     ecx, 0xCA000002
        mov     edx, 0xDA000003
        stdcall is_region_userspace, 0x40000000, 0xC0000001
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 0xAA000001
        jne     .fail
        cmp     ecx, 0xCA000002
        jne     .fail
        cmp     edx, 0xDA000003
        jne     .fail

        ; Extra ZF capture via setz (after regs already proven); clobbers AL only.
        stdcall is_region_userspace, 0, 1
        setz    al
        cmp     al, 1
        jne     .fail
        stdcall is_region_userspace, 0x80000000, 0
        setz    al
        test    al, al
        jnz     .fail

        mov     [rust_is_region_userspace_smoke_result], 0x49525553 ; 'SURI'
        mov     esp, ebp
        pop     ebp
        ret
.fail:
        mov     eax, 0xDEAD0C50         ; Cut P fail marker
        mov     ebx, 0x49525553         ; 'SURI'
        mov     ecx, 0x4641494C         ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_is_region_userspace(base, len) -> EAX ∈ {0,1}; ret 8
align 16
rust_is_region_userspace:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_is_region_userspace.bin'
