;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; blit_32 — Rust embed + ABI smoke (Cut CP)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_blit_32).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_blit_32.bin
;;
;; Requires USE_RUST_BLIT_32=1 so public blit_32 hits the trampoline
;; (not the FASM body). Independent of USE_RUST_BLIT_CLIP (inlined).
;;
;; Critical ABI: rust_blit_32(params, flags, ctx) ret 12; never add esp
;; after the call (REG-009). Snapshot args before stdcall (REG-010).
;;
;; Public blit_32 smoke is unsafe during boot (live LFB / current_slot).
;; This smoke calls rust_blit_32 with synthetic LFB/win_map/LUTs only
;; (REG-003/004). Non-fatal .fail (REG-004) — unique FAIL marker, no hang.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_blit_32_smoke_result dd 0
align 4
cp_smoke_lfb:
        rb  8*8*4
cp_smoke_map:
        rb  8*8
align 4
cp_smoke_bps:
        rd  8
cp_smoke_dw:
        rd  8
cp_smoke_src:
        dd  0xA1B2C3D4, 0x11223344
        dd  0x55667788, 0x99AABBCC
cp_smoke_params:
        dd  0, 0                ; dst_x, dst_y
        dd  2, 2                ; w, h
        dd  0, 0                ; src_x, src_y
        dd  8, 8                ; src_w, src_h
        dd  cp_smoke_src
        dd  8                   ; stride (2 pixels × 4)
align 4
cp_smoke_ctx:
        rb  68
endg

; Called once from high_code after Cut CD smoke.
; Exercises: rust_blit_32 stdcall → exact 32bpp hardware-cursor pixels;
; EBX/ESI/EDI/EBP; stack canary; DF remains 0.
align 4
blit_32_rust_smoke_test:
if ~ USE_RUST_BLIT_32
        ret
end if
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; LUTs: pitch = 32, width = 8
        xor     eax, eax
        mov     ecx, 8
        mov     edi, cp_smoke_bps
        mov     esi, cp_smoke_dw
@@:
        mov     [edi], eax
        mov     edx, eax
        shr     edx, 2          ; y * 8  (eax = y*32)
        mov     [esi], edx
        add     eax, 32
        add     edi, 4
        add     esi, 4
        dec     ecx
        jnz     @b

        mov     edi, cp_smoke_map
        mov     ecx, 8*8
        mov     al, 1
        rep stosb

        mov     edi, cp_smoke_lfb
        mov     ecx, 8*8
        mov     eax, 0xA5A5A5A5
        rep stosd

        ; ctx: win 7×7, slot 1, bpp 32, hardware cursor
        mov     edi, cp_smoke_ctx
        xor     eax, eax
        mov     [edi + 0], eax          ; win_left
        mov     [edi + 4], eax          ; win_top
        mov     dword [edi + 8], 7      ; win_width
        mov     dword [edi + 12], 7     ; win_height
        mov     [edi + 16], eax         ; client_left
        mov     [edi + 20], eax         ; client_top
        mov     dword [edi + 24], 1     ; slot_idx
        mov     dword [edi + 28], 32    ; bpp
        mov     dword [edi + 32], 8     ; display_width
        mov     dword [edi + 36], 32    ; lfb_pitch
        mov     dword [edi + 40], cp_smoke_map
        mov     dword [edi + 44], cp_smoke_lfb
        mov     dword [edi + 48], cp_smoke_bps
        mov     dword [edi + 52], cp_smoke_dw
        mov     dword [edi + 56], 0x11111111    ; select_cursor
        mov     dword [edi + 60], 0x22222222    ; software_cursor (≠)
        mov     [edi + 64], eax                 ; check_mouse = 0

        mov     ebx, 0xB10000C1
        mov     esi, 0x510000C2
        mov     edi, 0xD10000C3
        mov     ebp, 0xE10000C4
        push    0xC0C0C0C0              ; stack canary
        stdcall rust_blit_32, cp_smoke_params, 0, cp_smoke_ctx
        cmp     dword [esp], 0xC0C0C0C0
        jne     .fail
        pop     eax
        cmp     ebx, 0xB10000C1
        jne     .fail
        cmp     esi, 0x510000C2
        jne     .fail
        cmp     edi, 0xD10000C3
        jne     .fail
        cmp     ebp, 0xE10000C4
        jne     .fail
        pushfd
        pop     eax
        test    eax, 0x400              ; DF
        jnz     .fail

        cmp     dword [cp_smoke_lfb], 0xA1B2C3D4
        jne     .fail
        cmp     dword [cp_smoke_lfb + 4], 0x11223344
        jne     .fail
        cmp     dword [cp_smoke_lfb + 32], 0x55667788
        jne     .fail
        cmp     dword [cp_smoke_lfb + 36], 0x99AABBCC
        jne     .fail
        cmp     dword [cp_smoke_lfb + 8], 0xA5A5A5A5
        jne     .fail

        mov     [rust_blit_32_smoke_result], 0x32334C42 ; 'BL32'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     [rust_blit_32_smoke_result], 0xDEAD0C32
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

; Raw Rust stdcall body:
;   rust_blit_32(params, flags, ctx); ret 12
align 16
rust_blit_32:
        file '../../rust_kernel/kolibri_utils/out/rust_blit_32.bin'
