;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ext_write_time — Rust embed + ABI smoke (Cut BS)
;;
;; REG-003: isolated iglobal scratch; no live FS/inode structures.
;; Vectors 1–2: direct `rust_ext_write_time_pack` deterministic pack.
;; Vector 3: public `ext_write_time` trampoline (fsGetTime + pack).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ext_write_time_smoke_result dd 0
ext_wt_smoke_time:
        dd 0
ext_wt_smoke_extra:
        dd 0
endg

align 4
ext_write_time_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        mov     esi, 0xA10000B1
        mov     edi, 0xA10000B2
        mov     ebp, 0xA10000B3

        ; vector 1: direct Rust, kos=0, no extra (-1 sentinel)
        mov     dword [ext_wt_smoke_time], 0xDEADBEEF
        mov     eax, 0
        mov     ebx, ext_wt_smoke_time
        mov     ecx, -1
        stdcall rust_ext_write_time_pack, eax, ebx, ecx
        cmp     dword [ext_wt_smoke_time], 978307200
        jne     .fail

        ; vector 2: direct Rust, kos=1, with extra field
        mov     dword [ext_wt_smoke_extra], 0xCAFEBABE
        mov     eax, 1
        mov     ebx, ext_wt_smoke_time
        mov     ecx, ext_wt_smoke_extra
        stdcall rust_ext_write_time_pack, eax, ebx, ecx
        cmp     dword [ext_wt_smoke_time], 978307201
        jne     .fail
        cmp     dword [ext_wt_smoke_extra], 0
        jne     .fail

        ; vector 3: public ext_write_time trampoline + register canaries
        mov     dword [ext_wt_smoke_time], 0
        lea     eax, [ext_wt_smoke_time]
        mov     edx, -1
        stdcall ext_write_time, eax, edx
        cmp     esi, 0xA10000B1
        jne     .fail
        cmp     edi, 0xA10000B2
        jne     .fail
        cmp     ebp, 0xA10000B3
        jne     .fail
        cmp     dword [ext_wt_smoke_time], 0
        je      .fail

        mov     [rust_ext_write_time_smoke_result], 'EXBS'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C4C
        mov     ebx, 'EXBS'
        mov     ecx, 'FAIL'
        jmp     $

align 16
rust_ext_write_time_pack:
        file '../../rust_kernel/kolibri_utils/out/rust_ext_write_time_pack.bin'
