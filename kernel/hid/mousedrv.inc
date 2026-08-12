;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;                                                              ;;
;; Copyright (C) KolibriOS team 2004-2024. All rights reserved. ;;
;; Distributed under terms of the GNU General Public License    ;;
;;                                                              ;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;


;   check mouse
;
;
;   FB00  ->   FB0F   mouse memory 00 chunk count - FB0A-B x - FB0C-D y
;   FB10  ->   FB17   mouse color mem
;   FB21              x move
;   FB22              y move
;   FB30              color temp
;   FB28              high bits temp
;   FB4A  ->   FB4D   FB4A-B x-under - FB4C-D y-under
;   FC00  ->   FCFE   com1/ps2 buffer
;   FCFF              com1/ps2 buffer count starting from FC00

uglobal
;--------------------------------------
align 4
mousecount                      dd ?
mousedata                       dd ?
Y_UNDER_sub_CUR_hot_y_add_curh  dw ?
Y_UNDER_subtraction_CUR_hot_y   dw ?
X_UNDER_sub_CUR_hot_x_add_curh  dw ?
X_UNDER_subtraction_CUR_hot_x   dw ?
endg

iglobal
;--------------------------------------
align 4
mouse_speed_factor      dw 4
mouse_delay             db 3
mouse_doubleclick_delay db 64
endg

;-----------------------------------------------------------------------------

align 4
draw_mouse_under:

        ; return old picture
        cmp     [_display.restore_cursor], 0
        je      @F

        pushad
        movzx   eax, word [X_UNDER]
        movzx   ebx, word [Y_UNDER]
        stdcall [_display.restore_cursor], eax, ebx
        popad
@@:
        ret

;-----------------------------------------------------------------------------

align 4
save_draw_mouse:
        cmp     [_display.move_cursor], 0
        je      .exit
        pushad

        mov     [X_UNDER], ax
        mov     [Y_UNDER], bx
        movzx   eax, word [MOUSE_Y]
        movzx   ebx, word [MOUSE_X]
        push    eax
        push    ebx

        mov     eax, [d_width_calc_area + eax*4]

        add     eax, [_display.win_map]
        movzx   edx, byte [ebx + eax]
        shl     edx, BSF sizeof.WDATA
        ; edx - thread slot of window under cursor
        mov     esi, [window_data + edx + WDATA.cursor] ; cursor of window under cursor

        ; if cursor of window under cursor already equal to the
        ; current_cursor then just draw it
        cmp     esi, [current_cursor]
        je      .draw

        ; eax = thread slot of current active window
        mov     eax, [thread_count]
        movzx   eax, word [WIN_POS + eax*2]
        shl     eax, BSF sizeof.WDATA

        ; window under cursor == active window ?
        cmp     eax, edx
        je      @f

        ; check whether active window is being resized now:
        mov     bl, [mouse.active_sys_window.action]
        and     bl, mouse.WINDOW_RESIZE_S_FLAG or mouse.WINDOW_RESIZE_W_FLAG or mouse.WINDOW_RESIZE_E_FLAG
        test    bl, bl
        jz      .active_isnt_resizing
        mov     esi, [window_data + eax + WDATA.cursor] ; esi = cursor of active window, it is resizing cursor
        jmp     @f
        .active_isnt_resizing:

        ; if cursor of window under the cursor is resizing cursor then draw it.
        cmp     esi, [def_cursor_hresize]
        je      @f
        cmp     esi, [def_cursor_vresize]
        je      @f
        cmp     esi, [def_cursor_dresize1]
        je      @f
        cmp     esi, [def_cursor_dresize2]
        je      @f

        ; set cursor of window under cursor
        mov     esi, [window_data + edx + WDATA.cursor]
        cmp     esi, [current_cursor]
        je      .draw

@@:
        cmp     [_display.select_cursor], 0
        jz      .error

        stdcall [_display.select_cursor], esi
        mov     [current_cursor], esi
;--------------------------------------
align 4
.draw:
        stdcall [_display.move_cursor], esi
        popad
        ret
