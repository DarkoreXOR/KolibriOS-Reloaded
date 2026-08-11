;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; createMcbEntry — Rust embed + ABI smoke (Cut AX)
;;
;; Marker CMCE. Fail hang 0xDEAD0C58 / 'AXFA'.
;; Stack: FRS 0x200 + NTFS shadow 0x100 (fields through fileDataSize only).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ntfs_create_mcb_smoke_result dd 0
endg

align 4
ntfs_create_mcb_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 0x300

        mov     esi, esp                        ; FRS
        lea     edi, [esp+0x200]                ; NTFS shadow
        push    edi
        mov     edi, esi
        mov     ecx, 0x200
        mov     al, 0xA5
        rep stosb
        pop     edi
        push    edi
        mov     ecx, 0x100
        xor     al, al
        rep stosb
        pop     edi
        mov     [edi+NTFS.frs_buffer], esi

        ; ========== vector 1: direct rust_* no-extend ==========
        mov     dword [esi+recordRealSize], 0x100
        mov     dword [esi+recordAllocatedSize], 0x200
        mov     dword [esi+0x40+sizeWithHeader], 0x50
        sub     esp, 4
        lea     eax, [esi+0x80]
        mov     [esp], eax
        mov     ebx, esp
        lea     ecx, [esi+0x40]
        push    ebx
        push    esi
        push    ecx
        push    eax
        push    5
        push    2
        call    rust_ntfs_create_mcb_entry
        test    al, 1
        jnz     .fail
        mov     eax, [ebx]
        lea     edx, [esi+0x83]
        cmp     eax, edx
        jne     .fail
        cmp     byte [esi+0x80], 0x11
        jne     .fail
        cmp     byte [esi+0x81], 5
        jne     .fail
        cmp     byte [esi+0x82], 2
        jne     .fail
        cmp     byte [esi+0x83], 0
        jne     .fail
        add     esp, 4

        ; ========== vector 2: direct rust_* extend+slide ==========
        ; dest=0x8C → new_end=0x8F still fits; use 0x8E → new_end=0x91 > 0x90
        mov     dword [esi+recordRealSize], 0x100
        mov     dword [esi+recordAllocatedSize], 0x200
        mov     dword [esi+0x40+sizeWithHeader], 0x50
        sub     esp, 4
        lea     eax, [esi+0x8E]
        mov     [esp], eax
        mov     ebx, esp
        lea     ecx, [esi+0x40]
        push    ebx
        push    esi
        push    ecx
        push    eax
        push    5
        push    2
        call    rust_ntfs_create_mcb_entry
        test    al, 1
        jz      .fail
        cmp     dword [esi+recordRealSize], 0x108
        jne     .fail
        cmp     word [esi+0x40+sizeWithHeader], 0x58
        jne     .fail
        cmp     byte [esi+0x8E], 0x11
        jne     .fail
        mov     eax, [ebx]
        lea     edx, [esi+0x91]
        cmp     eax, edx
        jne     .fail
        add     esp, 4

        ; ========== vector 3: public trampoline no-extend ==========
        lea     esi, [ebp-0x300]
        lea     edi, [esi+0x200]
        mov     [edi+NTFS.frs_buffer], esi
        mov     dword [edi+NTFS.fileDataStart], 2
        mov     dword [edi+NTFS.fileDataSize], 5
        mov     dword [esi+recordRealSize], 0x100
        mov     dword [esi+recordAllocatedSize], 0x200
        mov     dword [esi+0x40+sizeWithHeader], 0x50
        mov     ebx, 0xB3000003
        push    ebp
        mov     ebp, edi
        mov     esi, [ebp+NTFS.frs_buffer]
        add     esi, 0x40
        mov     edi, [ebp+NTFS.frs_buffer]
        add     edi, 0x80
        call    createMcbEntry
        cmp     ebx, 0xB3000003
        jne     .fail_restore
        mov     eax, [ebp+NTFS.frs_buffer]
        cmp     byte [eax+0x80], 0x11
        jne     .fail_restore
        lea     edx, [eax+0x83]
        cmp     edi, edx
        jne     .fail_restore
        pop     ebp

        ; ========== vector 4: FRS-full early-out ==========
        lea     esi, [ebp-0x300]
        lea     edi, [esi+0x200]
        mov     [edi+NTFS.frs_buffer], esi
        mov     dword [esi+recordRealSize], 0x100
        mov     dword [esi+recordAllocatedSize], 0x100
        mov     dword [esi+0x40+sizeWithHeader], 0x50
        mov     dword [edi+NTFS.fileDataStart], 2
        mov     dword [edi+NTFS.fileDataSize], 5
        mov     ebx, 0xB4000004
        push    ebp
        mov     ebp, edi
        mov     esi, [ebp+NTFS.frs_buffer]
        add     esi, 0x40
        mov     edi, [ebp+NTFS.frs_buffer]
        add     edi, 0x8E
        push    edi
        call    createMcbEntry
        pop     edx
        cmp     edi, edx
        jne     .fail_restore
        mov     eax, [ebp+NTFS.frs_buffer]
        cmp     word [eax+0x40+sizeWithHeader], 0x58
        jne     .fail_restore
        cmp     dword [eax+recordRealSize], 0x100
        jne     .fail_restore
        cmp     ebx, 0xB4000004
        jne     .fail_restore
        pop     ebp

        mov     [rust_ntfs_create_mcb_smoke_result], 0x434D4345 ; 'CMCE'
        mov     esp, ebp
        pop     ebp
        ret

.fail_restore:
        pop     ebp
.fail:
        mov     eax, 0xDEAD0C58
        mov     ebx, 0x41584641                ; 'AXFA'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

align 16
rust_ntfs_create_mcb_entry:
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_create_mcb_entry.bin'
