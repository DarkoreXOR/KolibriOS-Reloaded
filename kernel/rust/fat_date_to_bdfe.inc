;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fat_date_to_bdfe — Rust embed + ABI smoke (Cut BW)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fat_date_to_bdfe).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fat_date_to_bdfe.bin
;;
;; Requires USE_RUST_FAT_DATE_TO_BDFE=1 so public fat_date_to_bdfe hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EAX FAT date in → EAX BDFE date out; trampoline preserves
;; ECX+EDX (REG-001); ret (not stdcall) at public symbol; Rust body is
;; stdcall ret 4.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fat_date_to_bdfe path ran.
rust_fat_date_to_bdfe_smoke_result dd 0
endg

; Called once from high_code after Cut AO smoke.
; Exercises: public trampoline + direct rust_*; named vectors; ECX/EDX canaries;
; high-bit FAT date; max packed date.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fat_date_to_bdfe_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- 1980-00-00 (FAT date 0); ECX/EDX canaries (REG-001) ---
        mov     ecx, 0xC1C10001
        mov     edx, 0xD2D20002
        mov     eax, 0
        call    fat_date_to_bdfe
        cmp     eax, 0x07BC0000
        jne     .fail
        cmp     ecx, 0xC1C10001
        jne     .fail
        cmp     edx, 0xD2D20002
        jne     .fail

        ; --- 2024-06-15: year=44<<9 | month=6<<5 | day=15 = 0x58CF ---
        mov     eax, 0x58CF
        call    fat_date_to_bdfe
        cmp     eax, 0x07E8060F
        jne     .fail

        ; --- 1980-03-01 ---
        mov     eax, 0x61
        call    fat_date_to_bdfe
        cmp     eax, 0x07BC0301
        jne     .fail

        ; --- day-only field (1980-01-31) ---
        mov     eax, 0x3F
        call    fat_date_to_bdfe
        cmp     eax, 0x07BC011F
        jne     .fail

        ; --- max packed u16 (2107-07-31) ---
        mov     eax, 0xFFFF
        call    fat_date_to_bdfe
        cmp     eax, 0x083B0F1F
        jne     .fail

        ; --- direct rust_* stdcall ---
        stdcall rust_fat_date_to_bdfe, 0x58CF
        cmp     eax, 0x07E8060F
        jne     .fail

        ; --- loop-counter survival (entry→BDFE pattern) ---
        mov     ecx, 8
@@:
        mov     eax, 0x58CF
        call    fat_date_to_bdfe
        cmp     eax, 0x07E8060F
        jne     .fail
        loop    @b
        test    ecx, ecx
        jnz     .fail

        mov     [rust_fat_date_to_bdfe_smoke_result], 0x46445442 ; 'FDTB'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEADB00F        ; Cut BW fail marker
        mov     ebx, 0x42455746        ; 'BWFD'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fat_date_to_bdfe(fat_date) -> EAX (BDFE date); ret 4
align 16
rust_fat_date_to_bdfe:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_fat_date_to_bdfe.bin'
