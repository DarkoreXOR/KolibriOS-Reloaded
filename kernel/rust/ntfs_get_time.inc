;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfsGetTime — Rust embed + ABI smoke (Cut BT)
;;
;; REG-003: isolated iglobal scratch only; no live NTFS/FS structures.
;; Vectors 1–2: direct `rust_ntfs_get_time_pack` deterministic pack.
;; Vector 3: public `ntfsGetTime` trampoline (fsGetTime + pack).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ntfs_get_time_smoke_result dd 0
endg

align 4
ntfs_get_time_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; vector 1: direct Rust, kos=0 → bias FILETIME
        mov     eax, 0
        stdcall rust_ntfs_get_time_pack, eax
        cmp     eax, 3365781504
        jne     .fail
        cmp     edx, 29389701
        jne     .fail

        ; vector 2: direct Rust, kos=1 → bias + 10_000_000
        mov     eax, 1
        stdcall rust_ntfs_get_time_pack, eax
        mov     ecx, 3365781504
        mov     ebx, 29389701
        add     ecx, 10000000
        adc     ebx, 0
        cmp     eax, ecx
        jne     .fail
        cmp     edx, ebx
        jne     .fail

        ; vector 3: fsGetTime kos → direct Rust vs FASM pack oracle (single CMOS read)
        call    fsGetTime
        push    eax
        stdcall rust_ntfs_get_time_pack, eax
        mov     esi, eax
        mov     edi, edx
        pop     eax
        mov     edx, 10000000
        mul     edx
        add     eax, 3365781504
        adc     edx, 29389701
        cmp     eax, esi
        jne     .fail
        cmp     edx, edi
        jne     .fail

        ; vector 4: public ntfsGetTime trampoline (fsGetTime + Rust pack; CMOS-backed)
        call    ntfsGetTime
        or      eax, edx
        jz      .fail

        mov     [rust_ntfs_get_time_smoke_result], 'NTGT'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C4C
        mov     ebx, 'NTGT'
        mov     ecx, 'FAIL'
        jmp     $

align 16
rust_ntfs_get_time_pack:
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_get_time_pack.bin'
