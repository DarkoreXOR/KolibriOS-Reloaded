;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fix_coff_relocs — Rust embed + ABI smoke (Cut Y)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fix_coff_relocs).
;; See docs/migration/cut-y-plan.md / cut-y-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-fix-coff-relocs.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fix_coff_relocs.bin
;;
;; kernel/core/dll.inc (USE_RUST_FIX_COFF_RELOCS=1) trampolines
;; fix_coff_relocs → rust_fix_coff_relocs.
;;
;; NOTE: Legacy FASM enters .fix_sec before testing nSections, so
;; nSections=0 infinite-loops / faults. Smoke never uses nSections=0
;; against the public ABI (production callers always have sections).
;; Rust exits cleanly on nSections=0 (documented limitation).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust COFF-reloc path ran.
rust_fix_coff_relocs_smoke_result dd 0
endg

uglobal
align 4
; Contiguous synthetic mini-COFF (must match coff+20 section layout):
; header(20) + section(40) + reloc×2(20) + sym(18) + pad + dwords
fix_coff_smoke_img:
fix_coff_smoke_coff:   rb 20
fix_coff_smoke_sec:    rb 40
fix_coff_smoke_reloc:  rb 20
fix_coff_smoke_sym:    rb 18
        rb 2                          ; pad to 4-byte align before dwords
fix_coff_smoke_dword0: dd ?
fix_coff_smoke_dword1: dd ?
endg

; Called once from high_code (after LTR).
; Exercises: stdcall(coff,sym,delta) DIR32 + REL32 + delta shift.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fix_coff_relocs_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic image ---
        mov     edi, fix_coff_smoke_img
        mov     ecx, (fix_coff_smoke_dword1 + 4 - fix_coff_smoke_img) / 4
        xor     eax, eax
        rep     stosd

        ; Header: nSections = 1 (never 0 — legacy FASM loop quirk)
        mov     word [fix_coff_smoke_coff + 2], 1

        ; Section at coff+20: VA=0 @+12, PtrReloc=60 @+24, NumReloc=1 @+32
        mov     dword [fix_coff_smoke_sec + 12], 0
        mov     dword [fix_coff_smoke_sec + 24], 60
        mov     word  [fix_coff_smoke_sec + 32], 1

        ; Sym0.Value = 0x1000
        mov     dword [fix_coff_smoke_sym + 8], 0x1000

        ; Reloc0 DIR32 → dword0
        mov     eax, fix_coff_smoke_dword0
        mov     [fix_coff_smoke_reloc], eax
        mov     dword [fix_coff_smoke_reloc + 4], 0
        mov     word  [fix_coff_smoke_reloc + 8], 6
        mov     dword [fix_coff_smoke_dword0], 0x5

        ; --- DIR32; preserve EBX/ESI/EBP ---
        mov     eax, 0xA9000001
        mov     ebx, 0xB9000002
        mov     ecx, 0xC9000003
        mov     edx, 0xD9000004
        mov     esi, 0x59000005
        mov     edi, 0xE9000006
        mov     ebp, 0xF9000007
        stdcall fix_coff_relocs, fix_coff_smoke_coff, fix_coff_smoke_sym, 0
        cmp     ebx, 0xB9000002
        jne     .fail
        cmp     esi, 0x59000005
        jne     .fail
        cmp     ebp, 0xF9000007
        jne     .fail
        cmp     dword [fix_coff_smoke_dword0], 0x1005
        jne     .fail

        ; --- REL32 → dword1 ---
        mov     eax, fix_coff_smoke_dword1
        mov     [fix_coff_smoke_reloc], eax
        mov     word  [fix_coff_smoke_reloc + 8], 20
        mov     dword [fix_coff_smoke_dword1], 0
        stdcall fix_coff_relocs, fix_coff_smoke_coff, fix_coff_smoke_sym, 0
        mov     eax, fix_coff_smoke_dword1
        mov     edx, 0x1000
        sub     edx, eax
        sub     edx, 4
        cmp     [fix_coff_smoke_dword1], edx
        jne     .fail

        ; --- delta shifts patch address ---
        mov     dword [fix_coff_smoke_dword0], 0x20
        mov     eax, fix_coff_smoke_dword0
        sub     eax, 0x10
        mov     [fix_coff_smoke_reloc], eax
        mov     word  [fix_coff_smoke_reloc + 8], 6
        mov     dword [fix_coff_smoke_sym + 8], 0x30
        stdcall fix_coff_relocs, fix_coff_smoke_coff, fix_coff_smoke_sym, 0x10
        cmp     dword [fix_coff_smoke_dword0], 0x50
        jne     .fail

        mov     [rust_fix_coff_relocs_smoke_result], 0x46435258 ; 'FCRX'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C59                ; Cut Y fail marker
        mov     ebx, 0x46435258                ; 'FCRX'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fix_coff_relocs(coff, sym, delta) -> void; ret 12
align 16
rust_fix_coff_relocs:
        file '../../rust_kernel/kolibri_utils/out/rust_fix_coff_relocs.bin'
