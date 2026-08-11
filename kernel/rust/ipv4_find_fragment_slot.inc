;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ipv4_find_fragment_slot — Rust embed + ABI smoke (Cut AU)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ipv4_find_fragment_slot).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ipv4_find_fragment_slot.bin
;;
;; Requires USE_RUST_IPV4_FIND_FRAGMENT_SLOT=1 so public
;; ipv4_find_fragment_slot hits the trampoline (not the FASM body).
;;
;; Critical ABI: EDX→IPv4_header in → ESI=slot|-1 out; preserves
;; EAX/EBX/ECX/EDX (REG-001). Trampoline also canaries EDI/EBP.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fragment-slot path ran.
rust_ipv4_find_fragment_slot_smoke_result dd 0
endg

uglobal
align 4
; Synthetic IPv4 header (20 bytes) + private fragment table (4 × 16 = 64).
ipv4_frag_smoke_hdr: rb 20
ipv4_frag_smoke_table: rb 64
endg

; Called once after stack_init (IPv4_fragments zeroed by ipv4_init).
; Exercises synthetic rust_* walk + public trampoline on live table.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ipv4_find_fragment_slot_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic arenas ---
        mov     edi, ipv4_frag_smoke_hdr
        mov     ecx, (20+64)/4
        xor     eax, eax
        rep     stosd

        ; header: id=0x1234 src=0x0A000001 dst=0x0A000002
        mov     word [ipv4_frag_smoke_hdr + 4], 0x1234
        mov     dword [ipv4_frag_smoke_hdr + 12], 0x0A000001
        mov     dword [ipv4_frag_smoke_hdr + 16], 0x0A000002

        ; slot0: miss keys
        mov     word [ipv4_frag_smoke_table + 2], 0x0001
        mov     dword [ipv4_frag_smoke_table + 4], 0x11111111
        mov     dword [ipv4_frag_smoke_table + 8], 0x22222222

        ; slot1: HIT
        mov     word [ipv4_frag_smoke_table + 16 + 2], 0x1234
        mov     dword [ipv4_frag_smoke_table + 16 + 4], 0x0A000001
        mov     dword [ipv4_frag_smoke_table + 16 + 8], 0x0A000002
        mov     dword [ipv4_frag_smoke_table + 16 + 12], 0xDEAD0001

        ; slot2: duplicate keys (first-match must stay slot1)
        mov     word [ipv4_frag_smoke_table + 32 + 2], 0x1234
        mov     dword [ipv4_frag_smoke_table + 32 + 4], 0x0A000001
        mov     dword [ipv4_frag_smoke_table + 32 + 8], 0x0A000002

        ; --- Direct rust_*: hit middle ---
        stdcall rust_ipv4_find_fragment_slot, ipv4_frag_smoke_hdr, \
                ipv4_frag_smoke_table, 4
        lea     ebx, [ipv4_frag_smoke_table + 16]
        cmp     eax, ebx
        jne     .fail

        ; --- Direct rust_*: miss ---
        mov     word [ipv4_frag_smoke_hdr + 4], 0x9999
        stdcall rust_ipv4_find_fragment_slot, ipv4_frag_smoke_hdr, \
                ipv4_frag_smoke_table, 4
        cmp     eax, -1
        jne     .fail

        ; --- Public trampoline: plant hit at live slot index 1 ---
        mov     edi, IPv4_fragments
        mov     ecx, (4 * 16)/4
        xor     eax, eax
        rep     stosd

        mov     word [IPv4_fragments + 0 + 2], 0x1111
        mov     dword [IPv4_fragments + 0 + 4], 1
        mov     dword [IPv4_fragments + 0 + 8], 2
        mov     word [IPv4_fragments + 16 + 2], 0xFACE
        mov     dword [IPv4_fragments + 16 + 4], 0x01020304
        mov     dword [IPv4_fragments + 16 + 8], 0x05060708

        mov     word [ipv4_frag_smoke_hdr + 4], 0xFACE
        mov     dword [ipv4_frag_smoke_hdr + 12], 0x01020304
        mov     dword [ipv4_frag_smoke_hdr + 16], 0x05060708

        mov     eax, 0xA6A60011
        mov     ebx, 0xB6B60012
        mov     ecx, 0xC6C60013
        mov     edx, ipv4_frag_smoke_hdr
        mov     esi, 0x51660015
        mov     edi, 0xE6E60016
        mov     ebp, 0xF6F60017
        call    ipv4_find_fragment_slot
        push    eax
        lea     eax, [IPv4_fragments + 16]
        cmp     esi, eax
        pop     eax
        jne     .fail
        cmp     eax, 0xA6A60011
        jne     .fail
        cmp     ebx, 0xB6B60012
        jne     .fail
        cmp     ecx, 0xC6C60013
        jne     .fail
        cmp     edx, ipv4_frag_smoke_hdr
        jne     .fail
        cmp     edi, 0xE6E60016
        jne     .fail
        cmp     ebp, 0xF6F60017
        jne     .fail

        ; --- Public miss ---
        mov     word [ipv4_frag_smoke_hdr + 4], 0x0000
        mov     dword [ipv4_frag_smoke_hdr + 12], 0xDEADBEEF
        mov     dword [ipv4_frag_smoke_hdr + 16], 0xCAFEBABE
        mov     eax, 0xA7A70021
        mov     ebx, 0xB7B70022
        mov     ecx, 0xC7C70023
        mov     edx, ipv4_frag_smoke_hdr
        mov     edi, 0xE7E70024
        mov     ebp, 0xF7F70025
        call    ipv4_find_fragment_slot
        cmp     esi, -1
        jne     .fail
        cmp     eax, 0xA7A70021
        jne     .fail
        cmp     ebx, 0xB7B70022
        jne     .fail
        cmp     ecx, 0xC7C70023
        jne     .fail
        cmp     edx, ipv4_frag_smoke_hdr
        jne     .fail
        cmp     edi, 0xE7E70024
        jne     .fail
        cmp     ebp, 0xF7F70025
        jne     .fail

        ; Restore live fragment table (zero first 4 slots we touched)
        mov     edi, IPv4_fragments
        mov     ecx, (4 * 16)/4
        xor     eax, eax
        rep     stosd

        mov     [rust_ipv4_find_fragment_slot_smoke_result], 0x47415246 ; 'FRAG'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C55                ; Cut AU fail marker
        mov     ebx, 0x47415246                ; 'FRAG'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ipv4_find_fragment_slot(packet, fragments, count) -> EAX; ret 12
align 16
rust_ipv4_find_fragment_slot:
        file '../../rust_kernel/kolibri_utils/out/rust_ipv4_find_fragment_slot.bin'
