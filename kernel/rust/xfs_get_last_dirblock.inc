;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs._.get_last_dirblock — Rust embed + ABI smoke (Cut BO)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_get_last_dirblock).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_get_last_dirblock.bin
;;
;; Requires USE_RUST_XFS_GET_LAST_DIRBLOCK=1 so public
;; xfs._.get_last_dirblock hits the trampoline (not the FASM body).
;;
;; Legacy ABI: register call; in EBX -> inode buffer, EBP -> XFS;
;; out EDX:EAX = last data dirblock; preserves EBX + ECX; ret 0.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_xfs_get_last_dirblock_smoke_result dd 0

xfs_last_dirblock_inode0:
        rb 80
xfs_last_dirblock_inode1:
        rb 96
xfs_last_dirblock_xfs:
        rb sizeof.XFS
endg

align 4
xfs_get_last_dirblock_rust_smoke_test:
        push    ebp
        push    ebx
        push    ecx
        push    esi
        push    edi

        ; inode0: nextents=1, core_size=64, last extent startoff=0, blockcount=4096.
        lea     edi, [xfs_last_dirblock_inode0]
        mov     ecx, 80
        xor     eax, eax
        rep stosb
        mov     dword [xfs_last_dirblock_inode0+0], 0x01000000 ; be32 1
        mov     dword [xfs_last_dirblock_inode0+76], 0x00100000 ; be32 4096

        ; inode1: nextents=2, use second extent: startoff=5000, blockcount=1024.
        lea     edi, [xfs_last_dirblock_inode1]
        mov     ecx, 96
        xor     eax, eax
        rep stosb
        mov     dword [xfs_last_dirblock_inode1+0], 0x02000000 ; be32 2
        ; second record at core_size 64 + 16
        mov     dword [xfs_last_dirblock_inode1+84], 0x00102700 ; be32 word1
        mov     dword [xfs_last_dirblock_inode1+92], 0x00040000 ; be32 blockcount 1024

        ; Direct rust_*: high-dword carry sample (startoff=0xFFFFFFFF, blockcount=2).
        lea     edi, [xfs_last_dirblock_inode0]
        mov     dword [xfs_last_dirblock_inode0+0], 0x01000000
        mov     dword [xfs_last_dirblock_inode0+64], 0xFF010000 ; be32 word0 = 0x000001FF
        mov     dword [xfs_last_dirblock_inode0+68], 0x00FEFFFF ; be32 word1 = 0xFFFFFE00
        mov     dword [xfs_last_dirblock_inode0+76], 0x02000000 ; be32 blockcount 2
        sub     esp, 4
        mov     esi, esp
        stdcall rust_xfs_get_last_dirblock, xfs_last_dirblock_inode0, 0, 64, 0, esi
        cmp     eax, 0
        jne     .fail
        cmp     dword [esi], 1
        jne     .fail
        add     esp, 4

        ; Reset inode0 before public-trampoline vectors (direct rust_* mutates it).
        lea     edi, [xfs_last_dirblock_inode0]
        mov     ecx, 80
        xor     eax, eax
        rep stosb
        mov     dword [xfs_last_dirblock_inode0+0], 0x01000000 ; be32 1
        mov     dword [xfs_last_dirblock_inode0+76], 0x00100000 ; be32 4096

        ; Public trampoline: result zero, EBX/ECX/ESI/EDI/EBP preserved.
        lea     edi, [xfs_last_dirblock_xfs]
        push    edi
        mov     ecx, sizeof.XFS
        mov     al, 0xA5
        rep stosb
        pop     edi
        mov     dword [edi+XFS.nextents_offset], 0
        mov     dword [edi+XFS.inode_core_size], 64
        mov     dword [edi+XFS.dirblklog], 12

        mov     ebx, xfs_last_dirblock_inode0
        mov     ecx, 0xC1000001
        mov     esi, 0x51000005
        mov     edi, 0xD7000007
        mov     eax, 0xE2000002
        push    eax
        mov     ebp, xfs_last_dirblock_xfs
        call    xfs._.get_last_dirblock
        cmp     eax, 0
        jne     .fail
        test    edx, edx
        jnz     .fail
        cmp     ebx, xfs_last_dirblock_inode0
        jne     .fail
        cmp     ecx, 0xC1000001
        jne     .fail
        cmp     esi, 0x51000005
        jne     .fail
        cmp     edi, 0xD7000007
        jne     .fail
        pop     eax
        cmp     ebp, xfs_last_dirblock_xfs
        jne     .fail

        ; Public trampoline: second extent selected, result 5001.
        mov     dword [xfs_last_dirblock_xfs+XFS.nextents_offset], 0
        mov     dword [xfs_last_dirblock_xfs+XFS.inode_core_size], 64
        mov     dword [xfs_last_dirblock_xfs+XFS.dirblklog], 9
        mov     ebx, xfs_last_dirblock_inode1
        mov     ecx, 0xC2000002
        mov     esi, 0x52000006
        mov     edi, 0xD8000008
        mov     ebp, xfs_last_dirblock_xfs
        call    xfs._.get_last_dirblock
        cmp     eax, 5001
        jne     .fail
        test    edx, edx
        jnz     .fail
        cmp     ebx, xfs_last_dirblock_inode1
        jne     .fail
        cmp     ecx, 0xC2000002
        jne     .fail
        cmp     esi, 0x52000006
        jne     .fail
        cmp     edi, 0xD8000008
        jne     .fail
        cmp     ebp, xfs_last_dirblock_xfs
        jne     .fail

        mov     [rust_xfs_get_last_dirblock_smoke_result], 'XFBO'
        pop     edi
        pop     esi
        pop     ecx
        pop     ebx
        pop     ebp
        ret

.fail:
        mov     eax, 0xDEAD0CB0
        mov     ebx, 'XFBO'
        mov     ecx, 'FAIL'
        jmp     $

align 16
rust_xfs_get_last_dirblock:
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_get_last_dirblock.bin'
