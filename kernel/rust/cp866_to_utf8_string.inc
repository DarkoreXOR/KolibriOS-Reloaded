;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; cp866toUTF8_string — Rust embed + ABI smoke (Cut BQ)
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_cp866_to_utf8_string_smoke_result dd 0

cp866_to_utf8_string_smoke_src:
        db 'H', 'i', 0
cp866_to_utf8_string_smoke_buf:
        rb 64
endg

align 4
cp866_to_utf8_string_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 32

        ; ========== NUL success: 'Hi' + CP866 NUL ==========
        lea     edi, [cp866_to_utf8_string_smoke_buf]
        mov     ecx, 64
        mov     al, 0xA5
        rep stosb
        lea     esi, [cp866_to_utf8_string_smoke_src]
        lea     edi, [cp866_to_utf8_string_smoke_buf]
        mov     ecx, 16
        mov     ebx, 0xB1000001
        mov     edx, 0xD1000002
        call    cp866toUTF8_string
        jns     @f
        jmp     .fail
@@:
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     edx, 0xD1000002
        jne     .fail
        cmp     esi, cp866_to_utf8_string_smoke_src + 3
        jne     .fail
        cmp     edi, cp866_to_utf8_string_smoke_buf + 3
        jne     .fail
        cmp     ecx, 13
        jne     .fail
        cmp     byte [cp866_to_utf8_string_smoke_buf], 'H'
        jne     .fail
        cmp     byte [cp866_to_utf8_string_smoke_buf+1], 'i'
        jne     .fail
        cmp     byte [cp866_to_utf8_string_smoke_buf+2], 0
        jne     .fail
        cmp     byte [cp866_to_utf8_string_smoke_buf+3], 0xA5
        jne     .fail

        ; ========== Overflow: 'A','A',0 with ECX=1 ==========
        lea     edi, [cp866_to_utf8_string_smoke_buf]
        mov     ecx, 64
        mov     al, 0xA5
        rep stosb
        lea     esi, [cp866_to_utf8_string_smoke_src]
        mov     byte [esi], 'A'
        mov     byte [esi+1], 'A'
        mov     byte [esi+2], 0
        lea     edi, [cp866_to_utf8_string_smoke_buf]
        mov     ecx, 1
        mov     ebx, 0xB2000001
        mov     edx, 0xD2000002
        call    cp866toUTF8_string
        js      @f
        jmp     .fail
@@:
        cmp     eax, 0x41
        jne     .fail
        cmp     ebx, 0xB2000001
        jne     .fail
        cmp     edx, 0xD2000002
        jne     .fail
        cmp     esi, cp866_to_utf8_string_smoke_src + 2
        jne     .fail
        cmp     edi, cp866_to_utf8_string_smoke_buf + 1
        jne     .fail
        cmp     ecx, -1
        jne     .fail
        cmp     byte [cp866_to_utf8_string_smoke_buf], 'A'
        jne     .fail
        cmp     byte [cp866_to_utf8_string_smoke_buf+1], 0xA5
        jne     .fail

        mov     [rust_cp866_to_utf8_string_smoke_result], 0x55544251 ; 'UTBQ'
        mov     esp, ebp
        pop     ebp
        ret

.fail:
        mov     eax, 0xDEAD0C52
        mov     ebx, 0x55544251
        mov     ecx, 0x4641494C
        jmp     $

align 16
rust_cp866_to_utf8_string:
        file '../../rust_kernel/kolibri_utils/out/rust_cp866_to_utf8_string.bin'
