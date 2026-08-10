;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; exFAT_hash_calculate — Rust embed (Cut AI / NameHash)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_exfat_hash_calculate).
;; See docs/migration/cut-ai-plan.md / cut-ai-implementation.md.
;;
;; Prerequisites:
;;   python scripts/build_rust.py
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_exfat_hash_calculate.bin
;;
;; kernel/fs/exfat.inc (USE_RUST_EXFAT_HASH_CALCULATE=1) trampolines
;; exFAT_hash_calculate → rust_exfat_hash_calculate.
;; ABI smoke lives in exfat.inc (next to the leaf).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust NameHash path ran.
rust_exfat_hash_calculate_smoke_result dd 0
endg

;   rust_exfat_hash_calculate(buf, len) -> EAX (AX=NameHash); ret 8
align 4
rust_exfat_hash_calculate:
        file '../../rust_kernel/kolibri_utils/out/rust_exfat_hash_calculate.bin'
