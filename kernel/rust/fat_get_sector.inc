;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fat_get_sector — Rust embed + ABI smoke (Cut CK)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fat_get_sector).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fat_get_sector.bin
;;
;; Requires USE_RUST_FAT_GET_SECTOR=1 so public fat_get_sector hits the
;; trampoline (not the FASM body). Independent of Cuts CC–CJ.
;;
;; Critical ABI: register (EAX→{cluster,ofs}, EBP→FAT*) → EAX=sector;
;; ECX restored; EBX/EDX/ESI/EDI/EBP untouched.
;; rust_fat_get_sector stdcall(cluster, ofs, spc, data_start); ret 16.
;; Trampoline preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001/011).
;; Smoke uses synthetic sizeof.FAT + pair only (REG-003).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust fat_get_sector path ran.
rust_fat_get_sector_smoke_result dd 0
endg

; Called once from early smoke cluster.
; On mismatch: distinctive hang (does not reach desktop).
align 4
fat_get_sector_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp
        sub     esp, sizeof.FAT + 16

        ; Clear synthetic FAT + pair region
        lea     edi, [esp]
        mov     ecx, (sizeof.FAT + 16) / 4
        xor     eax, eax
        rep stosd

        lea     ebp, [esp]                      ; EBP → synthetic FAT
        mov     dword [ebp + FAT.SECTORS_PER_CLUSTER], 8
        mov     dword [ebp + FAT.DATA_START], 0x20

        lea     eax, [esp + sizeof.FAT]         ; EAX → {cluster, ofs}
        mov     dword [eax], 3                  ; cluster 3
        mov     dword [eax + 4], 1              ; ofs 1
        ; expected: (3-2)*8 + 0x20 + 1 = 0x29

        mov     ebx, 0xB1000001
        mov     ecx, 0xC1000002
        mov     edx, 0xD1000003
        mov     esi, 0x51000004
        mov     edi, 0xD1000005
        call    fat_get_sector
        cmp     eax, 0x29
        jne     .fail
        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     ecx, 0xC1000002
        jne     .fail
        cmp     edx, 0xD1000003
        jne     .fail
        cmp     esi, 0x51000004
        jne     .fail
        cmp     edi, 0xD1000005
        jne     .fail
        lea     eax, [esp]
        cmp     ebp, eax
        jne     .fail

        ; Vector 2: cluster 2, ofs 0 → data_start
        lea     eax, [esp + sizeof.FAT]
        mov     dword [eax], 2
        mov     dword [eax + 4], 0
        call    fat_get_sector
        cmp     eax, 0x20
        jne     .fail

        ; Vector 3: direct rust_* (ret 16)
        stdcall rust_fat_get_sector, 100, 63, 64, 512
        cmp     eax, 6847
        jne     .fail

        ; Vector 4: cluster underflow (0) with spc=1, data=0 → -2 wrap
        lea     eax, [esp + sizeof.FAT]
        mov     dword [ebp + FAT.SECTORS_PER_CLUSTER], 1
        mov     dword [ebp + FAT.DATA_START], 0
        mov     dword [eax], 0
        mov     dword [eax + 4], 0
        call    fat_get_sector
        cmp     eax, -2
        jne     .fail

        mov     [rust_fat_get_sector_smoke_result], 'FSEC'
        add     esp, sizeof.FAT + 16
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_fat_get_sector_smoke_result], 0xDEAD0C6B
@@:
        jmp     @b

; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
align 16
rust_fat_get_sector:
        file '../../rust_kernel/kolibri_utils/out/rust_fat_get_sector.bin'
