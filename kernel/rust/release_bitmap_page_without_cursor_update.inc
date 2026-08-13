;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; release_bitmap_page_without_cursor_update — Rust embed + ABI smoke (Cut CT)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_release_bitmap_page_without_cursor_update).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_release_bitmap_page_without_cursor_update.bin
;;
;; When USE_RUST_RELEASE_BITMAP_PAGE_WITHOUT_CURSOR_UPDATE=1, public-symbol
;; checks exercise the production trampoline. When OFF, they exercise the
;; extracted FASM helper (same public ABI).
;;
;; Marker: RBPB. Fail hang: DEAD0C54 / 'RBPB' / 'FAIL'.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Cut CT smoke ran.
rust_release_bitmap_page_smoke_result dd 0
endg

uglobal
align 4
; Synthetic map for direct rust_* vectors (128 pages / 16 bytes).
rbpb_smoke_map: rb 16
endg

; Called once from high_code after paging init. On mismatch: hang.
align 4
release_bitmap_page_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic map ---
        mov     edi, rbpb_smoke_map
        mov     ecx, 16/4
        xor     eax, eax
        rep     stosd

        ; Snapshot live pages_free / page_start (helper must not touch)
        mov     esi, [pg_data.pages_free]
        mov     edi, [page_start]

        ; --- Direct rust_*: allocated → delta=1 ---
        stdcall rust_release_bitmap_page_without_cursor_update, 5, rbpb_smoke_map
        cmp     eax, 1
        jne     .fail
        test    byte [rbpb_smoke_map], 1 shl 5
        jz      .fail

        ; --- Direct rust_*: already-free → delta=0 ---
        stdcall rust_release_bitmap_page_without_cursor_update, 5, rbpb_smoke_map
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: dword boundary 31 then 32 ---
        stdcall rust_release_bitmap_page_without_cursor_update, 31, rbpb_smoke_map
        cmp     eax, 1
        jne     .fail
        stdcall rust_release_bitmap_page_without_cursor_update, 32, rbpb_smoke_map
        cmp     eax, 1
        jne     .fail

        ; --- pages_free / page_start still unchanged after direct calls ---
        cmp     esi, [pg_data.pages_free]
        jne     .fail
        cmp     edi, [page_start]
        jne     .fail

        ; --- Public helper on live sys_pgmap: already-free bit (delta=0) ---
        ; Scan first dword for a free bit so we do not allocate a live page.
        mov     eax, [sys_pgmap]
        bsf     eax, eax
        jz      .pass                    ; no free bit in dword0 — skip live path

        mov     ebx, 0xB0B00001
        mov     ecx, 0xC0C00002
        mov     edx, 0xD0D00003
        mov     esi, 0x51510004
        mov     edi, 0xE1E10005
        mov     ebp, 0xF1F10006
        push    dword [pg_data.pages_free]
        push    dword [page_start]
        cld
        call    release_bitmap_page_without_cursor_update
        test    eax, eax                 ; already free → delta 0
        jnz     .fail
        cmp     ebx, 0xB0B00001
        jne     .fail
        cmp     ecx, 0xC0C00002
        jne     .fail
        cmp     edx, 0xD0D00003
        jne     .fail
        cmp     esi, 0x51510004
        jne     .fail
        cmp     edi, 0xE1E10005
        jne     .fail
        cmp     ebp, 0xF1F10006
        jne     .fail
        pop     eax                      ; saved page_start
        cmp     eax, [page_start]
        jne     .fail
        pop     eax                      ; saved pages_free
        cmp     eax, [pg_data.pages_free]
        jne     .fail
        pushfd
        pop     eax
        test    eax, 0x400               ; DF must remain clear
        jnz     .fail

.pass:
        mov     [rust_release_bitmap_page_smoke_result], 0x52504242 ; 'RBPB'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C54                ; Cut CT fail marker
        mov     ebx, 0x52504242                ; 'RBPB'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_release_bitmap_page_without_cursor_update(page_index, map) -> EAX; ret 8
align 16
rust_release_bitmap_page_without_cursor_update:
        file '../../rust_kernel/kolibri_utils/out/rust_release_bitmap_page_without_cursor_update.bin'
