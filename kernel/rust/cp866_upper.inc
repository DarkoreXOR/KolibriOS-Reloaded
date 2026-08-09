;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; CP866 uppercase — Rust embed + runtime smoke (Cut B)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_cp866_to_upper).
;; See docs/migration/cut-b-plan.md / cut-b-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-cp866-upper.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_cp866_to_upper.bin
;;
;; kernel/fs/parse_fn.inc (USE_RUST_CP866_UPPER=1) trampolines
;; cp866toUpper → rust_cp866_to_upper.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; Latin 'a' → 'A' exercises the ASCII path through the real trampoline.
CP866_UPPER_SMOKE_IN     = 'a'
CP866_UPPER_SMOKE_EXPECT = 'A'
; CP866 ё (0xF1) → Ё (0xF0)
CP866_UPPER_SMOKE_IN2     = 0xF1
CP866_UPPER_SMOKE_EXPECT2 = 0xF0

iglobal
; Survives boot: observable proof that Rust cp866toUpper path ran.
rust_cp866_upper_smoke_result dd 0
endg

; Called once from high_code after UTF-8 smoke.
; Exercises: FASM callers → cp866toUpper trampoline → Rust body.
; On mismatch: distinctive hang (does not reach desktop).
align 4
cp866_upper_rust_smoke_test:
        mov     eax, CP866_UPPER_SMOKE_IN
        call    cp866toUpper
        cmp     al, CP866_UPPER_SMOKE_EXPECT
        jne     .fail
        mov     eax, CP866_UPPER_SMOKE_IN2
        call    cp866toUpper
        cmp     al, CP866_UPPER_SMOKE_EXPECT2
        jne     .fail
        movzx   eax, al
        mov     [rust_cp866_upper_smoke_result], eax
        ret
.fail:
        mov     eax, 0xDEAD8661        ; CP866 upper fail marker
        mov     ebx, 0x43505550        ; 'CPUP'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_cp866_to_upper(ch) -> EAX (AL = uppercased CP866); ret 4
align 16
rust_cp866_to_upper:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_cp866_to_upper.bin'
