;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; iso9660_compare_name — Rust embed + ABI smoke (Cut AJ)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_iso9660_compare_name).
;;
;; kernel/fs/iso9660.inc (USE_RUST_ISO9660_COMPARE_NAME=1) trampolines
;; iso9660_compare_name → rust_iso9660_compare_name.
;; ABI smoke lives in iso9660.inc (next to the leaf).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust ISO9660 compare path ran.
rust_iso9660_compare_name_smoke_result dd 0
endg

;   rust_iso9660_compare_name(esi_inout, dir_record, type_encoding)
;   -> EAX=0 match / 1 miss; ret 12
align 4
rust_iso9660_compare_name:
        file '../../rust_kernel/kolibri_utils/out/rust_iso9660_compare_name.bin'
