;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; hotkey_do_test — Rust embed + ABI smoke (Cut BE)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_hotkey_do_test).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_hotkey_do_test.bin
;;
;; Requires USE_RUST_HOTKEY_DO_TEST=1 so public hotkey_do_test hits
;; the trampoline (not the FASM body).
;;
;; Critical ABI: EAX→hotkey node in, CF out; preserve EAX/EBX/ECX/EDX
;; (REG-001 — hotkey list walk keeps node in EAX).
;; REG-003: smoke uses stack synthetic node + save/restore kb_state —
;; never mutate live hotkey_list / hotkey_scancodes.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust hotkey_do_test path ran.
rust_hotkey_do_test_smoke_result dd 0
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on a stack scratch node only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
hotkey_do_test_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 16

        ; Synthetic hotkey node at [ebp-16]: +0 next, +4 funcs, +8 slot, +12 link
        push    edi ecx eax
        lea     edi, [ebp-16]
        mov     ecx, 16/4
        xor     eax, eax
        rep stosd
        pop     eax ecx edi

        ; Save live kb_state (REG-003 — restore before return)
        push    dword [kb_state]

        lea     ebx, [ebp-16]

        ; funcs = 0x00000210 → n0=0 (test0), n1=1 (test1), n2=2 (test2)
        mov     dword [ebx+4], 0x00000210

        ; --- Direct rust_*: CL=0, kb=0, test0 on field0=0 → pass (0) ---
        stdcall rust_hotkey_do_test, 0x00000210, 0, 0
        test    eax, eax
        jnz     .fail_restore

        ; --- Direct rust_*: CL=0, kb=1, test0 on field0=1 → fail ---
        stdcall rust_hotkey_do_test, 0x00000210, 1, 0
        cmp     eax, 1
        jne     .fail_restore

        ; --- Direct rust_*: CL=2, kb=4 (ctrl=1), test1 parity → pass ---
        stdcall rust_hotkey_do_test, 0x00000210, 4, 2
        test    eax, eax
        jnz     .fail_restore

        ; --- Direct rust_*: CL=4, kb=0x30 (alt=3), test2 both → pass ---
        stdcall rust_hotkey_do_test, 0x00000210, 0x30, 4
        test    eax, eax
        jnz     .fail_restore

        ; --- Direct rust_*: out-of-range nibble 5 → fail ---
        stdcall rust_hotkey_do_test, 5, 0, 0
        cmp     eax, 1
        jne     .fail_restore

        ; --- Public path: CF clear + EAX/EBX/ESI/EDI canaries ---
        ; Note: legacy FASM clobbers EDX (and doubles CL). Trampoline may
        ; preserve more; do not assert EDX (OFF-gate must pass too).
        mov     [kb_state], 0                   ; field0=0 → test0 pass
        mov     eax, ebx                        ; node
        mov     ecx, 0                          ; CL=0
        mov     esi, 0x51515151
        mov     edi, 0xE5E5E5E5
        push    ebx
        mov     ebx, 0xB2B2B2B2
        call    hotkey_do_test
        jc      .fail_pop                       ; must pass (CF clear)
        cmp     eax, [esp]                      ; EAX must remain node
        jne     .fail_pop
        cmp     ebx, 0xB2B2B2B2
        jne     .fail_pop
        cmp     esi, 0x51515151
        jne     .fail_pop
        cmp     edi, 0xE5E5E5E5
        jne     .fail_pop
        pop     ebx

        ; --- Public: CL=0, kb=1 → CF set (fail); EAX preserved ---
        mov     [kb_state], 1
        mov     eax, ebx
        mov     ecx, 0xC1000000                 ; CL=0 + canary high
        call    hotkey_do_test
        jnc     .fail_restore                   ; must fail (CF set)
        cmp     eax, ebx
        jne     .fail_restore

        ; --- Public: CL=4, alt both → pass ---
        mov     [kb_state], 0x30
        mov     eax, ebx
        mov     ecx, 4
        call    hotkey_do_test
        jc      .fail_restore
        cmp     eax, ebx
        jne     .fail_restore

        ; Restore kb_state
        pop     dword [kb_state]

        mov     [rust_hotkey_do_test_smoke_result], 0x54444B48 ; 'HKDT'
        mov     esp, ebp
        pop     ebp
        ret

.fail_pop:
        pop     ebx
.fail_restore:
        ; Best-effort restore even on fail path (stack may still hold saved kb)
        mov     eax, [esp]
        mov     [kb_state], eax
.fail:
        mov     eax, 0xDEAD0CBE                ; Cut BE fail marker
        mov     ebx, 0x54444B48                ; 'HKDT'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_hotkey_do_test(funcs, kb_state, cl) -> EAX; ret 12
align 16
rust_hotkey_do_test:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_hotkey_do_test.bin'
