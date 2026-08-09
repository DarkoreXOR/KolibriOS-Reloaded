;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; UTF16to8 — Rust embed + prepared smoke (Cut Q Step 3)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_utf16_to_8).
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-utf16-to-8.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_utf16_to_8.bin
;;
;; Requires USE_RUST_UTF16_TO_8=1 so public UTF16to8
;; hits: FASM trampoline → rust_utf16_to_8 (not the FASM body).
;;
;; Step 4+: USE_RUST_UTF16_TO_8=1; smoke wired after Cut P in high_code.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust UTF16to8 path ran.
rust_utf16_to_8_smoke_result dd 0
endg

; Diagnostic smoke: public UTF16to8 → trampoline → Rust.
; Step 3 leaves this unwired from high_code.
; On mismatch: distinctive hang (does not reach desktop).
;
; Flag rule: observe SF via js/jns immediately after `call UTF16to8`
; (before any flag-clobbering insn).
align 4
utf16_to_8_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 80

        ; Fill working buffer [ebp-64 .. ebp-1] with sentinel 0xA5
        lea     edi, [ebp-64]
        mov     ecx, 64
        mov     al, 0xA5
        rep stosb

        ; ========== ASCII success: U+0041, ECX=1 ==========
        lea     edi, [ebp-48]
        mov     eax, 0x41
        mov     ecx, 1
        mov     ebx, 0xB1000001
        mov     edx, 0xD1000002
        mov     esi, 0x51000003
        push    edi
        call    UTF16to8
        jns     @f
        jmp     .fail
@@:
        cmp     eax, 0x41
        jne     .fail
        test    ecx, ecx
        jnz     .fail
        pop     eax                    ; original EDI
        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     edx, 0xD1000002
        jne     .fail
        cmp     esi, 0x51000003
        jne     .fail
        mov     ebx, eax
        inc     ebx
        cmp     edi, ebx
        jne     .fail
        cmp     byte [eax], 0x41
        jne     .fail
        cmp     byte [eax+1], 0xA5
        jne     .fail

        ; ========== ASCII exhaust: U+0041, ECX=0 ==========
        lea     edi, [ebp-48]
        mov     byte [edi], 0xA5
        mov     eax, 0x41
        mov     ecx, 0
        mov     ebx, 0xB2000001
        mov     edx, 0xD2000002
        mov     esi, 0x52000003
        push    edi
        call    UTF16to8
        js      @f
        jmp     .fail
@@:
        cmp     eax, 0x41
        jne     .fail
        cmp     ecx, -1
        jne     .fail
        pop     eax
        cmp     edi, eax
        jne     .fail
        cmp     byte [eax], 0xA5
        jne     .fail
        cmp     ebx, 0xB2000001
        jne     .fail
        cmp     edx, 0xD2000002
        jne     .fail
        cmp     esi, 0x52000003
        jne     .fail

        ; ========== 2-byte success: U+0080, ECX=2 ==========
        lea     edi, [ebp-48]
        mov     dword [edi], 0xA5A5A5A5
        mov     eax, 0x80
        mov     ecx, 2
        mov     ebx, 0xB3000001
        mov     edx, 0xD3000002
        mov     esi, 0x53000003
        push    edi
        call    UTF16to8
        jns     @f
        jmp     .fail
@@:
        cmp     eax, 0x000080C2
        jne     .fail
        test    ecx, ecx
        jnz     .fail
        pop     eax
        cmp     ebx, 0xB3000001
        jne     .fail
        cmp     edx, 0xD3000002
        jne     .fail
        cmp     esi, 0x53000003
        jne     .fail
        mov     ebx, eax
        add     ebx, 2
        cmp     edi, ebx
        jne     .fail
        cmp     byte [eax], 0xC2
        jne     .fail
        cmp     byte [eax+1], 0x80
        jne     .fail
        cmp     byte [eax+2], 0xA5
        jne     .fail

        ; ========== 2-byte mid-exhaust: U+0080, ECX=1 ==========
        lea     edi, [ebp-48]
        mov     dword [edi], 0xA5A5A5A5
        mov     eax, 0x80
        mov     ecx, 1
        mov     ebx, 0xB4000001
        mov     edx, 0xD4000002
        mov     esi, 0x54000003
        push    edi
        call    UTF16to8
        js      @f
        jmp     .fail
@@:
        cmp     eax, 0x80
        jne     .fail
        cmp     ecx, -1
        jne     .fail
        pop     eax
        cmp     edi, eax
        jne     .fail
        cmp     dword [eax], 0xA5A5A5A5
        jne     .fail
        cmp     ebx, 0xB4000001
        jne     .fail
        cmp     edx, 0xD4000002
        jne     .fail
        cmp     esi, 0x54000003
        jne     .fail

        ; ========== 3-byte success: U+0800, ECX=3 ==========
        lea     edi, [ebp-48]
        mov     dword [edi], 0xA5A5A5A5
        mov     eax, 0x800
        mov     ecx, 3
        mov     ebx, 0xB5000001
        mov     edx, 0xD5000002
        mov     esi, 0x55000003
        push    edi
        call    UTF16to8
        jns     @f
        jmp     .fail
