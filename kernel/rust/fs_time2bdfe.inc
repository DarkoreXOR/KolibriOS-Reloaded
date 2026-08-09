;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fsTime2bdfe — Rust embed + ABI smoke (Cut T)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fs_time2bdfe).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-fs-time2bdfe.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fs_time2bdfe.bin
;;
;; Requires USE_RUST_FS_TIME2BDFE=1 so public fsTime2bdfe hits the
;; trampoline (not the FASM body).
;;
;; Layout (BDFE datetime block):
;;   +0 sec, +1 min, +2 hour (word; pad at +3), +4 day, +5 month, +6 year u16
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fsTime2bdfe path ran.
rust_fs_time2bdfe_smoke_result dd 0
; Scratch BDFE buffers (8 bytes each); filled by public ABI.
fs_t2b_smoke_buf0:
        rb 8
fs_t2b_smoke_buf1:
        rb 8
fs_t2b_smoke_buf2:
        rb 8
fs_t2b_smoke_buf3:
        rb 8
endg

; Called once from high_code after Cut S smoke.
; Exercises: EAX secs in, EDI→block out, EDI+=8, BDFE layout, ESI/EBP preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fs_time2bdfe_rust_smoke_test:
        push    ebx esi edi ebp

        ; ========== vector 1: epoch 0 → 2001-01-01 00:00:00; EDI+=8 ==========
        mov     eax, 0
        mov     esi, 0x57000011
        mov     ebp, 0xE7000012
        mov     edi, fs_t2b_smoke_buf0
        call    fsTime2bdfe
        cmp     edi, fs_t2b_smoke_buf0 + 8
        jne     .fail
        cmp     esi, 0x57000011
        jne     .fail
        cmp     ebp, 0xE7000012
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf0], 0        ; sec
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf0+1], 0      ; min
        jne     .fail
        cmp     word [fs_t2b_smoke_buf0+2], 0      ; hour word (pad 0)
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf0+4], 1      ; day
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf0+5], 1      ; month
        jne     .fail
        cmp     word [fs_t2b_smoke_buf0+6], 2001
        jne     .fail

        ; ========== vector 2: leap 2004-02-29 (99705600 secs) ==========
        mov     eax, 99705600
        mov     edi, fs_t2b_smoke_buf1
        call    fsTime2bdfe
        cmp     edi, fs_t2b_smoke_buf1 + 8
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf1+4], 29
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf1+5], 2
        jne     .fail
        cmp     word [fs_t2b_smoke_buf1+6], 2004
        jne     .fail

        ; ========== vector 3: 2010-07-04 12:00:00 ==========
        mov     eax, 299937600
        mov     edi, fs_t2b_smoke_buf2
        call    fsTime2bdfe
        cmp     edi, fs_t2b_smoke_buf2 + 8
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf2], 0
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf2+1], 0
        jne     .fail
        cmp     word [fs_t2b_smoke_buf2+2], 12
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf2+4], 4
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf2+5], 7
        jne     .fail
        cmp     word [fs_t2b_smoke_buf2+6], 2010
        jne     .fail

        ; ========== vector 4: end-of-day 86399 → 23:59:59; hour word pad ==========
        mov     eax, 86399
        mov     edi, fs_t2b_smoke_buf3
        ; Poison pad byte so trampoline/Rust must clear via word store semantics
        mov     dword [fs_t2b_smoke_buf3], 0xAAAAAAAA
        mov     dword [fs_t2b_smoke_buf3+4], 0xBBBBBBBB
        call    fsTime2bdfe
        cmp     edi, fs_t2b_smoke_buf3 + 8
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf3], 59
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf3+1], 59
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf3+2], 23
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf3+3], 0      ; pad cleared
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf3+4], 1
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf3+5], 1
        jne     .fail
        cmp     word [fs_t2b_smoke_buf3+6], 2001
        jne     .fail

        ; ========== vector 5: consecutive writes (EDI chaining) ==========
        ; Two back-to-back calls must advance EDI by 16 total from start.
        mov     edi, fs_t2b_smoke_buf0
        mov     eax, 0
        call    fsTime2bdfe
        mov     eax, 86400
        call    fsTime2bdfe
        cmp     edi, fs_t2b_smoke_buf0 + 16
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf0+4], 1      ; first: day 1
        jne     .fail
        cmp     byte [fs_t2b_smoke_buf0+8+4], 2    ; second: day 2
        jne     .fail

        mov     [rust_fs_time2bdfe_smoke_result], 0x54324246 ; 'T2BF'
        pop     ebp edi esi ebx
        ret

.fail:
        mov     eax, 0xDEAD0C54        ; Cut T fail marker
        mov     ebx, 0x54324246        ; 'T2BF'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fs_time2bdfe(secs, out); ret 8
align 16
rust_fs_time2bdfe:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_fs_time2bdfe.bin'
