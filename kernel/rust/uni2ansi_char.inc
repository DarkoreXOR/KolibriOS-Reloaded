;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; uni2ansi_char — Rust embed + ABI smoke (Cut BZ)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_uni2ansi_char).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_uni2ansi_char.bin
;;
;; Requires USE_RUST_UNI2ANSI_CHAR=1 so public uni2ansi_char hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: AX in → AL out; trampoline preserves ECX+EDX (REG-001);
;; ret (not stdcall) at public symbol; Rust body is stdcall ret 4.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust uni2ansi_char path ran.
rust_uni2ansi_char_smoke_result dd 0
endg

; Called once from high_code after Cut BY smoke.
; Exercises: public trampoline + direct rust_*; known vectors; ECX/EDX canaries;
; F0–F7 special table; unk → '_'; loop-counter survival.
; On mismatch: distinctive hang (does not reach desktop).
align 4
uni2ansi_char_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- ASCII 'A' via public trampoline; ECX/EDX canaries (REG-001) ---
        mov     ecx, 0xC2C20001
        mov     edx, 0xD3D30002
        mov     ax, 0x0041
        call    uni2ansi_char
        cmp     al, 0x41
        jne     .fail
        cmp     ecx, 0xC2C20001
        jne     .fail
        cmp     edx, 0xD3D30002
        jne     .fail

        ; --- U+00B6 → 0x14 ---
        mov     ax, 0x00B6
        call    uni2ansi_char
        cmp     al, 20
        jne     .fail

        ; --- U+0410 → 0x80 ---
        mov     ax, 0x0410
        call    uni2ansi_char
        cmp     al, 0x80
        jne     .fail

        ; --- U+043F → 0xAF ---
        mov     ax, 0x043F
        call    uni2ansi_char
        cmp     al, 0xAF
        jne     .fail

        ; --- U+0440 → 0xE0 ---
        mov     ax, 0x0440
        call    uni2ansi_char
        cmp     al, 0xE0
        jne     .fail

        ; --- U+044F → 0xEF ---
        mov     ax, 0x044F
        call    uni2ansi_char
        cmp     al, 0xEF
        jne     .fail

        ; --- F0–F7 special table ---
        mov     ax, 0x0401
        call    uni2ansi_char
        cmp     al, 0xF0
        jne     .fail
        mov     ax, 0x0451
        call    uni2ansi_char
        cmp     al, 0xF1
        jne     .fail
        mov     ax, 0x045E
        call    uni2ansi_char
        cmp     al, 0xF7
        jne     .fail

        ; --- unk ranges → '_' ---
        mov     ax, 0x0400
        call    uni2ansi_char
        cmp     al, '_'
        jne     .fail
        mov     ax, 0x0450
        call    uni2ansi_char
        cmp     al, '_'
        jne     .fail

        ; --- direct rust_* stdcall (Ё via U+0401) ---
        stdcall rust_uni2ansi_char, 0x0401
        cmp     al, 0xF0
        jne     .fail

        ; --- loop-counter survival (ISO9660 / NTFS pattern) ---
        mov     ecx, 8
@@:
        mov     ax, 0x0410
        call    uni2ansi_char
        cmp     al, 0x80
        jne     .fail
        loop    @b
        test    ecx, ecx
        jnz     .fail

        mov     [rust_uni2ansi_char_smoke_result], 0x4E415332 ; 'U2AN'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD02A0        ; Cut BZ fail marker
        mov     ebx, 0x425A554E        ; 'BZUN'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_uni2ansi_char(cp) -> EAX (AL = CP866); ret 4
align 16
rust_uni2ansi_char:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_uni2ansi_char.bin'
