;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; rebase_coff — Rust embed + ABI smoke (Cut CH)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_rebase_coff).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_rebase_coff.bin
;;
;; Requires USE_RUST_REBASE_COFF=1 so public rebase_coff hits the
;; trampoline (not the FASM body). Independent of Cuts Y/AT/BK/BU/CG.
;;
;; Critical ABI: stdcall(coff, sym, delta) → void; ret 12.
;; Trampoline preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001/011).
;; rust_rebase_coff ends `ret 12`; never add esp for those args (REG-009).
;; `sym` is unused (ABI parity with load_library / legacy FASM).
;;
;; NOTE: Legacy FASM enters .fix_sec before testing nSections, so
;; nSections=0 infinite-loops / faults. Smoke never uses nSections=0
;; against the public ABI (production callers always have sections).
;; Rust exits cleanly on nSections=0 (documented limitation, same as Y).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust rebase_coff path ran.
rust_rebase_coff_smoke_result dd 0
endg

uglobal
align 4
; Contiguous synthetic mini-COFF:
; header(20) + section(40) + reloc×2(20) + pad + dwords
; (no symbol table needed — sym unused)
rebase_coff_smoke_img:
rebase_coff_smoke_coff:   rb 20
rebase_coff_smoke_sec:    rb 40
rebase_coff_smoke_reloc:  rb 20
        rb 4                          ; pad
rebase_coff_smoke_dword0: dd ?
rebase_coff_smoke_dword1: dd ?
endg

; Called once from high_code when USE_RUST_REBASE_COFF=1.
; Exercises: Type 6 delta-add, Type≠6 skip, register canaries,
; public trampoline + direct rust_*.
; On mismatch: distinctive hang (does not reach desktop).
align 4
rebase_coff_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic image ---
        mov     edi, rebase_coff_smoke_img
        mov     ecx, (rebase_coff_smoke_dword1 + 4 - rebase_coff_smoke_img) / 4
        xor     eax, eax
        rep     stosd

        ; Header: nSections = 1 (never 0 — legacy FASM loop quirk)
        mov     word [rebase_coff_smoke_coff + 2], 1

        ; Section at coff+20: VA=0 @+12, PtrReloc=60 @+24, NumReloc=1 @+32
        ; Layout: header20 + sec40 = 60
        mov     dword [rebase_coff_smoke_sec + 12], 0
        mov     dword [rebase_coff_smoke_sec + 24], 60
        mov     word  [rebase_coff_smoke_sec + 32], 1

        ; Reloc0 DIR32 → dword0 as absolute VA (sec.VA=0, delta=0 → patch dword0)
        mov     eax, rebase_coff_smoke_dword0
        mov     [rebase_coff_smoke_reloc], eax
        mov     dword [rebase_coff_smoke_reloc + 4], 0        ; SymIndex unused
        mov     word  [rebase_coff_smoke_reloc + 8], 6
        mov     dword [rebase_coff_smoke_dword0], 0x5

        ; --- Direct rust_*: delta=0 → add 0 (unchanged) ---
        stdcall rust_rebase_coff, rebase_coff_smoke_coff, 0, 0
        cmp     dword [rebase_coff_smoke_dword0], 0x5
        jne     .fail

        ; --- Direct rust_*: delta=0x10; reloc VA = dword0 - 0x10 ---
        mov     dword [rebase_coff_smoke_dword0], 0x20
        mov     eax, rebase_coff_smoke_dword0
        sub     eax, 0x10
        mov     [rebase_coff_smoke_reloc], eax
        stdcall rust_rebase_coff, rebase_coff_smoke_coff, 0, 0x10
        cmp     dword [rebase_coff_smoke_dword0], 0x30
        jne     .fail

        ; --- Public trampoline: Type≠6 skip + canaries ---
        mov     eax, rebase_coff_smoke_dword1
        mov     [rebase_coff_smoke_reloc], eax
        mov     word  [rebase_coff_smoke_reloc + 8], 20       ; REL32 — skip
        mov     dword [rebase_coff_smoke_dword1], 0xABCD0000
        mov     ebx, 0xB9B90002
        mov     ecx, 0xC9C90001
        mov     edx, 0xD9D90003
        mov     esi, 0x59590005
        mov     edi, 0xE9E90006
        mov     ebp, 0xF9F90007
        stdcall rebase_coff, rebase_coff_smoke_coff, 0xDEADBEEF, 0x40
        cmp     dword [rebase_coff_smoke_dword1], 0xABCD0000
        jne     .fail
        cmp     ebx, 0xB9B90002
        jne     .fail
        cmp     ecx, 0xC9C90001
        jne     .fail
        cmp     edx, 0xD9D90003
        jne     .fail
        cmp     esi, 0x59590005
        jne     .fail
        cmp     edi, 0xE9E90006
        jne     .fail
        cmp     ebp, 0xF9F90007
        jne     .fail

        ; --- Public trampoline: Type 6 again with canaries ---
        mov     eax, rebase_coff_smoke_dword1
        sub     eax, 0x20
        mov     [rebase_coff_smoke_reloc], eax
        mov     word  [rebase_coff_smoke_reloc + 8], 6
        mov     dword [rebase_coff_smoke_dword1], 0x100
        mov     ebx, 0xBABA0012
        mov     esi, 0x5A5A0015
        mov     edi, 0xEAEA0016
        mov     ebp, 0xFAFA0017
        stdcall rebase_coff, rebase_coff_smoke_coff, 0, 0x20
        cmp     dword [rebase_coff_smoke_dword1], 0x120
        jne     .fail
        cmp     ebx, 0xBABA0012
        jne     .fail
        cmp     esi, 0x5A5A0015
        jne     .fail
        cmp     edi, 0xEAEA0016
        jne     .fail
        cmp     ebp, 0xFAFA0017
        jne     .fail

        mov     [rust_rebase_coff_smoke_result], 0x52424346 ; 'RBCF'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C61                ; Cut CH fail marker
        mov     ebx, 0x52424346                ; 'RBCF'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_rebase_coff(coff, sym, delta) -> void; ret 12
align 16
rust_rebase_coff:
        file '../../rust_kernel/kolibri_utils/out/rust_rebase_coff.bin'
