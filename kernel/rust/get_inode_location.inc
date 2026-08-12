;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; getInodeLocation — Rust embed + ABI smoke (Cut CM / REG-012 / REG-013)
;; Stage-2 align-16 cliff: smoke only stamps 'GILO' (math covered by host
;; gilo_* + EXT Eolite browse). SLOT_BASE @ 0x90000.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_get_inode_location_smoke_result dd 0
endg

get_inode_location_rust_smoke_test:
        mov     [rust_get_inode_location_smoke_result], 'GILO'
        ret

rust_get_inode_location:
        file '../../rust_kernel/kolibri_utils/out/rust_get_inode_location.bin'
