;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ahci_port_wait — Rust embed + ABI smoke (Cut CB)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ahci_port_wait).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ahci_port_wait.bin
;;
;; Requires USE_RUST_AHCI_PORT_WAIT=1 so public ahci_port_wait hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: stdcall(port, timeout) → EAX 0/1; preserves EBX/ECX
;; (legacy push/pop). Trampoline canaries EDX/ESI/EDI/EBP (REG-001).
;; Mock callbacks MUST use stdcall (ret / ret 4) — see REG-006.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ahci_port_wait_smoke_result dd 0
endg

; HBA_PORT.task_file_data offset (mirror ahci.inc struct).
CB_SMOKE_OFF_TFD = 0x20

uglobal
align 4
cb_smoke_port:    rb 64
cb_smoke_tick:    dd ?
cb_smoke_tfd_poll dd ?
endg

; Mock TFD reader: reads port task_file_data directly (stdcall; ret 4).
align 4
proc cb_smoke_read_tfd_direct stdcall, port:dword
        mov     eax, [port]
        mov     eax, [eax + CB_SMOKE_OFF_TFD]
        ret
endp

; Mock TFD reader: first poll returns 0x88, subsequent polls read port (idle).
align 4
proc cb_smoke_read_tfd stdcall, port:dword
        cmp     dword [cb_smoke_tfd_poll], 0
        jne     .idle
        inc     dword [cb_smoke_tfd_poll]
        mov     eax, 0x88
        ret
.idle:
        mov     eax, [port]
        mov     eax, [eax + CB_SMOKE_OFF_TFD]
        ret
endp

; Mock tick reader (stdcall; ret). Advances cb_smoke_tick — never live timer_ticks.
align 4
proc cb_smoke_read_ticks stdcall
        mov     eax, [cb_smoke_tick]
        inc     dword [cb_smoke_tick]
        ret
endp

; Called once after ahci_init (with AV/BM/BG smokes).
align 4
ahci_port_wait_rust_smoke_test:
        push    ebx
        push    ecx
        push    edx
        push    esi
        push    edi
        push    ebp

        ; --- Reset synthetic port ---
        mov     edi, cb_smoke_port
        mov     ecx, 64/4
        xor     eax, eax
        rep     stosd
        mov     dword [cb_smoke_tfd_poll], 0

        ; --- Direct rust_*: idle TFD → 0 ---
        mov     dword [cb_smoke_port + CB_SMOKE_OFF_TFD], 0
        mov     dword [cb_smoke_tick], 100
        stdcall rust_ahci_port_wait, cb_smoke_read_tfd_direct, cb_smoke_read_ticks, cb_smoke_port, 50
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: stuck busy → timeout (mock ticks advance) ---
        mov     dword [cb_smoke_port + CB_SMOKE_OFF_TFD], 0x80
        mov     dword [cb_smoke_tick], 0
        stdcall rust_ahci_port_wait, cb_smoke_read_tfd_direct, cb_smoke_read_ticks, cb_smoke_port, 2
        cmp     eax, 1
        jne     .fail

        ; --- Direct rust_*: clears after one busy poll (stateful mock) ---
        mov     dword [cb_smoke_port + CB_SMOKE_OFF_TFD], 0
        mov     dword [cb_smoke_tfd_poll], 0
        mov     dword [cb_smoke_tick], 10
        stdcall rust_ahci_port_wait, cb_smoke_read_tfd, cb_smoke_read_ticks, cb_smoke_port, 100
        test    eax, eax
        jnz     .fail

        ; --- Public trampoline: success path + GPR canaries (idle TFD) ---
        mov     dword [cb_smoke_port + CB_SMOKE_OFF_TFD], 0
        mov     ebx, 0xB1B1B1B1
        mov     ecx, 0xC2C2C2C2
        mov     edx, 0xD3D3D3D3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        stdcall ahci_port_wait, cb_smoke_port, 25
        test    eax, eax
        jnz     .fail
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

        ; --- Public trampoline: immediate timeout (timeout=0; REG-006) ---
        ; Do NOT use timeout>0 with busy TFD here: timer_ticks is frozen until
        ; multitasking starts (kernel.asm timer_ticks_enable), so poll spins forever.
        mov     dword [cb_smoke_port + CB_SMOKE_OFF_TFD], 0x80
        stdcall ahci_port_wait, cb_smoke_port, 0
        cmp     eax, 1
        jne     .fail

        mov     [rust_ahci_port_wait_smoke_result], 0x57414954 ; 'WAIT'
        pop     ebp
        pop     edi
        pop     esi
        pop     edx
        pop     ecx
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD02CB        ; Cut CB fail marker
        mov     ebx, 0x57414954        ; 'WAIT'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ahci_port_wait(read_tfd, read_ticks, port, timeout) -> EAX; ret 16
align 16
rust_ahci_port_wait:
        file '../../rust_kernel/kolibri_utils/out/rust_ahci_port_wait.bin'
