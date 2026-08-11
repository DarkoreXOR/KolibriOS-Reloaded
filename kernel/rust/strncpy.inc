;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; strncpy — Rust embed + ABI smoke (Cut BF)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_strncpy).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_strncpy.bin
;;
;; Requires USE_RUST_STRNCPY=1 so public strncpy hits the trampoline
;; (not the FASM body).
;;
;; Critical ABI: stdcall(s1,s2,n) → EAX=s1; ECX/EDX clobbered (legacy);
;; preserve ESI/EDI/EBX/EBP; cld (DF). REG-003: synthetic buffers only.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust strncpy path ran.
rust_strncpy_smoke_result dd 0
strncpy_smoke_src_short:
        db 'short',0
strncpy_smoke_src_empty:
        db 0
strncpy_smoke_src_long:
        db '0123456789ABCDEF',0
; Writable destinations (initialized each smoke run).
strncpy_smoke_dst:
        rb 32
strncpy_smoke_dst2:
        rb 16
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on synthetic buffers only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
strncpy_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: pad after early NUL (n=8) ---
        mov     edi, strncpy_smoke_dst
        mov     ecx, 32
        mov     al, 0xA5
        rep stosb
        stdcall rust_strncpy, strncpy_smoke_dst, strncpy_smoke_src_short, 8
        cmp     eax, strncpy_smoke_dst
        jne     .fail
        cmp     dword [strncpy_smoke_dst], 'shor'
        jne     .fail
        cmp     byte [strncpy_smoke_dst+4], 't'
        jne     .fail
        cmp     byte [strncpy_smoke_dst+5], 0
        jne     .fail
        cmp     byte [strncpy_smoke_dst+6], 0
        jne     .fail
        cmp     byte [strncpy_smoke_dst+7], 0
        jne     .fail
        ; byte past n untouched
        cmp     byte [strncpy_smoke_dst+8], 0xA5
        jne     .fail

        ; --- Direct rust_*: n=0 leaves destination unchanged ---
        mov     byte [strncpy_smoke_dst], 0x5A
        stdcall rust_strncpy, strncpy_smoke_dst, strncpy_smoke_src_short, 0
        cmp     eax, strncpy_smoke_dst
        jne     .fail
        cmp     byte [strncpy_smoke_dst], 0x5A
        jne     .fail

        ; --- Direct rust_*: trunc without NUL in window (n=4 of longer) ---
        mov     edi, strncpy_smoke_dst2
        mov     ecx, 16
        mov     al, 0xA5
        rep stosb
        stdcall rust_strncpy, strncpy_smoke_dst2, strncpy_smoke_src_long, 4
        cmp     eax, strncpy_smoke_dst2
        jne     .fail
        cmp     dword [strncpy_smoke_dst2], '0123'
        jne     .fail
        cmp     byte [strncpy_smoke_dst2+4], 0xA5
        jne     .fail

        ; --- Direct rust_*: empty source pads all n ---
        stdcall rust_strncpy, strncpy_smoke_dst2, strncpy_smoke_src_empty, 4
        cmp     dword [strncpy_smoke_dst2], 0
        jne     .fail

        ; --- Public trampoline: ESI/EDI/EBX/EBP canaries ---
        ; ECX/EDX are FASM-clobbered — do NOT require preserve (Cut BE lesson).
        mov     edi, strncpy_smoke_dst
        mov     ecx, 32
        mov     al, 0xA5
        rep stosb
        mov     ebx, 0xB1B1B1B1
        mov     ecx, 0xC2C2C2C2
        mov     edx, 0xD3D3D3D3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        stdcall strncpy, strncpy_smoke_dst, strncpy_smoke_src_short, 8
        cmp     eax, strncpy_smoke_dst
        jne     .fail
        cmp     ebx, 0xB1B1B1B1
        jne     .fail
        ; ECX/EDX intentionally unchecked (legacy clobber)
        cmp     esi, 0xE4E4E4E4
        jne     .fail
        cmp     edi, 0xF5F5F5F5
        jne     .fail
        cmp     ebp, 0xA6A6A6A6
        jne     .fail
        cmp     dword [strncpy_smoke_dst], 'shor'
        jne     .fail
        cmp     byte [strncpy_smoke_dst+5], 0
        jne     .fail

        ; --- shmem-like 31-byte window ---
        mov     edi, strncpy_smoke_dst
        mov     ecx, 32
        mov     al, 0xA5
        rep stosb
        stdcall strncpy, strncpy_smoke_dst, strncpy_smoke_src_short, 31
        cmp     eax, strncpy_smoke_dst
        jne     .fail
        cmp     byte [strncpy_smoke_dst+5], 0
        jne     .fail
        cmp     byte [strncpy_smoke_dst+30], 0
        jne     .fail
        cmp     byte [strncpy_smoke_dst+31], 0xA5
        jne     .fail

        mov     [rust_strncpy_smoke_result], 'SNCP'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_strncpy_smoke_result], 'DEAD'
        ; Distinctive hang marker for Cut BF
@@:
        cli
        hlt
        jmp     @b

; ---------------------------------------------------------------------------
; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
; ---------------------------------------------------------------------------
align 4
rust_strncpy:
        file '../../rust_kernel/kolibri_utils/out/rust_strncpy.bin'
