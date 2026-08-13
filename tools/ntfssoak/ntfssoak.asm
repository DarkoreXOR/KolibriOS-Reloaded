;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfssoak — MENUET01 disposable NTFS SetFileInfo → GetFileInfo oracle
;;
;; Host/test tooling only. Does not change production FS semantics.
;;
;; NTFS SetFileInfo mutates the parent ``$I30`` index entry (not file MFT
;; ``$STANDARD_INFORMATION``). Preserve attrs+ctime from GetFileInfo in the
;; 32-byte SetFileInfo buffer (unlike EXT which zeroes the buffer).
;;
;; Evidence log NSFI.LOG lives on the NTFS CoW under test (/hd0/1/NSFI.LOG).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

use32
        org     0
        db      'MENUET01'
        dd      1, start, i_end, mem, stacktop, 0, 0

; Report layout v1 (binary LE) — keep in sync with host parser.
;   0   magic 'NSFI'
;   4   version = 1
;   8   flags
;  12   get1_eax, set_eax, get2_eax, get3_eax
;  28   edge_idem_eax, edge_second_eax, edge_miss_eax, pad
;  44   initial atime/mtime BDFE (16)
;  60   requested atime/mtime BDFE (16)
;  76   immediate atime/mtime BDFE (16)
;  92   final atime/mtime BDFE (16)
; 108   second-request atime/mtime BDFE (16)
; 124   second-readback atime/mtime BDFE (16)
; 140   run_id
; 144   create_log_eax
; 148   write_log_eax
; 152   target_mft_hint (=0 — host resolves by name)
; 156   path_tag 'ROOT'
; 160   ticks
; 164   end

FLAG_PASS           = 1
FLAG_GET1_OK        = 2
FLAG_SET_OK         = 4
FLAG_GET2_OK        = 8
FLAG_GET3_OK        = 16
FLAG_ATIME_MATCH    = 32
FLAG_MTIME_MATCH    = 64
FLAG_EDGE_IDEM_OK   = 128
FLAG_EDGE_SECOND_OK = 256
FLAG_EDGE_MISS_OK   = 512
FLAG_LOG_OK         = 1024

run_id_placeholder:
        dd      0xDEADBEEF
control_placeholder:
        dd      0xC0A1C0A1          ; patched to 1 = GetFileInfo-only control

start:
        mov     eax, [run_id_placeholder]
        mov     [report_run_id], eax
        mov     eax, 3
        int     0x40
        mov     [report_ticks], eax

        call    board_start

        ; --- primary GetFileInfo ---
        mov     eax, 70
        mov     ebx, file70_get1
        int     0x40
        mov     [report_get1_eax], eax
        test    eax, eax
        jnz     .fail_early
        or      dword [report_flags], FLAG_GET1_OK
        mov     esi, bdfe1
        add     esi, 16
        mov     edi, report_init_times
        mov     ecx, 16/4
        rep movsd

        cmp     dword [control_placeholder], 1
        je      .control_getonly

        ; --- SetFileInfo: copy GetFileInfo attrs+ctime, patch atime/mtime ---
        mov     esi, bdfe1
        mov     edi, setbuf
        mov     ecx, 32/4
        rep movsd
        mov     esi, req_atime
        mov     edi, setbuf+16
        mov     ecx, 8/4
        rep movsd
        mov     esi, req_mtime
        mov     edi, setbuf+24
        mov     ecx, 8/4
        rep movsd

        mov     eax, 70
        mov     ebx, file70_set
        int     0x40
        mov     [report_set_eax], eax
        test    eax, eax
        jnz     .fail_early
        or      dword [report_flags], FLAG_SET_OK
        call    board_set_ok

        ; --- immediate GetFileInfo ---
        mov     eax, 70
        mov     ebx, file70_get2
        int     0x40
        mov     [report_get2_eax], eax
        test    eax, eax
        jnz     .fail_early
        or      dword [report_flags], FLAG_GET2_OK
        mov     esi, bdfe2
        add     esi, 16
        mov     edi, report_imm_times
        mov     ecx, 16/4
        rep movsd

        mov     esi, req_atime
        mov     edi, report_imm_times
        mov     ecx, 8
        call    memcmp_ecx
        jnz     @f
        or      dword [report_flags], FLAG_ATIME_MATCH
