;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; utf8to16 — Rust embed + ABI smoke (Cut AB)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_utf8to16).
;; See docs/migration/cut-ab-plan.md / cut-ab-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-utf8to16.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_utf8to16.bin
;;
;; kernel/fs/parse_fn.inc (USE_RUST_UTF8TO16=1) trampolines
;; utf8to16 → rust_utf8to16.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust utf8to16 path ran.
rust_utf8to16_smoke_result dd 0
endg

uglobal
align 4
utf8to16_smoke_buf:     rb 32
endg

; Called once from high_code with other Unicode smokes.
; Exercises: direct rust_* + public trampoline; ESI advance; AX values;
; EBX/ECX/EDX/EDI/EBP preserve; restart + multi-byte adversarial.
; On mismatch: distinctive hang (does not reach desktop).
align 4
utf8to16_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- ASCII 'A' via rust_* ---
        mov     edi, utf8to16_smoke_buf
        mov     byte [edi], 'A'
        mov     byte [edi+1], 0
        mov     eax, edi
        push    eax                     ; esi_inout storage
        mov     ecx, esp
        stdcall rust_utf8to16, ecx, 0
        cmp     eax, 'A'
        jne     .fail
        pop     esi
        lea     edx, [edi+1]
        cmp     esi, edx
        jne     .fail

        ; --- U+00E9 = C3 A9 ---
        mov     edi, utf8to16_smoke_buf
        mov     byte [edi], 0xC3
        mov     byte [edi+1], 0xA9
        mov     byte [edi+2], 0
        mov     eax, edi
        push    eax
        mov     ecx, esp
        stdcall rust_utf8to16, ecx, 0
        cmp     ax, 0x00E9
        jne     .fail
        pop     esi
        lea     edx, [edi+2]
        cmp     esi, edx
        jne     .fail

        ; --- U+0800 = E0 A0 80 ---
        mov     edi, utf8to16_smoke_buf
        mov     byte [edi], 0xE0
        mov     byte [edi+1], 0xA0
        mov     byte [edi+2], 0x80
        mov     byte [edi+3], 0
        mov     eax, edi
        push    eax
        mov     ecx, esp
        stdcall rust_utf8to16, ecx, 0
        cmp     ax, 0x0800
        jne     .fail
        pop     esi
        lea     edx, [edi+3]
        cmp     esi, edx
        jne     .fail

        ; --- Invalid lead restart: 80 'Z' ---
        mov     edi, utf8to16_smoke_buf
        mov     byte [edi], 0x80
        mov     byte [edi+1], 'Z'
        mov     eax, edi
        push    eax
        mov     ecx, esp
        stdcall rust_utf8to16, ecx, 0
        cmp     eax, 'Z'
        jne     .fail
        pop     esi
        lea     edx, [edi+2]
        cmp     esi, edx
        jne     .fail

        ; --- Public trampoline: U+20AC euro E2 82 AC; preserve regs ---
        mov     esi, utf8to16_smoke_buf
        mov     byte [esi], 0xE2
        mov     byte [esi+1], 0x82
        mov     byte [esi+2], 0xAC
        mov     byte [esi+3], 0
        mov     ebx, 0xB1AB0001
        mov     ecx, 0xC1AB0002
        mov     edx, 0xD1AB0003
        mov     edi, 0xE1AB0005
        mov     ebp, 0xF1AB0006
        xor     eax, eax
        call    utf8to16
        cmp     ax, 0x20AC
        jne     .fail
        cmp     ebx, 0xB1AB0001
        jne     .fail
        cmp     ecx, 0xC1AB0002
        jne     .fail
        cmp     edx, 0xD1AB0003
        jne     .fail
        cmp     edi, 0xE1AB0005
        jne     .fail
        cmp     ebp, 0xF1AB0006
        jne     .fail
        ; ESI advanced by 3
        mov     eax, utf8to16_smoke_buf
        add     eax, 3
        cmp     esi, eax
        jne     .fail

        ; --- Chained public call: C3 A9 then '!' with leftover EAX ---
        mov     esi, utf8to16_smoke_buf
        mov     byte [esi], 0xC3
        mov     byte [esi+1], 0xA9
        mov     byte [esi+2], '!'
        mov     byte [esi+3], 0
        xor     eax, eax
        call    utf8to16
        cmp     ax, 0x00E9
        jne     .fail
        call    utf8to16                ; EAX still 0x00E9
        cmp     ax, '!'
        jne     .fail

        ; --- Mid-stream ASCII abort: C3 'X' ---
        mov     esi, utf8to16_smoke_buf
        mov     byte [esi], 0xC3
        mov     byte [esi+1], 'X'
        xor     eax, eax
        call    utf8to16
        cmp     ax, 'X'
        jne     .fail

        mov     dword [rust_utf8to16_smoke_result], 0x36543855 ; 'U8T6'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0CAB                ; Cut AB fail marker
        mov     ebx, 0x36543855                ; 'U8T6'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_utf8to16(esi_inout, initial_eax) -> EAX; ret 8
align 16
rust_utf8to16:
        file '../../rust_kernel/kolibri_utils/out/rust_utf8to16.bin'
