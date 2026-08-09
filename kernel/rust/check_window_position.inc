;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; window._.check_window_position — Rust embed + ABI smoke (Cut S)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_check_window_position).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-check-window-position.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_check_window_position.bin
;;
;; Requires USE_RUST_CHECK_WINDOW_POSITION=1 so public
;; window._.check_window_position hits the trampoline (not the FASM body).
;;
;; Layout note (verified against kernel/const.inc):
;;   BOX: left, top, width, height (4×dd) = 16
;;   WDATA.box = 0 → EDI→WDATA is interchangeable with EDI→BOX for this leaf
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust window-position path ran.
rust_check_window_position_smoke_result dd 0
; Scratch BOX buffers (left, top, width, height).
cwp_smoke_box_inside:
        dd 10, 20, 100, 50
cwp_smoke_box_oversize:
        dd 0, 0, 900, 700
cwp_smoke_box_left_low:
        dd -40, 10, 50, 40
cwp_smoke_box_left_high:
        dd 750, 10, 50, 40
cwp_smoke_box_top_high:
        dd 10, 580, 50, 40
endg

; Called once from high_code after Cut R smoke.
; Exercises: EDI→BOX, display globals via trampoline, box mutation,
; EAX/EBX/ECX/EDX/ESI/EDI preservation.
; On mismatch: distinctive hang (does not reach desktop).
align 4
check_window_position_rust_smoke_test:
        push    eax ebx ecx edx esi edi

        ; Save live display dims; install known 800×600 for deterministic vectors.
        push    dword [_display.width]
        push    dword [_display.height]
        mov     dword [_display.width], 800
        mov     dword [_display.height], 600

        ; ========== vector 1: fully inside — unchanged ==========
        mov     eax, 0xA2000001
        mov     ebx, 0xB2000002
        mov     ecx, 0xC2000003
        mov     edx, 0xD2000004
        mov     esi, 0x52000005
        mov     edi, cwp_smoke_box_inside
        call    window._.check_window_position
        cmp     edi, cwp_smoke_box_inside
        jne     .fail_restore
        cmp     eax, 0xA2000001
        jne     .fail_restore
        cmp     ebx, 0xB2000002
        jne     .fail_restore
        cmp     ecx, 0xC2000003
        jne     .fail_restore
        cmp     edx, 0xD2000004
        jne     .fail_restore
        cmp     esi, 0x52000005
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_inside], 10
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_inside+4], 20
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_inside+8], 100
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_inside+12], 50
        jne     .fail_restore

        ; ========== vector 2: oversize → 799×599 ==========
        mov     edi, cwp_smoke_box_oversize
        call    window._.check_window_position
        cmp     dword [cwp_smoke_box_oversize], 0
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_oversize+4], 0
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_oversize+8], 799
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_oversize+12], 599
        jne     .fail_restore

        ; ========== vector 3: left < 0 → 0 ==========
        mov     edi, cwp_smoke_box_left_low
        call    window._.check_window_position
        cmp     dword [cwp_smoke_box_left_low], 0
        jne     .fail_restore
        cmp     dword [cwp_smoke_box_left_low+4], 10
        jne     .fail_restore

        ; ========== vector 4: left high → 749 ==========
        mov     edi, cwp_smoke_box_left_high
        call    window._.check_window_position
        cmp     dword [cwp_smoke_box_left_high], 749
        jne     .fail_restore

        ; ========== vector 5: top high → 559 ==========
        mov     edi, cwp_smoke_box_top_high
        call    window._.check_window_position
        cmp     dword [cwp_smoke_box_top_high+4], 559
        jne     .fail_restore
        cmp     edi, cwp_smoke_box_top_high
        jne     .fail_restore

        ; Restore display dims
        pop     dword [_display.height]
        pop     dword [_display.width]

        mov     [rust_check_window_position_smoke_result], 0x43575053 ; 'CWPS'
        pop     edi esi edx ecx ebx eax
        ret

.fail_restore:
        pop     dword [_display.height]
        pop     dword [_display.width]
.fail:
        mov     eax, 0xDEAD0C53        ; Cut S fail marker
        mov     ebx, 0x43575053        ; 'CWPS'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_check_window_position(box, width, height); ret 12
align 16
rust_check_window_position:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_check_window_position.bin'