@@:
        mov     esi, req_mtime
        mov     edi, report_imm_times+8
        mov     ecx, 8
        call    memcmp_ecx
        jnz     @f
        or      dword [report_flags], FLAG_MTIME_MATCH
@@:

        ; --- delay then final GetFileInfo ---
        mov     eax, 5
        mov     ebx, 20
        int     0x40
        mov     eax, 70
        mov     ebx, file70_get3
        int     0x40
        mov     [report_get3_eax], eax
        test    eax, eax
        jnz     .fail_early
        or      dword [report_flags], FLAG_GET3_OK
        mov     esi, bdfe3
        add     esi, 16
        mov     edi, report_fin_times
        mov     ecx, 16/4
        rep movsd

        mov     esi, req_atime
        mov     edi, report_fin_times
        mov     ecx, 8
        call    memcmp_ecx
        jnz     .fail_early
        mov     esi, req_mtime
        mov     edi, report_fin_times+8
        mov     ecx, 8
        call    memcmp_ecx
        jnz     .fail_early
        mov     eax, [report_flags]
        and     eax, FLAG_ATIME_MATCH or FLAG_MTIME_MATCH
        cmp     eax, FLAG_ATIME_MATCH or FLAG_MTIME_MATCH
        jne     .fail_early

        ; --- edge: idempotent SetFileInfo ---
        mov     eax, 70
        mov     ebx, file70_set
        int     0x40
        mov     [report_edge_idem_eax], eax
        test    eax, eax
        jnz     @f
        or      dword [report_flags], FLAG_EDGE_IDEM_OK
@@:

        ; --- edge: second distinct times ---
        mov     esi, bdfe2
        mov     edi, setbuf
        mov     ecx, 32/4
        rep movsd
        mov     esi, req2_atime
        mov     edi, setbuf+16
        mov     ecx, 8/4
        rep movsd
        mov     esi, req2_mtime
        mov     edi, setbuf+24
        mov     ecx, 8/4
        rep movsd
        mov     esi, req2_atime
        mov     edi, report_req2_times
        mov     ecx, 16/4
        rep movsd

        mov     eax, 70
        mov     ebx, file70_set
        int     0x40
        mov     [report_edge_second_eax], eax
        test    eax, eax
        jnz     @f
        mov     eax, 70
        mov     ebx, file70_get2
        int     0x40
        test    eax, eax
        jnz     @f
        mov     esi, bdfe2
        add     esi, 16
        mov     edi, report_sec_times
        mov     ecx, 16/4
        rep movsd
        mov     esi, req2_atime
        mov     edi, report_sec_times
        mov     ecx, 16
        call    memcmp_ecx
        jnz     @f
        or      dword [report_flags], FLAG_EDGE_SECOND_OK
@@:

        ; --- edge: nonexistent file ---
        mov     eax, 70
        mov     ebx, file70_set_miss
        int     0x40
        mov     [report_edge_miss_eax], eax
        test    eax, eax
        jz      @f
        or      dword [report_flags], FLAG_EDGE_MISS_OK
@@:

        ; Restore primary times for host oracle.
        mov     esi, bdfe1
        mov     edi, setbuf
        mov     ecx, 32/4
        rep movsd
        mov     esi, req_atime
        mov     edi, setbuf+16
        mov     ecx, 8/4
        rep movsd
        mov     esi, req_mtime
        mov     edi, setbuf+24
        mov     ecx, 8/4
        rep movsd
        mov     eax, 70
        mov     ebx, file70_set
        int     0x40
        test    eax, eax
        jnz     .fail_early

        or      dword [report_flags], FLAG_PASS
        call    board_pass
        call    board_imm_hex
        call    board_fin_hex
        call    write_report
        test    eax, eax
        jnz     .log_fail
        call    board_log_ok
        jmp     .finish

