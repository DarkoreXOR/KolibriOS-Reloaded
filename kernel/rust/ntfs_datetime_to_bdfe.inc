;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfs_datetime_to_bdfe — Rust embed + ABI smoke (Cut AE)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ntfs_datetime_to_bdfe).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-ntfs-datetime-to-bdfe.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ntfs_datetime_to_bdfe.bin
;;
;; Requires USE_RUST_NTFS_DATETIME_TO_BDFE=1 so public ntfs_datetime_to_bdfe
;; hits the trampoline (not the FASM body).
;;
;; Layout (BDFE datetime block):
;;   +0 sec, +1 min, +2 hour (word; pad at +3), +4 day, +5 month, +6 year u16
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust ntfs_datetime_to_bdfe path ran.
rust_ntfs_datetime_to_bdfe_smoke_result dd 0
; Scratch BDFE buffers (8 bytes each); filled by public ABI.
ntfs_dt_smoke_buf0:
        rb 8
ntfs_dt_smoke_buf1:
        rb 8
ntfs_dt_smoke_buf2:
        rb 8
ntfs_dt_smoke_buf3:
        rb 8
endg

; Called once from high_code after Cut T smoke.
; Exercises: EDX:EAX FILETIME in, EDI→block out, EDI+=8, BDFE layout, ESI/EBP preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ntfs_datetime_to_bdfe_rust_smoke_test:
        push    ebx esi edi ebp

        ; ========== vector 1: FILETIME bias → 2001-01-01 00:00:00; EDI+=8 ==========
        ; Bias constants from ntfs.inc: lo=3365781504, hi=29389701
        mov     eax, 3365781504
        mov     edx, 29389701
        mov     esi, 0xAE000011
        mov     ebp, 0xAE000012
        mov     edi, ntfs_dt_smoke_buf0
        call    ntfs_datetime_to_bdfe
        cmp     edi, ntfs_dt_smoke_buf0 + 8
        jne     .fail
        cmp     esi, 0xAE000011
        jne     .fail
        cmp     ebp, 0xAE000012
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf0], 0        ; sec
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf0+1], 0      ; min
        jne     .fail
        cmp     word [ntfs_dt_smoke_buf0+2], 0      ; hour word (pad 0)
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf0+4], 1      ; day
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf0+5], 1      ; month
        jne     .fail
        cmp     word [ntfs_dt_smoke_buf0+6], 2001
        jne     .fail

        ; ========== vector 2: +1 second FILETIME ==========
        ; bias + 10_000_000
        mov     eax, 3365781504
        add     eax, 10000000
        mov     edx, 29389701
        adc     edx, 0
        mov     edi, ntfs_dt_smoke_buf1
        call    ntfs_datetime_to_bdfe
        cmp     edi, ntfs_dt_smoke_buf1 + 8
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf1], 1
        jne     .fail
        cmp     word [ntfs_dt_smoke_buf1+6], 2001
        jne     .fail

        ; ========== vector 3: leap 2004-02-29 (99705600 secs) ==========
        ; filetime = bias + 99705600 * 10000000
        mov     eax, 99705600
        mov     ecx, 10000000
        mul     ecx
        add     eax, 3365781504
        adc     edx, 29389701
        mov     edi, ntfs_dt_smoke_buf2
        call    ntfs_datetime_to_bdfe
        cmp     edi, ntfs_dt_smoke_buf2 + 8
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf2+4], 29
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf2+5], 2
        jne     .fail
        cmp     word [ntfs_dt_smoke_buf2+6], 2004
        jne     .fail

        ; ========== vector 4: end-of-day 86399 → 23:59:59; hour word pad ==========
        mov     eax, 86399
        mov     ecx, 10000000
        mul     ecx
        add     eax, 3365781504
        adc     edx, 29389701
        mov     edi, ntfs_dt_smoke_buf3
        mov     dword [ntfs_dt_smoke_buf3], 0xAAAAAAAA
        mov     dword [ntfs_dt_smoke_buf3+4], 0xBBBBBBBB
        call    ntfs_datetime_to_bdfe
        cmp     edi, ntfs_dt_smoke_buf3 + 8
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf3], 59
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf3+1], 59
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf3+2], 23
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf3+3], 0      ; pad cleared
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf3+4], 1
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf3+5], 1
        jne     .fail
        cmp     word [ntfs_dt_smoke_buf3+6], 2001
        jne     .fail

        ; ========== vector 5: consecutive writes (EDI chaining) ==========
        mov     edi, ntfs_dt_smoke_buf0
        mov     eax, 3365781504
        mov     edx, 29389701
        call    ntfs_datetime_to_bdfe
        ; +86400 secs → day 2
        mov     eax, 86400
        mov     ecx, 10000000
        mul     ecx
        add     eax, 3365781504
        adc     edx, 29389701
        call    ntfs_datetime_to_bdfe
        cmp     edi, ntfs_dt_smoke_buf0 + 16
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf0+4], 1      ; first: day 1
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf0+8+4], 2    ; second: day 2
        jne     .fail

        ; ========== vector 6: clamp path (EDX>=1e7 after bias → EDX:=0) ==========
        ; lo = bias_lo + 50_000_000; hi = bias_hi + 10_000_000 → secs = 5
        mov     eax, 3365781504
        add     eax, 50000000
        mov     edx, 29389701
        add     edx, 10000000
        mov     edi, ntfs_dt_smoke_buf1
        call    ntfs_datetime_to_bdfe
        cmp     edi, ntfs_dt_smoke_buf1 + 8
        jne     .fail
        cmp     byte [ntfs_dt_smoke_buf1], 5        ; 5 seconds
        jne     .fail
        cmp     word [ntfs_dt_smoke_buf1+6], 2001
        jne     .fail

        mov     [rust_ntfs_datetime_to_bdfe_smoke_result], 0x4E544442 ; 'NTDB'
        pop     ebp edi esi ebx
        ret

.fail:
        mov     eax, 0xDEAD0CAE        ; Cut AE fail marker
        mov     ebx, 0x4E544442        ; 'NTDB'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ntfs_datetime_to_bdfe(ft_lo, ft_hi, out); ret 12
align 16
rust_ntfs_datetime_to_bdfe:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_datetime_to_bdfe.bin'
