;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; UTF-16 encode — Rust embed + runtime smoke (removable diagnostics)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_unicode_utf16_encode).
;; See docs/migration/utf16-migration.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-utf16.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_unicode_utf16_encode.bin
;;
;; kernel/unicode.inc (USE_RUST_UTF16=1) trampolines
;; unicode.utf16.encode → rust_unicode_utf16_encode.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; U+1F600 packed as FASM unicode.utf16.encode: low=D83D high=DE00.
UTF16_SMOKE_CP     = 0x1F600
UTF16_SMOKE_EXPECT = 0xDE00D83D

iglobal
; Survives boot: observable proof that Rust UTF-16 path ran.
rust_utf16_smoke_result dd 0
endg

; Called once from high_code after CRC smoke.
; Exercises: FASM callers → unicode.utf16.encode trampoline → Rust body.
; On mismatch: distinctive hang (does not reach desktop).
align 4
utf16_rust_smoke_test:
        mov     eax, UTF16_SMOKE_CP
        call    unicode.utf16.encode
        cmp     eax, UTF16_SMOKE_EXPECT
        jne     .fail
        mov     [rust_utf16_smoke_result], eax
        ret
.fail:
        mov     eax, 0xDEADF160        ; UTF-16 fail marker
        mov     ebx, 0x55313645        ; 'U16E'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_unicode_utf16_encode(cp) -> EAX; ret 4
align 16
rust_unicode_utf16_encode:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_unicode_utf16_encode.bin'
