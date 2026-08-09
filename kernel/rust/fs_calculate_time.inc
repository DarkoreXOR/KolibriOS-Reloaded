;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fsCalculateTime — Rust embed + runtime smoke (Cut G)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fs_calculate_time).
;; See docs/migration/cut-g-plan.md / cut-g-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-fs-calculate-time.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fs_calculate_time.bin
;;
;; kernel/fs/fs_common.inc (USE_RUST_FS_CALCULATE_TIME=1) trampolines
;; fsCalculateTime → rust_fs_calculate_time.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fsCalculateTime path ran.
rust_fs_calculate_time_smoke_result dd 0
; Static BDFE blocks for smoke vectors (sec,min,hour,pad,day,month,year_lo,year_hi).
fs_calc_smoke_epoch:
        db 0, 0, 0, 0, 1, 1
        dw 2001
fs_calc_smoke_leap:
        db 0, 0, 0, 0, 29, 2
        dw 2004
fs_calc_smoke_2010:
        db 0, 0, 12, 0, 4, 7
        dw 2010
fs_calc_smoke_eod:
        db 59, 59, 23, 0, 1, 1
        dw 2001
fs_calc_smoke_clamp:
        db 0, 0, 0, 0, 1, 1
        dw 1999
endg

; Called once from high_code after Cut F smoke.
; Exercises: FASM callers → fsCalculateTime trampoline → Rust body.
; Vectors: epoch, leap Feb 29, known 2010 date, end-of-day, year clamp;
; ESI/EDI/EBP preservation (required: ESI for caller add esi,8).
; On mismatch: distinctive hang (does not reach desktop).
align 4
fs_calculate_time_rust_smoke_test:
        mov     ebx, 0xB7000001
        mov     esi, 0x57000002
        mov     edi, 0xD7000003
        mov     ebp, 0xE7000004

        ; 2001-01-01 00:00:00 → 0
        mov     esi, fs_calc_smoke_epoch
        call    fsCalculateTime
        test    eax, eax
        jnz     .fail

        ; 2004-02-29 00:00:00 → 1154 * 86400 = 0x05F1_9900? 
        ; days = 3*365 + 0 + 31 + 29 - 1 = 1154
        ; 1154 * 86400 = 99705600 = 0x05F19900
        mov     esi, fs_calc_smoke_leap
        call    fsCalculateTime
        cmp     eax, 99705600
        jne     .fail

        ; 2010-07-04 12:00:00
        ; month_sum Jan..Jun non-leap = 181; days = 9*365+2+181+4-1 = 3471+185 = 3656?
        ; 9*365=3285; +2=3287; +181=3468; +4-1=3471
        ; 3471*86400 + 12*3600 = 299894400 + 43200 = 299937600 = 0x11E0_4A00
        mov     esi, fs_calc_smoke_2010
        call    fsCalculateTime
        cmp     eax, 299937600
        jne     .fail

        ; 2001-01-01 23:59:59 → 86399
        mov     esi, fs_calc_smoke_eod
        call    fsCalculateTime
        cmp     eax, 86399
        jne     .fail

        ; year 1999 clamps to same as 2001-01-01 → 0
        mov     esi, fs_calc_smoke_clamp
        call    fsCalculateTime
        test    eax, eax
        jnz     .fail

        ; Register preservation (ESI/EDI/EBP); EBX may be clobbered by FASM body
        ; but Rust stdcall preserves EBX — either is acceptable for ESI/EDI/EBP.
        mov     ebx, 0xB7000001
        mov     esi, fs_calc_smoke_epoch
        mov     edi, 0xD7000003
        mov     ebp, 0xE7000004
        call    fsCalculateTime
        test    eax, eax
        jnz     .fail
        cmp     esi, fs_calc_smoke_epoch
        jne     .fail
        cmp     edi, 0xD7000003
        jne     .fail
        cmp     ebp, 0xE7000004
        jne     .fail

        mov     [rust_fs_calculate_time_smoke_result], 0x46534354 ; 'FSCT'
        ret
.fail:
        mov     eax, 0xDEAD0C47        ; Cut G fail marker
        mov     ebx, 0x46534354        ; 'FSCT'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fs_calculate_time(block) -> EAX (seconds); ret 4
align 16
rust_fs_calculate_time:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_fs_calculate_time.bin'
