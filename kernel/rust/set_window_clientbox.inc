;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; window._.set_window_clientbox — Rust embed + ABI smoke (Cut CE)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_set_window_clientbox).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_set_window_clientbox.bin
;;
;; Requires USE_RUST_SET_WINDOW_CLIENTBOX=1 so public
;; window._.set_window_clientbox hits the trampoline (not the FASM body).
;; Independent of USE_RUST_CHECK_WINDOW_POSITION.
;;
;; Critical ABI: EDI → WDATA*; preserves EAX/ECX/EDI; mutates clientbox +
;; window_topleft[3..4].top from _skinh. FASM `stdcall` only —
;; rust_set_window_clientbox ends `ret 12`; never add esp (REG-009).
;;
;; Layout note (verified against kernel/const.inc):
;;   WDATA.box = 0 (16)
;;   WDATA.fl_wstyle = 19
;;   WDATA.clientbox = 32 (16)
;;   WSTYLE_CLIENTRELATIVE = 0x20
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust clientbox path ran.
rust_set_window_clientbox_smoke_result dd 0
; Single reusable WDATA scratch (48 B through clientbox) — keep iglobal lean.
align 4
swcb_smoke_wdata:
        rd 12
endg

; Called once from high_code when USE_RUST_SET_WINDOW_CLIENTBOX=1.
; Exercises: public trampoline → Rust; whole-window + style0 + style3;
; skin-top mutation; EAX/ECX/EDI preservation.
align 4
set_window_clientbox_rust_smoke_test:
        push    eax ebx ecx edx esi edi ebp

        push    dword [_skinh]
        push    dword [window_topleft + 8 * 3 + 4]
        push    dword [window_topleft + 8 * 4 + 4]
        mov     dword [_skinh], 22
        mov     dword [window_topleft + 0], 1
        mov     dword [window_topleft + 4], 21
        mov     dword [window_topleft + 8 * 3], 5
        mov     dword [window_topleft + 8 * 4], 5

        ; --- clear scratch ---
        mov     edi, swcb_smoke_wdata
        mov     ecx, 12
        xor     eax, eax
        rep stosd

        ; ========== vector 1: whole-window ==========
        mov     dword [swcb_smoke_wdata + 0], 10
        mov     dword [swcb_smoke_wdata + 4], 20
        mov     dword [swcb_smoke_wdata + 8], 300
        mov     dword [swcb_smoke_wdata + 12], 200
        mov     dword [swcb_smoke_wdata + 32], 0x11111111
        mov     dword [swcb_smoke_wdata + 36], 0x22222222
        mov     dword [swcb_smoke_wdata + 40], 0x33333333
        mov     dword [swcb_smoke_wdata + 44], 0x44444444
        mov     eax, 0xA3000001
        mov     ecx, 0xC3000003
        mov     edi, swcb_smoke_wdata
        call    window._.set_window_clientbox
        cmp     edi, swcb_smoke_wdata
        jne     .fail_restore
        cmp     eax, 0xA3000001
        jne     .fail_restore
        cmp     ecx, 0xC3000003
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 32], 0
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 36], 0
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 40], 300
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 44], 200
        jne     .fail_restore
        cmp     dword [window_topleft + 8 * 3 + 4], 22
        jne     .fail_restore
        cmp     dword [window_topleft + 8 * 4 + 4], 22
        jne     .fail_restore

        ; ========== vector 2: clientrel style 0 ==========
        mov     edi, swcb_smoke_wdata
        mov     ecx, 12
        xor     eax, eax
        rep stosd
        mov     dword [swcb_smoke_wdata + 8], 100
        mov     dword [swcb_smoke_wdata + 12], 80
        mov     dword [swcb_smoke_wdata + 16], 0x20000000 ; fl_wstyle=0x20
        mov     dword [_skinh], 22
        mov     edi, swcb_smoke_wdata
        call    window._.set_window_clientbox
        cmp     dword [swcb_smoke_wdata + 32], 1
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 36], 21
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 40], 99
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 44], 59
        jne     .fail_restore

        ; ========== vector 3: clientrel style 3 + skinh ==========
        mov     edi, swcb_smoke_wdata
        mov     ecx, 12
        xor     eax, eax
        rep stosd
        mov     dword [swcb_smoke_wdata + 8], 200
        mov     dword [swcb_smoke_wdata + 12], 150
        mov     dword [swcb_smoke_wdata + 16], 0x23000000 ; fl_wstyle=0x23
        mov     dword [_skinh], 25
        mov     edi, swcb_smoke_wdata
        call    window._.set_window_clientbox
        cmp     dword [swcb_smoke_wdata + 32], 5
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 36], 25
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 40], 191
        jne     .fail_restore
        cmp     dword [swcb_smoke_wdata + 44], 121
        jne     .fail_restore
        cmp     dword [window_topleft + 8 * 3 + 4], 25
        jne     .fail_restore
        cmp     dword [window_topleft + 8 * 4 + 4], 25
        jne     .fail_restore
        cmp     edi, swcb_smoke_wdata
        jne     .fail_restore

        pop     dword [window_topleft + 8 * 4 + 4]
        pop     dword [window_topleft + 8 * 3 + 4]
        pop     dword [_skinh]

        mov     [rust_set_window_clientbox_smoke_result], 0x53574342 ; 'SWCB'
        pop     ebp edi esi edx ecx ebx eax
        ret

.fail_restore:
        pop     dword [window_topleft + 8 * 4 + 4]
        pop     dword [window_topleft + 8 * 3 + 4]
        pop     dword [_skinh]
.fail:
        mov     eax, 0xDEAD0CE0
        mov     ebx, 0x53574342
        mov     ecx, 0x4641494C
        jmp     $

align 16
rust_set_window_clientbox:
        file '../../rust_kernel/kolibri_utils/out/rust_set_window_clientbox.bin'
