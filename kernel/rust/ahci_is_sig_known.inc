;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ahci_is_sig_known — Rust embed + ABI smoke (Cut BM)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ahci_is_sig_known).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ahci_is_sig_known.bin
;;
;; Requires USE_RUST_AHCI_IS_SIG_KNOWN=1 so public ahci_is_sig_known hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EAX=PxSIG in; ZF=1 known / ZF=0 unknown out; preserves
;; EBX/ECX/EDX/ESI/EDI/EBP (legacy destroys flags only; REG-001).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust AHCI signature path ran.
rust_ahci_is_sig_known_smoke_result dd 0
endg

; PxSIG constants (mirror ahci.inc — smoke assembles before ahci.inc is included).
BM_SMOKE_SIG_ATA   = 0x00000101
BM_SMOKE_SIG_ATAPI = 0xEB140101
BM_SMOKE_SIG_SEMB   = 0xC33C0101
BM_SMOKE_SIG_PM     = 0x96690101

; Called once after ahci_init (alongside Cut AV/BG smokes). Exercises rust_* and
; the public trampoline ZF contract on synthetic signatures only.
align 4
ahci_is_sig_known_rust_smoke_test:
        push    ebx
        push    ecx
        push    edx
        push    esi
        push    edi
        push    ebp

        ; --- Direct rust_*: four known signatures → 1 ---
        stdcall rust_ahci_is_sig_known, BM_SMOKE_SIG_ATA
        cmp     eax, 1
        jne     .fail
        stdcall rust_ahci_is_sig_known, BM_SMOKE_SIG_ATAPI
        cmp     eax, 1
        jne     .fail
        stdcall rust_ahci_is_sig_known, BM_SMOKE_SIG_SEMB
        cmp     eax, 1
        jne     .fail
        stdcall rust_ahci_is_sig_known, BM_SMOKE_SIG_PM
        cmp     eax, 1
        jne     .fail

        ; --- Direct rust_*: unknown → 0 ---
        stdcall rust_ahci_is_sig_known, 0xDEADBEEF
        test    eax, eax
        jnz     .fail

        ; --- Public trampoline: known → ZF=1 (jz taken) ---
        mov     ebx, 0xB1B1B1B1
        mov     ecx, 0xC2C2C2C2
        mov     edx, 0xD3D3D3D3
        mov     esi, 0xE4E4E4E4
        mov     edi, 0xF5F5F5F5
        mov     ebp, 0xA6A6A6A6
        mov     eax, BM_SMOKE_SIG_ATA
        call    ahci_is_sig_known
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

        ; --- Public trampoline: unknown → ZF=0 (jnz taken) ---
        mov     eax, 0x12345678
        call    ahci_is_sig_known
        jz      .fail

        ; --- Edge: last cmp target PM and first ATA still known ---
        mov     eax, BM_SMOKE_SIG_PM
        call    ahci_is_sig_known
        jz      @f
        jmp     .fail
@@:
        mov     eax, BM_SMOKE_SIG_ATA
        call    ahci_is_sig_known
        jz      @f
        jmp     .fail
@@:

        mov     [rust_ahci_is_sig_known_smoke_result], 0x41534947 ; 'ASIG'
        pop     ebp
        pop     edi
        pop     esi
        pop     edx
        pop     ecx
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C56                ; Cut BM fail marker
        mov     ebx, 0x41534947                ; 'ASIG'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ahci_is_sig_known(sig) -> EAX (1 known / 0 unknown); ret 4
align 16
rust_ahci_is_sig_known:
        file '../../rust_kernel/kolibri_utils/out/rust_ahci_is_sig_known.bin'
