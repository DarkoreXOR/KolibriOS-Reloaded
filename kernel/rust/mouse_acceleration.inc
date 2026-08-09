;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; mouse_acceleration — Rust embed + runtime smoke (Cut L)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_mouse_acceleration).
;; See docs/migration/cut-l-plan.md / cut-l-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-mouse-acceleration.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_mouse_acceleration.bin
;;
;; kernel/hid/mousedrv.inc (USE_RUST_MOUSE_ACCELERATION=1) trampolines
;; mouse_acceleration → rust_mouse_acceleration.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust mouse acceleration path ran.
rust_mouse_acceleration_smoke_result dd 0
endg

; Called once from high_code after Cut K smoke.
; Exercises: public symbol → trampoline → Rust → AX result + CX clobber OK.
; On mismatch: distinctive hang (does not reach desktop).
align 4
mouse_acceleration_rust_smoke_test:

        ; Defaults: mouse_delay=3, mouse_speed_factor=4
        ; |d|=1 → result AX=1
        mov     eax, 1
        mov     ebx, 0xB1000001
        mov     esi, 0x51000002
        mov     edi, 0xD1000003
        mov     ebp, 0xE1000004
        call    mouse_acceleration
        cmp     ax, 1
        jne     .fail
        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     esi, 0x51000002
        jne     .fail
        cmp     edi, 0xD1000003
        jne     .fail
        cmp     ebp, 0xE1000004
        jne     .fail

        ; |d|=5 → AX=4
        mov     eax, 5
        call    mouse_acceleration
        cmp     ax, 4
        jne     .fail

        ; negative dword delta −5 → AX=−4 (high-word sign restore)
        mov     eax, -5
        call    mouse_acceleration
        cmp     ax, -4
        jne     .fail

        ; zero stays non-negative small result: AL=3 → 9 → 8 → shr4=0 → inc=1
        xor     eax, eax
        call    mouse_acceleration
        cmp     ax, 1
        jne     .fail

        ; large positive 100 with defaults
        mov     eax, 100
        call    mouse_acceleration
        ; |100|: AL=103 → 103^2=10609 → 10608 → shr4=663 → inc=664
        cmp     ax, 664
        jne     .fail

        mov     [rust_mouse_acceleration_smoke_result], 0x4D4F5553 ; 'MOUS'
        ret
.fail:
        mov     eax, 0xDEAD0C4C        ; Cut L fail marker
        mov     ebx, 0x4D4F5553        ; 'MOUS'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_mouse_acceleration(delta, delay, speed_factor) -> EAX; ret 12
align 16
rust_mouse_acceleration:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_mouse_acceleration.bin'
