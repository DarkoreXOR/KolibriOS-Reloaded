;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fix_coff_symbols — Rust embed + ABI smoke (Cut BU)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fix_coff_symbols).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;;
;; kernel/core/dll.inc (USE_RUST_FIX_COFF_SYMBOLS=1) trampolines
;; fix_coff_symbols → rust_fix_coff_symbols(..., get_proc_ex).
;;
;; Critical ABI: stdcall(sec,symbols,sym_count,strings,imports) → EAX;
;; trampoline injects get_proc_ex; ret 20 legacy / Rust ret 24.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fix_coff_symbols path ran.
rust_fix_coff_symbols_smoke_result dd 0
endg

; Smoke-only get_proc_ex stub: 'T'/'t' names → 0x6000, 'U' → 0, else 0xA000.
align 4
fix_coff_symbols_smoke_get_proc_ex:
        mov     eax, [esp+4]
        mov     al, [eax]
        cmp     al, 'T'
        je      .hit
        cmp     al, 't'
        je      .hit
        cmp     al, 'U'
        je      .miss
        mov     eax, 0xA000
        ret     8
.hit:
        mov     eax, 0x6000
        ret     8
.miss:
        xor     eax, eax
        ret     8

uglobal
align 4
fix_coff_symbols_smoke_sec:     rb 40
fix_coff_symbols_smoke_table:   rb 54          ; 3 × COFF_SYM
fix_coff_symbols_smoke_strings: db 'TargetFn',0
endg

; Called once from high_code (near Cut AT/Y smokes).
align 4
fix_coff_symbols_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic buffers ---
        mov     edi, fix_coff_symbols_smoke_sec
        mov     ecx, (fix_coff_symbols_smoke_strings + 9 - fix_coff_symbols_smoke_sec) / 4
        xor     eax, eax
        rep     stosd

        ; Section 0 VA = 0x1000 @ offset 12
        mov     dword [fix_coff_symbols_smoke_sec + 12], 0x1000

        ; sym0 internal: section 1, Value 0x50 → 0x1050
        mov     edi, fix_coff_symbols_smoke_table
        mov     byte [edi+0], 'I'
        mov     byte [edi+1], 'N'
        mov     byte [edi+2], 'T'
        mov     byte [edi+3], 0
        mov     dword [edi+8], 0x50
        mov     word  [edi+12], 1

        ; sym1 external inline: TEST → 0x6000
        lea     edi, [fix_coff_symbols_smoke_table + 18]
        mov     byte [edi+0], 'T'
        mov     byte [edi+1], 'E'
        mov     byte [edi+2], 'S'
        mov     byte [edi+3], 'T'
        mov     word  [edi+12], 0

        ; sym2 external unresolved: UNRES → retval 0
        lea     edi, [fix_coff_symbols_smoke_table + 36]
        mov     byte [edi+0], 'U'
        mov     byte [edi+1], 'N'
        mov     byte [edi+2], 'R'
        mov     byte [edi+3], 'E'
        mov     byte [edi+4], 'S'
        mov     word  [edi+12], 0

        ; --- Vector 1: direct Rust with smoke get_proc_ex ---
        mov     eax, 0xA9000001
        mov     ebx, 0xB9000002
        mov     ecx, 0xC9000003
        mov     edx, 0xD9000004
        mov     esi, 0x59000005
        mov     edi, 0x69000006
        mov     ebp, 0x79000007

        stdcall rust_fix_coff_symbols, fix_coff_symbols_smoke_sec, \
                fix_coff_symbols_smoke_table, 3, \
                fix_coff_symbols_smoke_strings, 0, \
                fix_coff_symbols_smoke_get_proc_ex

        cmp     eax, 0
        jne     .hang
        cmp     dword [fix_coff_symbols_smoke_table + 8], 0x1050
        jne     .hang
        cmp     dword [fix_coff_symbols_smoke_table + 18 + 8], 0x6000
        jne     .hang
        cmp     dword [fix_coff_symbols_smoke_table + 36 + 8], 0
        jne     .hang

        ; preserve EBX/ESI/EDI/EBP (REG-001 class)
        cmp     ebx, 0xB9000002
        jne     .hang
        cmp     esi, 0x59000005
        jne     .hang
        cmp     edi, 0x69000006
        jne     .hang
        cmp     ebp, 0x79000007
        jne     .hang

        ; --- Vector 2: public fix_coff_symbols trampoline (gate ON) ---
        ; Internal-only (real get_proc_ex needs live imports for externals).
        mov     edi, fix_coff_symbols_smoke_table
        mov     ecx, 18/4
        xor     eax, eax
        rep     stosd
        mov     edi, fix_coff_symbols_smoke_table
        mov     byte [edi+0], 'I'
        mov     byte [edi+1], 'N'
        mov     byte [edi+2], 'T'
        mov     byte [edi+3], 0
        mov     dword [edi+8], 0x50
        mov     word  [edi+12], 1

        stdcall fix_coff_symbols, fix_coff_symbols_smoke_sec, \
                fix_coff_symbols_smoke_table, 1, \
                fix_coff_symbols_smoke_strings, 0

        cmp     eax, 1
        jne     .hang
        cmp     dword [fix_coff_symbols_smoke_table + 8], 0x1050
        jne     .hang

        mov     dword [rust_fix_coff_symbols_smoke_result], 'FCFS'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.hang:
        cli
        hlt
        jmp     .hang

; Raw Rust stdcall body:
;   rust_fix_coff_symbols(sec,symbols,sym_count,strings,imports,get_proc_ex) -> EAX; ret 24
align 16
rust_fix_coff_symbols:
        file '../../rust_kernel/kolibri_utils/out/rust_fix_coff_symbols.bin'
