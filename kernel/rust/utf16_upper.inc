;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; UTF-16 uppercase — Rust embed + runtime smoke (Cut C)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_utf16_to_upper).
;; See docs/migration/cut-c-plan.md / cut-c-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-utf16-upper.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_utf16_to_upper.bin
;;
;; kernel/fs/parse_fn.inc (USE_RUST_UTF16_UPPER=1) trampolines
;; utf16toUpper → rust_utf16_to_upper.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; Latin 'a' → 'A' exercises the ASCII path through the real trampoline.
UTF16_UPPER_SMOKE_IN     = 'a'
UTF16_UPPER_SMOKE_EXPECT = 'A'
; Cyrillic ѐ (U+0450) → Ѐ (U+0400) exercises the −80 path.
UTF16_UPPER_SMOKE_IN2     = 0x450
UTF16_UPPER_SMOKE_EXPECT2 = 0x400

iglobal
; Survives boot: observable proof that Rust utf16toUpper path ran.
rust_utf16_upper_smoke_result dd 0
endg

; Called once from high_code after Cut B smoke.
; Exercises: FASM callers → utf16toUpper trampoline → Rust body.
; On mismatch: distinctive hang (does not reach desktop).
align 4
utf16_upper_rust_smoke_test:
        mov     eax, UTF16_UPPER_SMOKE_IN
        call    utf16toUpper
        cmp     ax, UTF16_UPPER_SMOKE_EXPECT
        jne     .fail
        mov     eax, UTF16_UPPER_SMOKE_IN2
        call    utf16toUpper
        cmp     ax, UTF16_UPPER_SMOKE_EXPECT2
        jne     .fail
        movzx   eax, ax
        mov     [rust_utf16_upper_smoke_result], eax
        ret
.fail:
        mov     eax, 0xDEAD1651        ; UTF-16 upper fail marker
        mov     ebx, 0x55313655        ; 'U16U'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_utf16_to_upper(ch) -> EAX (AX = uppercased UTF-16); ret 4
align 16
rust_utf16_to_upper:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_utf16_to_upper.bin'
