;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; iso9660_copy_name — Rust embed + ABI smoke result (Cut BI)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_iso9660_copy_name).
;;
;; kernel/fs/iso9660.inc (USE_RUST_ISO9660_COPY_NAME=1) trampolines
;; iso9660_copy_name → rust_iso9660_copy_name.
;; ABI smoke lives in iso9660.inc (next to the leaf).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust ISO9660 copy-name path ran.
rust_iso9660_copy_name_smoke_result dd 0
endg

;   rust_iso9660_copy_name(esi_inout, edi_inout, max_len, nameenc, type_encoding)
;   ret 20
align 4
rust_iso9660_copy_name:
        file '../../rust_kernel/kolibri_utils/out/rust_iso9660_copy_name.bin'
