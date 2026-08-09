;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; UTF-8 decode — Rust embed + runtime smoke (removable diagnostics)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_unicode_utf8_decode).
;; See docs/migration/utf8-migration.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-utf8.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_unicode_utf8_decode.bin
;;
;; kernel/unicode.inc (USE_RUST_UTF8=1) trampolines
;; unicode.utf8.decode → rust_unicode_utf8_decode.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; U+1F600 (😀) UTF-8 F0 9F 98 80 — exercises 4-byte path + ESI/ECX update.
UTF8_SMOKE_EXPECT_CP   = 0x1F600
UTF8_SMOKE_EXPECT_LEN  = 0

iglobal
; Survives boot: observable proof that Rust UTF-8 path ran.
rust_utf8_smoke_result dd 0
; Smoke input buffers (immutable reference bytes).
utf8_smoke_ascii  db 'A'
utf8_smoke_cyr    db 0xD0, 0x90          ; U+0410
utf8_smoke_euro   db 0xE2, 0x82, 0xAC    ; U+20AC
utf8_smoke_emoji  db 0xF0, 0x9F, 0x98, 0x80 ; U+1F600
utf8_smoke_trunc  db 0xD0                ; truncated → 0xFFFD, consume 1
utf8_smoke_bad    db 0xD0, 0x20          ; bad cont → 0xFFFD, consume 1
endg

; Called once from high_code after CP866 smoke.
; Exercises: FASM callers → unicode.utf8.decode trampoline → Rust body.
; On mismatch: distinctive hang (does not reach desktop).
align 4
utf8_rust_smoke_test:
        ; ASCII "A"
        mov     esi, utf8_smoke_ascii
        mov     ecx, 1
        call    unicode.utf8.decode
        cmp     eax, 'A'
        jne     .fail
        test    ecx, ecx
        jnz     .fail

        ; 2-byte Cyrillic A
        mov     esi, utf8_smoke_cyr
        mov     ecx, 2
        call    unicode.utf8.decode
        cmp     eax, 0x0410
        jne     .fail
        test    ecx, ecx
        jnz     .fail

        ; 3-byte Euro
        mov     esi, utf8_smoke_euro
        mov     ecx, 3
        call    unicode.utf8.decode
        cmp     eax, 0x20AC
        jne     .fail
        test    ecx, ecx
        jnz     .fail

        ; 4-byte emoji (primary stored result)
        mov     esi, utf8_smoke_emoji
        mov     ecx, 4
        call    unicode.utf8.decode
        cmp     eax, UTF8_SMOKE_EXPECT_CP
        jne     .fail
        cmp     ecx, UTF8_SMOKE_EXPECT_LEN
        jne     .fail
        mov     [rust_utf8_smoke_result], eax

        ; Truncated lead → FFFD, consume 1
        mov     esi, utf8_smoke_trunc
        mov     ecx, 1
        call    unicode.utf8.decode
        cmp     eax, 0xFFFD
        jne     .fail
        test    ecx, ecx
        jnz     .fail

        ; Invalid continuation → FFFD, consume 1 (length left = 1)
        mov     esi, utf8_smoke_bad
        mov     ecx, 2
        call    unicode.utf8.decode
        cmp     eax, 0xFFFD
        jne     .fail
        cmp     ecx, 1
        jne     .fail
        ret
.fail:
        mov     eax, 0xDEADF800        ; UTF-8 fail marker
        mov     ebx, 0x55384644        ; 'U8FD'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_unicode_utf8_decode(ptr_inout, len_inout) -> EAX; ret 8
align 16
rust_unicode_utf8_decode:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_unicode_utf8_decode.bin'
