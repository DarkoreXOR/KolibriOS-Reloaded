;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; coff_get_align — Rust embed + ABI smoke (Cut BK)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_coff_get_align).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_coff_get_align.bin
;;
;; Requires USE_RUST_COFF_GET_ALIGN=1 so public coff_get_align hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EDX→COFF_SECTION; EAX=align mask; preserve ECX+EDX
;; (REG-001). Synthetic section only — no live DLL mutation (REG-003).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust coff_get_align path ran.
rust_coff_get_align_smoke_result dd 0
endg

uglobal
align 4
; Synthetic COFF_SECTION (40 bytes) — only Characteristics is meaningful.
coff_get_align_smoke_section: rb 40
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on synthetic COFF_SECTION only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
coff_get_align_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic section ---
        mov     edi, coff_get_align_smoke_section
        mov     ecx, 40/4
        xor     eax, eax
        rep     stosd

        ; --- Direct rust_*: Characteristics=0 → default 4K mask 0xFFF ---
        ; Characteristics at offset 36
        mov     dword [coff_get_align_smoke_section+36], 0
        stdcall rust_coff_get_align, coff_get_align_smoke_section
        cmp     eax, 0x0FFF
        jne     .fail

        ; --- Direct rust_*: ALIGN_1BYTES (0x00100000) → mask 0 ---
        mov     dword [coff_get_align_smoke_section+36], 0x00100000
        stdcall rust_coff_get_align, coff_get_align_smoke_section
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: ALIGN_4096 (0x00D00000) → mask 0xFFF ---
        mov     dword [coff_get_align_smoke_section+36], 0x00D00000
        stdcall rust_coff_get_align, coff_get_align_smoke_section
        cmp     eax, 0x0FFF
        jne     .fail

        ; --- Direct rust_*: ALIGN_8192 (0x00E00000) → clamp 4K ---
        mov     dword [coff_get_align_smoke_section+36], 0x00E00000
        stdcall rust_coff_get_align, coff_get_align_smoke_section
        cmp     eax, 0x0FFF
        jne     .fail

        ; --- Direct rust_*: ALIGN_2BYTES (0x00200000) → mask 1 ---
        mov     dword [coff_get_align_smoke_section+36], 0x00200000
        stdcall rust_coff_get_align, coff_get_align_smoke_section
        cmp     eax, 1
        jne     .fail

        ; --- Public trampoline: EDX→section + ECX/EDX/EBX/ESI/EDI/EBP canaries ---
        mov     dword [coff_get_align_smoke_section+36], 0x00C00000 ; 2KiB → mask 0x7FF
        mov     edx, coff_get_align_smoke_section
        mov     eax, 0xA1A1A1A1         ; should be overwritten with mask
        mov     ebx, 0xB2B2B2B2
        mov     ecx, 0xC3C3C3C3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        call    coff_get_align
        cmp     eax, 0x07FF
        jne     .fail
        cmp     ebx, 0xB2B2B2B2
        jne     .fail
        cmp     ecx, 0xC3C3C3C3
        jne     .fail
        cmp     edx, coff_get_align_smoke_section
        jne     .fail
        cmp     esi, 0xE4E4E4E4
        jne     .fail
        cmp     edi, 0xF5F5F5F5
        jne     .fail
        cmp     ebp, 0xA6A6A6A6
        jne     .fail

        ; --- Public trampoline: default path (chars=0) ---
        mov     dword [coff_get_align_smoke_section+36], 0
        mov     edx, coff_get_align_smoke_section
        mov     ecx, 0x11111111
        call    coff_get_align
        cmp     eax, 0x0FFF
        jne     .fail
        cmp     ecx, 0x11111111
        jne     .fail
        cmp     edx, coff_get_align_smoke_section
        jne     .fail

        mov     [rust_coff_get_align_smoke_result], 'CGAL'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_coff_get_align_smoke_result], 'DEAD'
        ; Distinctive hang marker for Cut BK
@@:
        cli
        hlt
        jmp     @b

; ---------------------------------------------------------------------------
; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
; ---------------------------------------------------------------------------
align 4
rust_coff_get_align:
        file '../../rust_kernel/kolibri_utils/out/rust_coff_get_align.bin'
