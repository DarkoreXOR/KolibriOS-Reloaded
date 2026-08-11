;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ansi2uni_char — Rust embed + ABI smoke (Cut AN)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ansi2uni_char).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ansi2uni_char.bin
;;
;; Requires USE_RUST_ANSI2UNI_CHAR=1 so public ansi2uni_char hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: AL in → AX out; trampoline preserves ECX+EDX (REG-001);
;; ret (not stdcall) at public symbol; Rust body is stdcall ret 4.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust ansi2uni_char path ran.
rust_ansi2uni_char_smoke_result dd 0
endg

; Called once from high_code after Cut AM smoke.
; Exercises: public trampoline + direct rust_*; known vectors; ECX/EDX canaries;
; high-bit AL truncate; special F0–F7 table; unk → '_'.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ansi2uni_char_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- ASCII 'A' via public trampoline; ECX/EDX canaries (REG-001) ---
        mov     ecx, 0xC1C10001
        mov     edx, 0xD2D20002
        mov     eax, 0xFFFF0041         ; high junk + 'A'
        call    ansi2uni_char
        cmp     ax, 0x0041
        jne     .fail
        cmp     ecx, 0xC1C10001
        jne     .fail
        cmp     edx, 0xD2D20002
        jne     .fail

        ; --- 0x14 → U+00B6 ---
        mov     al, 20
        call    ansi2uni_char
        cmp     ax, 0x00B6
        jne     .fail

        ; --- 0x80 → U+0410 ---
        mov     al, 0x80
        call    ansi2uni_char
        cmp     ax, 0x0410
        jne     .fail

        ; --- 0xAF → U+043F ---
        mov     al, 0xAF
        call    ansi2uni_char
        cmp     ax, 0x043F
        jne     .fail

        ; --- 0xE0 → U+0440 ---
        mov     al, 0xE0
        call    ansi2uni_char
        cmp     ax, 0x0440
        jne     .fail

        ; --- 0xEF → U+044F ---
        mov     al, 0xEF
        call    ansi2uni_char
        cmp     ax, 0x044F
        jne     .fail

        ; --- F0–F7 special table ---
        mov     al, 0xF0
        call    ansi2uni_char
        cmp     ax, 0x0401
        jne     .fail
        mov     al, 0xF1
        call    ansi2uni_char
        cmp     ax, 0x0451
        jne     .fail
        mov     al, 0xF7
        call    ansi2uni_char
        cmp     ax, 0x045E
        jne     .fail

        ; --- unk ranges → '_' ---
        mov     al, 0xB0
        call    ansi2uni_char
        cmp     ax, '_'
        jne     .fail
        mov     al, 0xF8
        call    ansi2uni_char
        cmp     ax, '_'
        jne     .fail

        ; --- direct rust_* stdcall (Ё via F0) ---
        stdcall rust_ansi2uni_char, 0xF0
        cmp     ax, 0x0401
        jne     .fail

        ; --- loop-counter survival (FAT/ISO pattern) ---
        mov     ecx, 8
@@:
        mov     al, 0x80
        call    ansi2uni_char
        cmp     ax, 0x0410
        jne     .fail
        loop    @b
        test    ecx, ecx
        jnz     .fail

        mov     [rust_ansi2uni_char_smoke_result], 0x41533255 ; 'A2SU'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEADA201        ; Cut AN fail marker
        mov     ebx, 0x414E5332        ; 'ANS2'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ansi2uni_char(ch) -> EAX (AX = Unicode); ret 4
align 16
rust_ansi2uni_char:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_ansi2uni_char.bin'
