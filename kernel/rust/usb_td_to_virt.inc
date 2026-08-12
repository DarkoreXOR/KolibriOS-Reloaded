;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; usb_td_to_virt — Rust embed + ABI smoke (Cut CI)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_usb_td_to_virt).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_usb_td_to_virt.bin
;;
;; Requires USE_RUST_USB_TD_TO_VIRT=1 so public usb_td_to_virt hits the
;; trampoline (not the FASM body). Composes Cut AQ get_pg_addr (inlined).
;;
;; Critical ABI: register (EAX=first page, ECX=TD phys) → EAX=linear|0;
;; ECX = (TD phys & 0xFFF) on hit, unchanged on miss.
;; rust_usb_td_to_virt stdcall(first, td_phys, page_tabs, os_base); ret 16.
;; Trampoline preserves EBX/EDX/ESI/EDI/EBP (REG-001/011); reconstructs ECX.
;; Smoke uses kernel_alloc + stack locals — no large .bss (REG-003).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust usb_td_to_virt path ran.
rust_usb_td_to_virt_smoke_result dd 0
endg

; Called once after init_kernel_heap + RING0 stack (needs kernel_alloc).
; Exercises: hit/miss/offset, register canaries, public trampoline + rust_*.
; On mismatch: distinctive hang (does not reach desktop).
align 4
usb_td_to_virt_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp
        sub     esp, 16
        ; [esp+0]=page0 [esp+4]=page1 [esp+8]=phys0 [esp+12]=phys1

        stdcall kernel_alloc, 0x2000
        test    eax, eax
        jz      .fail
        mov     [esp], eax                      ; page0
        lea     ecx, [eax+0x1000]
        mov     [esp+4], ecx                    ; page1

        mov     edi, eax
        mov     ecx, 0x2000/4
        xor     eax, eax
        rep     stosd

        mov     esi, [esp]
        mov     edi, [esp+4]
        mov     dword [esi+0x1000-4], edi
        mov     dword [edi+0x1000-4], 0

        mov     eax, esi
        call    get_pg_addr
        mov     [esp+8], eax                    ; phys0
        mov     eax, edi
        call    get_pg_addr
        mov     [esp+12], eax                   ; phys1

        ; --- Direct rust_*: miss ---
        stdcall rust_usb_td_to_virt, esi, 0xDEAD0000, page_tabs, OS_BASE
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: hit page0 offset 0 ---
        mov     eax, [esp+8]
        stdcall rust_usb_td_to_virt, esi, eax, page_tabs, OS_BASE
        cmp     eax, esi
        jne     .fail

        ; --- Direct rust_*: hit page0 + 0x123 ---
        mov     eax, [esp+8]
        add     eax, 0x123
        stdcall rust_usb_td_to_virt, esi, eax, page_tabs, OS_BASE
        mov     ecx, esi
        add     ecx, 0x123
        cmp     eax, ecx
        jne     .fail

        ; --- Direct rust_*: hit page1 + 0x40 ---
        mov     eax, [esp+12]
        add     eax, 0x40
        stdcall rust_usb_td_to_virt, esi, eax, page_tabs, OS_BASE
        mov     ecx, [esp+4]
        add     ecx, 0x40
        cmp     eax, ecx
        jne     .fail

        ; --- Public trampoline: miss + canaries (ECX preserved) ---
        mov     eax, [esp]
        mov     ecx, 0xCAFE0000
        mov     ebx, 0xB1B10001
        mov     edx, 0xD1D10002
        mov     esi, 0x51510003
        mov     edi, 0xE1E10004
        mov     ebp, 0xF1F10005
        call    usb_td_to_virt
        test    eax, eax
        jnz     .fail
        cmp     ecx, 0xCAFE0000
        jne     .fail
        cmp     ebx, 0xB1B10001
        jne     .fail
        cmp     edx, 0xD1D10002
        jne     .fail
        cmp     esi, 0x51510003
        jne     .fail
        cmp     edi, 0xE1E10004
        jne     .fail
        cmp     ebp, 0xF1F10005
        jne     .fail

        ; --- Public trampoline: hit page0+0x20; ECX OUT = 0x20 ---
        mov     eax, [esp]
        mov     ecx, [esp+8]
        add     ecx, 0x20
        mov     ebx, 0xB2B20011
        mov     edx, 0xD2D20012
        mov     esi, 0x52520013
        mov     edi, 0xE2E20014
        mov     ebp, 0xF2F20015
        call    usb_td_to_virt
        mov     edx, [esp]
        add     edx, 0x20
        cmp     eax, edx
        jne     .fail
        cmp     ecx, 0x20
        jne     .fail
        cmp     ebx, 0xB2B20011
        jne     .fail
        cmp     esi, 0x52520013
        jne     .fail
        cmp     edi, 0xE2E20014
        jne     .fail
        cmp     ebp, 0xF2F20015
        jne     .fail

        ; --- Public trampoline: hit page1+0xFFF ---
        mov     eax, [esp]
        mov     ecx, [esp+12]
        add     ecx, 0xFFF
        call    usb_td_to_virt
        mov     edx, [esp+4]
        add     edx, 0xFFF
        cmp     eax, edx
        jne     .fail
        cmp     ecx, 0xFFF
        jne     .fail

        stdcall kernel_free, [esp]

        mov     [rust_usb_td_to_virt_smoke_result], 0x55544456 ; 'UTDV'
        add     esp, 16
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C62                ; Cut CI fail marker
        mov     ebx, 0x55544456                ; 'UTDV'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_usb_td_to_virt(first, td_phys, page_tabs, os_base) -> EAX; ret 16
align 16
rust_usb_td_to_virt:
        file '../../rust_kernel/kolibri_utils/out/rust_usb_td_to_virt.bin'
