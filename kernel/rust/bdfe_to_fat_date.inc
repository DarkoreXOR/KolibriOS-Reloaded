;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; bdfe_to_fat_date — Rust embed + ABI smoke (Cut BX)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_bdfe_to_fat_date).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_bdfe_to_fat_date.bin
;;
;; Requires USE_RUST_BDFE_TO_FAT_DATE=1 so public bdfe_to_fat_date hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EAX BDFE date in → EAX FAT packed date out; trampoline
;; preserves ECX+EDX (REG-001); ret (not stdcall) at public symbol; Rust body
;; is stdcall ret 4.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust bdfe_to_fat_date path ran.
rust_bdfe_to_fat_date_smoke_result dd 0
endg

; Called once from high_code after Cut BW smoke.
; Exercises: public trampoline + direct rust_*; named vectors; ECX/EDX canaries;
; round-trip with fat_date_to_bdfe; max BDFE date.
; On mismatch: distinctive hang (does not reach desktop).
align 4
bdfe_to_fat_date_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- 1980-00-00 → FAT date 0; ECX/EDX canaries (REG-001) ---
        mov     ecx, 0xC2C20001
        mov     edx, 0xD3D30002
        mov     eax, 0x07BC0000
        call    bdfe_to_fat_date
        test    eax, eax
        jnz     .fail
        cmp     ecx, 0xC2C20001
        jne     .fail
        cmp     edx, 0xD3D30002
        jne     .fail

        ; --- 1980-01-01 → 0x21 ---
        mov     eax, 0x07BC0101
        call    bdfe_to_fat_date
        cmp     eax, 0x21
        jne     .fail

        ; --- 2024-06-15 → 0x58CF ---
        mov     eax, 0x07E8060F
        call    bdfe_to_fat_date
        cmp     eax, 0x58CF
        jne     .fail

        ; --- max BDFE (2107-07-31) → 0xFFFF ---
        mov     eax, 0x083B0F1F
        call    bdfe_to_fat_date
        cmp     eax, 0xFFFF
        jne     .fail

        ; --- direct rust_* stdcall ---
        stdcall rust_bdfe_to_fat_date, 0x07E8060F
        cmp     eax, 0x58CF
        jne     .fail

        ; --- round-trip via Cut BW fat_date_to_bdfe ---
        mov     eax, 0x58CF
        call    fat_date_to_bdfe
        call    bdfe_to_fat_date
        cmp     eax, 0x58CF
        jne     .fail

        ; --- loop-counter survival (bdfe_to_fat_entry pattern) ---
        mov     ecx, 6
@@:
        mov     eax, 0x07E8060F
        call    bdfe_to_fat_date
        cmp     eax, 0x58CF
        jne     .fail
        loop    @b
        test    ecx, ecx
        jnz     .fail

        mov     [rust_bdfe_to_fat_date_smoke_result], 0x44324642 ; 'B2FD'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEADB00F        ; Cut BX fail marker
        mov     ebx, 0x42584642        ; 'BXFD'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_bdfe_to_fat_date(bdfe_date) -> EAX (FAT packed date); ret 4
align 16
rust_bdfe_to_fat_date:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_bdfe_to_fat_date.bin'
