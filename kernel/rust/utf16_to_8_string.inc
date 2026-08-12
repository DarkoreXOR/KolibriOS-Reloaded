;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; UTF16to8_string — Rust embed + ABI smoke (Cut BP)
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_utf16_to_8_string_smoke_result dd 0

utf16_to_8_string_smoke_src:
        dw 'H', 'i', 0
utf16_to_8_string_smoke_buf:
        rb 64
endg

align 4
utf16_to_8_string_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 32

        ; ========== NUL success: 'Hi' + UTF-16 NUL ==========
        lea     edi, [utf16_to_8_string_smoke_buf]
        mov     ecx, 64
        mov     al, 0xA5
        rep stosb
        lea     esi, [utf16_to_8_string_smoke_src]
        lea     edi, [utf16_to_8_string_smoke_buf]
        mov     ecx, 16
        mov     ebx, 0xB1000001
        mov     edx, 0xD1000002
        call    UTF16to8_string
        jns     @f
        jmp     .fail
@@:
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     edx, 0xD1000002
        jne     .fail
        cmp     esi, utf16_to_8_string_smoke_src + 6
        jne     .fail
        cmp     edi, utf16_to_8_string_smoke_buf + 3
        jne     .fail
        cmp     ecx, 13
        jne     .fail
        cmp     byte [utf16_to_8_string_smoke_buf], 'H'
        jne     .fail
        cmp     byte [utf16_to_8_string_smoke_buf+1], 'i'
        jne     .fail
        cmp     byte [utf16_to_8_string_smoke_buf+2], 0
        jne     .fail
        cmp     byte [utf16_to_8_string_smoke_buf+3], 0xA5
        jne     .fail

        ; ========== Overflow: 'A','A',0 with ECX=1 ==========
        lea     edi, [utf16_to_8_string_smoke_buf]
        mov     ecx, 64
        mov     al, 0xA5
        rep stosb
        lea     esi, [utf16_to_8_string_smoke_src]
        mov     word [esi], 0x41
        mov     word [esi+2], 0x41
        mov     word [esi+4], 0
        lea     edi, [utf16_to_8_string_smoke_buf]
        mov     ecx, 1
        mov     ebx, 0xB2000001
        mov     edx, 0xD2000002
        call    UTF16to8_string
        js      @f
        jmp     .fail
@@:
        cmp     eax, 0x41
        jne     .fail
        cmp     ebx, 0xB2000001
        jne     .fail
        cmp     edx, 0xD2000002
        jne     .fail
        cmp     esi, utf16_to_8_string_smoke_src + 4
        jne     .fail
        cmp     edi, utf16_to_8_string_smoke_buf + 1
        jne     .fail
        cmp     ecx, -1
        jne     .fail
        cmp     byte [utf16_to_8_string_smoke_buf], 'A'
        jne     .fail
        cmp     byte [utf16_to_8_string_smoke_buf+1], 0xA5
        jne     .fail

        mov     [rust_utf16_to_8_string_smoke_result], 0x55544250 ; 'UTBP'
        mov     esp, ebp
        pop     ebp
        ret

.fail:
        mov     eax, 0xDEAD0C52
        mov     ebx, 0x55544250
        mov     ecx, 0x4641494C
        jmp     $

align 16
rust_utf16_to_8_string:
        file '../../rust_kernel/kolibri_utils/out/rust_utf16_to_8_string.bin'
