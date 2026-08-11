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

        ; --- Ensure 0xF100..0xF107 denied ---
        mov     edi, tss._io_map_0
        mov     eax, 0xF100
        mov     ebp, 1
.set_den:
        call    set_io_access_rights
        inc     eax
        cmp     eax, 0xF107
        jbe     .set_den

        ; --- Direct rust_*: reserve ---
        mov     eax, [current_slot]
        mov     eax, [eax + APPDATA.tid]
        stdcall rust_r_f_port_area, 0, 0xF100, 0xF107, RESERVED_PORTS, eax, tss._io_map_0
        test    eax, eax
        jnz     .fail

        mov     edi, tss._io_map_0
        mov     eax, 0xF100
.chk_en:
        bt      [edi], eax
        jc      .fail
        inc     eax
        cmp     eax, 0xF107
        jbe     .chk_en

        ; --- Direct rust_*: free (exact match) ---
        mov     eax, [current_slot]
        mov     eax, [eax + APPDATA.tid]
        stdcall rust_r_f_port_area, 1, 0xF100, 0xF107, RESERVED_PORTS, eax, tss._io_map_0
        test    eax, eax
        jnz     .fail

        mov     edi, tss._io_map_0
        mov     eax, 0xF100
.chk_dis:
        bt      [edi], eax
        jnc     .fail
        inc     eax
        cmp     eax, 0xF107
        jbe     .chk_dis

        ; --- Public trampoline early-error paths (legacy .err preserves
        ;     ECX/EDX; may clobber EBX on overlap walk). REG-001 canaries. ---
        xor     ebx, ebx
        mov     ecx, 0xC0C00020
        mov     edx, 0xD0D00028
        ; Force overlap args (system range 0x20..0x28)
        mov     ecx, 0x20
        mov     edx, 0x28
        mov     esi, 0x515100A4
        call    r_f_port_area
        cmp     eax, 1
        jne     .fail
        cmp     ecx, 0x20
        jne     .fail
        cmp     edx, 0x28
        jne     .fail

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

        ; --- Public trampoline round-trip: reserve checks ECX/EDX live;
        ;     free only checks EAX (legacy FASM free clobbers ECX via rep). ---
        xor     ebx, ebx
        mov     ecx, 0xF110
        mov     edx, 0xF113
        call    r_f_port_area
        test    eax, eax
        jnz     .fail
        cmp     ecx, 0xF110
        jne     .fail
        cmp     edx, 0xF113
        jne     .fail

        mov     ebx, 1
        mov     ecx, 0xF110
        mov     edx, 0xF113
        call    r_f_port_area
        test    eax, eax
        jnz     .fail
        ; ECX may be clobbered on FASM free path — do not assert.

        ; Restore count snapshot (net zero reservations)
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
        ; Distinctive hang marker for Cut AR ABI smoke
        mov     eax, 0xDEAD0C52
@@:
        jmp     @b

; Reloc-free Rust blob (stdcall ret 24).
align 16
rust_r_f_port_area:
        file '../../rust_kernel/kolibri_utils/out/rust_r_f_port_area.bin'
