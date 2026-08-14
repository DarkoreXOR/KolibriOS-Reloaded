;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ext_SetFileInfo — Rust embed + ABI smoke (Cut CV)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ext_set_file_info).
;;
;; Critical ABI: rust_ext_set_file_info(ctx) ret 4; never add esp after the
;; call for the stdcall arg (REG-009). Snapshot callbacks before stdcall
;; (REG-010). Production trampoline calls extfsWritingInit BEFORE Rust.
;; Marker 'ESFI' / fail DEAD0C56.
;;
;; Smoke fixtures live on the stack (REG-012 memory pack). Result/seq/
;; calc_count stay iglobal so callbacks and the boot marker remain stable.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ext_set_file_info_smoke_result dd 0
esfi_smoke_seq dd 0
esfi_smoke_calc_count dd 0
esfi_smoke_path db 'ROOT.TXT', 0
endg

align 4
rust_ext_set_file_info:
        file '../../rust_kernel/kolibri_utils/out/rust_ext_set_file_info.bin'

; ---- synthetic callbacks (register ABI) ----
align 4
esfi_smoke_find_ok:
        mov     esi, 12
        xor     eax, eax
        clc
        ret

align 4
esfi_smoke_find_miss:
        movi    eax, ERROR_FILE_NOT_FOUND
        stc
        ret

align 4
esfi_smoke_calc:
        ; Avoid nesting through Cut G trampoline in smoke (isolates CV ABI).
        ; Return KOS secs for oracle fixture: unix − UNIXTIME_TO_KOS_OFFSET.
        inc     dword [esfi_smoke_calc_count]
        cmp     dword [esfi_smoke_calc_count], 1
        jne     .mtime
        mov     eax, 1341411731 - 978307200
        ret
.mtime:
        mov     eax, 1542963930 - 978307200
        ret

align 4
esfi_smoke_write_ok:
        or      dword [esfi_smoke_seq], 1
        xor     eax, eax
        clc
        ret

align 4
esfi_smoke_write_fail:
        or      dword [esfi_smoke_seq], 2
        movi    eax, ERROR_DEVICE
        stc
        ret

align 4
esfi_smoke_sb:
        or      dword [esfi_smoke_seq], 4
        ret

align 4
esfi_smoke_sync:
        or      dword [esfi_smoke_seq], 8
        ret

align 4
esfi_smoke_unlock:
        or      dword [esfi_smoke_seq], 16
        ret

; Fill ctx at EDI from stack fixtures. ESI = inode base, ECX = find, EDX = write.
; Layout from ESI: inode160, bdfe32, f70 32, disk 4, extfs 16.
align 4
esfi_smoke_fill_ctx:
        lea     eax, [esi+192]
        mov     dword [edi+0], eax            ; f70
        lea     eax, [esi+228]
        mov     dword [edi+4], eax            ; extfs
        mov     dword [edi+8], esfi_smoke_path
        mov     dword [edi+12], esi           ; inode
        lea     eax, [esi+224]
        mov     [edi+16], eax                 ; disk
        mov     [edi+20], ecx                 ; find_inode
        mov     dword [edi+24], esfi_smoke_calc
        mov     [edi+28], edx                 ; write_inode
        mov     dword [edi+32], esfi_smoke_sb
        mov     dword [edi+36], esfi_smoke_sync
        mov     dword [edi+40], esfi_smoke_unlock
        ret

align 4
ext_set_file_info_rust_smoke_test:
if ~ USE_RUST_EXT_SET_FILE_INFO
        ret
end if
        push    ebx
        push    esi
        push    edi
        push    ebp
        cld

        ; ctx44 + inode160 + bdfe32 + f70 32 + disk4 + extfs16 = 288
        sub     esp, 288
        lea     edi, [esp+44]
        mov     ecx, (288-44)/4
        xor     eax, eax
        rep stosd
        lea     eax, [esp+44+160]
        mov     dword [esp+44+192+16], eax    ; f70.bdfe

        ; Fixture atime 2012-07-04 14:22:11 / mtime 2018-11-23 09:05:30
        mov     byte [esp+44+160+16], 11
        mov     byte [esp+44+160+17], 22
        mov     byte [esp+44+160+18], 14
        mov     byte [esp+44+160+20], 4
        mov     byte [esp+44+160+21], 7
        mov     word [esp+44+160+22], 2012
        mov     byte [esp+44+160+24], 30
        mov     byte [esp+44+160+25], 5
        mov     byte [esp+44+160+26], 9
        mov     byte [esp+44+160+28], 23
        mov     byte [esp+44+160+29], 11
        mov     word [esp+44+160+30], 2018

        ; Vector 0: success path
        and     dword [esfi_smoke_seq], 0
        and     dword [esfi_smoke_calc_count], 0
        lea     edi, [esp]
        lea     esi, [esp+44]
        mov     ecx, esfi_smoke_find_ok
        mov     edx, esfi_smoke_write_ok
        call    esfi_smoke_fill_ctx
        mov     ebx, 0xB10000C2
        mov     esi, 0xB10000C3
        mov     ebp, 0xB10000C4
        lea     eax, [esp]
        stdcall rust_ext_set_file_info, eax
        cmp     ebx, 0xB10000C2
        jne     .fail
        cmp     esi, 0xB10000C3
        jne     .fail
        cmp     ebp, 0xB10000C4
        jne     .fail
        test    eax, eax
        jnz     .fail
        cmp     dword [esfi_smoke_seq], 1+4+8+16
        jne     .fail
        cmp     dword [esp+44+8], 1341411731
        jne     .fail
        cmp     dword [esp+44+16], 1542963930
        jne     .fail

        ; Vector 1: missing path — unlock only
        and     dword [esfi_smoke_seq], 0
        lea     edi, [esp]
        lea     esi, [esp+44]
        mov     ecx, esfi_smoke_find_miss
        mov     edx, esfi_smoke_write_ok
        call    esfi_smoke_fill_ctx
        lea     eax, [esp]
        stdcall rust_ext_set_file_info, eax
        cmp     eax, ERROR_FILE_NOT_FOUND
        jne     .fail
        cmp     dword [esfi_smoke_seq], 16
        jne     .fail

        ; Vector 2: immutable — unlock, no SB/sync
        and     dword [esfi_smoke_seq], 0
        mov     dword [esp+44+32], EXT4_IMMUTABLE_FL
        lea     edi, [esp]
        lea     esi, [esp+44]
        mov     ecx, esfi_smoke_find_ok
        mov     edx, esfi_smoke_write_ok
        call    esfi_smoke_fill_ctx
        lea     eax, [esp]
        stdcall rust_ext_set_file_info, eax
        cmp     eax, ERROR_ACCESS_DENIED
        jne     .fail
        cmp     dword [esfi_smoke_seq], 16
        jne     .fail
        and     dword [esp+44+32], 0

        ; Vector 3: writeInode fail — unlock, no SB/sync
        and     dword [esfi_smoke_seq], 0
        and     dword [esfi_smoke_calc_count], 0
        lea     edi, [esp]
        lea     esi, [esp+44]
        mov     ecx, esfi_smoke_find_ok
        mov     edx, esfi_smoke_write_fail
        call    esfi_smoke_fill_ctx
        lea     eax, [esp]
        stdcall rust_ext_set_file_info, eax
        cmp     eax, ERROR_DEVICE
        jne     .fail
        cmp     dword [esfi_smoke_seq], 2+16
        jne     .fail

        mov     [rust_ext_set_file_info_smoke_result], 'ESFI'
        add     esp, 288
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     [rust_ext_set_file_info_smoke_result], 0xDEAD0C56
@@:
        jmp     @b
