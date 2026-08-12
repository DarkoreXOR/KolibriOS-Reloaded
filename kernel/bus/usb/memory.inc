;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;                                                              ;;
;; Copyright (C) KolibriOS team 2013-2024. All rights reserved. ;;
;; Distributed under terms of the GNU General Public License    ;;
;;                                                              ;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;


; Memory management for USB structures.
; Protocol layer uses the common kernel heap malloc/free.
; Hardware layer has special requirements:
; * memory blocks should be properly aligned
; * memory blocks should not cross page boundary
; Hardware layer allocates fixed-size blocks.
; Thus, hardware layer uses the system slab allocator.

; Helper procedure: translate physical address in ecx
; of some transfer descriptor to linear address.
; in: eax = address of first page
; Cut CI: USE_RUST_USB_TD_TO_VIRT=1 routes through Rust
; rust_usb_td_to_virt (see rust/usb_td_to_virt.inc).
; Set USE_RUST_USB_TD_TO_VIRT=0 to restore the original FASM body
; without deleting it. Independent of Cuts CC–CH. Composes Cut AQ.

USE_RUST_USB_TD_TO_VIRT = 1

if USE_RUST_USB_TD_TO_VIRT

; Compatibility trampoline: register ABI → Rust stdcall → EAX (+ ECX OUT).
; Injects page_tabs + OS_BASE so the Rust blob stays reloc-free.
; Preserves EBX/EDX/ESI/EDI/EBP (REG-001/011). ECX reconstructed:
;   miss → original td_phys; hit → td_phys & 0xFFF.
; Plain label (not `proc`) to avoid any hidden frame — matches AQ style.
align 4
usb_td_to_virt:
        push    ebx
        push    edx
        push    esi
        push    edi
        push    ebp
        push    ecx                             ; save td_phys (REG-010)
        stdcall rust_usb_td_to_virt, eax, ecx, page_tabs, OS_BASE
        pop     ecx
        test    eax, eax
        jz      @f
        and     ecx, 0xFFF
@@:
        pop     ebp
        pop     edi
        pop     esi
        pop     edx
        pop     ebx
        ret

else

align 4
proc usb_td_to_virt
; Traverse all pages used for transfer descriptors, looking for the one
; with physical address as in ecx.
@@:
        test    eax, eax
        jz      .zero
        push    eax
        call    get_pg_addr
        sub     eax, ecx
        jz      .found
        cmp     eax, -0x1000
        ja      .found
        pop     eax
        mov     eax, [eax+0x1000-4]
        jmp     @b
.found:
; When found, combine page address from eax with page offset from ecx.
        pop     eax
        and     ecx, 0xFFF
        add     eax, ecx
.zero:
        ret
endp

end if
