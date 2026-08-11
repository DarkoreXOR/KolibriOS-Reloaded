;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; pci_make_config_cmd — Rust embed + ABI smoke (Cut BA)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_pci_make_config_cmd).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_pci_make_config_cmd.bin
;;
;; Requires USE_RUST_PCI_MAKE_CONFIG_CMD=1 so public pci_make_config_cmd hits
;; the trampoline (not the FASM body).
;;
;; Critical ABI: AH=bus, BH=devfn, BL=reg → EAX=config dword; preserve
;; EBX/ECX/EDX (REG-001). Pure compute — no live PCI / MMIO mutation (REG-003).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust pci_make_config_cmd path ran.
rust_pci_make_config_cmd_smoke_result dd 0
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on synthetic register inputs only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
pci_make_config_cmd_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: bus=1,devfn=0x18,reg=0x04 → 0x80011804 ---
        stdcall rust_pci_make_config_cmd, 1, 0x18, 0x04
        cmp     eax, 0x80011804
        jne     .fail

        ; --- Direct rust_*: zeros → 0x80000000 ---
        stdcall rust_pci_make_config_cmd, 0, 0, 0
        cmp     eax, 0x80000000
        jne     .fail

        ; --- Direct rust_*: max → 0x80ffffff ---
        stdcall rust_pci_make_config_cmd, 0xff, 0xff, 0xff
        cmp     eax, 0x80ffffff
        jne     .fail

        ; --- Direct rust_*: wide args truncated to 8 bits ---
        stdcall rust_pci_make_config_cmd, 0x101, 0x218, 0x304
        cmp     eax, 0x80011804
        jne     .fail

        ; --- Public trampoline: AH/BH/BL + EBX/ECX/EDX canaries ---
        mov     eax, 0x00000500         ; AH=5 (bus)
        mov     ebx, 0xB1B10A03         ; BH=0x0A, BL=0x03 (high canary kept in EBX)
        mov     ecx, 0xC2C2C2C2
        mov     edx, 0xD3D3D3D3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        call    pci_make_config_cmd
        cmp     eax, 0x80050A03
        jne     .fail
        cmp     ebx, 0xB1B10A03
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

        ; --- Public trampoline: AL junk ignored (only AH is bus) ---
        mov     eax, 0x00ABCD12         ; AH=0xCD → bus; AL/high junk
        mov     ebx, 0x00003408
        call    pci_make_config_cmd
        cmp     eax, 0x80CD3408
        jne     .fail

        mov     [rust_pci_make_config_cmd_smoke_result], 'PCIC'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_pci_make_config_cmd_smoke_result], 'DEAD'
        ; Distinctive hang marker for Cut BA
@@:
        cli
        hlt
        jmp     @b

; ---------------------------------------------------------------------------
; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
; ---------------------------------------------------------------------------
align 4
rust_pci_make_config_cmd:
        file '../../rust_kernel/kolibri_utils/out/rust_pci_make_config_cmd.bin'
