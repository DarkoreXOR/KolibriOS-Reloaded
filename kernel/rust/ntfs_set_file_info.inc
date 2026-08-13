;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfs_SetFileInfo — Rust embed + ABI smoke (Cut CW)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ntfs_set_file_info).
;;
;; Critical ABI: rust_ntfs_set_file_info(ctx) ret 4; never add esp after the
;; call for the stdcall arg (REG-009). Snapshot ctx dwords before stdcall
;; (REG-010). Production trampoline: lock + find_lfn + fragment + INDX
;; fixup BEFORE Rust. Marker 'NSFI' / fail DEAD0C57.
;;
;; Smoke fixtures live on the stack (REG-012 memory pack). Idempotent
;; second write is covered by host tests, not this boot smoke.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ntfs_set_file_info_smoke_result dd 0
nsfi_smoke_seq dd 0
nsfi_smoke_calc_count dd 0
nsfi_smoke_last_ebx dd 0
nsfi_smoke_last_edx dd 0
nsfi_smoke_last_ebp dd 0
endg

align 4
rust_ntfs_set_file_info:
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_set_file_info.bin'

nsfi_smoke_calc:
        inc     dword [nsfi_smoke_calc_count]
        cmp     dword [nsfi_smoke_calc_count], 1
        je      .ctime
        cmp     dword [nsfi_smoke_calc_count], 2
        je      .atime
        mov     eax, 0xAEB6F900
        mov     edx, 0x01D4830B
        ret
.ctime:
        mov     eax, 0x22222222
        mov     edx, 0x11111111
        ret
.atime:
        mov     eax, 0x667E1B80
        mov     edx, 0x01CD4C70
        ret

nsfi_smoke_write:
        or      dword [nsfi_smoke_seq], 1
        mov     [nsfi_smoke_last_ebx], ebx
        mov     [nsfi_smoke_last_edx], edx
        mov     [nsfi_smoke_last_ebp], ebp
        xor     eax, eax
        ret

nsfi_smoke_done:
        or      dword [nsfi_smoke_seq], 2
        xor     eax, eax
        ret

ntfs_set_file_info_rust_smoke_test:
if ~ USE_RUST_NTFS_SET_FILE_INFO
        ret
end if
        push    ebx
        push    esi
        push    edi
        push    ebp
        cld
        ; ctx32 + entry80 + record16 + bdfe32 + f70 32 + ntfs16 = 208
        sub     esp, 208
        lea     edi, [esp+32]
        mov     ecx, (208-32)/4
        xor     eax, eax
        rep stosd
        mov     dword [esp+32+0x48], 0x10000020
        mov     dword [esp+32+0x28], 0xAAAAAAAA
        mov     dword [esp+32+0x2C], 0xBBBBBBBB
        mov     dword [esp+32+0x40], 57
        lea     eax, [esp+128]
        mov     [esp+160+16], eax
        mov     dword [esp+128], 0x21
        lea     ecx, [esp+160]
        lea     ebx, [esp+192]
        lea     eax, [esp+32]
        lea     edx, [esp+112]
        mov     [esp+0], ecx
        mov     [esp+4], ebx
        mov     [esp+8], eax
        mov     [esp+12], edx
        mov     dword [esp+16], 0x55AA0001
        mov     dword [esp+20], nsfi_smoke_calc
        mov     dword [esp+24], nsfi_smoke_write
        mov     dword [esp+28], nsfi_smoke_done
        and     dword [nsfi_smoke_seq], 0
        and     dword [nsfi_smoke_calc_count], 0
        mov     ebx, 0xC20000D1
        mov     esi, 0xC20000D2
        mov     ebp, 0xC20000D3
        lea     eax, [esp]
        stdcall rust_ntfs_set_file_info, eax
        cmp     ebx, 0xC20000D1
        jne     .fail
        cmp     esi, 0xC20000D2
        jne     .fail
        cmp     ebp, 0xC20000D3
        jne     .fail
        test    eax, eax
        jnz     .fail
        cmp     dword [nsfi_smoke_seq], 1+2
        jne     .fail
        cmp     dword [nsfi_smoke_calc_count], 3
        jne     .fail
        lea     eax, [esp+112]
        cmp     [nsfi_smoke_last_ebx], eax
        jne     .fail
        cmp     dword [nsfi_smoke_last_edx], 0x55AA0001
        jne     .fail
        lea     eax, [esp+192]
        cmp     [nsfi_smoke_last_ebp], eax
        jne     .fail
        cmp     dword [esp+32+0x48], 0x10000021
        jne     .fail
        cmp     dword [esp+32+0x18], 0x22222222
        jne     .fail
        cmp     dword [esp+32+0x1C], 0x11111111
        jne     .fail
        cmp     dword [esp+32+0x30], 0x667E1B80
        jne     .fail
        cmp     dword [esp+32+0x34], 0x01CD4C70
        jne     .fail
        cmp     dword [esp+32+0x20], 0xAEB6F900
        jne     .fail
        cmp     dword [esp+32+0x24], 0x01D4830B
        jne     .fail
        cmp     dword [esp+32+0x28], 0xAAAAAAAA
        jne     .fail
        cmp     dword [esp+32+0x2C], 0xBBBBBBBB
        jne     .fail
        cmp     dword [esp+32+0x40], 57
        jne     .fail
        mov     [rust_ntfs_set_file_info_smoke_result], 'NSFI'
        add     esp, 208
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     [rust_ntfs_set_file_info_smoke_result], 0xDEAD0C57
@@:
        jmp     @b
