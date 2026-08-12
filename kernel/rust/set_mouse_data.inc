;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; set_mouse_data — Rust embed + ABI smoke (Cut CF)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_set_mouse_data).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_set_mouse_data.bin
;;
;; Requires USE_RUST_SET_MOUSE_DATA=1 so public set_mouse_data hits the
;; trampoline (not the FASM body). Independent of
;; USE_RUST_MOUSE_ACCELERATION (accel is inlined into this blob).
;;
;; Critical ABI: stdcall 5 args / ret 20; preserves ECX/EDX and
;; EBX/ESI/EDI/EBP (REG-011 — PE SetMouseData callers).
;; rust_set_mouse_data ends `ret 24`; never add esp for those args (REG-009).
;; ABI smoke MUST save/restore live HID globals (REG-003 class).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust set_mouse_data path ran.
rust_set_mouse_data_smoke_result dd 0
endg

; Called once from high_code when USE_RUST_SET_MOUSE_DATA=1.
; Exercises: public trampoline → Rust; relative + absolute + scroll;
; EBX/ECX/EDX/ESI/EDI/EBP preservation; save/restore of live HID state.
align 4
set_mouse_data_rust_smoke_test:
        push    eax ebx ecx edx esi edi ebp

        ; ---- save live HID globals (REG-003) ----
        push    dword [BTN_DOWN]
        push    dword [mouse_active]
        push    dword [osloop_nonperiodic_work]
        movzx   eax, word [MOUSE_X]
        push    eax
        movzx   eax, word [MOUSE_Y]
        push    eax
        movzx   eax, word [MOUSE_SCROLL_H]
        push    eax
        movzx   eax, word [MOUSE_SCROLL_V]
        push    eax
        push    dword [_display.width]
        push    dword [_display.height]
        movzx   eax, byte [mouse_delay]
        push    eax
        movzx   eax, word [mouse_speed_factor]
        push    eax

        ; synthetic display + tunables for known vectors
        mov     dword [_display.width], 800
        mov     dword [_display.height], 600
        mov     byte [mouse_delay], 3
        mov     word [mouse_speed_factor], 4

        ; ========== vector 1: relative X=+5 from (400,300) ==========
        mov     word [MOUSE_X], 400
        mov     word [MOUSE_Y], 300
        mov     word [MOUSE_SCROLL_H], 0
        mov     word [MOUSE_SCROLL_V], 0
        mov     dword [BTN_DOWN], 0xDEADBEEF
        mov     dword [mouse_active], 0
        mov     dword [osloop_nonperiodic_work], 0
        mov     ebx, 0xB100000B
        mov     ecx, 0xC1000001
        mov     edx, 0xD1000002
        mov     esi, 0x51000003
        mov     edi, 0xD1000004
        mov     ebp, 0xE1000005
        stdcall set_mouse_data, 0, 5, 0, 0, 0
        cmp     ebx, 0xB100000B
        jne     .fail_restore
        cmp     ecx, 0xC1000001
        jne     .fail_restore
        cmp     edx, 0xD1000002
        jne     .fail_restore
        cmp     esi, 0x51000003
        jne     .fail_restore
        cmp     edi, 0xD1000004
        jne     .fail_restore
        cmp     ebp, 0xE1000005
        jne     .fail_restore
        ; accel(5)=4 → 400+4=404
        cmp     word [MOUSE_X], 404
        jne     .fail_restore
        cmp     word [MOUSE_Y], 300
        jne     .fail_restore
        cmp     dword [BTN_DOWN], 0
        jne     .fail_restore
        cmp     dword [mouse_active], 1
        jne     .fail_restore
        cmp     dword [osloop_nonperiodic_work], 1
        jne     .fail_restore

        ; ========== vector 2: absolute mid ==========
        ; (0x4000 * 800) >> 15 = 400; same for Y/height 600 → 300
        mov     word [MOUSE_X], 0
        mov     word [MOUSE_Y], 0
        mov     dword [osloop_nonperiodic_work], 0
        mov     ebx, 0xB200000B
        mov     esi, 0x52000003
        mov     edi, 0xD2000004
        mov     ebp, 0xE2000005
        stdcall set_mouse_data, 0xC0000000, 0x4000, 0x4000, 0, 0
        cmp     ebx, 0xB200000B
        jne     .fail_restore
        cmp     esi, 0x52000003
        jne     .fail_restore
        cmp     edi, 0xD2000004
        jne     .fail_restore
        cmp     ebp, 0xE2000005
        jne     .fail_restore
        cmp     word [MOUSE_X], 400
        jne     .fail_restore
        cmp     word [MOUSE_Y], 300
        jne     .fail_restore
        cmp     dword [BTN_DOWN], 0
        jne     .fail_restore

        ; ========== vector 3: scroll bits ==========
        mov     word [MOUSE_SCROLL_V], 10
        mov     word [MOUSE_SCROLL_H], 20
        mov     dword [BTN_DOWN], 0
        stdcall set_mouse_data, 0x7, 0, 0, 3, 5
        cmp     word [MOUSE_SCROLL_V], 13
        jne     .fail_restore
        cmp     word [MOUSE_SCROLL_H], 25
        jne     .fail_restore
        ; btn 0x7 | bit15 | bit23
        mov     eax, [BTN_DOWN]
        cmp     eax, (0x7 or (1 shl 15) or (1 shl 23))
        jne     .fail_restore

        ; ---- restore live HID globals ----
        pop     eax
        mov     word [mouse_speed_factor], ax
        pop     eax
        mov     byte [mouse_delay], al
        pop     dword [_display.height]
        pop     dword [_display.width]
        pop     eax
        mov     word [MOUSE_SCROLL_V], ax
        pop     eax
        mov     word [MOUSE_SCROLL_H], ax
        pop     eax
        mov     word [MOUSE_Y], ax
        pop     eax
        mov     word [MOUSE_X], ax
        pop     dword [osloop_nonperiodic_work]
        pop     dword [mouse_active]
        pop     dword [BTN_DOWN]

        mov     [rust_set_mouse_data_smoke_result], 0x534D4454 ; 'SMDT'
        pop     ebp edi esi edx ecx ebx eax
        ret

.fail_restore:
        pop     eax
        mov     word [mouse_speed_factor], ax
        pop     eax
        mov     byte [mouse_delay], al
        pop     dword [_display.height]
        pop     dword [_display.width]
        pop     eax
        mov     word [MOUSE_SCROLL_V], ax
        pop     eax
        mov     word [MOUSE_SCROLL_H], ax
        pop     eax
        mov     word [MOUSE_Y], ax
        pop     eax
        mov     word [MOUSE_X], ax
        pop     dword [osloop_nonperiodic_work]
        pop     dword [mouse_active]
        pop     dword [BTN_DOWN]
.fail:
        mov     eax, 0xDEAD0CF0
        mov     ebx, 0x534D4454
        mov     ecx, 0x4641494C
        jmp     $

align 16
rust_set_mouse_data:
        file '../../rust_kernel/kolibri_utils/out/rust_set_mouse_data.bin'
