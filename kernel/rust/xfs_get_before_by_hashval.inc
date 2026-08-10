;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs._.get_before_by_hashval — Rust embed + ABI smoke (Cut AM)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_get_before_by_hashval).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_get_before_by_hashval.bin
;;
;; Requires USE_RUST_XFS_GET_BEFORE_BY_HASHVAL=1 so public
;; xfs._.get_before_by_hashval hits the trampoline (not the FASM body).
;;
;; Critical ABI: stdcall _base = node+sizeof(intnode); trampoline uses
;; entries=_base-8; EAX=before/error; ZF found/miss; retn 12.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust XFS node-hash path ran.
rust_xfs_get_before_by_hashval_smoke_result dd 0
; v4 node: header 16 bytes + 3 entries (hash/before BE)
; entries: (1,0x100), (5,0x500), (9,0x900)
xfs_before_smoke_v4:
        rb 16
        db 0x00,0x00,0x00,0x01,  0x00,0x00,0x01,0x00
        db 0x00,0x00,0x00,0x05,  0x00,0x00,0x05,0x00
        db 0x00,0x00,0x00,0x09,  0x00,0x00,0x09,0x00
; v5 node: header 64 bytes + 1 entry (0x12345678 → 0xAABBCCDD)
xfs_before_smoke_v5:
        rb 64
        db 0x12,0x34,0x56,0x78,  0xAA,0xBB,0xCC,0xDD
endg

; Called once from high_code after Cut AL smoke.
; Exercises: stdcall _base=node+sizeof / count / hash; EAX+ZF out;
; EBX/ESI/EDI/EDX preserve; retn 12.
; On mismatch: distinctive hang (does not reach desktop).
align 4
xfs_get_before_by_hashval_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, sizeof.XFS + 64

        lea     edi, [esp+32]           ; fake XFS
        push    edi
        mov     ecx, sizeof.XFS
        xor     al, al
        rep stosb
        pop     edi
        mov     dword [edi+XFS.version], 4

        ; --- hit mid: hash=5 → before=0x500, ZF=1 ---
        ; (count=0 miss hangs in legacy FASM; covered only by Rust host tests)
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v4
        lea     eax, [ebx+sizeof.xfs_da_intnode]
        mov     esi, 0x53000012
        mov     edx, 0xD3000003
        stdcall xfs._.get_before_by_hashval, eax, 3, 5
        jz      @f
        jmp     .fail_restore
@@:
        cmp     eax, 0x500
        jne     .fail_restore
        cmp     ebx, xfs_before_smoke_v4
        jne     .fail_restore
        cmp     esi, 0x53000012
        jne     .fail_restore
        cmp     edx, 0xD3000003
        jne     .fail_restore
        cmp     ebp, edi
        jne     .fail_restore
        pop     ebp

        ; --- first-match jae: hash=3 → entry hash 5 / before 0x500 ---
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v4
        lea     eax, [ebx+sizeof.xfs_da_intnode]
        stdcall xfs._.get_before_by_hashval, eax, 3, 3
        jz      @f
        jmp     .fail_restore
@@:
        cmp     eax, 0x500
        jne     .fail_restore
        pop     ebp

        ; --- hit first ---
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v4
        lea     eax, [ebx+sizeof.xfs_da_intnode]
        stdcall xfs._.get_before_by_hashval, eax, 3, 1
        jz      @f
        jmp     .fail_restore
@@:
        cmp     eax, 0x100
        jne     .fail_restore
        pop     ebp

        ; --- hit last ---
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v4
        lea     eax, [ebx+sizeof.xfs_da_intnode]
        stdcall xfs._.get_before_by_hashval, eax, 3, 9
        jz      @f
        jmp     .fail_restore
@@:
        cmp     eax, 0x900
        jne     .fail_restore
        pop     ebp

        ; --- miss above ---
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v4
        lea     eax, [ebx+sizeof.xfs_da_intnode]
        stdcall xfs._.get_before_by_hashval, eax, 3, 100
        jnz     @f
        jmp     .fail_restore
@@:
        cmp     eax, 5
        jne     .fail_restore
        pop     ebp

        ; --- v5 single hit (layout offset 64) ---
        mov     dword [edi+XFS.version], 5
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v5
        lea     eax, [ebx+sizeof.xfs_da3_intnode]
        stdcall xfs._.get_before_by_hashval, eax, 1, 0x12345678
        jz      @f
        jmp     .fail_restore
@@:
        cmp     eax, 0xAABBCCDD
        jne     .fail_restore
        pop     ebp

        ; --- v5 miss ---
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v5
        lea     eax, [ebx+sizeof.xfs_da3_intnode]
        stdcall xfs._.get_before_by_hashval, eax, 1, 0x12345679
        jnz     @f
        jmp     .fail_restore
@@:
        cmp     eax, 5
        jne     .fail_restore
        pop     ebp

        ; --- adversarial: EBX/ESI/EDI/EDX live across miss ---
        mov     dword [edi+XFS.version], 4
        push    ebp
        mov     ebp, edi
        mov     ebx, xfs_before_smoke_v4
        lea     eax, [ebx+sizeof.xfs_da_intnode]
        mov     esi, 0x53AD53AD
        mov     edx, 0xD3ADD3AD
        stdcall xfs._.get_before_by_hashval, eax, 3, 0x7FFFFFFF
        jnz     @f
        jmp     .fail_restore
@@:
        cmp     eax, 5
        jne     .fail_restore
        cmp     ebx, xfs_before_smoke_v4
        jne     .fail_restore
        cmp     esi, 0x53AD53AD
        jne     .fail_restore
        cmp     edx, 0xD3ADD3AD
        jne     .fail_restore
        pop     ebp

        mov     [rust_xfs_get_before_by_hashval_smoke_result], 'XFBF'
        mov     esp, ebp
        pop     ebp
        ret

.fail_restore:
        ; EBP may be fake XFS; hang with distinctive regs.
        mov     eax, 0xDEAD0CB1
        mov     ebx, 'XFBF'
        mov     ecx, 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_xfs_get_before_by_hashval(entries, count, hash) -> EDX:EAX; ret 12
align 16
rust_xfs_get_before_by_hashval:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_get_before_by_hashval.bin'
