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
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ext_set_file_info_smoke_result dd 0
esfi_smoke_seq dd 0
esfi_smoke_inode rb 160
esfi_smoke_bdfe rb 32
esfi_smoke_f70 rb 32
esfi_smoke_disk dd 0
esfi_smoke_extfs rb 16
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
esfi_smoke_calc_count dd 0

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

; Fill ctx at EDI from current smoke globals. ECX = find, EDX = write.
align 4
esfi_smoke_fill_ctx:
        mov     dword [edi+0], esfi_smoke_f70
        mov     dword [edi+4], esfi_smoke_extfs
        mov     dword [edi+8], esfi_smoke_path
        mov     dword [edi+12], esfi_smoke_inode
        mov     eax, esfi_smoke_disk
        mov     [edi+16], eax
        mov     [edi+20], ecx                 ; find_inode
        mov     dword [edi+24], esfi_smoke_calc
        mov     [edi+28], edx                 ; write_inode
        mov     dword [edi+32], esfi_smoke_sb
        mov     dword [edi+36], esfi_smoke_sync
        mov     dword [edi+40], esfi_smoke_unlock
        ret

iglobal
esfi_smoke_path db 'ROOT.TXT', 0
endg

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

        ; Clear inode + bdfe + f70
        mov     edi, esfi_smoke_inode
        mov     ecx, 160/4
        xor     eax, eax
        rep stosd
        mov     edi, esfi_smoke_bdfe
        mov     ecx, 32/4
        rep stosd
        mov     edi, esfi_smoke_f70
        mov     ecx, 32/4
        rep stosd
        mov     dword [esfi_smoke_f70+16], esfi_smoke_bdfe

        ; Fixture atime 2012-07-04 14:22:11 / mtime 2018-11-23 09:05:30
        mov     byte [esfi_smoke_bdfe+16], 11
        mov     byte [esfi_smoke_bdfe+17], 22
        mov     byte [esfi_smoke_bdfe+18], 14
        mov     byte [esfi_smoke_bdfe+20], 4
        mov     byte [esfi_smoke_bdfe+21], 7
        mov     word [esfi_smoke_bdfe+22], 2012
        mov     byte [esfi_smoke_bdfe+24], 30
        mov     byte [esfi_smoke_bdfe+25], 5
        mov     byte [esfi_smoke_bdfe+26], 9
        mov     byte [esfi_smoke_bdfe+28], 23
        mov     byte [esfi_smoke_bdfe+29], 11
        mov     word [esfi_smoke_bdfe+30], 2018

        sub     esp, 44

        ; Vector 0: success path
        and     dword [esfi_smoke_seq], 0
        and     dword [esfi_smoke_calc_count], 0
        lea     edi, [esp]
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
        cmp     dword [esfi_smoke_inode+8], 1341411731
        jne     .fail
        cmp     dword [esfi_smoke_inode+16], 1542963930
        jne     .fail

        ; Vector 1: missing path — unlock only
        and     dword [esfi_smoke_seq], 0
        lea     edi, [esp]
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
        mov     dword [esfi_smoke_inode+32], EXT4_IMMUTABLE_FL
        lea     edi, [esp]
        mov     ecx, esfi_smoke_find_ok
        mov     edx, esfi_smoke_write_ok
        call    esfi_smoke_fill_ctx
        lea     eax, [esp]
        stdcall rust_ext_set_file_info, eax
        cmp     eax, ERROR_ACCESS_DENIED
        jne     .fail
        cmp     dword [esfi_smoke_seq], 16
        jne     .fail
        and     dword [esfi_smoke_inode+32], 0

        ; Vector 3: writeInode fail — unlock, no SB/sync
        and     dword [esfi_smoke_seq], 0
        and     dword [esfi_smoke_calc_count], 0
        lea     edi, [esp]
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
        add     esp, 44
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     [rust_ext_set_file_info_smoke_result], 0xDEAD0C56
@@:
        jmp     @b
