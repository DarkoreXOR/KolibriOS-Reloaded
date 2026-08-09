;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs._.extent_unpack — Rust embed + ABI/EBP smoke (Cut R)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_extent_unpack).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-xfs-extent-unpack.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_extent_unpack.bin
;;
;; Requires USE_RUST_XFS_EXTENT_UNPACK=1 so public xfs._.extent_unpack
;; hits: omit-FP trampoline → rust_xfs_extent_unpack (not the FASM body).
;;
;; Layout note (verified against kernel/fs/xfs.inc):
;;   xfs_bmbt_irec: startoff(8) startblock(8) blockcount(4) state(4) = 24
;;   Smoke uses sizeof.XFS + XFS.extent.* (fs_lfn/xfs included above).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust XFS extent unpack path ran.
rust_xfs_extent_unpack_smoke_result dd 0
; Packed BE test records (byte 0 = MSB). Computed via bit layout:
;   bit127=state, bits126:73=startoff(54), bits72:21=startblock(52),
;   bits20:0=blockcount(21).
xfs_ext_smoke_rec_state1:
        ; state=1, startoff=0xABC, startblock=0xDEF, blockcount=0x123
        db 0x80, 0x00, 0x00, 0x00, 0x00, 0x15, 0x78, 0x00
        db 0x00, 0x00, 0x00, 0x01, 0xBD, 0xE0, 0x01, 0x23
xfs_ext_smoke_rec_zero:
        db 0,0,0,0, 0,0,0,0, 0,0,0,0, 0,0,0,0
xfs_ext_smoke_rec_max:
        db 0xFF,0xFF,0xFF,0xFF, 0xFF,0xFF,0xFF,0xFF
        db 0xFF,0xFF,0xFF,0xFF, 0xFF,0xFF,0xFF,0xFF
endg

; Called once from high_code after Cut Q smoke.
; Exercises: EBP→fake XFS, stdcall stack arg, extent field writes,
; EAX/EBX/ECX/EDX/EBP preservation, retn 4 stack cleanup.
; On mismatch: distinctive hang (does not reach desktop).
align 4
xfs_extent_unpack_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, sizeof.XFS + 64

        ; ========== vector 1: state=1, non-trivial fields ==========
        lea     edi, [esp+32]           ; fake XFS (pad below for safety)
        push    edi
        mov     ecx, sizeof.XFS
        mov     al, 0xA5
        rep stosb
        pop     edi

        mov     eax, 0xA1000001
        mov     ebx, 0xB1000002
        mov     ecx, 0xC1000003
        mov     edx, 0xD1000004
        mov     esi, 0x51000005
        push    ebp                     ; save smoke frame EBP
        mov     ebp, edi                ; EBP → fake XFS (critical ABI)
        stdcall xfs._.extent_unpack, xfs_ext_smoke_rec_state1
        cmp     ebp, edi                ; EBP must be unchanged XFS ptr
        jne     .fail_restore
        mov     edi, ebp                ; keep XFS in edi
        pop     ebp                     ; restore smoke frame
        cmp     eax, 0xA1000001
        jne     .fail
        cmp     ebx, 0xB1000002
        jne     .fail
        cmp     ecx, 0xC1000003
        jne     .fail
        cmp     edx, 0xD1000004
        jne     .fail
        cmp     esi, 0x51000005
        jne     .fail
        cmp     dword [edi+XFS.extent.br_state], 1
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startoff.lo], 0xABC
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startoff.hi], 0
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startblock.lo], 0xDEF
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startblock.hi], 0
        jne     .fail
        cmp     dword [edi+XFS.extent.br_blockcount], 0x123
        jne     .fail
        cmp     byte [edi+XFS.extent+sizeof.xfs_bmbt_irec], 0xA5
        jne     .fail

        ; ========== vector 2: all-zero boundary ==========
        lea     edi, [esp+32]
        push    edi
        mov     ecx, sizeof.XFS
        mov     al, 0xA5
        rep stosb
        pop     edi
        mov     eax, 0xA2000001
        mov     ebx, 0xB2000002
        mov     ecx, 0xC2000003
        mov     edx, 0xD2000004
        push    ebp
        mov     ebp, edi
        stdcall xfs._.extent_unpack, xfs_ext_smoke_rec_zero
        cmp     ebp, edi
        jne     .fail_restore
        mov     edi, ebp
        pop     ebp
        cmp     eax, 0xA2000001
        jne     .fail
        cmp     ebx, 0xB2000002
        jne     .fail
        cmp     ecx, 0xC2000003
        jne     .fail
        cmp     edx, 0xD2000004
        jne     .fail
        cmp     dword [edi+XFS.extent.br_state], 0
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startoff.lo], 0
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startoff.hi], 0
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startblock.lo], 0
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startblock.hi], 0
        jne     .fail
        cmp     dword [edi+XFS.extent.br_blockcount], 0
        jne     .fail

        ; ========== vector 3: all-FF max fields ==========
        lea     edi, [esp+32]
        push    edi
        mov     ecx, sizeof.XFS
        mov     al, 0xA5
        rep stosb
        pop     edi
        mov     eax, 0xA3000001
        mov     ebx, 0xB3000002
        mov     ecx, 0xC3000003
        mov     edx, 0xD3000004
        push    ebp
        mov     ebp, edi
        stdcall xfs._.extent_unpack, xfs_ext_smoke_rec_max
        cmp     ebp, edi
        jne     .fail_restore
        mov     edi, ebp
        pop     ebp
        cmp     eax, 0xA3000001
        jne     .fail
        cmp     ebx, 0xB3000002
        jne     .fail
        cmp     ecx, 0xC3000003
        jne     .fail
        cmp     edx, 0xD3000004
        jne     .fail
        cmp     dword [edi+XFS.extent.br_state], 1
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startoff.lo], 0xFFFFFFFF
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startoff.hi], 0x003FFFFF
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startblock.lo], 0xFFFFFFFF
        jne     .fail
        cmp     dword [edi+XFS.extent.br_startblock.hi], 0x000FFFFF
        jne     .fail
        cmp     dword [edi+XFS.extent.br_blockcount], 0x001FFFFF
        jne     .fail
        mov     [rust_xfs_extent_unpack_smoke_result], 0x52534658 ; 'XFSR'
        mov     esp, ebp
        pop     ebp
        ret

.fail_restore:
        ; EBP still points at fake XFS; recover smoke frame from stack.
        add     esp, 4                  ; drop saved frame ptr without using it
.fail:
        mov     eax, 0xDEAD0C52         ; Cut R fail marker
        mov     ebx, 0x52534658         ; 'XFSR'
        mov     ecx, 0x4641494C         ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_xfs_extent_unpack(extent_data, extent_out); ret 8
align 16
rust_xfs_extent_unpack:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_extent_unpack.bin'
