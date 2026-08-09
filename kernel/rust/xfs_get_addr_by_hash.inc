;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs._.get_addr_by_hash — Rust embed + ABI smoke (Cut W)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_get_addr_by_hash).
;; See docs/migration/cut-w-plan.md / cut-w-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-xfs-get-addr-by-hash.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_get_addr_by_hash.bin
;;
;; kernel/fs/xfs.asm (USE_RUST_XFS_GET_ADDR_BY_HASH=1) trampolines
;; xfs._.get_addr_by_hash → rust_xfs_get_addr_by_hash.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust XFS hash lookup path ran.
rust_xfs_get_addr_by_hash_smoke_result dd 0
; Sorted BE leaf table: hash=1/5/9 → addr=0x100/0x500/0x900
xfs_hash_smoke_table:
        db 0x00,0x00,0x00,0x01,  0x00,0x00,0x01,0x00
        db 0x00,0x00,0x00,0x05,  0x00,0x00,0x05,0x00
        db 0x00,0x00,0x00,0x09,  0x00,0x00,0x09,0x00
; Single-entry table
xfs_hash_smoke_one:
        db 0x12,0x34,0x56,0x78,  0xAA,0xBB,0xCC,0xDD
endg

; Called once from high_code after Cut V smoke.
; Exercises: EAX hash in; stdcall base/len; EAX+ZF out; EBX/ESI preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
xfs_get_addr_by_hash_rust_smoke_test:
        push    ebp
        mov     ebp, esp

        ; --- empty table → miss EAX=5, ZF=0 ---
        mov     eax, 0x11111111
        mov     ebx, 0xB3000001
        mov     esi, 0x53000002
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 0
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 5
        jne     .fail
        cmp     ebx, 0xB3000001
        jne     .fail
        cmp     esi, 0x53000002
        jne     .fail

        ; --- hit mid: hash=5 → addr=0x500, ZF=1 ---
        mov     eax, 5
        mov     ebx, 0xB3000011
        mov     esi, 0x53000012
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 3
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0x500
        jne     .fail
        cmp     ebx, 0xB3000011
        jne     .fail
        cmp     esi, 0x53000012
        jne     .fail

        ; --- hit first ---
        mov     eax, 1
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 3
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0x100
        jne     .fail

        ; --- hit last ---
        mov     eax, 9
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 3
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0x900
        jne     .fail

        ; --- miss gap ---
        mov     eax, 7
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 3
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 5
        jne     .fail

        ; --- miss below ---
        mov     eax, 0
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 3
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 5
        jne     .fail

        ; --- miss above ---
        mov     eax, 100
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 3
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 5
        jne     .fail

        ; --- single hit (BE hash/addr) ---
        mov     eax, 0x12345678
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_one, 1
        jz      @f
        jmp     .fail
@@:
        cmp     eax, 0xAABBCCDD
        jne     .fail

        ; --- single miss ---
        mov     eax, 0x12345679
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_one, 1
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 5
        jne     .fail

        ; --- adversarial: EBX/ESI live across miss (legacy `uses`) ---
        mov     eax, 0xDEADBEEF
        mov     ebx, 0xB3ADB3AD
        mov     esi, 0x53AD53AD
        stdcall xfs._.get_addr_by_hash, xfs_hash_smoke_table, 0
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, 5
        jne     .fail
        cmp     ebx, 0xB3ADB3AD
        jne     .fail
        cmp     esi, 0x53AD53AD
        jne     .fail

        mov     [rust_xfs_get_addr_by_hash_smoke_result], 0x58465357 ; 'XFSW'
        mov     esp, ebp
        pop     ebp
        ret
.fail:
        mov     eax, 0xDEAD0C57                ; Cut W fail marker
        mov     ebx, 0x58465357                ; 'XFSW'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_xfs_get_addr_by_hash(hash, base, len) -> EDX:EAX; ret 12
align 16
rust_xfs_get_addr_by_hash:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_get_addr_by_hash.bin'
