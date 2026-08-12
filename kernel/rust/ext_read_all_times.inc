;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ext_read_all_times — Rust embed + ABI smoke (Cut BR)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ext_read_all_times).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ext_read_all_times.bin
;;
;; Requires USE_RUST_EXT_READ_ALL_TIMES=1 so public ext_read_all_times hits
;; the trampoline (not the FASM body).
;;
;; Legacy ABI: register call; in ESI -> inode, EDI -> 3× BDFE out;
;; clobbers EAX/EDX/ECX/EDI (+24); ESI/EBX/EBP preserved by callers.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ext_read_all_times_smoke_result dd 0

ext_rat_smoke_inode0:
        rb 160
ext_rat_smoke_inode1:
        rb 160
ext_rat_smoke_out0:
        rb 24
ext_rat_smoke_out1:
        rb 24
ext_rat_smoke_out2:
        rb 24
endg

align 4
ext_read_all_times_rust_smoke_test:
        push    ebp
        push    ebx
        push    ecx
        push    esi
        push    edi

        ; ========== vector 1: no extra fields (extraISize=4) ==========
        lea     edi, [ext_rat_smoke_inode0]
        mov     ecx, 160
        xor     eax, eax
        rep stosb
        mov     word [ext_rat_smoke_inode0+128], 4
        mov     dword [ext_rat_smoke_inode0+12], 978307200   ; cTime
        mov     dword [ext_rat_smoke_inode0+8], 978307201    ; aTime
        mov     dword [ext_rat_smoke_inode0+16], 978307202   ; mTime

        lea     edi, [ext_rat_smoke_out0]
        mov     ecx, 24
        mov     al, 0xA5
        rep stosb
        mov     esi, ext_rat_smoke_inode0
        lea     edi, [ext_rat_smoke_out0]
        call    ext_read_all_times
        lea     edi, [ext_rat_smoke_out0+24]
        cmp     edi, ext_rat_smoke_out0 + 24
        jne     .fail
        cmp     byte [ext_rat_smoke_out0], 0
        jne     .fail
        cmp     byte [ext_rat_smoke_out0+4], 1
        jne     .fail
        cmp     word [ext_rat_smoke_out0+6], 2001
        jne     .fail
        cmp     byte [ext_rat_smoke_out0+8], 1
        jne     .fail
        cmp     byte [ext_rat_smoke_out0+16], 2
        jne     .fail

        ; ========== vector 2: fast path (extraISize=24) ==========
        lea     edi, [ext_rat_smoke_inode1]
        mov     ecx, 160
        xor     eax, eax
        rep stosb
        mov     word [ext_rat_smoke_inode1+128], 24
        mov     dword [ext_rat_smoke_inode1+144], 978307203   ; crTime
        xor     eax, eax
        mov     [ext_rat_smoke_inode1+148], eax               ; crTimeExtra=0
        mov     dword [ext_rat_smoke_inode1+8], 978307201     ; aTime
        mov     [ext_rat_smoke_inode1+140], eax               ; aTimeExtra=0
        mov     dword [ext_rat_smoke_inode1+16], 978307202     ; mTime
        mov     [ext_rat_smoke_inode1+136], eax               ; mTimeExtra=0

        lea     edi, [ext_rat_smoke_out1]
        mov     ecx, 24
        mov     al, 0xA5
        rep stosb
        mov     esi, ext_rat_smoke_inode1
        mov     ebx, 0xB1000001
        mov     ecx, 0xC1000001
        mov     ebp, 0xD1000001
        lea     edi, [ext_rat_smoke_out1]
        call    ext_read_all_times
        cmp     esi, ext_rat_smoke_inode1
        jne     .fail
        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     ebp, 0xD1000001
        jne     .fail
        cmp     byte [ext_rat_smoke_out1], 3
        jne     .fail
        cmp     byte [ext_rat_smoke_out1+8], 1
        jne     .fail
        cmp     byte [ext_rat_smoke_out1+16], 2
        jne     .fail

        ; ========== vector 3: direct rust_* partial path (extraISize=8, cTimeExtra only) ==========
        lea     edi, [ext_rat_smoke_inode0]
        mov     word [ext_rat_smoke_inode0+128], 8
        mov     dword [ext_rat_smoke_inode0+12], 978307200
        mov     dword [ext_rat_smoke_inode0+132], 0
        mov     dword [ext_rat_smoke_inode0+8], 978307201
        mov     dword [ext_rat_smoke_inode0+16], 978307202

        lea     edi, [ext_rat_smoke_out2]
        mov     ecx, 24
        mov     al, 0xA5
        rep stosb
        stdcall rust_ext_read_all_times, ext_rat_smoke_inode0, ext_rat_smoke_out2
        cmp     byte [ext_rat_smoke_out2], 0
        jne     .fail
        cmp     byte [ext_rat_smoke_out2+8], 1
        jne     .fail
        cmp     byte [ext_rat_smoke_out2+16], 2
        jne     .fail

        mov     [rust_ext_read_all_times_smoke_result], 'EXBR'
        pop     edi
        pop     esi
        pop     ecx
        pop     ebx
        pop     ebp
        ret

.fail:
        mov     eax, 0xDEAD0420
        mov     ebx, 'EXBR'
        mov     ecx, 'FAIL'
        jmp     $

align 16
rust_ext_read_all_times:
        file '../../rust_kernel/kolibri_utils/out/rust_ext_read_all_times.bin'
