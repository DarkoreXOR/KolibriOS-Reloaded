;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; strrchr — Rust embed + ABI smoke (Cut BB)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_strrchr).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_strrchr.bin
;;
;; Requires USE_RUST_STRRCHR=1 so public strrchr hits the trampoline
;; (not the FASM body).
;;
;; Critical ABI: stdcall(s,c) → EAX; preserve EDX (REG-001); cld (DF).
;; Pure read — synthetic iglobal strings only (REG-003).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust strrchr path ran.
rust_strrchr_smoke_result dd 0
strrchr_smoke_path:
        db '/sys/example',0
strrchr_smoke_noslash:
        db 'example',0
strrchr_smoke_empty:
        db 0
strrchr_smoke_multi:
        db '/a/b/c',0
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on synthetic strings only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
strrchr_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: last '/' in path ---
        stdcall rust_strrchr, strrchr_smoke_path, '/'
        cmp     eax, strrchr_smoke_path + 4   ; "/sys/example" → '/' before example
        jne     .fail

        ; --- Direct rust_*: no match ---
        stdcall rust_strrchr, strrchr_smoke_noslash, '/'
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: empty + NUL needle → pointer to NUL ---
        stdcall rust_strrchr, strrchr_smoke_empty, 0
        cmp     eax, strrchr_smoke_empty
        jne     .fail

        ; --- Direct rust_*: multi-slash → last ---
        stdcall rust_strrchr, strrchr_smoke_multi, '/'
        cmp     eax, strrchr_smoke_multi + 4  ; "/a/b/c" last '/'
        jne     .fail

        ; --- Public trampoline: EDX (+siblings) canaries ---
        mov     ebx, 0xB1B1B1B1
        mov     ecx, 0xC2C2C2C2
        mov     edx, 0xD3D3D3D3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        stdcall strrchr, strrchr_smoke_path, '/'
        cmp     eax, strrchr_smoke_path + 4
        jne     .fail
        cmp     ebx, 0xB1B1B1B1
        jne     .fail
        ; ECX is FASM-clobbered — do not require preserve
        cmp     edx, 0xD3D3D3D3
        jne     .fail
        cmp     esi, 0xE4E4E4E4
        jne     .fail
        cmp     edi, 0xF5F5F5F5
        jne     .fail
        cmp     ebp, 0xA6A6A6A6
        jne     .fail

        ; --- Wide c truncates to low byte ('/') ---
        stdcall strrchr, strrchr_smoke_multi, 0xAB00_002F
        cmp     eax, strrchr_smoke_multi + 4
        jne     .fail

        mov     [rust_strrchr_smoke_result], 'STRR'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_strrchr_smoke_result], 'DEAD'
        ; Distinctive hang marker for Cut BB
@@:
        cli
        hlt
        jmp     @b

; ---------------------------------------------------------------------------
; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
; ---------------------------------------------------------------------------
align 4
rust_strrchr:
        file '../../rust_kernel/kolibri_utils/out/rust_strrchr.bin'
