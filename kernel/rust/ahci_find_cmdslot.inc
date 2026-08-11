;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ahci_find_cmdslot — Rust embed + ABI smoke (Cut AV)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ahci_find_cmdslot).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ahci_find_cmdslot.bin
;;
;; Requires USE_RUST_AHCI_FIND_CMDSLOT=1 so public ahci_find_cmdslot hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: stdcall(pdata) → EAX=slot|-1; preserves EBX/ECX/EDX/ESI
;; (REG-001). Trampoline also canaries EDI/EBP. Rust leaf is pure
;; (slots, ncs) arithmetic after trampoline MMIO extract.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust AHCI cmdslot path ran.
rust_ahci_find_cmdslot_smoke_result dd 0
endg

; Offsets mirrored from ahci.inc structs (packed FASM struct.inc — no align):
;   PORT_DATA.port = 136, PORT_DATA.ctr_ptr = 153
;   HBA_PORT.sata_active = 0x34, HBA_PORT.command_issue = 0x38
;   HBA_MEM.cap = 0, AHCI_CTR.abar = 0
AHCI_SMOKE_OFF_PORT      = 136
AHCI_SMOKE_OFF_CTR_PTR   = 153
AHCI_SMOKE_OFF_SACT      = 0x34
AHCI_SMOKE_OFF_CI        = 0x38

uglobal
align 4
ahci_smoke_pdata: rb 160
ahci_smoke_port:  rb 64
ahci_smoke_ctr:   rd 1
ahci_smoke_hba:   rd 1
endg

; Called once after ahci_init. Exercises rust_* + public trampoline on synthetic
; PORT_DATA→HBA_PORT→AHCI_CTR→HBA_MEM. On mismatch: distinctive hang.
align 4
ahci_find_cmdslot_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic arenas ---
        mov     edi, ahci_smoke_pdata
        mov     ecx, 160/4
        xor     eax, eax
        rep     stosd
        mov     edi, ahci_smoke_port
        mov     ecx, 64/4
        rep     stosd
        mov     dword [ahci_smoke_ctr], 0
        mov     dword [ahci_smoke_hba], 0

        ; Wire pdata → port / ctr → abar → hba
        mov     dword [ahci_smoke_pdata + AHCI_SMOKE_OFF_PORT], ahci_smoke_port
        mov     dword [ahci_smoke_pdata + AHCI_SMOKE_OFF_CTR_PTR], ahci_smoke_ctr
        mov     dword [ahci_smoke_ctr], ahci_smoke_hba

        ; CAP.NCS field = 8 (bits 12:8); after >>8 &0xf → ncs=8
        mov     dword [ahci_smoke_hba], (8 shl 8)

        ; --- Direct rust_*: first free at 0 ---
        stdcall rust_ahci_find_cmdslot, 0, 8
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: bits 0..2 busy → slot 3 ---
        stdcall rust_ahci_find_cmdslot, 7, 8
        cmp     eax, 3
        jne     .fail

        ; --- Direct rust_*: all 8 busy → -1 ---
        stdcall rust_ahci_find_cmdslot, 0xff, 8
        cmp     eax, -1
        jne     .fail

        ; --- Direct rust_*: ncs=0 → -1 ---
        stdcall rust_ahci_find_cmdslot, 0, 0
        cmp     eax, -1
        jne     .fail

        ; --- Public trampoline: SACT=1 CI=2 → slots=3 → free at 2 ---
        mov     dword [ahci_smoke_port + AHCI_SMOKE_OFF_SACT], 1
        mov     dword [ahci_smoke_port + AHCI_SMOKE_OFF_CI], 2
        mov     ebx, 0xB1B1B1B1
        mov     ecx, 0xC2C2C2C2
        mov     edx, 0xD3D3D3D3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        stdcall ahci_find_cmdslot, ahci_smoke_pdata
        cmp     eax, 2
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

        ; --- Public trampoline: all busy in ncs=8 → -1 ---
        mov     dword [ahci_smoke_port + AHCI_SMOKE_OFF_SACT], 0xff
        mov     dword [ahci_smoke_port + AHCI_SMOKE_OFF_CI], 0
        stdcall ahci_find_cmdslot, ahci_smoke_pdata
        cmp     eax, -1
        jne     .fail

        ; --- NCS &0xf quirk: CAP bits 12:8 = 0x1F → masked ncs=0xF ---
        ; slots bits 0..14 set → miss even though bit15 clear
        mov     dword [ahci_smoke_hba], (0x1F shl 8)
        mov     dword [ahci_smoke_port + AHCI_SMOKE_OFF_SACT], 0x7fff
        mov     dword [ahci_smoke_port + AHCI_SMOKE_OFF_CI], 0
        stdcall ahci_find_cmdslot, ahci_smoke_pdata
        cmp     eax, -1
        jne     .fail

        mov     [rust_ahci_find_cmdslot_smoke_result], 0x544F4C53 ; 'SLOT'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C56                ; Cut AV fail marker
        mov     ebx, 0x544F4C53                ; 'SLOT'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ahci_find_cmdslot(slots, ncs) -> EAX; ret 8
align 16
rust_ahci_find_cmdslot:
        file '../../rust_kernel/kolibri_utils/out/rust_ahci_find_cmdslot.bin'
