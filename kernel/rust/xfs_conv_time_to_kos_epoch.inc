;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs._.conv_time_to_kos_epoch — Rust embed + ABI smoke (Cut BN)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_conv_time_to_kos_epoch).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_conv_time_to_kos_epoch.bin
;;
;; Requires USE_RUST_XFS_CONV_TIME_TO_KOS_EPOCH=1 so public
;; xfs._.conv_time_to_kos_epoch hits the trampoline (not the FASM body).
;;
;; Classic XFS time DQ layout: hi_be at +0 carries seconds since 2001-01-01;
;; lo_be at +4 is ignored by the legacy leaf. Public ABI composes with
;; fsTime2bdfe, so EDI must advance by exactly 8.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_xfs_conv_time_to_kos_epoch_smoke_result dd 0
xfs_time_smoke_buf0:
        rb 8
xfs_time_smoke_buf1:
        rb 8
xfs_time_smoke_buf2:
        rb 8
xfs_time_smoke_buf3:
        rb 8
; hi_be = seconds since 2001, lo_be ignored.
xfs_time_smoke_epoch:
        db 0x00, 0x00, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44
xfs_time_smoke_plus1:
        db 0x00, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0xCC, 0xDD
xfs_time_smoke_eod:
        db 0x00, 0x01, 0x51, 0x7F, 0xDE, 0xAD, 0xBE, 0xEF
xfs_time_smoke_leap:
        db 0x05, 0xF1, 0x63, 0x00, 0xFE, 0xED, 0xFA, 0xCE
endg

; Called once from high_code after Cut AK smoke.
; Exercises direct rust_* and the public trampoline: ECX->DQ, EDI->BDFE, EDI+=8,
; low dword ignored, ESI/EBP preserved for the omit-FP XFS caller shape.
align 4
xfs_conv_time_to_kos_epoch_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: epoch ---
        stdcall rust_xfs_conv_time_to_kos_epoch, 0, xfs_time_smoke_buf0
        cmp     byte [xfs_time_smoke_buf0], 0
        jne     .fail
        cmp     byte [xfs_time_smoke_buf0+1], 0
        jne     .fail
        cmp     word [xfs_time_smoke_buf0+2], 0
        jne     .fail
        cmp     byte [xfs_time_smoke_buf0+4], 1
        jne     .fail
        cmp     byte [xfs_time_smoke_buf0+5], 1
        jne     .fail
        cmp     word [xfs_time_smoke_buf0+6], 2001
        jne     .fail

        ; --- Direct rust_*: +1 second ---
        stdcall rust_xfs_conv_time_to_kos_epoch, 1, xfs_time_smoke_buf1
        cmp     byte [xfs_time_smoke_buf1], 1
        jne     .fail
        cmp     word [xfs_time_smoke_buf1+6], 2001
        jne     .fail

        ; --- Public trampoline: end of day + low-dword ignored + EDI+=8 ---
        mov     esi, 0xB1B1B1B1
        mov     ebp, 0xC2C2C2C2
        mov     ecx, xfs_time_smoke_eod
        mov     edi, xfs_time_smoke_buf2
        call    xfs._.conv_time_to_kos_epoch
        cmp     edi, xfs_time_smoke_buf2 + 8
        jne     .fail
        cmp     esi, 0xB1B1B1B1
        jne     .fail
        cmp     ebp, 0xC2C2C2C2
        jne     .fail
        cmp     byte [xfs_time_smoke_buf2], 59
        jne     .fail
        cmp     byte [xfs_time_smoke_buf2+1], 59
        jne     .fail
        cmp     word [xfs_time_smoke_buf2+2], 23
        jne     .fail
        cmp     byte [xfs_time_smoke_buf2+4], 1
        jne     .fail
        cmp     byte [xfs_time_smoke_buf2+5], 1
        jne     .fail
        cmp     word [xfs_time_smoke_buf2+6], 2001
        jne     .fail

        ; --- Public trampoline: leap-day sample ---
        mov     ecx, xfs_time_smoke_leap
        mov     edi, xfs_time_smoke_buf3
        call    xfs._.conv_time_to_kos_epoch
        cmp     edi, xfs_time_smoke_buf3 + 8
        jne     .fail
        cmp     byte [xfs_time_smoke_buf3], 0
        jne     .fail
        cmp     byte [xfs_time_smoke_buf3+1], 0
        jne     .fail
        cmp     word [xfs_time_smoke_buf3+2], 0
        jne     .fail
        cmp     byte [xfs_time_smoke_buf3+4], 29
        jne     .fail
        cmp     byte [xfs_time_smoke_buf3+5], 2
        jne     .fail
        cmp     word [xfs_time_smoke_buf3+6], 2004
        jne     .fail

        mov     [rust_xfs_conv_time_to_kos_epoch_smoke_result], 'XFCT'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C4E
        mov     ebx, 'XFCT'
        mov     ecx, 'FAIL'
        jmp     $

align 16
rust_xfs_conv_time_to_kos_epoch:
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_conv_time_to_kos_epoch.bin'
