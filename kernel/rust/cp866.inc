;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; CP866 encode — Rust embed + runtime smoke (removable diagnostics)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_unicode_cp866_encode).
;; See docs/migration/cp866-migration.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-cp866.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_unicode_cp866_encode.bin
;;
;; kernel/unicode.inc (USE_RUST_CP866=1) trampolines
;; unicode.cp866.encode → rust_unicode_cp866_encode.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; U+0401 (Ё) → CP866 0xF0 via FASM uni2ansi_char special table.
CP866_SMOKE_CP     = 0x0401
CP866_SMOKE_EXPECT = 0xF0

iglobal
; Survives boot: observable proof that Rust CP866 path ran.
rust_cp866_smoke_result dd 0
endg

; Called once from high_code after UTF-16 smoke.
; Exercises: FASM callers → unicode.cp866.encode trampoline → Rust body.
; On mismatch: distinctive hang (does not reach desktop).
align 4
cp866_rust_smoke_test:
        mov     eax, CP866_SMOKE_CP
        call    unicode.cp866.encode
        cmp     al, CP866_SMOKE_EXPECT
        jne     .fail
        movzx   eax, al
        mov     [rust_cp866_smoke_result], eax
        ret
.fail:
        mov     eax, 0xDEAD8660        ; CP866 fail marker
        mov     ebx, 0x43503836        ; 'CP86'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_unicode_cp866_encode(cp) -> EAX (AL = CP866); ret 4
align 16
rust_unicode_cp866_encode:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_unicode_cp866_encode.bin'
