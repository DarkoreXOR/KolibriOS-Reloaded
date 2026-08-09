;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fat_gen_short_name — Rust embed + ABI smoke (Cut U)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fat_gen_short_name).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-fat-gen-short-name.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fat_gen_short_name.bin
;;
;; Requires USE_RUST_FAT_GEN_SHORT_NAME=1 so public fat_gen_short_name hits
;; the trampoline (not the FASM body).
;;
;; ABI: ESI → UTF-8 NUL name; EDI → ≥12-byte out; pushad/popad preserve GPRs.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fat_gen_short_name path ran.
rust_fat_gen_short_name_smoke_result dd 0
endg

uglobal
align 4
fat_gen_smoke_out0:  rb 12
fat_gen_smoke_out1:  rb 12
fat_gen_smoke_out2:  rb 12
fat_gen_smoke_src0:  rb 32
fat_gen_smoke_src1:  rb 32
fat_gen_smoke_src2:  rb 32
endg

; Called once from high_code after Cut T smoke.
; Exercises: ESI/EDI public ABI, 8.3 layout, lossy→~1, GPR preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fat_gen_short_name_rust_smoke_test:
        push    ebx esi edi ebp

        ; ========== vector 1: file.txt → FILE____TXT ==========
        mov     edi, fat_gen_smoke_src0
        mov     eax, 'file'
        mov     [edi], eax
        mov     eax, '.txt'
        mov     [edi+4], eax
        mov     byte [edi+8], 0

        mov     eax, 0xA8000001
        mov     ebx, 0xB8000002
        mov     ecx, 0xC8000003
        mov     edx, 0xD8000004
        mov     esi, fat_gen_smoke_src0
        mov     edi, fat_gen_smoke_out0
        mov     ebp, 0xE8000005
        call    fat_gen_short_name
        cmp     eax, 0xA8000001
        jne     .fail
        cmp     ebx, 0xB8000002
        jne     .fail
        cmp     ecx, 0xC8000003
        jne     .fail
        cmp     edx, 0xD8000004
        jne     .fail
        cmp     esi, fat_gen_smoke_src0
        jne     .fail
        cmp     edi, fat_gen_smoke_out0
        jne     .fail
        cmp     ebp, 0xE8000005
        jne     .fail
        ; FILE    TXT
        cmp     dword [fat_gen_smoke_out0], 'FILE'
        jne     .fail
        cmp     dword [fat_gen_smoke_out0+4], '    '
        jne     .fail
        cmp     word [fat_gen_smoke_out0+8], 'TX'
        jne     .fail
        cmp     byte [fat_gen_smoke_out0+10], 'T'
        jne     .fail
        cmp     byte [fat_gen_smoke_out0+11], ' '   ; 12th stosd space
        jne     .fail

        ; ========== vector 2: longfilename.txt → LONGFI~1TXT ==========
        mov     edi, fat_gen_smoke_src1
        mov     eax, 'long'
        mov     [edi], eax
        mov     eax, 'file'
        mov     [edi+4], eax
        mov     eax, 'name'
        mov     [edi+8], eax
        mov     eax, '.txt'
        mov     [edi+12], eax
        mov     byte [edi+16], 0
        mov     esi, fat_gen_smoke_src1
        mov     edi, fat_gen_smoke_out1
        call    fat_gen_short_name
        cmp     dword [fat_gen_smoke_out1], 'LONG'
        jne     .fail
        cmp     dword [fat_gen_smoke_out1+4], 'FI~1'
        jne     .fail
        cmp     word [fat_gen_smoke_out1+8], 'TX'
        jne     .fail
        cmp     byte [fat_gen_smoke_out1+10], 'T'
        jne     .fail

        ; ========== vector 3: a.b.c → AB~1____C__ ==========
        mov     edi, fat_gen_smoke_src2
        mov     eax, 'a.b.'
        mov     [edi], eax
        mov     byte [edi+4], 'c'
        mov     byte [edi+5], 0
        mov     esi, fat_gen_smoke_src2
        mov     edi, fat_gen_smoke_out2
        call    fat_gen_short_name
        cmp     dword [fat_gen_smoke_out2], 'AB~1'
        jne     .fail
        cmp     dword [fat_gen_smoke_out2+4], '    '
        jne     .fail
        cmp     byte [fat_gen_smoke_out2+8], 'C'
        jne     .fail

        mov     [rust_fat_gen_short_name_smoke_result], 0x4647534E ; 'FGSN'
        pop     ebp edi esi ebx
        ret

.fail:
        mov     eax, 0xDEAD0C55        ; Cut U fail marker
        mov     ebx, 0x4647534E        ; 'FGSN'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fat_gen_short_name(src, out); ret 8
align 16
rust_fat_gen_short_name:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_fat_gen_short_name.bin'
