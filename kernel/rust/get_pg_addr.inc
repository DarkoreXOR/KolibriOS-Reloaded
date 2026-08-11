;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; get_pg_addr — Rust embed + ABI smoke (Cut AQ)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_get_pg_addr).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_get_pg_addr.bin
;;
;; Requires USE_RUST_GET_PG_ADDR=1 so public get_pg_addr hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EAX=linear → EAX=phys page; trampoline preserves
;; ECX+EDX (REG-001 — HD DMA / USB get_phys_addr live registers).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust get_pg_addr path ran.
rust_get_pg_addr_smoke_result dd 0
endg

uglobal
align 4
; Compact synthetic PTE table for high-path smoke (os_base=0 indices).
; Index 0x401 holds a planted PTE for linear 0x401000.
get_pg_addr_smoke_ptes: rb 0x500*4
endg

; Called once from high_code after Cut AP smoke / early enough that
; page_tabs is live for the public trampoline low-path checks.
; On mismatch: distinctive hang (does not reach desktop).
align 4
get_pg_addr_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic PTE table ---
        mov     edi, get_pg_addr_smoke_ptes
        mov     ecx, (0x500*4)/4
        xor     eax, eax
        rep     stosd

        ; Plant PTE at index 0x401: phys 0xABC000 + flags 0x003
        ; (0x00ABCD03 & ~0xFFF = 0x00ABC000)
        mov     dword [get_pg_addr_smoke_ptes + 0x401*4], 0x00ABCD03

        ; --- Direct rust_*: high path with os_base=0 ---
        stdcall rust_get_pg_addr, 0x401000, get_pg_addr_smoke_ptes, 0
        cmp     eax, 0x00ABC000
        jne     .fail

        ; --- Direct rust_*: low path identity (os_base=0) ---
        stdcall rust_get_pg_addr, 0x1234, get_pg_addr_smoke_ptes, 0
        cmp     eax, 0x1000
        jne     .fail

        ; --- Direct rust_*: page-mask strips flags (high path) ---
        mov     dword [get_pg_addr_smoke_ptes + 0x405*4], 0x00100FFF
        stdcall rust_get_pg_addr, 0x405000, get_pg_addr_smoke_ptes, 0
        cmp     eax, 0x00100000
        jne     .fail

        ; --- Direct rust_*: boundary IDENTITY_WINDOW uses high path ---
        mov     dword [get_pg_addr_smoke_ptes + 0x400*4], 0x12345067
        stdcall rust_get_pg_addr, 0x400000, get_pg_addr_smoke_ptes, 0
        cmp     eax, 0x12345000
        jne     .fail

        ; --- Public trampoline: OS_BASE → phys 0; ECX/EDX canaries (REG-001) ---
        mov     eax, OS_BASE
        mov     ebx, 0xB0B00001
        mov     ecx, 0xC0C00002
        mov     edx, 0xD0D00003
        mov     esi, 0x51510004
        mov     edi, 0xE1E10005
        mov     ebp, 0xF1F10006
        call    get_pg_addr
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB0B00001
        jne     .fail
        cmp     ecx, 0xC0C00002
        jne     .fail
        cmp     edx, 0xD0D00003
        jne     .fail
        cmp     esi, 0x51510004
        jne     .fail
        cmp     edi, 0xE1E10005
        jne     .fail
        cmp     ebp, 0xF1F10006
        jne     .fail

        ; --- Public trampoline: OS_BASE+0x1234 → 0x1000 ---
        mov     eax, OS_BASE + 0x1234
        mov     ecx, 0xC0C00012
        mov     edx, 0xD0D00013
        call    get_pg_addr
        cmp     eax, 0x1000
        jne     .fail
        cmp     ecx, 0xC0C00012
        jne     .fail
        cmp     edx, 0xD0D00013
        jne     .fail

        ; --- Public trampoline: last byte of identity window ---
        mov     eax, OS_BASE + 0x400000 - 1
        call    get_pg_addr
        cmp     eax, 0x400000 - 0x1000
        jne     .fail

        ; Live high-path (HEAP_BASE / DMA buffers) is exercised after heap
        ; init by production callers; synthetic high-path above covers the
        ; PTE-fetch algorithm without depending on boot mapping order.

        mov     [rust_get_pg_addr_smoke_result], 0x47504144 ; 'GPAD'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C51                ; Cut AQ fail marker
        mov     ebx, 0x47504144                ; 'GPAD'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_get_pg_addr(linear, page_tabs, os_base) -> EAX; ret 12
align 16
rust_get_pg_addr:
        file '../../rust_kernel/kolibri_utils/out/rust_get_pg_addr.bin'
