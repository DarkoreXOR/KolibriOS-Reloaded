;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fat_name_is_legal — Rust embed + ABI smoke (Cut BC)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fat_name_is_legal).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fat_name_is_legal.bin
;;
;; Requires USE_RUST_FAT_NAME_IS_LEGAL=1 so public fat_name_is_legal hits
;; the trampoline (not the FASM body).
;;
;; Critical ABI: ESI → CF; preserve ECX/EDX (REG-001); synthetic names only
;; (REG-003 — never mutate live FAT mount / directory buffers).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fat_name_is_legal path ran.
rust_fat_name_is_legal_smoke_result dd 0
fat_legal_smoke_ok:
        db 'readme.txt',0
fat_legal_smoke_bad:
        db 'a*b',0
fat_legal_smoke_empty:
        db 0
fat_legal_smoke_lfn:
        db 'ok+plus',0
fat_legal_smoke_brace:
        db 'x|y',0
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on synthetic iglobal strings only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fat_name_is_legal_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: legal name ---
        stdcall rust_fat_name_is_legal, fat_legal_smoke_ok
        cmp     eax, 1
        jne     .fail

        ; --- Direct rust_*: illegal '*' ---
        stdcall rust_fat_name_is_legal, fat_legal_smoke_bad
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: empty → legal ---
        stdcall rust_fat_name_is_legal, fat_legal_smoke_empty
        cmp     eax, 1
        jne     .fail

        ; --- Direct rust_*: LFN-only '+' ---
        stdcall rust_fat_name_is_legal, fat_legal_smoke_lfn
        cmp     eax, 1
        jne     .fail

        ; --- Direct rust_*: '|' illegal ---
        stdcall rust_fat_name_is_legal, fat_legal_smoke_brace
        test    eax, eax
        jnz     .fail

        ; --- Public trampoline: CF + ECX/EDX (+siblings) canaries ---
        mov     eax, 0xA1A1A1A1
        mov     ebx, 0xB2B2B2B2
        mov     ecx, 0xC3C3C3C3
        mov     edx, 0xD4D4D4D4
        mov     esi, fat_legal_smoke_ok
        mov     edi, 0xE5E5E5E5
        mov     ebp, 0xF6F6F6F6
        call    fat_name_is_legal
        jnc     .fail
        cmp     ebx, 0xB2B2B2B2
        jne     .fail
        cmp     ecx, 0xC3C3C3C3
        jne     .fail
        cmp     edx, 0xD4D4D4D4
        jne     .fail
        cmp     esi, fat_legal_smoke_ok
        jne     .fail
        cmp     edi, 0xE5E5E5E5
        jne     .fail
        cmp     ebp, 0xF6F6F6F6
        jne     .fail

        ; --- Public trampoline: illegal clears CF; ESI preserved ---
        mov     ecx, 0x11111111
        mov     edx, 0x22222222
        mov     esi, fat_legal_smoke_bad
        call    fat_name_is_legal
        jc      .fail
        cmp     ecx, 0x11111111
        jne     .fail
        cmp     edx, 0x22222222
        jne     .fail
        cmp     esi, fat_legal_smoke_bad
        jne     .fail

        mov     [rust_fat_name_is_legal_smoke_result], 'FNIL'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_fat_name_is_legal_smoke_result], 'DEAD'
        ; Distinctive hang marker for Cut BC
@@:
        cli
        hlt
        jmp     @b

; ---------------------------------------------------------------------------
; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
; ---------------------------------------------------------------------------
align 4
rust_fat_name_is_legal:
        file '../../rust_kernel/kolibri_utils/out/rust_fat_name_is_legal.bin'
