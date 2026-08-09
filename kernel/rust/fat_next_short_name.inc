;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fat_next_short_name — Rust embed + runtime smoke (Cut K)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fat_next_short_name).
;; See docs/migration/cut-k-plan.md / cut-k-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-fat-next-short-name.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fat_next_short_name.bin
;;
;; kernel/fs/fat.inc (USE_RUST_FAT_NEXT_SHORT_NAME=1) trampolines
;; fat_next_short_name → rust_fat_next_short_name → CF + cld.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust FAT short-name path ran.
rust_fat_next_short_name_smoke_result dd 0
endg

uglobal
align 4
fat_smoke_name:  rb 11
fat_smoke_name2: rb 11
endg

; Called once from high_code after Cut J smoke.
; Exercises: public symbol → trampoline → Rust → CF + memory mutate + DF clear.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fat_next_short_name_rust_smoke_test:

        ; --- vector 1: insert ~1 into full basename ---
        mov     edi, fat_smoke_name
        mov     eax, 'FILE'
        mov     [edi], eax
        mov     eax, 'NAME'
        mov     [edi+4], eax
        mov     ax, 'TX'
        mov     [edi+8], ax
        mov     byte [edi+10], 'T'

        mov     eax, 0xA9000001
        mov     ebx, 0xB9000002
        mov     ecx, 0xC9000003
        mov     edx, 0xD9000004
        mov     esi, 0xE9000005
        mov     ebp, 0x19000007
        call    fat_next_short_name
        jc      .fail
        cmp     eax, 0xA9000001
        jne     .fail
        cmp     ebx, 0xB9000002
        jne     .fail
        cmp     ecx, 0xC9000003
        jne     .fail
        cmp     edx, 0xD9000004
        jne     .fail
        cmp     esi, 0xE9000005
        jne     .fail
        cmp     edi, fat_smoke_name
        jne     .fail
        cmp     ebp, 0x19000007
        jne     .fail
        ; FILENA~1TXT
        cmp     dword [fat_smoke_name], 'FILE'
        jne     .fail
        cmp     dword [fat_smoke_name+4], 'NA~1'
        jne     .fail
        cmp     word [fat_smoke_name+8], 'TX'
        jne     .fail
        cmp     byte [fat_smoke_name+10], 'T'
        jne     .fail
        ; DF must be clear (trampoline cld)
        pushfd
        pop     eax
        test    eax, 400h
        jnz     .fail

        ; --- vector 2: increment digit (FILE~1__ → FILE~200 via zerofill) ---
        mov     edi, fat_smoke_name2
        mov     eax, 'FILE'
        mov     [edi], eax
        mov     byte [edi+4], '~'
        mov     byte [edi+5], '1'
        mov     byte [edi+6], ' '
        mov     byte [edi+7], ' '
        mov     ax, 'TX'
        mov     [edi+8], ax
        mov     byte [edi+10], 'T'
        call    fat_next_short_name
        jc      .fail
        cmp     byte [fat_smoke_name2+5], '2'
        jne     .fail
        cmp     byte [fat_smoke_name2+6], '0'
        jne     .fail
        cmp     byte [fat_smoke_name2+7], '0'
        jne     .fail

        ; --- vector 3: expand ~9 + spaces → ~10 ---
        mov     edi, fat_smoke_name
        mov     eax, 'FILE'
        mov     [edi], eax
        mov     byte [edi+4], '~'
        mov     byte [edi+5], '9'
        mov     byte [edi+6], ' '
        mov     byte [edi+7], ' '
        mov     ax, 'TX'
        mov     [edi+8], ax
        mov     byte [edi+10], 'T'
        call    fat_next_short_name
        jc      .fail
        cmp     byte [fat_smoke_name+5], '1'
        jne     .fail
        cmp     byte [fat_smoke_name+6], '0'
        jne     .fail
        cmp     byte [fat_smoke_name+7], ' '
        jne     .fail

        ; --- vector 4: exhausted (X~999999) → CF set, basename unchanged ---
        mov     edi, fat_smoke_name2
        mov     byte [edi], 'X'
        mov     byte [edi+1], '~'
        mov     eax, '9999'
        mov     [edi+2], eax
        mov     word [edi+6], '99'
        mov     ax, 'TX'
        mov     [edi+8], ax
        mov     byte [edi+10], 'T'
        call    fat_next_short_name
        jnc     .fail
        cmp     byte [fat_smoke_name2], 'X'
        jne     .fail
        cmp     byte [fat_smoke_name2+1], '~'
        jne     .fail
        cmp     dword [fat_smoke_name2+2], '9999'
        jne     .fail
        cmp     word [fat_smoke_name2+6], '99'
        jne     .fail

        mov     [rust_fat_next_short_name_smoke_result], 0x4641544B ; 'FATK'
        ret
.fail:
        mov     eax, 0xDEAD0C4B        ; Cut K fail marker
        mov     ebx, 0x4641544B        ; 'FATK'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fat_next_short_name(name) -> EAX 0/1; ret 4
align 16
rust_fat_next_short_name:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_fat_next_short_name.bin'
