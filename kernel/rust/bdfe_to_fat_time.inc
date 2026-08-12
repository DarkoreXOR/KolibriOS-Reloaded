;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; bdfe_to_fat_time — Rust embed + ABI smoke (Cut BY)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_bdfe_to_fat_time).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_bdfe_to_fat_time.bin
;;
;; Requires USE_RUST_BDFE_TO_FAT_TIME=1 so public bdfe_to_fat_time hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EAX BDFE time in → EAX FAT packed time out; trampoline
;; preserves ECX+EDX (REG-001); ret (not stdcall) at public symbol; Rust body
;; is stdcall ret 4.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust bdfe_to_fat_time path ran.
rust_bdfe_to_fat_time_smoke_result dd 0
endg

; Called once from high_code after Cut BX smoke.
; Exercises: public trampoline + direct rust_*; named vectors; ECX/EDX canaries;
; round-trip with fat_time_to_bdfe; max BDFE time.
; On mismatch: distinctive hang (does not reach desktop).
align 4
bdfe_to_fat_time_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- midnight → FAT time 0; ECX/EDX canaries (REG-001) ---
        mov     ecx, 0xC3C30001
        mov     edx, 0xD4D40002
        mov     eax, 0
        call    bdfe_to_fat_time
        test    eax, eax
        jnz     .fail
        cmp     ecx, 0xC3C30001
        jne     .fail
        cmp     edx, 0xD4D40002
        jne     .fail

        ; --- 00:00:02 (BDFE sec=2) → FAT sec field=1 ---
        mov     eax, 2
        call    bdfe_to_fat_time
        cmp     eax, 1
        jne     .fail

        ; --- 00:01:00 → 0x20 ---
        mov     eax, 0x100
        call    bdfe_to_fat_time
        cmp     eax, 0x20
        jne     .fail

        ; --- 01:00:00 → 0x800 ---
        mov     eax, 0x10000
        call    bdfe_to_fat_time
        cmp     eax, 0x800
        jne     .fail

        ; --- 23:59:58 → 0xBF7D ---
        mov     eax, 0x00173B3A
        call    bdfe_to_fat_time
        cmp     eax, 0xBF7D
        jne     .fail

        ; --- direct rust_* stdcall ---
        stdcall rust_bdfe_to_fat_time, 0x00173B3A
        cmp     eax, 0xBF7D
        jne     .fail

        ; --- round-trip via Cut AO fat_time_to_bdfe ---
        mov     eax, 0xBF7D
        call    fat_time_to_bdfe
        call    bdfe_to_fat_time
        cmp     eax, 0xBF7D
        jne     .fail

        ; --- loop-counter survival (bdfe_to_fat_entry pattern) ---
        mov     ecx, 5
@@:
        mov     eax, 0x00173B3A
        call    bdfe_to_fat_time
        cmp     eax, 0xBF7D
        jne     .fail
        loop    @b
        test    ecx, ecx
        jnz     .fail

        mov     [rust_bdfe_to_fat_time_smoke_result], 0x54324642 ; 'B2FT'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEADB00F        ; Cut BY fail marker
        mov     ebx, 0x42594642        ; 'BYFD'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_bdfe_to_fat_time(bdfe_time) -> EAX (FAT packed time); ret 4
align 16
rust_bdfe_to_fat_time:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_bdfe_to_fat_time.bin'
