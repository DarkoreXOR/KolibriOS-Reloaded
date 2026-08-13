;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; Cut CU / Slice E — Rust phys-bitmap blobs
;;
;; Embeds reloc-free machine code for:
;;   rust_alloc_page / rust_free_page / rust_alloc_pages
;;   rust_release_bitmap_page_mode_b
;;
;; Prerequisites: python scripts/build_rust.py
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

; Raw Rust stdcall bodies (file embed)
align 16
rust_alloc_page:
        file '../../rust_kernel/kolibri_utils/out/rust_alloc_page.bin'

align 16
rust_free_page:
        file '../../rust_kernel/kolibri_utils/out/rust_free_page.bin'

align 16
rust_alloc_pages:
        file '../../rust_kernel/kolibri_utils/out/rust_alloc_pages.bin'

align 16
rust_release_bitmap_page_mode_b:
        file '../../rust_kernel/kolibri_utils/out/rust_release_bitmap_page_mode_b.bin'
