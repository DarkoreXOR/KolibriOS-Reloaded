;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; checksum_1 — Rust embed + runtime smoke (Cut E)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_checksum_1).
;; See docs/migration/cut-e-plan.md / cut-e-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-checksum1.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_checksum_1.bin
;;
;; kernel/network/stack.inc (USE_RUST_CHECKSUM_1=1) trampolines
;; checksum_1 → rust_checksum_1.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust checksum_1 path ran.
rust_checksum_1_smoke_result dd 0
checksum1_smoke_icmp:
        db 0x08, 0x00, 0x00, 0x00, 0x12, 0x34, 0x00, 0x01
checksum1_smoke_word:
        db 0xde, 0xad
checksum1_smoke_odd:
        db 0xff
checksum1_smoke_five:
        db 0x01, 0x02, 0x03, 0x04, 0x05
checksum1_smoke_nine:
        db 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa
endg

; Called once from high_code after Cut D smoke.
; Exercises: FASM callers → checksum_1 trampoline → Rust body.
; Vectors: empty, odd, 2/5/8/9-byte, non-zero seed; EBX/EDI/EBP preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
checksum1_rust_smoke_test:
        ; Callee-saved regs the FASM body preserved (ESI/ECX are clobbered).
        mov     ebx, 0xB1000001
        mov     edi, 0xD1000002
        mov     ebp, 0xE1000003

        ; empty / seed 0 → 0
        xor     edx, edx
        mov     esi, checksum1_smoke_icmp
        xor     ecx, ecx
        call    checksum_1
        test    edx, edx
        jnz     .fail

        ; 8-byte ICMP-like, seed 0 → 0x1A35
        xor     edx, edx
        mov     esi, checksum1_smoke_icmp
        mov     ecx, 8
        call    checksum_1
        cmp     edx, 0x1A35
        jne     .fail

        ; 2-byte, seed 0 → 0xDEAD
        xor     edx, edx
        mov     esi, checksum1_smoke_word
        mov     ecx, 2
        call    checksum_1
        cmp     edx, 0xDEAD
        jne     .fail

        ; 1-byte odd remnant, seed 0x12345678 → 0x12345579
        mov     edx, 0x12345678
        mov     esi, checksum1_smoke_odd
        mov     ecx, 1
        call    checksum_1
        cmp     edx, 0x12345579
        jne     .fail

        ; 5-byte (4+1), seed 0 → 0x0906
        xor     edx, edx
        mov     esi, checksum1_smoke_five
        mov     ecx, 5
        call    checksum_1
        cmp     edx, 0x0906
        jne     .fail

        ; 9-byte (8+1), seed 0xFFFF → 0x54AB
        mov     edx, 0xFFFF
        mov     esi, checksum1_smoke_nine
        mov     ecx, 9
        call    checksum_1
        cmp     edx, 0x54AB
        jne     .fail

        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     edi, 0xD1000002
        jne     .fail
        cmp     ebp, 0xE1000003
        jne     .fail

        mov     [rust_checksum_1_smoke_result], 0x434B5331 ; 'CKS1'
        ret
.fail:
        mov     eax, 0xDEAD0C51        ; checksum_1 fail marker
        mov     ebx, 0x434B5331        ; 'CKS1'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_checksum_1(seed, data, length) -> EAX (partial sum); ret 12
align 16
rust_checksum_1:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_checksum_1.bin'
