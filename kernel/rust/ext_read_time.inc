;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ext_read_time — Rust embed + ABI smoke (Cut AL)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ext_unix_to_secs).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ext_unix_to_secs.bin
;;
;; Requires USE_RUST_EXT_READ_TIME=1 so public ext_read_time hits the
;; trampoline (not the FASM body).
;;
;; Layout (BDFE datetime block via fsTime2bdfe):
;;   +0 sec, +1 min, +2 hour (word; pad at +3), +4 day, +5 month, +6 year u16
;;
;; Trampoline: EAX/EDX → rust_ext_unix_to_secs → call fsTime2bdfe (EDI+=8);
;; ECX preserved (ext_read_all_times reuses ECX across calls).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust EXT read-time path ran.
rust_ext_read_time_smoke_result dd 0
; Scratch BDFE buffers (8 bytes each); filled by public ABI.
ext_rt_smoke_buf0:
        rb 8
ext_rt_smoke_buf1:
        rb 8
ext_rt_smoke_buf2:
        rb 8
ext_rt_smoke_buf3:
        rb 8
endg

; Called once from high_code after Cut AK smoke.
; Exercises: EAX=unix, EDX=extra, EDI→block out, EDI+=8, ECX/ESI preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ext_read_time_rust_smoke_test:
        push    ebx esi edi ebp

        ; ========== vector 1: Unix KOS epoch → 2001-01-01; EDI+=8; ECX live ==========
        mov     esi, 0xA1000021
        mov     ebp, 0xA1000022
        mov     ecx, 0xA1000023
        mov     eax, 978307200          ; UNIXTIME_TO_KOS_OFFSET
        xor     edx, edx
        mov     edi, ext_rt_smoke_buf0
        call    ext_read_time
        cmp     edi, ext_rt_smoke_buf0 + 8
        jne     .fail
        cmp     esi, 0xA1000021
        jne     .fail
        cmp     ebp, 0xA1000022
        jne     .fail
        cmp     ecx, 0xA1000023
        jne     .fail
        cmp     byte [ext_rt_smoke_buf0], 0        ; sec
        jne     .fail
        cmp     byte [ext_rt_smoke_buf0+1], 0      ; min
        jne     .fail
        cmp     word [ext_rt_smoke_buf0+2], 0      ; hour word (pad 0)
        jne     .fail
        cmp     byte [ext_rt_smoke_buf0+4], 1      ; day
        jne     .fail
        cmp     byte [ext_rt_smoke_buf0+5], 1      ; month
        jne     .fail
        cmp     word [ext_rt_smoke_buf0+6], 2001
        jne     .fail

        ; ========== vector 2: +1 second ==========
        mov     eax, 978307201
        xor     edx, edx
        mov     edi, ext_rt_smoke_buf1
        call    ext_read_time
        cmp     edi, ext_rt_smoke_buf1 + 8
        jne     .fail
        cmp     byte [ext_rt_smoke_buf1], 1
        jne     .fail
        cmp     word [ext_rt_smoke_buf1+6], 2001
        jne     .fail

        ; ========== vector 3: end of day 23:59:59 ==========
        mov     eax, 978307200 + 86399
        xor     edx, edx
        mov     edi, ext_rt_smoke_buf2
        call    ext_read_time
        cmp     edi, ext_rt_smoke_buf2 + 8
        jne     .fail
        cmp     byte [ext_rt_smoke_buf2], 59
        jne     .fail
        cmp     byte [ext_rt_smoke_buf2+1], 59
        jne     .fail
        cmp     word [ext_rt_smoke_buf2+2], 23
        jne     .fail
        cmp     word [ext_rt_smoke_buf2+6], 2001
        jne     .fail

        ; ========== vector 4: pre-epoch / signed-negative clamp ==========
        xor     eax, eax
        xor     edx, edx
        mov     edi, ext_rt_smoke_buf3
        call    ext_read_time
        cmp     edi, ext_rt_smoke_buf3 + 8
        jne     .fail
        cmp     word [ext_rt_smoke_buf3+6], 2001
        jne     .fail

        mov     [rust_ext_read_time_smoke_result], 'EXTR'
        pop     ebp edi esi ebx
        ret

.fail:
        mov     eax, 0xDEAD0C4C
        mov     ebx, 'EXTR'
        mov     ecx, 'FAIL'
        jmp     $

; Reloc-free Rust body (secs only; calendar via fsTime2bdfe).
align 16
rust_ext_unix_to_secs:
        file '../../rust_kernel/kolibri_utils/out/rust_ext_unix_to_secs.bin'
