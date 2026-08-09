;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; strncmp — Rust embed + runtime smoke (Cut D)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_strncmp).
;; See docs/migration/cut-d-plan.md / cut-d-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-strncmp.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_strncmp.bin
;;
;; kernel/core/string.inc (USE_RUST_STRNCMP=1) trampolines
;; strncmp → rust_strncmp.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust strncmp path ran.
rust_strncmp_smoke_result dd 0
strncmp_smoke_a:
        db 'kolibri',0
strncmp_smoke_b:
        db 'kolibri',0
strncmp_smoke_c:
        db 'kolibrj',0
strncmp_smoke_empty:
        db 0
endg

; Called once from high_code after Cut C smoke.
; Exercises: FASM callers → strncmp trampoline → Rust body.
; Vectors: equal, unequal, n=0, n=-1; also ESI/EDI preservation.
; On mismatch: distinctive hang (does not reach desktop).
align 4
strncmp_rust_smoke_test:
        ; Preserve check: seed ESI/EDI, require unchanged after calls.
        mov     esi, 0x11111111
        mov     edi, 0x22222222

        ; equal, n=7
        stdcall strncmp, strncmp_smoke_a, strncmp_smoke_b, 7
        test    eax, eax
        jnz     .fail

        ; unequal last letter → expect -1 ('i' < 'j')
        stdcall strncmp, strncmp_smoke_a, strncmp_smoke_c, 7
        cmp     eax, -1
        jne     .fail

        ; n=0 → always equal
        stdcall strncmp, strncmp_smoke_a, strncmp_smoke_c, 0
        test    eax, eax
        jnz     .fail

        ; n=-1 unlimited equal
        stdcall strncmp, strncmp_smoke_a, strncmp_smoke_b, dword -1
        test    eax, eax
        jnz     .fail

        ; empty vs empty
        stdcall strncmp, strncmp_smoke_empty, strncmp_smoke_empty, 1
        test    eax, eax
        jnz     .fail

        cmp     esi, 0x11111111
        jne     .fail
        cmp     edi, 0x22222222
        jne     .fail

        mov     [rust_strncmp_smoke_result], 0x53544E43 ; 'STNC'
        ret
.fail:
        mov     eax, 0xDEAD57C3        ; strncmp fail marker
        mov     ebx, 0x53544E43        ; 'STNC'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_strncmp(s1, s2, n) -> EAX (-1/0/1); ret 12
align 16
rust_strncmp:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_strncmp.bin'