@@:
        cmp     eax, 0x000080A0
        jne     .fail
        test    ecx, ecx
        jnz     .fail
        pop     eax
        cmp     ebx, 0xB5000001
        jne     .fail
        cmp     edx, 0xD5000002
        jne     .fail
        cmp     esi, 0x55000003
        jne     .fail
        mov     ebx, eax
        add     ebx, 3
        cmp     edi, ebx
        jne     .fail
        cmp     byte [eax], 0xE0
        jne     .fail
        cmp     byte [eax+1], 0xA0
        jne     .fail
        cmp     byte [eax+2], 0x80
        jne     .fail
        cmp     byte [eax+3], 0xA5
        jne     .fail

        ; ========== 3-byte mid-exhaust #1: U+0800, ECX=1 ==========
        lea     edi, [ebp-48]
        mov     dword [edi], 0xA5A5A5A5
        mov     eax, 0x800
        mov     ecx, 1
        push    edi
        call    UTF16to8
        js      @f
        jmp     .fail
@@:
        cmp     eax, 0x800
        jne     .fail
        cmp     ecx, -1
        jne     .fail
        pop     eax
        cmp     edi, eax
        jne     .fail
        cmp     dword [eax], 0xA5A5A5A5
        jne     .fail

        ; ========== 3-byte mid-exhaust #2: U+0800, ECX=2 ==========
        lea     edi, [ebp-48]
        mov     dword [edi], 0xA5A5A5A5
        mov     eax, 0x800
        mov     ecx, 2
        push    edi
        call    UTF16to8
        js      @f
        jmp     .fail
@@:
        cmp     eax, 0x800
        jne     .fail
        cmp     ecx, -1
        jne     .fail
        pop     eax
        cmp     edi, eax
        jne     .fail
        cmp     dword [eax], 0xA5A5A5A5
        jne     .fail

        ; ========== negative budget: U+0041, ECX=-1 ==========
        lea     edi, [ebp-48]
        mov     byte [edi], 0xA5
        mov     eax, 0x41
        mov     ecx, -1
        mov     ebx, 0xB6000001
        mov     edx, 0xD6000002
        mov     esi, 0x56000003
        push    edi
        call    UTF16to8
        js      @f
        jmp     .fail
@@:
        cmp     eax, 0x41
        jne     .fail
        cmp     ecx, -2
        jne     .fail
        pop     eax
        cmp     edi, eax
        jne     .fail
        cmp     byte [eax], 0xA5
        jne     .fail
        cmp     ebx, 0xB6000001
        jne     .fail
        cmp     edx, 0xD6000002
        jne     .fail
        cmp     esi, 0x56000003
        jne     .fail

        ; ========== INT_MIN: U+0041, ECX=0x80000000 ==========
        lea     edi, [ebp-48]
        mov     byte [edi], 0xA5
        mov     byte [edi+1], 0xA5
        mov     eax, 0x41
        mov     ecx, 0x80000000
        mov     ebx, 0xB7000001
        mov     edx, 0xD7000002
        mov     esi, 0x57000003
        push    edi
        call    UTF16to8
        jns     @f
        jmp     .fail
@@:
        cmp     eax, 0x41
        jne     .fail
        cmp     ecx, 0x7FFFFFFF
        jne     .fail
        pop     eax
        cmp     ebx, 0xB7000001
        jne     .fail
        cmp     edx, 0xD7000002
        jne     .fail
        cmp     esi, 0x57000003
        jne     .fail
        mov     ebx, eax
        inc     ebx
        cmp     edi, ebx
        jne     .fail
        cmp     byte [eax], 0x41
        jne     .fail
        cmp     byte [eax+1], 0xA5
        jne     .fail

        ; ========== surrogate: U+D800, ECX=3 ==========
        lea     edi, [ebp-48]
        mov     dword [edi], 0xA5A5A5A5
        mov     eax, 0xD800
        mov     ecx, 3
        mov     ebx, 0xB8000001
        mov     edx, 0xD8000002
        mov     esi, 0x58000003
        push    edi
        call    UTF16to8
        jns     @f
        jmp     .fail
@@:
        cmp     eax, 0x000080A0
        jne     .fail
        test    ecx, ecx
        jnz     .fail
        pop     eax
        cmp     ebx, 0xB8000001
        jne     .fail
        cmp     edx, 0xD8000002
        jne     .fail
        cmp     esi, 0x58000003
        jne     .fail
        mov     ebx, eax
        add     ebx, 3
        cmp     edi, ebx
        jne     .fail
        cmp     byte [eax], 0xED
        jne     .fail
        cmp     byte [eax+1], 0xA0
        jne     .fail
        cmp     byte [eax+2], 0x80
        jne     .fail
        cmp     byte [eax+3], 0xA5
        jne     .fail

        ; ========== EAX residues ==========
        lea     edi, [ebp-48]
        mov     eax, 0x07FF
        mov     ecx, 2
        call    UTF16to8
        jns     @f
        jmp     .fail
@@:
        cmp     eax, 0x0000BFDF
        jne     .fail

        lea     edi, [ebp-48]
        mov     dword [edi], 0xA5A5A5A5
        mov     eax, 0xFFFF
        mov     ecx, 3
        call    UTF16to8
        jns     @f
        jmp     .fail
@@:
        cmp     eax, 0x0000BFBF
        jne     .fail
        cmp     byte [ebp-48], 0xEF
        jne     .fail
        cmp     byte [ebp-47], 0xBF
        jne     .fail
        cmp     byte [ebp-46], 0xBF
        jne     .fail
        cmp     byte [ebp-45], 0xA5
        jne     .fail

        mov     [rust_utf16_to_8_smoke_result], 0x38544655 ; 'UTF8'
        mov     esp, ebp
        pop     ebp
        ret

.fail:
        mov     eax, 0xDEAD0C51         ; Cut Q fail marker
        mov     ebx, 0x38544655         ; 'UTF8'
        mov     ecx, 0x4641494C         ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_utf16_to_8(ch, dest_inout, ecx_inout) -> packed (SF<<31)|EAX; ret 12
align 16
rust_utf16_to_8:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_utf16_to_8.bin'
