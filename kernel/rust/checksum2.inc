;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; checksum_2 — Rust embed + runtime smoke (Cut F)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_checksum_2).
;; See docs/migration/cut-f-plan.md / cut-f-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-checksum2.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_checksum_2.bin
;;
;; kernel/network/stack.inc (USE_RUST_CHECKSUM_2=1) trampolines
;; checksum_2 → rust_checksum_2.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust checksum_2 path ran.
rust_checksum_2_smoke_result dd 0
; ICMP-like buffer for real-caller-style checksum_1 → checksum_2 chain.
checksum2_smoke_icmp:
        db 0x08, 0x00, 0x00, 0x00, 0x12, 0x34, 0x00, 0x01
endg

; Called once from high_code after Cut E smoke.
; Exercises: FASM callers → checksum_2 trampoline → Rust body.
; Vectors: zero quirk, ~FFFF, ICMP final, max fold, checksum_1→checksum_2 chain;
; EBX/ESI/EDI/EBP preservation (FASM body left these alone).
; On mismatch: distinctive hang (does not reach desktop).
align 4
checksum2_rust_smoke_test:
        mov     ebx, 0xB2000001
        mov     esi, 0x52000002
        mov     edi, 0xD2000003
        mov     ebp, 0xE2000004

        ; edx=0 → DX=0xFEFF (zero quirk)
        xor     edx, edx
        call    checksum_2
        cmp     dx, 0xFEFF
        jne     .fail

        ; edx=0xFFFF → DX=0
        mov     edx, 0xFFFF
        call    checksum_2
        test    dx, dx
        jnz     .fail

        ; edx=0x1A35 → DX=0xCAE5 (ICMP-like partial finalize)
        mov     edx, 0x1A35
        call    checksum_2
        cmp     dx, 0xCAE5
        jne     .fail

        ; edx=0xFFFFFFFF → DX=0
        mov     edx, 0xFFFFFFFF
        call    checksum_2
        test    dx, dx
        jnz     .fail

        ; edx=0x12345678 → DX=0x5397
        mov     edx, 0x12345678
        call    checksum_2
        cmp     dx, 0x5397
        jne     .fail

        ; Real-caller-style chain: checksum_1 then checksum_2 on static ICMP bytes.
        ; (Uses public checksum_1 / checksum_2 symbols — production call sequence.)
        xor     edx, edx
        mov     esi, checksum2_smoke_icmp
        mov     ecx, 8
        call    checksum_1
        cmp     edx, 0x1A35
        jne     .fail
        call    checksum_2
        cmp     dx, 0xCAE5
        jne     .fail

        cmp     ebx, 0xB2000001
        jne     .fail
        ; ESI was clobbered by checksum_1 in the chain test — re-check only
        ; regs that checksum_2 alone must preserve. Re-run a pure checksum_2
        ; preserve check with fresh sentinels for ESI.
        mov     ebx, 0xB2000001
        mov     esi, 0x52000002
        mov     edi, 0xD2000003
        mov     ebp, 0xE2000004
        mov     edx, 0x1A35
        call    checksum_2
        cmp     dx, 0xCAE5
        jne     .fail
        cmp     ebx, 0xB2000001
        jne     .fail
        cmp     esi, 0x52000002
        jne     .fail
        cmp     edi, 0xD2000003
        jne     .fail
        cmp     ebp, 0xE2000004
        jne     .fail

        mov     [rust_checksum_2_smoke_result], 0x434B5332 ; 'CKS2'
        ret
.fail:
        mov     eax, 0xDEAD0C52        ; checksum_2 fail marker
        mov     ebx, 0x434B5332        ; 'CKS2'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_checksum_2(sum) -> EAX (final checksum); ret 4
align 16
rust_checksum_2:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_checksum_2.bin'