;--------------------------------------
;align 4
;.fail:
;        mov     ecx, [def_cursor]
;        mov     [window_data + edx + WDATA.cursor], ecx
;        stdcall [_display.move_cursor], ecx        ; stdcall: [esp]=ebx,eax
;        popad
;        ret
;--------------------------------------
align 4
.error:
        popad
.exit:
        ret

;-----------------------------------------------------------------------------

align 4
__sys_draw_pointer:
        pushad
        movzx   ecx, word [X_UNDER]
        movzx   edx, word [Y_UNDER]
        movzx   ebx, word [MOUSE_Y]
        movzx   eax, word [MOUSE_X]
        cmp     [redrawmouse_unconditional], 0
        je      @f
        mov     [redrawmouse_unconditional], 0
        jmp     .redrawmouse
  @@:
        cmp     eax, ecx
        jne     .redrawmouse
        cmp     ebx, edx
        je      .nodmp

;--------------------------------------

align 4
.redrawmouse:
        pushfd
        cli
        call    draw_mouse_under
        call    save_draw_mouse

        cmp     [_display.select_cursor], select_cursor
        jne     @f

        xor     eax, eax
        mov     esi, [current_cursor]

        mov     ax, [Y_UNDER]
        sub     eax, [esi + CURSOR.hot_y]
        mov     [Y_UNDER_subtraction_CUR_hot_y], ax
        add     eax, [cur.h]
        mov     [Y_UNDER_sub_CUR_hot_y_add_curh], ax

        mov     ax, [X_UNDER]
        sub     eax, [esi + CURSOR.hot_x]
        mov     [X_UNDER_subtraction_CUR_hot_x], ax
        add     eax, [cur.w]
        mov     [X_UNDER_sub_CUR_hot_x_add_curh], ax
  @@:
        popfd
.nodmp:
        popad
        ret

;-----------------------------------------------------------------------------

align 4
;-----------------------------------------------------------------------------
; Cut CF: USE_RUST_SET_MOUSE_DATA=1 routes through Rust
; rust_set_mouse_data (see rust/set_mouse_data.inc).
; Set USE_RUST_SET_MOUSE_DATA=0 to restore the original FASM body
; without deleting it. Independent of Cut L / USE_RUST_MOUSE_ACCELERATION
; (accel is inlined into the Cut CF blob).

USE_RUST_SET_MOUSE_DATA = 1

if USE_RUST_SET_MOUSE_DATA

; Compatibility trampoline: stdcall 5 args → Rust stdcall + ctx; ret 20.
; Preserves ECX/EDX (FASM `uses ecx edx`) AND EBX/ESI/EDI/EBP — original
; `proc` kept them intact (EBP via frame; others unused). PE SetMouseData
; callers rely on that (REG-011).
; HID globals injected via stack ctx so the blob stays reloc-free.
; rust_set_mouse_data ends `ret 24` — do NOT add esp for those args (REG-009).
align 4
set_mouse_data:
        push    ebx
        push    esi
        push    edi
        push    ebp
        push    ecx
        push    edx
        ; Build SetMouseDataCtx on stack (11 dwords = 44 bytes):
        ;   +0  mouse_x*, +4 mouse_y*, +8 scroll_h*, +12 scroll_v*
        ;   +16 btn_down*, +20 mouse_active*
        ;   +24 display_width, +28 display_height
        ;   +32 mouse_delay, +36 mouse_speed_factor
        ;   +40 osloop*
        sub     esp, 44
        mov     dword [esp + 0], MOUSE_X
        mov     dword [esp + 4], MOUSE_Y
        mov     dword [esp + 8], MOUSE_SCROLL_H
        mov     dword [esp + 12], MOUSE_SCROLL_V
        mov     dword [esp + 16], BTN_DOWN
        mov     dword [esp + 20], mouse_active
        mov     eax, [_display.width]
        mov     [esp + 24], eax
        mov     eax, [_display.height]
        mov     [esp + 28], eax
        movzx   eax, byte [mouse_delay]
        mov     [esp + 32], eax
        movzx   eax, word [mouse_speed_factor]
        mov     [esp + 36], eax
        mov     dword [esp + 40], osloop_nonperiodic_work
        ; Args above: ctx(44) + edx,ecx,ebp,edi,esi,ebx(24) + retaddr(4) = 72
        ; [esp+72]=BtnState, +76=XMoving, +80=YMoving, +84=VScroll, +88=HScroll
        mov     eax, [esp + 72]
        mov     ebx, [esp + 76]
        mov     esi, [esp + 80]
        mov     edi, [esp + 84]
        mov     ebp, [esp + 88]
        stdcall rust_set_mouse_data, eax, ebx, esi, edi, ebp, esp
        add     esp, 44                 ; local ctx only — not Rust args
        pop     edx
        pop     ecx
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret     20

