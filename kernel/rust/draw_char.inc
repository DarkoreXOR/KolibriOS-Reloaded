;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; drawChar — Rust embed + ABI smoke (Cut CR)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_draw_char).
;;
;; Critical ABI: rust_draw_char(ctx) ret 4; never add esp after the
;; call (REG-009). Snapshot dtext stack slots before stdcall (REG-010).
;; Marker 'DCHR' / fail DEAD0C72.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_draw_char_smoke_result dd 0
align 4
cr_smoke_buf:
        rd  16
cr_smoke_glyph:
        db  0, 0, 0x08, 0, 0, 0, 0, 0
endg

align 4
rust_draw_char:
        file '../../rust_kernel/kolibri_utils/out/rust_draw_char.bin'

; Called from high_code. Synthetic buffer, smoothing=0, delta=0, one bit.
; Does not call syscall_getpixel (REG-003).
align 4
draw_char_rust_smoke_test:
if ~ USE_RUST_DRAW_CHAR
        ret
end if
        push    ebx
        push    esi
        push    edi
        push    ebp
        cld

        mov     edi, cr_smoke_buf
        mov     ecx, 16
        mov     eax, 0xA5A5A5A5
        rep stosd

        sub     esp, 40
        mov     dword [esp + 0], 0x00C0FFEE     ; color
        mov     dword [esp + 4], 1              ; multiplier
        mov     dword [esp + 8], cr_smoke_buf
        mov     dword [esp + 12], cr_smoke_glyph + 2
        mov     dword [esp + 16], 1             ; rows
        mov     dword [esp + 20], 64            ; widthX (16 dwords)
        mov     dword [esp + 24], 0             ; delta
        mov     dword [esp + 28], 0             ; smoothing
        mov     dword [esp + 32], 0             ; getpixel
        mov     dword [esp + 36], 0             ; edx_in
        mov     ebx, 0xB10000C1
        mov     esi, 0x510000C2
        mov     edi, 0xD10000C3
        mov     ebp, 0xE10000C4
        mov     eax, esp
        stdcall rust_draw_char, eax
        cmp     ebx, 0xB10000C1
        jne     .fail
        cmp     esi, 0x510000C2
        jne     .fail
        cmp     edi, 0xD10000C3
        jne     .fail
        cmp     ebp, 0xE10000C4
        jne     .fail
        cmp     dword [cr_smoke_buf + 12], 0x00C0FFEE
        jne     .fail
        cmp     dword [cr_smoke_buf], 0xA5A5A5A5
        jne     .fail
        add     esp, 40
        mov     [rust_draw_char_smoke_result], 0x52484344 ; 'DCHR'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     [rust_draw_char_smoke_result], 0xDEAD0C72
@@:
        jmp     @b
