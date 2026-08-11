;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; strlen — Rust embed + ABI smoke (Cut BH)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_strlen).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_strlen.bin
;;
;; Requires USE_RUST_STRLEN=1 so public strlen hits the trampoline
;; (not the FASM body).
;;
;; Critical ABI: register ESI→string in; ECX=length out; plain ret.
;; Preserve EAX/EBX/EDX/ESI/EDI/EBP (legacy restores EAX/EDI; leaves
;; ESI/EBX/EDX/EBP alone). DF unchanged (legacy has no cld).
;; REG-003: synthetic C strings only.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust strlen path ran.
rust_strlen_smoke_result dd 0
strlen_smoke_empty:
        db 0
strlen_smoke_short:
        db 'abc',0
strlen_smoke_path:
        db '/sys/lib/name.obj',0
; Marker for hang diagnostics
strlen_smoke_marker:
        db 'STRL',0
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on synthetic strings only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
strlen_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: empty → 0 ---
        stdcall rust_strlen, strlen_smoke_empty
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: "abc" → 3 ---
        stdcall rust_strlen, strlen_smoke_short
        cmp     eax, 3
        jne     .fail

        ; --- Direct rust_*: path length ---
        stdcall rust_strlen, strlen_smoke_path
        cmp     eax, 17
        jne     .fail

        ; --- Public trampoline: empty + full canaries ---
        mov     eax, 0xA0A0A0A0
        mov     ebx, 0xB1B1B1B1
        mov     edx, 0xD3D3D3D3
        mov     esi, strlen_smoke_empty
        mov     edi, 0xE4E4E4E4
        mov     ebp, 0xF5F5F5F5
        pushf
        pop     ecx
        and     ecx, not 400h          ; clear DF in saved flags image
        push    ecx
        popf                           ; known DF=0 entering leaf
        call    strlen
        cmp     ecx, 0
        jne     .fail
        cmp     eax, 0xA0A0A0A0
        jne     .fail
        cmp     ebx, 0xB1B1B1B1
        jne     .fail
        cmp     edx, 0xD3D3D3D3
        jne     .fail
        cmp     esi, strlen_smoke_empty
        jne     .fail
        cmp     edi, 0xE4E4E4E4
        jne     .fail
        cmp     ebp, 0xF5F5F5F5
        jne     .fail
        pushf
        pop     eax
        test    eax, 400h              ; DF must still be clear
        jnz     .fail

        ; --- Public trampoline: "abc" → ECX=3 ---
        mov     eax, 0x11111111
        mov     ebx, 0x22222222
        mov     edx, 0x33333333
        mov     esi, strlen_smoke_short
        mov     edi, 0x44444444
        mov     ebp, 0x55555555
        call    strlen
        cmp     ecx, 3
        jne     .fail
        cmp     eax, 0x11111111
        jne     .fail
        cmp     ebx, 0x22222222
        jne     .fail
        cmp     edx, 0x33333333
        jne     .fail
        cmp     esi, strlen_smoke_short
        jne     .fail
        cmp     edi, 0x44444444
        jne     .fail
        cmp     ebp, 0x55555555
        jne     .fail

        ; --- Public trampoline: DF=1 preserved (empty string: one scasb hits NUL
        ;     regardless of DF, so OFF/ON both safe); length still 0 ---
        mov     eax, 0x66666666
        mov     ebx, 0x77777777
        mov     edx, 0x88888888
        mov     esi, strlen_smoke_empty
        mov     edi, 0x99999999
        mov     ebp, 0xAAAAAAAA
        std                            ; set DF — leaf must not clear it
        call    strlen
        pushf
        pop     ecx
        cld                            ; restore DF for rest of kernel init
        test    ecx, 400h              ; DF still set across call
        jz      .fail
        ; ECX was flags — re-call for length with DF cleared (already cld)
        mov     eax, 0x66666666
        mov     esi, strlen_smoke_empty
        call    strlen
        test    ecx, ecx
        jnz     .fail
        cmp     eax, 0x66666666
        jne     .fail
        cmp     ebx, 0x77777777
        jne     .fail
        cmp     edx, 0x88888888
        jne     .fail
        cmp     esi, strlen_smoke_empty
        jne     .fail
        cmp     edi, 0x99999999
        jne     .fail
        cmp     ebp, 0xAAAAAAAA
        jne     .fail

        ; --- Public trampoline: path → 17 ---
        mov     eax, 0xBBBBBBBB
        mov     ebx, 0xCCCCCCCC
        mov     edx, 0xDDDDDDDD
        mov     esi, strlen_smoke_path
        mov     edi, 0xEEEEEEEE
        mov     ebp, 0xFFFFFFFF
        call    strlen
        cmp     ecx, 17
        jne     .fail
        cmp     eax, 0xBBBBBBBB
        jne     .fail
        cmp     ebx, 0xCCCCCCCC
        jne     .fail
        cmp     edx, 0xDDDDDDDD
        jne     .fail
        cmp     esi, strlen_smoke_path
        jne     .fail
        cmp     edi, 0xEEEEEEEE
        jne     .fail
        cmp     ebp, 0xFFFFFFFF
        jne     .fail

        mov     [rust_strlen_smoke_result], 'STRL'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        ; Distinctive hang: marker in result + EAX, do not reach desktop.
        mov     [rust_strlen_smoke_result], 'DEAD'
        mov     eax, 'STRL'
        jmp     $

; ---------------------------------------------------------------------------
align 4
rust_strlen:
        file '../../rust_kernel/kolibri_utils/out/rust_strlen.bin'