else

proc set_mouse_data stdcall uses ecx edx, BtnState:dword, XMoving:dword, YMoving:dword, VScroll:dword, HScroll:dword

        mov     eax, [BtnState]
        and     eax, 0x3FFFFFFF         ; Top 2 bits are used to flag absolute movements
        mov     [BTN_DOWN], eax
;--------------------------------------
        mov     eax, [XMoving]
        test    [BtnState], 0x80000000
        jnz     .absolute_x
        test    eax, eax
        jz      @f
        call    mouse_acceleration
        add     ax, [MOUSE_X]
        jns     .check_x
        xor     eax, eax
        jmp     .set_x
 .absolute_x:
        mov     edx, [_display.width]
        mul     edx
        shr     eax, 15
 .check_x:
        cmp     ax, word[_display.width]
        jl      .set_x
        mov     ax, word[_display.width]
        dec     ax
 .set_x:
        mov     [MOUSE_X], ax
;--------------------------------------
@@:
        mov     eax, [YMoving]
        test    [BtnState], 0x40000000
        jnz     .absolute_y
        test    eax, eax
        jz      @f
        neg     eax
        call    mouse_acceleration
        add     ax, [MOUSE_Y]
        jns     .check_y
        xor     eax, eax
        jmp     .set_y
 .absolute_y:
        mov     edx, [_display.height]
        mul     edx
        shr     eax, 15
 .check_y:
        cmp     ax, word[_display.height]
        jl      .set_y
        mov     ax, word[_display.height]
        dec     ax
 .set_y:
        mov     [MOUSE_Y], ax
;--------------------------------------
@@:
        mov     eax, [VScroll]
        test    eax, eax
        jz      @f
        add     [MOUSE_SCROLL_V], ax
        bts     word [BTN_DOWN], 15
@@:
        mov     eax, [HScroll]
        test    eax, eax
        jz      @f
        add     [MOUSE_SCROLL_H], ax
        bts     dword [BTN_DOWN], 23
@@:
        mov     [mouse_active], 1
        call    wakeup_osloop
        ret
endp

end if

;-----------------------------------------------------------------------------
; Cut L: USE_RUST_MOUSE_ACCELERATION=1 routes through Rust
; rust_mouse_acceleration (see rust/mouse_acceleration.inc).
; Set USE_RUST_MOUSE_ACCELERATION=0 to restore the original FASM body
; without deleting it. Independent of Cuts A–K.

USE_RUST_MOUSE_ACCELERATION = 1

if USE_RUST_MOUSE_ACCELERATION

; Compatibility trampoline: register ABI → Rust stdcall → EAX.
; Callers keep `call mouse_acceleration` with EAX delta in/out.
; CX may be clobbered (matches original FASM body loading speed factor).
; Tunables stay in FASM globals so the Rust blob remains reloc-free.
align 4
mouse_acceleration:
        movzx   ecx, byte [mouse_delay]
        movzx   edx, word [mouse_speed_factor]
        stdcall rust_mouse_acceleration, eax, ecx, edx
        ret

else

mouse_acceleration:
        neg     ax
        jl      mouse_acceleration
        add     al, [mouse_delay]
        mul     al
        mov     cx, [mouse_speed_factor]
        dec     ax
        shr     ax, cl
        inc     ax
        test    eax, eax
        jns     @f
        neg     ax
@@:
        ret

end if
