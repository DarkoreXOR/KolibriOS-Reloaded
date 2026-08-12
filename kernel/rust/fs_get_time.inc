;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fsGetTime — Rust embed + ABI smoke (Cut BV)
;;
;; REG-003: mock CMOS reader only for vectors 1–2; vector 3 uses live RTC.
;; Vectors 1–2: direct `rust_fs_get_time` with synthetic CMOS table.
;; Vector 3: public `fsGetTime` trampoline (live CMOS + Cut G calendar).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_fs_get_time_smoke_result dd 0
endg

; Mock CMOS reader: reg 0/2/4/7/8/9 → 2026-06-15 12:45:30 decoded fields.
align 4
fs_get_time_smoke_read_cmos:
        mov     eax, [esp+4]
        movzx   ecx, al
        cmp     ecx, 0
        je      .sec
        cmp     ecx, 2
        je      .min
        cmp     ecx, 4
        je      .hour
        cmp     ecx, 7
        je      .day
        cmp     ecx, 8
        je      .month
        cmp     ecx, 9
        je      .year
        xor     eax, eax
        ret     4
.sec:   mov     eax, 30
        ret     4
.min:   mov     eax, 45
        ret     4
.hour:  mov     eax, 12
        ret     4
.day:   mov     eax, 15
        ret     4
.month: mov     eax, 6
        ret     4
.year:  mov     eax, 26
        ret     4

align 4
fs_get_time_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; vector 1: direct Rust with mock CMOS → 2026-06-15 12:45:30
        mov     eax, 0xB1000001
        mov     ebx, 0xB2000002
        mov     ecx, 0xB3000003
        mov     edx, 0xB4000004
        mov     esi, 0xB5000005
        mov     edi, 0xB6000006
        mov     ebp, 0xB7000007

        stdcall rust_fs_get_time, fs_get_time_smoke_read_cmos
        cmp     eax, 803220330
        jne     .fail

        cmp     ebx, 0xB2000002
        jne     .fail
        cmp     esi, 0xB5000005
        jne     .fail
        cmp     edi, 0xB6000006
        jne     .fail
        cmp     ebp, 0xB7000007
        jne     .fail

        ; vector 2: public fsGetTime trampoline (live CMOS-backed sanity)
        call    fsGetTime
        test    eax, eax
        js      .fail

        ; vector 3: ntfsGetTime composition still valid (BT path uses fsGetTime)
        call    ntfsGetTime
        or      eax, edx
        jz      .fail

        mov     [rust_fs_get_time_smoke_result], 'FSGT'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C4C
        mov     ebx, 'FSGT'
        mov     ecx, 'FAIL'
        jmp     $

align 16
rust_fs_get_time:
        file '../../rust_kernel/kolibri_utils/out/rust_fs_get_time.bin'
