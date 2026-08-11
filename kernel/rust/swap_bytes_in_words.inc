;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; swap_bytes_in_words — Rust embed + ABI smoke (Cut BG)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_swap_bytes_in_words).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_swap_bytes_in_words.bin
;;
;; Requires USE_RUST_SWAP_BYTES_IN_WORDS=1 so public swap_bytes_in_words hits
;; the trampoline (not the FASM body).
;;
;; Critical ABI: stdcall(base,len); preserves EAX/EBX/ECX (legacy push/pop)
;; and EDX/ESI/EDI/EBP (legacy untouched). REG-003: synthetic buffer only.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust swap_bytes_in_words path ran.
rust_swap_bytes_in_words_smoke_result dd 0
; Writable synthetic word buffer (never touches live AHCI modelstr).
align 4
swap_bytes_smoke_buf:
        rw 32
endg

; Called once after ahci_init (with ahci_find_cmdslot smoke).
; Exercises rust_* + public trampoline on synthetic buffers only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
swap_bytes_in_words_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: two words 0x1234 0xABCD → 0x3412 0xCDAB ---
        mov     word [swap_bytes_smoke_buf], 0x1234
        mov     word [swap_bytes_smoke_buf+2], 0xABCD
        mov     word [swap_bytes_smoke_buf+4], 0x5A5A ; sentinel past len
        stdcall rust_swap_bytes_in_words, swap_bytes_smoke_buf, 2
        cmp     word [swap_bytes_smoke_buf], 0x3412
        jne     .fail
        cmp     word [swap_bytes_smoke_buf+2], 0xCDAB
        jne     .fail
        cmp     word [swap_bytes_smoke_buf+4], 0x5A5A
        jne     .fail

        ; --- Direct rust_*: len=0 leaves buffer unchanged ---
        mov     word [swap_bytes_smoke_buf], 0xBEEF
        stdcall rust_swap_bytes_in_words, swap_bytes_smoke_buf, 0
        cmp     word [swap_bytes_smoke_buf], 0xBEEF
        jne     .fail

        ; --- Direct rust_*: single word 0x00FF → 0xFF00 ---
        mov     word [swap_bytes_smoke_buf], 0x00FF
        stdcall rust_swap_bytes_in_words, swap_bytes_smoke_buf, 1
        cmp     word [swap_bytes_smoke_buf], 0xFF00
        jne     .fail

        ; --- Public trampoline: ATA-model-sized 20 words + full canaries ---
        ; Fill 20 words with pattern (i << 8) | (0x40+i)
        xor     ecx, ecx
.fill20:
        cmp     ecx, 20
        jae     .filled20
        mov     eax, ecx
        mov     ah, al
        add     al, 0x40
        mov     word [swap_bytes_smoke_buf + ecx*2], ax
        inc     ecx
        jmp     .fill20
.filled20:
        mov     word [swap_bytes_smoke_buf + 40], 0xA5A5 ; sentinel

        mov     eax, 0xA0A0A0A0
        mov     ebx, 0xB1B1B1B1
        mov     ecx, 0xC2C2C2C2
        mov     edx, 0xD3D3D3D3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        stdcall swap_bytes_in_words, swap_bytes_smoke_buf, 20
        ; EAX restored (legacy push/pop) — must match pre-call canary
        cmp     eax, 0xA0A0A0A0
        jne     .fail
        cmp     ebx, 0xB1B1B1B1
        jne     .fail
        cmp     ecx, 0xC2C2C2C2
        jne     .fail
        cmp     edx, 0xD3D3D3D3
        jne     .fail
        cmp     esi, 0xE4E4E4E4
        jne     .fail
        cmp     edi, 0xF5F5F5F5
        jne     .fail
        cmp     ebp, 0xA6A6A6A6
        jne     .fail

        ; Verify first/last swapped words + sentinel
        ; word0 was 0x0040 → 0x4000
        cmp     word [swap_bytes_smoke_buf], 0x4000
        jne     .fail
        ; word19 was 0x1353 (ah=19, al=0x40+19=0x53) → 0x5313
        cmp     word [swap_bytes_smoke_buf + 38], 0x5313
        jne     .fail
        cmp     word [swap_bytes_smoke_buf + 40], 0xA5A5
        jne     .fail

        mov     [rust_swap_bytes_in_words_smoke_result], 'SWBW'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_swap_bytes_in_words_smoke_result], 'DEAD'
        ; Distinctive hang marker for Cut BG
@@:
        cli
        hlt
        jmp     @b

; ---------------------------------------------------------------------------
; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
; ---------------------------------------------------------------------------
align 4
rust_swap_bytes_in_words:
        file '../../rust_kernel/kolibri_utils/out/rust_swap_bytes_in_words.bin'
