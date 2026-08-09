;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; antiAliasing — Rust embed + runtime smoke (Cut N)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_anti_aliasing).
;; See docs/migration/cut-n-plan.md / cut-n-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-anti-aliasing.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_anti_aliasing.bin
;;
;; kernel/gui/font.inc (USE_RUST_ANTI_ALIASING=1) trampolines
;; antiAliasing → rust_anti_aliasing.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust anti-aliasing path ran.
rust_anti_aliasing_smoke_result dd 0
endg

; Called once from high_code after Cut M smoke.
; Exercises: public symbol → trampoline → Rust → EAX blend + EBP=EBX.
; On mismatch: distinctive hang (does not reach desktop).
align 4
anti_aliasing_rust_smoke_test:
        ; Equal colors: identity
        mov     eax, 0x00AABBCC
        mov     ebx, 0x00AABBCC
        mov     ecx, 0xC3000001
        mov     edx, 0xD3000002
        mov     esi, 0x53000003
        mov     edi, 0xE3000004
        mov     ebp, 0xDEADBEEF
        call    antiAliasing
        cmp     eax, 0x00AABBCC
        jne     .fail
        cmp     ebp, ebx                ; trampoline restores EBP = EBX
        jne     .fail
        cmp     ebx, 0x00AABBCC
        jne     .fail
        cmp     esi, 0x53000003
        jne     .fail
        cmp     edi, 0xE3000004
        jne     .fail

        ; Blend toward black: (3*0xFF)>>2 = 0xBF per low byte lane used
        mov     eax, 0x000000FF
        mov     ebx, 0
        call    antiAliasing
        cmp     eax, 0x000000BF
        jne     .fail
        cmp     ebp, 0
        jne     .fail

        ; Multi-channel toward black: 0x00AABBCC → 0x007F8C99
        mov     eax, 0x00AABBCC
        xor     ebx, ebx
        call    antiAliasing
        cmp     eax, 0x007F8C99
        jne     .fail

        ; High byte preserved: fg=0xA1020304 bg=0 → blend low3 only
        mov     eax, 0xA1020304
        xor     ebx, ebx
        call    antiAliasing
        ; bytes: 04→03, 03→02, 02→01, high A1 kept → 0xA1010203
        ; (3*4)>>2=3, (3*3)>>2=2, (3*2)>>2=1
        cmp     eax, 0xA1010203
        jne     .fail
        mov     ecx, eax
        shr     ecx, 24
        cmp     ecx, 0xA1
        jne     .fail

        ; Toward white: (0*3+0xFF)>>2 = 0x3F
        xor     eax, eax
        mov     ebx, 0x000000FF
        call    antiAliasing
        cmp     eax, 0x0000003F
        jne     .fail
        cmp     ebp, 0x000000FF
        jne     .fail

        mov     [rust_anti_aliasing_smoke_result], 0x41414E54 ; 'AANT'
        ret
.fail:
        mov     eax, 0xDEAD0C4E        ; Cut N fail marker
        mov     ebx, 0x41414E54        ; 'AANT'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_anti_aliasing(fg, bg) -> EAX; ret 8
align 16
rust_anti_aliasing:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_anti_aliasing.bin'