.log_fail:
        call    board_log_fail
        jmp     .finish

.control_getonly:
        ; Boot + GetFileInfo + log; no SetFileInfo (ROOT.TXT metadata must stay put).
        call    board_ctrl
        mov     eax, 70
        mov     ebx, file70_get2
        int     0x40
        mov     [report_get2_eax], eax
        test    eax, eax
        jnz     .fail_early
        or      dword [report_flags], FLAG_GET2_OK
        mov     esi, bdfe2
        add     esi, 16
        mov     edi, report_imm_times
        mov     ecx, 16/4
        rep movsd
        mov     eax, 5
        mov     ebx, 20
        int     0x40
        mov     eax, 70
        mov     ebx, file70_get3
        int     0x40
        mov     [report_get3_eax], eax
        test    eax, eax
        jnz     .fail_early
        or      dword [report_flags], FLAG_GET3_OK
        mov     esi, bdfe3
        add     esi, 16
        mov     edi, report_fin_times
        mov     ecx, 16/4
        rep movsd
        mov     esi, report_init_times
        mov     edi, report_imm_times
        mov     ecx, 16
        call    memcmp_ecx
        jnz     .fail_early
        mov     esi, report_init_times
        mov     edi, report_fin_times
        mov     ecx, 16
        call    memcmp_ecx
        jnz     .fail_early
        or      dword [report_flags], FLAG_PASS
        call    board_pass
        call    board_imm_hex
        call    board_fin_hex
        call    write_report
        test    eax, eax
        jnz     .log_fail
        call    board_log_ok
        jmp     .finish

.fail_early:
        call    board_fail
        call    write_report
.finish:
        mov     eax, 70
        mov     ebx, file70_launcher
        int     0x40
        or      eax, -1
        int     0x40

memcmp_ecx:
        push    eax
@@:
        mov     al, [esi]
        cmp     al, [edi]
        jnz     .ne
        inc     esi
        inc     edi
        dec     ecx
        jnz     @b
        pop     eax
        xor     eax, eax
        ret
.ne:
        pop     eax
        or      eax, 1
        ret

board_start:
        mov     esi, msg_start
        jmp     board_puts
board_set_ok:
        mov     esi, msg_set
        jmp     board_puts
board_ctrl:
        mov     esi, msg_ctrl
        jmp     board_puts
board_pass:
        mov     esi, msg_pass
        jmp     board_puts
board_fail:
        mov     esi, msg_fail
        jmp     board_puts
board_log_ok:
        mov     esi, msg_log_ok
        jmp     board_puts
board_log_fail:
        mov     esi, msg_log_fail
board_puts:
@@:
        lodsb
        test    al, al
        jz      .done
        mov     ebx, 1
        movzx   ecx, al
        mov     eax, 63
        int     0x40
        jmp     @b
.done:
        ret

board_imm_hex:
        mov     esi, msg_imm
        call    board_puts
        mov     esi, report_imm_times
        mov     ecx, 16
        call    board_hex_bytes
        mov     esi, msg_crlf
        jmp     board_puts

board_fin_hex:
        mov     esi, msg_fin
        call    board_puts
        mov     esi, report_fin_times
        mov     ecx, 16
        call    board_hex_bytes
        mov     esi, msg_crlf
        jmp     board_puts

board_hex_bytes:
@@:
        push    ecx
        mov     al, [esi]
        shr     al, 4
        call    .nibble
        mov     al, [esi]
        and     al, 0x0f
        call    .nibble
        inc     esi
        pop     ecx
        dec     ecx
        jnz     @b
        ret
.nibble:
        cmp     al, 10
        jb      .dig
        add     al, 'a' - 10
        jmp     .out
.dig:
        add     al, '0'
.out:
        push    esi
        mov     ebx, 1
        movzx   ecx, al
        mov     eax, 63
        int     0x40
        pop     esi
        ret

