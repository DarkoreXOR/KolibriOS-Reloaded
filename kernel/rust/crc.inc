;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; CRC32 — Rust embed + runtime smoke (removable diagnostics)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_crc_32). See docs/migration/crc32-migration.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-crc.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_crc_32.bin
;;
;; kernel/crc.inc (USE_RUST_CRC=1) trampolines crc_32 → rust_crc_32.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; IEEE CRC-32/ISO-HDLC of "123456789" after init/final XOR with -1.
CRC_SMOKE_EXPECT = 0xCBF43926

iglobal
; Survives boot: observable proof that Rust CRC path ran.
rust_crc_smoke_result dd 0
; Known vector buffer (must remain readable at smoke-test time).
crc_smoke_data:
        db '123456789'
endg

; Called once from high_code after Phase C probe.
; Exercises: FASM callers → crc_32 trampoline → Rust body → expected digest.
; On mismatch: distinctive hang (does not reach desktop).
align 4
crc_rust_smoke_test:
        mov     eax, -1
        stdcall crc_32, 0xEDB88320, crc_smoke_data, 9
        xor     eax, -1
        cmp     eax, CRC_SMOKE_EXPECT
        jne     .fail
        mov     [rust_crc_smoke_result], eax
        ret
.fail:
        mov     eax, 0xDEADC2C0        ; CRC fail marker
        mov     ebx, 0x43524352        ; 'RCRC'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_crc_32(partial, poly, buffer, length) -> EAX; ret 16
align 16
rust_crc_32:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_crc_32.bin'
