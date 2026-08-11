;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fat_time_to_bdfe — Rust embed + ABI smoke (Cut AO)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fat_time_to_bdfe).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fat_time_to_bdfe.bin
;;
;; Requires USE_RUST_FAT_TIME_TO_BDFE=1 so public fat_time_to_bdfe hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EAX FAT time in → EAX BDFE time out; trampoline preserves
;; ECX+EDX (REG-001); ret (not stdcall) at public symbol; Rust body is
;; stdcall ret 4.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fat_time_to_bdfe path ran.
rust_fat_time_to_bdfe_smoke_result dd 0
endg

; Called once from high_code after Cut AN smoke.
; Exercises: public trampoline + direct rust_*; named vectors; ECX/EDX canaries;
; high-bit FAT time; max packed time.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fat_time_to_bdfe_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- midnight; ECX/EDX canaries (REG-001) ---
        mov     ecx, 0xC0C00001
        mov     edx, 0xD0D00002
        mov     eax, 0
        call    fat_time_to_bdfe
        test    eax, eax
        jnz     .fail
        cmp     ecx, 0xC0C00001
        jne     .fail
        cmp     edx, 0xD0D00002
        jne     .fail

        ; --- 00:00:02 (FAT sec field = 1) ---
        mov     eax, 1
        call    fat_time_to_bdfe
        cmp     eax, 2
        jne     .fail

        ; --- 00:01:00 (min field = 1 → bit 5) ---
        mov     eax, 0x20
        call    fat_time_to_bdfe
        cmp     eax, 0x100
        jne     .fail

        ; --- 01:00:00 (hour field = 1 → bit 11) ---
        mov     eax, 0x800
        call    fat_time_to_bdfe
        cmp     eax, 0x10000
        jne     .fail

        ; --- 23:59:58 ---
        ; hours=23<<11 | mins=59<<5 | secs/2=29 = 0xBF7D → BDFE 0x00173B3A
        mov     eax, 0xBF7D
        call    fat_time_to_bdfe
        cmp     eax, 0x00173B3A
        jne     .fail

        ; --- high junk above word (movzx callers clear; still match FASM) ---
        mov     eax, 0xDEAD0800
        call    fat_time_to_bdfe
        ; hours = 0xDEAD0800 >> 11; secs=0; mins=0
        mov     ebx, 0xDEAD0800
        shr     ebx, 11
        shl     ebx, 16
        cmp     eax, ebx
        jne     .fail

        ; --- direct rust_* stdcall ---
        stdcall rust_fat_time_to_bdfe, 0x800
        cmp     eax, 0x10000
        jne     .fail

        ; --- loop-counter survival (entry→BDFE pattern) ---
        mov     ecx, 8
@@:
        mov     eax, 0x20
        call    fat_time_to_bdfe
        cmp     eax, 0x100
        jne     .fail
        loop    @b
        test    ecx, ecx
        jnz     .fail

        mov     [rust_fat_time_to_bdfe_smoke_result], 0x46544244 ; 'FTBD'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEADA00F        ; Cut AO fail marker
        mov     ebx, 0x414F4641        ; 'AOFA'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fat_time_to_bdfe(fat_time) -> EAX (BDFE time); ret 4
align 16
rust_fat_time_to_bdfe:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_fat_time_to_bdfe.bin'