write_report:
        mov     dword [report_magic], 'NSFI'
        mov     dword [report_version], 1
        mov     dword [report_target_mft], 0
        mov     dword [report_path_tag], 'ROOT'
        mov     esi, req_atime
        mov     edi, report_req_times
        mov     ecx, 16/4
        rep movsd

        mov     eax, 70
        mov     ebx, file70_create_log
        int     0x40
        mov     [report_create_eax], eax
        mov     eax, 70
        mov     ebx, file70_write_log
        int     0x40
        mov     [report_write_eax], eax
        test    eax, eax
        jnz     .fail
        mov     eax, 70
        mov     ebx, file70_write_log
        int     0x40
        mov     [report_write_eax], eax
        test    eax, eax
        jnz     .fail
        or      dword [report_flags], FLAG_LOG_OK
        mov     eax, 70
        mov     ebx, file70_write_log
        int     0x40
        mov     [report_write_eax], eax
        test    eax, eax
        jnz     .fail
        xor     eax, eax
        ret
.fail:
        mov     eax, 1
        ret

align 4
req_atime:
        db 11, 22, 14, 0, 4, 7
        dw 2012
req_mtime:
        db 30, 5, 9, 0, 23, 11
        dw 2018
req2_atime:
        db 1, 2, 3, 0, 5, 6
        dw 2020
req2_mtime:
        db 10, 20, 12, 0, 15, 8
        dw 2021

align 4
file70_get1:
        dd 5, 0, 0, 0, bdfe1
        db '/hd0/1/ROOT.TXT',0
align 4
file70_get2:
        dd 5, 0, 0, 0, bdfe2
        db '/hd0/1/ROOT.TXT',0
align 4
file70_get3:
        dd 5, 0, 0, 0, bdfe3
        db '/hd0/1/ROOT.TXT',0
align 4
file70_set:
        dd 6, 0, 0, 0, setbuf
        db '/hd0/1/ROOT.TXT',0
align 4
file70_set_miss:
        dd 6, 0, 0, 0, setbuf
        db '/hd0/1/NO_SUCH_NTFSOAK.TXT',0
align 4
file70_create_log:
        dd 2, 0, 0, 0, 0
        db '/hd0/1/NSFI.LOG',0
align 4
file70_write_log:
        dd 3, 0, 0, report_size, report
        db '/hd0/1/NSFI.LOG',0
align 4
file70_launcher:
        dd 7, 0, 0, 0, 0
        db '/sys/LAUNCHER',0

msg_start    db 'NTFSOAK START',13,10,0
msg_ctrl     db 'NTFSOAK CTRL',13,10,0
msg_set      db 'NTFSOAK SET',13,10,0
msg_pass     db 'NTFSOAK PASS',13,10,0
msg_fail     db 'NTFSOAK FAIL',13,10,0
msg_log_ok   db 'NTFSOAK LOG',13,10,0
msg_log_fail db 'NTFSOAK LOGFAIL',13,10,0
msg_imm      db 'NTFSOAK IMM ',0
msg_fin      db 'NTFSOAK FIN ',0
msg_crlf     db 13,10,0

align 4
bdfe1:  rb 40
bdfe2:  rb 40
bdfe3:  rb 40
setbuf: rb 32

align 4
report:
report_magic            dd ?
report_version          dd ?
report_flags            dd 0
report_get1_eax         dd ?
report_set_eax          dd ?
report_get2_eax         dd ?
report_get3_eax         dd ?
report_edge_idem_eax    dd ?
report_edge_second_eax  dd ?
report_edge_miss_eax    dd ?
                        dd 0
report_init_times:      rb 16
report_req_times:       rb 16
report_imm_times:       rb 16
report_fin_times:       rb 16
report_req2_times:      rb 16
report_sec_times:       rb 16
report_run_id           dd ?
report_create_eax       dd ?
report_write_eax        dd ?
report_target_mft       dd ?
report_path_tag         dd ?
report_ticks            dd ?
report_end:
report_size = report_end - report

i_end:
        rb 4096
stacktop = $
mem = $ + 0x10000
