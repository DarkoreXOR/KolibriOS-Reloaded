;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs._.blkrel2sectabs — Rust embed + ABI smoke (Cut AW)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_blkrel2sectabs).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_blkrel2sectabs.bin
;;
;; Requires USE_RUST_XFS_BLKREL2SECTABS=1 so public xfs._.blkrel2sectabs
;; hits the trampoline (not the FASM body).
;;
;; Critical ABI: register call; in EDX:EAX=AG-rel block, EBP→XFS;
;; out EDX:EAX=absolute sector; preserves ESI (uses) + EBX (live buffer);
;; clobbers ECX like legacy. ret 0.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust blkrel2sectabs path ran.
rust_xfs_blkrel2sectabs_smoke_result dd 0
endg

; Called once from high_code after Cut AP smoke.
; Exercises: EBP→fake XFS, public trampoline + direct rust_*;
; EBX/ESI/EDI/EBP canaries; EDX:EAX sector out.
; On mismatch: distinctive hang (does not reach desktop).
align 4
xfs_blkrel2sectabs_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, sizeof.XFS + 64

        ; ========== vector 1: AG0 block 0x123, agblklog=10, sectpblog=3 ==========
        lea     edi, [esp+32]           ; fake XFS
        push    edi
        mov     ecx, sizeof.XFS
        mov     al, 0xA5
        rep stosb
        pop     edi

        mov     dword [edi+XFS.agblklog], 10
        mov     dword [edi+XFS.agblocks], 1024
        mov     dword [edi+XFS.agblockmask.lo], 0x3FF
        mov     dword [edi+XFS.agblockmask.hi], 0
        mov     dword [edi+XFS.sectpblog], 3

        mov     ebx, 0xB1000002
        mov     esi, 0x51000005
        push    ebp                     ; save smoke frame EBP
        mov     ebp, edi                ; EBP → fake XFS
        mov     eax, 0x123
        xor     edx, edx
        call    xfs._.blkrel2sectabs
        cmp     ebp, edi                ; EBP must be unchanged XFS ptr
        jne     .fail_restore
        mov     edi, ebp
        pop     ebp
        ; expected: 0x123 << 3 = 0x918
        cmp     eax, 0x918
        jne     .fail
        test    edx, edx
        jnz     .fail
        cmp     ebx, 0xB1000002
        jne     .fail
        cmp     esi, 0x51000005
        jne     .fail

        ; ========== vector 2: AG1 block 5 (0x405), sectpblog=3 ==========
        lea     edi, [esp+32]
        mov     dword [edi+XFS.agblklog], 10
        mov     dword [edi+XFS.agblocks], 1024
        mov     dword [edi+XFS.agblockmask.lo], 0x3FF
        mov     dword [edi+XFS.agblockmask.hi], 0
        mov     dword [edi+XFS.sectpblog], 3

        mov     ebx, 0xB2000002
        mov     esi, 0x52000005
        push    ebp
        mov     ebp, edi
        mov     eax, 0x405
        xor     edx, edx
        call    xfs._.blkrel2sectabs
        cmp     ebp, edi
        jne     .fail_restore
        ; expected: (1*1024+5)<<3 = 1029<<3 = 8232 = 0x2028
        cmp     eax, 0x2028
        jne     .fail_restore
        test    edx, edx
        jnz     .fail_restore
        cmp     ebx, 0xB2000002
        jne     .fail_restore
        cmp     esi, 0x52000005
        jne     .fail_restore
        pop     ebp

        ; ========== vector 3: direct rust_* (ret 32) ==========
        sub     esp, 4
        mov     ebx, esp                ; out_hi slot
        stdcall rust_xfs_blkrel2sectabs, 0x123, 0, 10, 1024, 0x3FF, 0, 3, ebx
        cmp     eax, 0x918
        jne     .fail_rust
        cmp     dword [ebx], 0
        jne     .fail_rust
        add     esp, 4

        ; ========== vector 4: sectpblog=0 ==========
        lea     edi, [esp+32]
        mov     dword [edi+XFS.sectpblog], 0
        mov     ebx, 0xB3000003
        mov     esi, 0x53000007
        push    ebp
        mov     ebp, edi
        mov     eax, 0x405
        xor     edx, edx
        call    xfs._.blkrel2sectabs
        cmp     ebp, edi
        jne     .fail_restore
        cmp     eax, 0x405              ; 1*1024+5, no sector shift
        jne     .fail_restore
        test    edx, edx
        jnz     .fail_restore
        cmp     ebx, 0xB3000003
        jne     .fail_restore
        cmp     esi, 0x53000007
        jne     .fail_restore
        pop     ebp

        ; ========== vector 5: EDI canary across public call ==========
        lea     edi, [esp+32]
        mov     dword [edi+XFS.sectpblog], 3
        mov     eax, 0xE5000008
        push    eax                     ; saved EDI canary value
        push    ebp
        mov     ebp, edi
        mov     edi, eax                ; EDI canary
        mov     ebx, 0xB5000005
        mov     esi, 0x55000009
        mov     eax, 0x123
        xor     edx, edx
        call    xfs._.blkrel2sectabs
        cmp     edi, 0xE5000008
        jne     .fail_restore
        cmp     ebx, 0xB5000005
        jne     .fail_restore
        cmp     esi, 0x55000009
        jne     .fail_restore
        cmp     eax, 0x918
        jne     .fail_restore
        pop     ebp
        pop     eax                     ; drop canary copy

        mov     [rust_xfs_blkrel2sectabs_smoke_result], 0x424C3253 ; 'BL2S'
        mov     esp, ebp
        pop     ebp
        ret

.fail_restore:
        ; May have an extra saved smoke-frame EBP on stack; hang anyway.
        pop     ebp
.fail:
.fail_rust:
        mov     eax, 0xDEAD0C57        ; Cut AW fail marker
        mov     ebx, 0x41574641        ; 'AWFA'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_xfs_blkrel2sectabs(...) -> EAX (sector_lo); *out_hi=sector_hi; ret 32
align 16
rust_xfs_blkrel2sectabs:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_blkrel2sectabs.bin'
