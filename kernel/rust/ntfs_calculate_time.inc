;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfsCalculateTime — Rust embed + ABI smoke (Cut AF)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ntfs_calculate_time).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-ntfs-calculate-time.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ntfs_calculate_time.bin
;;
;; Requires USE_RUST_NTFS_CALCULATE_TIME=1 so public ntfsCalculateTime
;; hits the trampoline (not the FASM body).
;;
;; Layout (BDFE datetime block):
;;   +0 sec, +1 min, +2 hour (word; pad at +3), +4 day, +5 month, +6 year u16
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust ntfsCalculateTime path ran.
rust_ntfs_calculate_time_smoke_result dd 0
; Scratch BDFE buffers (8 bytes each); filled before public ABI calls.
ntfs_ct_smoke_buf0:
        rb 8
ntfs_ct_smoke_buf1:
        rb 8
ntfs_ct_smoke_buf2:
        rb 8
endg

; Called once from high_code after Cut AE smoke.
; Exercises: ESI→BDFE in, EDX:EAX FILETIME out, ESI preserve, named epochs.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ntfs_calculate_time_rust_smoke_test:
        push    ebx esi edi ebp

        ; ========== vector 1: 2001-01-01 00:00:00 → bias FILETIME ==========
        mov     dword [ntfs_ct_smoke_buf0], 0          ; sec/min/hour/pad
        mov     byte [ntfs_ct_smoke_buf0+4], 1         ; day
        mov     byte [ntfs_ct_smoke_buf0+5], 1         ; month
        mov     word [ntfs_ct_smoke_buf0+6], 2001
        mov     esi, ntfs_ct_smoke_buf0
        mov     ebp, 0xAF000011
        mov     edi, 0xAF000012
        call    ntfsCalculateTime
        cmp     eax, 3365781504
        jne     .fail
        cmp     edx, 29389701
        jne     .fail
        cmp     esi, ntfs_ct_smoke_buf0
        jne     .fail
        cmp     ebp, 0xAF000011
        jne     .fail
        cmp     edi, 0xAF000012
        jne     .fail

        ; ========== vector 2: +1 second ==========
        mov     dword [ntfs_ct_smoke_buf1], 0
        mov     byte [ntfs_ct_smoke_buf1], 1           ; sec = 1
        mov     byte [ntfs_ct_smoke_buf1+4], 1
        mov     byte [ntfs_ct_smoke_buf1+5], 1
        mov     word [ntfs_ct_smoke_buf1+6], 2001
        mov     esi, ntfs_ct_smoke_buf1
        call    ntfsCalculateTime
        ; bias + 10_000_000
        mov     ecx, 3365781504
        mov     ebx, 29389701
        add     ecx, 10000000
        adc     ebx, 0
        cmp     eax, ecx
        jne     .fail
        cmp     edx, ebx
        jne     .fail

        ; ========== vector 3: leap 2004-02-29 ==========
        ; secs = fsCalculateTime(2004-02-29) then ×1e7 + bias
        mov     dword [ntfs_ct_smoke_buf2], 0
        mov     byte [ntfs_ct_smoke_buf2+4], 29
        mov     byte [ntfs_ct_smoke_buf2+5], 2
        mov     word [ntfs_ct_smoke_buf2+6], 2004
        mov     esi, ntfs_ct_smoke_buf2
        call    ntfsCalculateTime
        ; Independent check: call fsCalculateTime then FASM mul/add
        push    eax
        push    edx
        mov     esi, ntfs_ct_smoke_buf2
        call    fsCalculateTime
        mov     ecx, eax
        mov     edx, 10000000
        mul     edx
        add     eax, 3365781504
        adc     edx, 29389701
        pop     ebx                     ; saved hi
        pop     esi                     ; saved lo (reuse ESI briefly)
        cmp     esi, eax
        jne     .fail
        cmp     ebx, edx
        jne     .fail

        ; ========== vector 4: end-of-day 23:59:59 ==========
        mov     byte [ntfs_ct_smoke_buf0], 59          ; sec
        mov     byte [ntfs_ct_smoke_buf0+1], 59        ; min
        mov     byte [ntfs_ct_smoke_buf0+2], 23        ; hour
        mov     byte [ntfs_ct_smoke_buf0+3], 0
        mov     byte [ntfs_ct_smoke_buf0+4], 1
        mov     byte [ntfs_ct_smoke_buf0+5], 1
        mov     word [ntfs_ct_smoke_buf0+6], 2001
        mov     esi, ntfs_ct_smoke_buf0
        call    ntfsCalculateTime
        push    eax
        push    edx
        mov     esi, ntfs_ct_smoke_buf0
        call    fsCalculateTime
        mov     edx, 10000000
        mul     edx
        add     eax, 3365781504
        adc     edx, 29389701
        pop     ebx
        pop     esi
        cmp     esi, eax
        jne     .fail
        cmp     ebx, edx
        jne     .fail

        ; ========== vector 5: ESI chaining (SetFileInfo pattern) ==========
        ; Three consecutive BDFE blocks; ESI must not be advanced by the call.
        mov     dword [ntfs_ct_smoke_buf0], 0
        mov     byte [ntfs_ct_smoke_buf0+4], 1
        mov     byte [ntfs_ct_smoke_buf0+5], 1
        mov     word [ntfs_ct_smoke_buf0+6], 2001
        mov     dword [ntfs_ct_smoke_buf1], 0
        mov     byte [ntfs_ct_smoke_buf1], 1
        mov     byte [ntfs_ct_smoke_buf1+4], 1
        mov     byte [ntfs_ct_smoke_buf1+5], 1
        mov     word [ntfs_ct_smoke_buf1+6], 2001
        mov     esi, ntfs_ct_smoke_buf0
        call    ntfsCalculateTime
        cmp     esi, ntfs_ct_smoke_buf0
        jne     .fail
        add     esi, 8
        call    ntfsCalculateTime
        cmp     esi, ntfs_ct_smoke_buf1
        jne     .fail
        ; +1s FILETIME expected
        mov     ecx, 3365781504
        mov     ebx, 29389701
        add     ecx, 10000000
        adc     ebx, 0
        cmp     eax, ecx
        jne     .fail
        cmp     edx, ebx
        jne     .fail

        mov     [rust_ntfs_calculate_time_smoke_result], 0x4E544354 ; 'NTCT'
        pop     ebp edi esi ebx
        ret

.fail:
        mov     eax, 0xDEAD0CAF        ; Cut AF fail marker
        mov     ebx, 0x4E544354        ; 'NTCT'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ntfs_calculate_time(block) -> EDX:EAX; ret 4
align 16
rust_ntfs_calculate_time:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_calculate_time.bin'
