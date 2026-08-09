;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; is_partition_table_entry — Rust embed + ABI smoke (Cut Z)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_is_partition_table_entry).
;; See docs/migration/cut-z-plan.md / cut-z-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-is-partition-table-entry.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_is_partition_table_entry.bin
;;
;; kernel/blkdev/disk.inc (USE_RUST_IS_PARTITION_TABLE_ENTRY=1) trampolines
;; is_partition_table_entry → rust_is_partition_table_entry → CF.
;;
;; Capacity is injected from [ESI+DISK.MediaInfo.Capacity] so the Rust
;; blob never references DISK layout globals.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust partition-validate path ran.
rust_is_partition_table_entry_smoke_result dd 0
endg

uglobal
align 4
; Synthetic PARTITION_TABLE_ENTRY (16 bytes) + stub DISK through Capacity.
part_entry_smoke:       rb 16
; Minimal DISK stub: Capacity lives at DISK.MediaInfo.Capacity.
; Layout verified against disk.inc: MediaInfo @48, Capacity @+8 → offset 56.
part_disk_smoke:        rb 64
endg

; Called once from high_code (after LTR, with Cuts X/Y smokes).
; Exercises: register ABI ECX/EBP/ESI → CF; bootable mask; half-capacity.
; On mismatch: distinctive hang (does not reach desktop).
align 4
is_partition_table_entry_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear buffers ---
        mov     edi, part_entry_smoke
        mov     ecx, 4
        xor     eax, eax
        rep     stosd
        mov     edi, part_disk_smoke
        mov     ecx, 16
        rep     stosd

        ; Capacity = 100 sectors at DISK.MediaInfo.Capacity (offset 56)
        mov     dword [part_disk_smoke + 56], 100
        mov     dword [part_disk_smoke + 60], 0

        ; --- Valid: Bootable=0, first=0, length=10, ebp=0 → half=5 < 100 ---
        mov     byte [part_entry_smoke], 0
        mov     dword [part_entry_smoke + 8], 0
        mov     dword [part_entry_smoke + 12], 10
        mov     eax, 0xA9000001
        mov     ebx, 0xB9000002
        mov     ecx, part_entry_smoke
        mov     edx, 0xD9000004
        mov     esi, part_disk_smoke
        mov     edi, 0xE9000006
        mov     ebp, 0
        call    is_partition_table_entry
        jc      .fail
        cmp     ecx, part_entry_smoke
        jne     .fail
        cmp     esi, part_disk_smoke
        jne     .fail
        cmp     ebp, 0
        jne     .fail
        cmp     ebx, 0xB9000002
        jne     .fail
        cmp     edi, 0xE9000006
        jne     .fail

        ; --- Valid bootable 0x80 ---
        mov     byte [part_entry_smoke], 0x80
        mov     ecx, part_entry_smoke
        mov     esi, part_disk_smoke
        xor     ebp, ebp
        call    is_partition_table_entry
        jc      .fail

        ; --- Invalid bootable 0x01 ---
        mov     byte [part_entry_smoke], 0x01
        mov     ecx, part_entry_smoke
        mov     esi, part_disk_smoke
        xor     ebp, ebp
        call    is_partition_table_entry
        jnc     .fail

        ; --- Boundary: sum=200 → half=100 == capacity → invalid ---
        mov     byte [part_entry_smoke], 0
        mov     dword [part_entry_smoke + 8], 50
        mov     dword [part_entry_smoke + 12], 150
        mov     ecx, part_entry_smoke
        mov     esi, part_disk_smoke
        xor     ebp, ebp
        call    is_partition_table_entry
        jnc     .fail

        ; --- Boundary: sum=198 → half=99 < 100 → valid ---
        mov     dword [part_entry_smoke + 12], 148
        mov     ecx, part_entry_smoke
        mov     esi, part_disk_smoke
        xor     ebp, ebp
        call    is_partition_table_entry
        jc      .fail

        ; --- ebp base included: ebp=40, first=10, length=10 → half=30 ---
        mov     dword [part_entry_smoke + 8], 10
        mov     dword [part_entry_smoke + 12], 10
        mov     ecx, part_entry_smoke
        mov     esi, part_disk_smoke
        mov     ebp, 40
        call    is_partition_table_entry
        jc      .fail
        cmp     ebp, 40
        jne     .fail

        ; --- High capacity dword ---
        mov     dword [part_disk_smoke + 56], 0
        mov     dword [part_disk_smoke + 60], 1          ; capacity = 2^32
        mov     dword [part_entry_smoke + 8], 0
        mov     dword [part_entry_smoke + 12], 1
        mov     ecx, part_entry_smoke
        mov     esi, part_disk_smoke
        xor     ebp, ebp
        call    is_partition_table_entry
        jc      .fail

        mov     [rust_is_partition_table_entry_smoke_result], 0x49505445 ; 'IPTE'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C5A                ; Cut Z fail marker
        mov     ebx, 0x49505445                ; 'IPTE'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_is_partition_table_entry(entry, ebp, cap_lo, cap_hi) -> EAX; ret 16
align 16
rust_is_partition_table_entry:
        file '../../rust_kernel/kolibri_utils/out/rust_is_partition_table_entry.bin'
