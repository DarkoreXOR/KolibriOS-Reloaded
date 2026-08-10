;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs._.conv_bigtime_to_kos_epoch — Rust embed + ABI smoke (Cut AK)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_bigtime_to_secs).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_bigtime_to_secs.bin
;;
;; Requires USE_RUST_XFS_CONV_BIGTIME_TO_KOS_EPOCH=1 so public
;; xfs._.conv_bigtime_to_kos_epoch hits the trampoline (not the FASM body).
;;
;; Layout (BDFE datetime block via fsTime2bdfe):
;;   +0 sec, +1 min, +2 hour (word; pad at +3), +4 day, +5 month, +6 year u16
;; Bigtime DQ (on-disk BE): hi_be at +0, lo_be at +4 (const.inc struct DQ).
;;
;; Trampoline: movbe → rust_xfs_bigtime_to_secs → call fsTime2bdfe (EDI+=8).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust XFS bigtime path ran.
rust_xfs_conv_bigtime_to_kos_epoch_smoke_result dd 0
; Scratch BDFE buffers (8 bytes each); filled by public ABI.
xfs_bt_smoke_buf0:
        rb 8
xfs_bt_smoke_buf1:
        rb 8
xfs_bt_smoke_buf2:
        rb 8
xfs_bt_smoke_buf3:
        rb 8
; On-disk BE64 bigtime vectors (hi_be || lo_be).
; Epoch bias NS = 0x2B610A3711350000 → BE bytes:
xfs_bt_smoke_epoch:
        db 0x2B, 0x61, 0x0A, 0x37, 0x11, 0x35, 0x00, 0x00
; +1 second = bias + 1e9 = 0x2B610A374CCFCA00
xfs_bt_smoke_plus1:
        db 0x2B, 0x61, 0x0A, 0x37, 0x4C, 0xCF, 0xCA, 0x00
; +86399 seconds end-of-day = 0x2B6158CB66E93600
xfs_bt_smoke_eod:
        db 0x2B, 0x61, 0x58, 0xCB, 0x66, 0xE9, 0x36, 0x00
; Pre-epoch zero → clamp to 2001-01-01 00:00:00
xfs_bt_smoke_zero:
        db 0,0,0,0, 0,0,0,0
endg

; Called once from high_code after Cut AJ smoke.
; Exercises: ECX→BE64 DQ, EDI→block out, EDI+=8, BDFE layout, ESI/EBP preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
xfs_conv_bigtime_to_kos_epoch_rust_smoke_test:
        push    ebx esi edi ebp

        ; ========== vector 1: bias → 2001-01-01 00:00:00; EDI+=8 ==========
        mov     esi, 0xA1000011
        mov     ebp, 0xA1000012
        mov     ecx, xfs_bt_smoke_epoch
        mov     edi, xfs_bt_smoke_buf0
        call    xfs._.conv_bigtime_to_kos_epoch
        cmp     edi, xfs_bt_smoke_buf0 + 8
        jne     .fail
        cmp     esi, 0xA1000011
        jne     .fail
        cmp     ebp, 0xA1000012
        jne     .fail
        cmp     byte [xfs_bt_smoke_buf0], 0        ; sec
        jne     .fail
        cmp     byte [xfs_bt_smoke_buf0+1], 0      ; min
        jne     .fail
        cmp     word [xfs_bt_smoke_buf0+2], 0      ; hour word (pad 0)
        jne     .fail
        cmp     byte [xfs_bt_smoke_buf0+4], 1      ; day
        jne     .fail
        cmp     byte [xfs_bt_smoke_buf0+5], 1      ; month
        jne     .fail
        cmp     word [xfs_bt_smoke_buf0+6], 2001
        jne     .fail

        ; ========== vector 2: +1 second ==========
        mov     ecx, xfs_bt_smoke_plus1
        mov     edi, xfs_bt_smoke_buf1
        call    xfs._.conv_bigtime_to_kos_epoch
        cmp     edi, xfs_bt_smoke_buf1 + 8
        jne     .fail
        cmp     byte [xfs_bt_smoke_buf1], 1
        jne     .fail
        cmp     word [xfs_bt_smoke_buf1+6], 2001
        jne     .fail

        ; ========== vector 3: end of day 23:59:59 ==========
        mov     ecx, xfs_bt_smoke_eod
        mov     edi, xfs_bt_smoke_buf2
        call    xfs._.conv_bigtime_to_kos_epoch
        cmp     edi, xfs_bt_smoke_buf2 + 8
        jne     .fail
        cmp     byte [xfs_bt_smoke_buf2], 59
        jne     .fail
        cmp     byte [xfs_bt_smoke_buf2+1], 59
        jne     .fail
        cmp     word [xfs_bt_smoke_buf2+2], 23
        jne     .fail
        cmp     word [xfs_bt_smoke_buf2+6], 2001
        jne     .fail

        ; ========== vector 4: pre-epoch clamp ==========
        mov     ecx, xfs_bt_smoke_zero
        mov     edi, xfs_bt_smoke_buf3
        call    xfs._.conv_bigtime_to_kos_epoch
        cmp     edi, xfs_bt_smoke_buf3 + 8
        jne     .fail
        cmp     word [xfs_bt_smoke_buf3+6], 2001
        jne     .fail

        mov     [rust_xfs_conv_bigtime_to_kos_epoch_smoke_result], 'XFBT'
        pop     ebp edi esi ebx
        ret

.fail:
        mov     eax, 0xDEAD0C4B
        mov     ebx, 'XFBT'
        mov     ecx, 'FAIL'
        jmp     $

; Reloc-free Rust body (secs only; calendar via fsTime2bdfe).
align 16
rust_xfs_bigtime_to_secs:
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_bigtime_to_secs.bin'
