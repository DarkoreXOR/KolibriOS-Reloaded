;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; get_proc_ex — Rust embed + ABI smoke (Cut CG)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_get_proc_ex).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_get_proc_ex.bin
;;
;; Requires USE_RUST_GET_PROC_EX=1 so public get_proc_ex hits the
;; trampoline (not the FASM body). Independent of Cuts AT/BU.
;;
;; Critical ABI: stdcall(proc_name, imports) → EAX; ret 8.
;; Trampoline preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001/011).
;; rust_get_proc_ex ends `ret 8`; never add esp for those args (REG-009).
;; Smoke builds a synthetic PE export directory with RVAs = addr - OS_BASE.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust get_proc_ex path ran.
rust_get_proc_ex_smoke_result dd 0
; Lookup names (NUL-terminated).
get_proc_ex_smoke_name_foo:
        db 'Foo',0
get_proc_ex_smoke_name_bar:
        db 'Bar',0
get_proc_ex_smoke_name_miss:
        db 'Baz',0
endg

uglobal
align 4
; Synthetic IMAGE_EXPORT_DIRECTORY-ish block (40 bytes used).
; +24 NumberOfNames, +28 AddressOfFunctions RVA, +32 AddressOfNames RVA
get_proc_ex_smoke_exp:      rb 40
get_proc_ex_smoke_names:    rd 2        ; RVAs of name strings
get_proc_ex_smoke_funcs:    rd 2        ; RVAs of "functions" (here: markers)
; Fake function markers (absolute VAs used as identity via RVA = VA - OS_BASE)
get_proc_ex_smoke_fn_foo:   dd 0x60001000
get_proc_ex_smoke_fn_bar:   dd 0x60002000
endg

; Called once from high_code when USE_RUST_GET_PROC_EX=1.
; Exercises: null imports, hit first/second, miss, register canaries,
; public trampoline + direct rust_*.
; On mismatch: distinctive hang (does not reach desktop).
align 4
get_proc_ex_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Build synthetic export directory (RVAs = addr - OS_BASE) ---
        mov     edi, get_proc_ex_smoke_exp
        mov     ecx, 40/4
        xor     eax, eax
        rep     stosd

        mov     dword [get_proc_ex_smoke_exp + 24], 2          ; NumberOfNames
        mov     eax, get_proc_ex_smoke_funcs
        sub     eax, OS_BASE
        mov     [get_proc_ex_smoke_exp + 28], eax              ; AddressOfFunctions
        mov     eax, get_proc_ex_smoke_names
        sub     eax, OS_BASE
        mov     [get_proc_ex_smoke_exp + 32], eax              ; AddressOfNames

        mov     eax, get_proc_ex_smoke_name_foo
        sub     eax, OS_BASE
        mov     [get_proc_ex_smoke_names], eax
        mov     eax, get_proc_ex_smoke_name_bar
        sub     eax, OS_BASE
        mov     [get_proc_ex_smoke_names + 4], eax

        mov     eax, get_proc_ex_smoke_fn_foo
        sub     eax, OS_BASE
        mov     [get_proc_ex_smoke_funcs], eax
        mov     eax, get_proc_ex_smoke_fn_bar
        sub     eax, OS_BASE
        mov     [get_proc_ex_smoke_funcs + 4], eax

        ; --- Direct rust_*: imports=0 → 0 ---
        stdcall rust_get_proc_ex, get_proc_ex_smoke_name_foo, 0
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: hit Foo ---
        stdcall rust_get_proc_ex, get_proc_ex_smoke_name_foo, get_proc_ex_smoke_exp
        cmp     eax, get_proc_ex_smoke_fn_foo
        jne     .fail

        ; --- Public trampoline: hit Bar + preserve canaries ---
        mov     ebx, 0xB7B70002
        mov     ecx, 0xC7C70001
        mov     edx, 0xD7D70003
        mov     esi, 0x51510005
        mov     edi, 0xE5E50006
        mov     ebp, 0xF5F50007
        stdcall get_proc_ex, get_proc_ex_smoke_name_bar, get_proc_ex_smoke_exp
        cmp     eax, get_proc_ex_smoke_fn_bar
        jne     .fail
        cmp     ebx, 0xB7B70002
        jne     .fail
        cmp     ecx, 0xC7C70001
        jne     .fail
        cmp     edx, 0xD7D70003
        jne     .fail
        cmp     esi, 0x51510005
        jne     .fail
        cmp     edi, 0xE5E50006
        jne     .fail
        cmp     ebp, 0xF5F50007
        jne     .fail

        ; --- Miss + preserve canaries ---
        mov     ebx, 0xB8B80012
        mov     esi, 0x51880015
        mov     edi, 0xE8E80016
        mov     ebp, 0xF8F80017
        stdcall get_proc_ex, get_proc_ex_smoke_name_miss, get_proc_ex_smoke_exp
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB8B80012
        jne     .fail
        cmp     esi, 0x51880015
        jne     .fail
        cmp     edi, 0xE8E80016
        jne     .fail
        cmp     ebp, 0xF8F80017
        jne     .fail

        mov     [rust_get_proc_ex_smoke_result], 0x47504558 ; 'GPEX'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C60                ; Cut CG fail marker
        mov     ebx, 0x47504558                ; 'GPEX'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_get_proc_ex(proc_name, imports) -> EAX; ret 8
align 16
rust_get_proc_ex:
        file '../../rust_kernel/kolibri_utils/out/rust_get_proc_ex.bin'
