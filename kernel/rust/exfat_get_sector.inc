;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; exFAT_get_sector — Rust embed + ABI smoke (Cut CL)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_exfat_get_sector).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_exfat_get_sector.bin
;;
;; Requires USE_RUST_EXFAT_GET_SECTOR=1 so public exFAT_get_sector hits the
;; trampoline (not the FASM body). Independent of Cuts CC–CK.
;;
;; Critical ABI: register (EAX→{cluster,ofs}, EBP→exFAT*) → EAX=sector;
;; ECX restored; EBX/EDX/ESI/EDI/EBP untouched.
;; rust_exfat_get_sector stdcall(cluster, ofs, spc, heap_start); ret 16.
;; Trampoline preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001/011).
;; Smoke uses synthetic sizeof.exFAT + pair only (REG-003).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust exFAT_get_sector path ran.
rust_exfat_get_sector_smoke_result dd 0
endg

; Called once from early smoke cluster.
; On mismatch: distinctive hang (does not reach desktop).
align 4
exfat_get_sector_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp
        sub     esp, sizeof.exFAT + 16

        ; Clear synthetic exFAT + pair region
        lea     edi, [esp]
        mov     ecx, (sizeof.exFAT + 16) / 4
        xor     eax, eax
        rep stosd

        lea     ebp, [esp]                      ; EBP → synthetic exFAT
        mov     dword [ebp + exFAT.SECTORS_PER_CLUSTER], 8
        mov     dword [ebp + exFAT.CLUSTER_HEAP_START], 0x40

        lea     eax, [esp + sizeof.exFAT]       ; EAX → {cluster, ofs}
        mov     dword [eax], 3                  ; cluster 3
        mov     dword [eax + 4], 1              ; ofs 1
        ; expected: (3-2)*8 + 0x40 + 1 = 0x49

        mov     ebx, 0xB2000001
        mov     ecx, 0xC2000002
        mov     edx, 0xD2000003
        mov     esi, 0x52000004
        mov     edi, 0xD2000005
        call    exFAT_get_sector
        cmp     eax, 0x49
        jne     .fail
        cmp     ebx, 0xB2000001
        jne     .fail
        cmp     ecx, 0xC2000002
        jne     .fail
        cmp     edx, 0xD2000003
        jne     .fail
        cmp     esi, 0x52000004
        jne     .fail
        cmp     edi, 0xD2000005
        jne     .fail
        lea     eax, [esp]
        cmp     ebp, eax
        jne     .fail

        ; Vector 2: cluster 2, ofs 0 → heap_start
        lea     eax, [esp + sizeof.exFAT]
        mov     dword [eax], 2
        mov     dword [eax + 4], 0
        call    exFAT_get_sector
        cmp     eax, 0x40
        jne     .fail

        ; Vector 3: direct rust_* (ret 16)
        stdcall rust_exfat_get_sector, 100, 63, 64, 512
        cmp     eax, 6847
        jne     .fail

        ; Vector 4: cluster underflow (0) with spc=1, heap=0 → -2 wrap
        lea     eax, [esp + sizeof.exFAT]
        mov     dword [ebp + exFAT.SECTORS_PER_CLUSTER], 1
        mov     dword [ebp + exFAT.CLUSTER_HEAP_START], 0
        mov     dword [eax], 0
        mov     dword [eax + 4], 0
        call    exFAT_get_sector
        cmp     eax, -2
        jne     .fail

        mov     [rust_exfat_get_sector_smoke_result], 'ESEC'
        add     esp, sizeof.exFAT + 16
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_exfat_get_sector_smoke_result], 0xDEAD0C6C
@@:
        jmp     @b

; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
align 16
rust_exfat_get_sector:
        file '../../rust_kernel/kolibri_utils/out/rust_exfat_get_sector.bin'
