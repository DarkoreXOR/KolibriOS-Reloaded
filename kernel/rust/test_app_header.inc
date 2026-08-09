;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; test_app_header — Rust embed + runtime smoke (Cut O)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_test_app_header).
;; See docs/migration/cut-o-plan.md / cut-o-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-test-app-header.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_test_app_header.bin
;;
;; kernel/core/taskman.inc (USE_RUST_TEST_APP_HEADER=1) trampolines
;; test_app_header → rust_test_app_header.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust test_app_header path ran.
rust_test_app_header_smoke_result dd 0
endg

; Called once from high_code after Cut N smoke.
; Exercises: public symbol → trampoline → Rust → APP_HDR fill / reject.
; On mismatch: distinctive hang (does not reach desktop).
align 4
test_app_header_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 128

        ; Zero scratch: fake header at [ebp-64], APP_HDR at [ebp-128]
        push    edi ecx eax
        lea     edi, [ebp-128]
        mov     ecx, 128/4
        xor     eax, eax
        rep stosd
        pop     eax ecx edi

        ; --- Success MENUET01 ---
        lea     eax, [ebp-64]
        mov     dword [eax], 'MENU'
        mov     word [eax+4], 'ET'
        mov     word [eax+6], '01'
        mov     dword [eax+8], 0xDEADBEEF   ; version ignored
        mov     dword [eax+12], 0x00001000  ; start → eip
        mov     dword [eax+16], 0x00002000  ; i_end → _edata
        mov     dword [eax+20], 0x00003000  ; mem_size → _emem
        mov     dword [eax+24], 0x00002FF0  ; stack_top → esp
        mov     dword [eax+28], 0x11111111  ; i_param → cmdline
        mov     dword [eax+32], 0x22222222  ; i_icon → path

        lea     ebx, [ebp-128]
        ; Poison APP_HDR so we detect stores
        mov     dword [ebx], 0xA5A5A5A5
        mov     dword [ebx+4], 0xA5A5A5A5
        mov     dword [ebx+8], 0xA5A5A5A5
        mov     dword [ebx+12], 0xA5A5A5A5
        mov     dword [ebx+16], 0xA5A5A5A5
        mov     dword [ebx+20], 0xA5A5A5A5

        ; Temporarily ensure pages_free is large enough (save/restore)
        push    dword [pg_data.pages_free]
        mov     dword [pg_data.pages_free], 16   ; 16*4K = 0x10000 > 0x3000
        mov     ecx, 0xC4000001
        mov     edx, 0xD4000002
        mov     esi, 0x54000003
        mov     edi, 0xE4000004
        call    test_app_header
        lea     ecx, [ebp-64]
        cmp     eax, ecx
        jne     .fail
        cmp     dword [ebx+8], 0x00001000       ; eip
        jne     .fail
        cmp     dword [ebx+0x14], 0x00003000    ; _emem
        jne     .fail
        cmp     dword [ebx+0x0C], 0x00002FF0    ; esp
        jne     .fail
        cmp     dword [ebx], 0x11111111         ; cmdline
        jne     .fail
        cmp     dword [ebx+4], 0x22222222       ; path
        jne     .fail
        cmp     dword [ebx+0x10], 0x00002000    ; _edata
        jne     .fail
        ; register preservation (ESI/EDI; ECX/EDX may be clobbered)
        cmp     esi, 0x54000003
        jne     .fail
        cmp     edi, 0xE4000004
        jne     .fail
        pop     dword [pg_data.pages_free]

        ; --- Fail: bad magic (no mutate) ---
        lea     eax, [ebp-64]
        mov     dword [eax], 'XXXX'
        lea     ebx, [ebp-128]
        mov     dword [ebx], 0xA5A5A5A5
        mov     dword [ebx+8], 0xA5A5A5A5
        push    dword [pg_data.pages_free]
        mov     dword [pg_data.pages_free], 16
        call    test_app_header
        test    eax, eax
        jnz     .fail
        cmp     dword [ebx+8], 0xA5A5A5A5
        jne     .fail
        pop     dword [pg_data.pages_free]

        ; --- Fail after eip write: mem_size < i_end ---
        lea     eax, [ebp-64]
        mov     dword [eax], 'MENU'
        mov     word [eax+4], 'ET'
        mov     word [eax+6], '01'
        mov     dword [eax+12], 0xABCD1234
        mov     dword [eax+16], 0x00005000      ; i_end
        mov     dword [eax+20], 0x00004000      ; mem_size < i_end
        lea     ebx, [ebp-128]
        mov     dword [ebx], 0xA5A5A5A5
        mov     dword [ebx+8], 0xA5A5A5A5
        mov     dword [ebx+0x14], 0xA5A5A5A5
        push    dword [pg_data.pages_free]
        mov     dword [pg_data.pages_free], 100
        call    test_app_header
        test    eax, eax
        jnz     .fail
        cmp     dword [ebx+8], 0xABCD1234       ; eip written
        jne     .fail
        cmp     dword [ebx+0x14], 0xA5A5A5A5    ; _emem not written
        jne     .fail
        pop     dword [pg_data.pages_free]

        ; --- MENUET02 success ---
        lea     eax, [ebp-64]
        mov     dword [eax], 'MENU'
        mov     word [eax+4], 'ET'
        mov     word [eax+6], '02'
        mov     dword [eax+12], 0x10
        mov     dword [eax+16], 0x20
        mov     dword [eax+20], 0x30
        mov     dword [eax+24], 0x28
        mov     dword [eax+28], 0
        mov     dword [eax+32], 0
        lea     ebx, [ebp-128]
        push    dword [pg_data.pages_free]
        mov     dword [pg_data.pages_free], 1    ; 4K >= 0x30? 0x1000 > 0x30 ok
        call    test_app_header
        lea     ecx, [ebp-64]
        cmp     eax, ecx
        jne     .fail
        cmp     dword [ebx+0x14], 0x30
        jne     .fail
        pop     dword [pg_data.pages_free]

        mov     [rust_test_app_header_smoke_result], 0x54415048 ; 'TAPH'
        mov     esp, ebp
        pop     ebp
        ret
.fail:
        mov     eax, 0xDEAD0C4F        ; Cut O fail marker
        mov     ebx, 0x54415048        ; 'TAPH'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_test_app_header(header, app_hdr, pages_free) -> EAX; ret 12
align 16
rust_test_app_header:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_test_app_header.bin'
