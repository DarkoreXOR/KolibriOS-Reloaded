;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfs_test_bootsec — Rust embed + ABI smoke (Cut AG)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ntfs_test_bootsec).
;; See docs/migration/cut-ag-plan.md / cut-ag-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-ntfs-test-bootsec.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ntfs_test_bootsec.bin
;;
;; kernel/fs/ntfs.inc (USE_RUST_NTFS_TEST_BOOTSEC=1) trampolines
;; ntfs_test_bootsec → rust_ntfs_test_bootsec → CF.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust ntfs_test_bootsec path ran.
rust_ntfs_test_bootsec_smoke_result dd 0
endg

uglobal
align 4
; Synthetic 512-byte bootsector for ABI smoke vectors.
ntfs_bootsec_smoke:     rb 512
endg

; Called once from high_code (with Cuts AE/AF smokes).
; Exercises: EBX/EDX → CF; canonical valid; OEM/bps/spc/MFT/FRS rejects;
; EBX/EDX/ESI/EDI/EBP preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ntfs_test_bootsec_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear buffer ---
        mov     edi, ntfs_bootsec_smoke
        mov     ecx, 512/4
        xor     eax, eax
        rep     stosd

        ; --- Canonical valid NTFS bootsector ---
        ; OEM "NTFS    " at +3
        mov     dword [ntfs_bootsec_smoke + 3], 'NTFS'
        mov     dword [ntfs_bootsec_smoke + 7], '    '
        mov     word [ntfs_bootsec_smoke + 11], 0x200   ; bps
        mov     byte [ntfs_bootsec_smoke + 13], 8       ; spc
        ; total sectors = 1000 at +0x28
        mov     dword [ntfs_bootsec_smoke + 0x28], 1000
        ; $MFT LCN=1, $MFTMirr LCN=2
        mov     dword [ntfs_bootsec_smoke + 0x30], 1
        mov     dword [ntfs_bootsec_smoke + 0x38], 2
        ; ClustersPerFRS / Index = -10
        mov     byte [ntfs_bootsec_smoke + 0x40], -10
        mov     byte [ntfs_bootsec_smoke + 0x44], -10

        mov     eax, 0xA7000011
        mov     ecx, 0xC7000012
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        mov     esi, 0xE7000013
        mov     edi, 0xF7000014
        mov     ebp, 0xD7000015
        call    ntfs_test_bootsec
        jc      .fail                                      ; expect CF clear
        cmp     ebx, ntfs_bootsec_smoke
        jne     .fail
        cmp     edx, 1000
        jne     .fail
        cmp     esi, 0xE7000013
        jne     .fail
        cmp     edi, 0xF7000014
        jne     .fail
        cmp     ebp, 0xD7000015
        jne     .fail

        ; --- Reject: bad OEM ---
        mov     byte [ntfs_bootsec_smoke + 3], 'X'
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        call    ntfs_test_bootsec
        jnc     .fail                                      ; expect CF set
        mov     byte [ntfs_bootsec_smoke + 3], 'N'

        ; --- Reject: bad bytes/sector ---
        mov     word [ntfs_bootsec_smoke + 11], 0x400
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        call    ntfs_test_bootsec
        jnc     .fail
        mov     word [ntfs_bootsec_smoke + 11], 0x200

        ; --- Reject: spc non-power-of-2 ---
        mov     byte [ntfs_bootsec_smoke + 13], 3
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        call    ntfs_test_bootsec
        jnc     .fail
        mov     byte [ntfs_bootsec_smoke + 13], 8

        ; --- Reject: total sectors > partition ---
        mov     dword [ntfs_bootsec_smoke + 0x28], 1001
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        call    ntfs_test_bootsec
        jnc     .fail
        mov     dword [ntfs_bootsec_smoke + 0x28], 1000

        ; --- Reject: FRS clusters = 0 ---
        mov     byte [ntfs_bootsec_smoke + 0x40], 0
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        call    ntfs_test_bootsec
        jnc     .fail
        mov     byte [ntfs_bootsec_smoke + 0x40], -10

        ; --- Accept: ClustersPerFRS power-of-2 (8) ---
        mov     byte [ntfs_bootsec_smoke + 0x40], 8
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        call    ntfs_test_bootsec
        jc      .fail
        mov     byte [ntfs_bootsec_smoke + 0x40], -10

        ; --- Restore canonical and confirm again ---
        mov     ebx, ntfs_bootsec_smoke
        mov     edx, 1000
        call    ntfs_test_bootsec
        jc      .fail

        mov     [rust_ntfs_test_bootsec_smoke_result], 0x4E544253 ; 'NTBS'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0CA7                ; Cut AG fail marker
        mov     ebx, 0x4E544253                ; 'NTBS'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ntfs_test_bootsec(boot, part_sectors) -> EAX; ret 8
align 16
rust_ntfs_test_bootsec:
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_test_bootsec.bin'
