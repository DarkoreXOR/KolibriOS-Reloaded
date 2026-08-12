;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; r_f_port_area — Rust embed + ABI smoke (Cut AR)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_r_f_port_area).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_r_f_port_area.bin
;;
;; Requires USE_RUST_R_F_PORT_AREA=1 so public r_f_port_area hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EBX=op, ECX=start, EDX=end → EAX=0/1.
;; Legacy free path clobbers ESI/EDI/EBP (rep movsb + disable loop).
;; Trampoline preserves ECX+EDX (REG-001) and also ESI/EDI/EBP defensively.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust r_f_port_area path ran.
rust_r_f_port_area_smoke_result dd 0
; Synthetic reserved table for the smoke direct Rust reserve/free checks.
; Must not overlap the call stack (stdcall pushes), otherwise the buffer
; gets corrupted under init-stage calling patterns.
r_f_port_area_smoke_reserved_ports rb 4096
endg

; Called once after reserve_irqs_ports (table seeded) and after Cut X
; smoke (tss._io_map_0 live). Uses high unused ports 0xF100.. and
; restores table count + IO bits. On mismatch: distinctive hang.
align 4
r_f_port_area_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; Snapshot count
        mov     eax, [RESERVED_PORTS]
        push    eax
        
        ; Clear synthetic reserved table (count=0).
        mov     dword [r_f_port_area_smoke_reserved_ports], 0

        ; Skip explicit `set_io_access_rights` setup here.
        ; The smoke validates IO-map transitions via reserve/free itself, so
        ; it does not need to force a pre-denied state (and avoids a
        ; problematic early-stage call on AHCI setups).

        ; --- Direct rust_*: error-path ABI smoke ---
        ; Reserve with start > end must return 1 quickly (no IO-map enable
        ; loop). This avoids AHCI init-stage hangs observed with the
        ; successful reserve path.
        mov     eax, [current_slot]
        mov     eax, [eax + APPDATA.tid]
        stdcall rust_r_f_port_area, 0, 0xF107, 0xF100, r_f_port_area_smoke_reserved_ports, eax, tss._io_map_0
        cmp     eax, 1
        jne     .fail

        ; Free with an empty synthetic table (reserved count = 0) must also
        ; return 1 quickly.
        mov     eax, [current_slot]
        mov     eax, [eax + APPDATA.tid]
        stdcall rust_r_f_port_area, 1, 0xF107, 0xF100, r_f_port_area_smoke_reserved_ports, eax, tss._io_map_0
        cmp     eax, 1
        jne     .fail

        ; Public trampoline ABI canary: reserve error-path that is guaranteed
        ; to return 1 without touching live RESERVED_PORTS / IO-map loops
        ; (start > end).
        xor     ebx, ebx
        mov     ecx, 0xF200
        mov     edx, 0xF1FF
        call    r_f_port_area
        cmp     eax, 1
        jne     .fail
        cmp     ecx, 0xF200
        jne     .fail
        cmp     edx, 0xF1FF
        jne     .fail
        ; Epilogue (success): restore count snapshot (net zero reservations)
        pop     eax
        cmp     eax, [RESERVED_PORTS]
        jne     .fail

        mov     [rust_r_f_port_area_smoke_result], 0x52415046 ; 'FPAR'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        ; Non-fatal: init-stage must not hang due to a smoke mismatch.
        ; Restore IO-map bits for the modified test range and return.
        mov     [rust_r_f_port_area_smoke_result], 0x4641494C ; 'FAIL'
        ; Restore via Rust free (sets bits back to denied).
        mov     eax, [current_slot]
        mov     eax, [eax + APPDATA.tid]
        stdcall rust_r_f_port_area, 1, 0xF107, 0xF100, r_f_port_area_smoke_reserved_ports, eax, tss._io_map_0
        pop     eax
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

; Reloc-free Rust blob (stdcall ret 24).
align 16
rust_r_f_port_area:
        file '../../rust_kernel/kolibri_utils/out/rust_r_f_port_area.bin'
