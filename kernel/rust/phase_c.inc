;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; Phase C — minimal FASM → Rust call proof (removable after verified)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (symbol rust_phase_c_probe). See docs/migration/phase-c-integration.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-phase-c.ps1
;; which writes:
;;   ../rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; Magic must match rust_kernel/kolibri_utils/src/ffi.rs PHASE_C_PROBE_MAGIC
PHASE_C_PROBE_MAGIC = 0xC0DEA11C

iglobal
; Survives boot: observable proof that Rust ran (desktop smoke + memory).
rust_phase_c_result dd 0
endg

; Called once from high_code after paging + segment reload.
; On mismatch: distinctive hang (does not reach desktop).
align 4
phase_c_smoke_test:
        call    rust_phase_c_probe
        cmp     eax, PHASE_C_PROBE_MAGIC
        jne     .fail
        mov     [rust_phase_c_result], eax
        ret
.fail:
        mov     eax, 0xDEADBEEF
        mov     ebx, 0x50433043        ; 'C0CP' — Phase C probe fail marker
        mov     ecx, 0x50524F42        ; 'BORP'
        jmp     $

; Raw Rust stdcall body (0 args): returns PHASE_C_PROBE_MAGIC in EAX, then RET.
align 16
rust_phase_c_probe:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin'
