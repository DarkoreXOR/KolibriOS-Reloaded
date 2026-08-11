;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; get_coff_sym — Rust embed + ABI smoke (Cut AT)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_get_coff_sym).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_get_coff_sym.bin
;;
;; Requires USE_RUST_GET_COFF_SYM=1 so public get_coff_sym hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: stdcall(pSym, count, sz_sym) → EAX=Value|0; ret 12.
;; Trampoline preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust get_coff_sym path ran.
rust_get_coff_sym_smoke_result dd 0
; Lookup names (8 bytes each; match COFF_SYM.Name / strncmp length).
get_coff_sym_smoke_name_exports:
        db 'EXPORTS',0                ; 8 bytes
get_coff_sym_smoke_name_under:
        db '_EXPORTS'                 ; exactly 8
get_coff_sym_smoke_name_eight:
        db 'ABCDEFGH'                 ; exactly 8
get_coff_sym_smoke_name_miss:
        db 'MISSING',0                ; 8 bytes
endg

uglobal
align 4
; Synthetic COFF_SYM table: 4 entries × 18 bytes = 72.
; [0] EXPORTS\0 Value=0x10001000
; [1] _EXPORTS  Value=0x20002000
; [2] OTHER\0.. Value=0x30003000
; [3] ABCDEFGH  Value=0x40004000
get_coff_sym_smoke_table: rb 72
endg

; Called once from high_code (after LTR / near Cut Y smoke).
; Exercises: hit first/mid/last, miss, EXPORTS/_EXPORTS, register canaries.
; On mismatch: distinctive hang (does not reach desktop).
align 4
get_coff_sym_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic symbol table ---
        mov     edi, get_coff_sym_smoke_table
        mov     ecx, 72/4
        xor     eax, eax
        rep     stosd

        ; sym0: EXPORTS\0 + Value 0x10001000
        mov     edi, get_coff_sym_smoke_table
        mov     byte [edi+0], 'E'
        mov     byte [edi+1], 'X'
        mov     byte [edi+2], 'P'
        mov     byte [edi+3], 'O'
        mov     byte [edi+4], 'R'
        mov     byte [edi+5], 'T'
        mov     byte [edi+6], 'S'
        mov     byte [edi+7], 0
        mov     dword [edi+8], 0x10001000

        ; sym1: _EXPORTS + Value 0x20002000
        lea     edi, [get_coff_sym_smoke_table + 18]
        mov     byte [edi+0], '_'
        mov     byte [edi+1], 'E'
        mov     byte [edi+2], 'X'
        mov     byte [edi+3], 'P'
        mov     byte [edi+4], 'O'
        mov     byte [edi+5], 'R'
        mov     byte [edi+6], 'T'
        mov     byte [edi+7], 'S'
        mov     dword [edi+8], 0x20002000

        ; sym2: OTHER\0\0\0 + Value 0x30003000
        lea     edi, [get_coff_sym_smoke_table + 36]
        mov     byte [edi+0], 'O'
        mov     byte [edi+1], 'T'
        mov     byte [edi+2], 'H'
        mov     byte [edi+3], 'E'
        mov     byte [edi+4], 'R'
        mov     byte [edi+5], 0
        mov     byte [edi+6], 0
        mov     byte [edi+7], 0
        mov     dword [edi+8], 0x30003000

        ; sym3: ABCDEFGH + Value 0x40004000
        lea     edi, [get_coff_sym_smoke_table + 54]
        mov     byte [edi+0], 'A'
        mov     byte [edi+1], 'B'
        mov     byte [edi+2], 'C'
        mov     byte [edi+3], 'D'
        mov     byte [edi+4], 'E'
        mov     byte [edi+5], 'F'
        mov     byte [edi+6], 'G'
        mov     byte [edi+7], 'H'
        mov     dword [edi+8], 0x40004000

        ; --- Direct rust_*: hit EXPORTS ---
        stdcall rust_get_coff_sym, get_coff_sym_smoke_table, 4, \
                get_coff_sym_smoke_name_exports
        cmp     eax, 0x10001000
        jne     .fail

        ; --- Hit _EXPORTS ---
        stdcall rust_get_coff_sym, get_coff_sym_smoke_table, 4, \
                get_coff_sym_smoke_name_under
        cmp     eax, 0x20002000
        jne     .fail

        ; --- Public trampoline: hit last + preserve canaries ---
        ; Legacy FASM leaves EBX/ESI/EDI/EBP untouched; ECX/EDX may be
        ; clobbered by strncmp. Rust trampoline also keeps ECX/EDX (superset).
        mov     ebx, 0xB5B50002
        mov     esi, 0x51510005
        mov     edi, 0xE5E50006
        mov     ebp, 0xF5F50007
        stdcall get_coff_sym, get_coff_sym_smoke_table, 4, \
                get_coff_sym_smoke_name_eight
        cmp     eax, 0x40004000
        jne     .fail
        cmp     ebx, 0xB5B50002
        jne     .fail
        cmp     esi, 0x51510005
        jne     .fail
        cmp     edi, 0xE5E50006
        jne     .fail
        cmp     ebp, 0xF5F50007
        jne     .fail

        ; --- Miss + preserve canaries (legacy-visible regs) ---
        mov     ebx, 0xB6B60012
        mov     esi, 0x51660015
        mov     edi, 0xE6E60016
        mov     ebp, 0xF6F60017
        stdcall get_coff_sym, get_coff_sym_smoke_table, 4, \
                get_coff_sym_smoke_name_miss
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB6B60012
        jne     .fail
        cmp     esi, 0x51660015
        jne     .fail
        cmp     edi, 0xE6E60016
        jne     .fail
        cmp     ebp, 0xF6F60017
        jne     .fail

        ; --- count=1 misses later symbols ---
        stdcall get_coff_sym, get_coff_sym_smoke_table, 1, \
                get_coff_sym_smoke_name_under
        test    eax, eax
        jnz     .fail

        ; --- count=2 hits _EXPORTS ---
        stdcall get_coff_sym, get_coff_sym_smoke_table, 2, \
                get_coff_sym_smoke_name_under
        cmp     eax, 0x20002000
        jne     .fail

        mov     [rust_get_coff_sym_smoke_result], 0x59534347 ; 'GCSY'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C54                ; Cut AT fail marker
        mov     ebx, 0x59534347                ; 'GCSY'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_get_coff_sym(pSym, count, sz_sym) -> EAX; ret 12
align 16
rust_get_coff_sym:
        file '../../rust_kernel/kolibri_utils/out/rust_get_coff_sym.bin'
