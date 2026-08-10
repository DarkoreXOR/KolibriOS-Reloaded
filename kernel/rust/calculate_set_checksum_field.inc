;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; calculate_SetChecksum_field — Rust embed (Cut AH)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_calculate_set_checksum_field).
;; See docs/migration/cut-ah-plan.md / cut-ah-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-calculate-set-checksum-field.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_calculate_set_checksum_field.bin
;;
;; kernel/fs/exfat.inc (USE_RUST_CALCULATE_SET_CHECKSUM_FIELD=1) trampolines
;; calculate_SetChecksum_field → rust_calculate_set_checksum_field.
;; ABI smoke lives in exfat.inc (needs exFAT struct offsets).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust SetChecksum path ran.
rust_calculate_set_checksum_field_smoke_result dd 0
endg

;   rust_calculate_set_checksum_field(buf, len) -> EAX (AX=checksum); ret 8
;   Also writes LE checksum to [buf+2].
align 4
rust_calculate_set_checksum_field:
        file '../../rust_kernel/kolibri_utils/out/rust_calculate_set_checksum_field.bin'
