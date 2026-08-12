;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; process_partition_table_entry — Rust embed + ABI smoke (Cut CC)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_process_partition_table_entry).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_process_partition_table_entry.bin
;;
;; Requires USE_RUST_PROCESS_PARTITION_TABLE_ENTRY=1 so public
;; process_partition_table_entry hits the trampoline (not the FASM body).
;;
;; Critical ABI: ECX→entry, EBP=MBR/EBR base, ESI→DISK; caller [esp+4]
;; extended-partition slot; preserves ECX/ESI/EBP (REG-005).
;; Mock/live add_partition MUST be stdcall (ret 20), no FASM proc frame
;; that early-`ret`s past `leave` (REG-007/009).
;; Live path must set ESI+EBX before disk_add_partition (REG-008).
;; Trampoline must use FASM `stdcall` — never call+add esp,28 (REG-009).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_process_partition_table_entry_smoke_result dd 0
endg

uglobal
align 4
cc_smoke_entry:         rb 16
cc_smoke_disk:          rb 64
cc_smoke_ext:           dd ?
cc_smoke_add_lo:        dd ?
cc_smoke_add_hi:        dd ?
cc_smoke_add_len:       dd ?
cc_smoke_add_disk:      dd ?
cc_smoke_add_count:     dd ?
; Saved by public trampoline before Rust (REG-008): disk_add_partition uses ESI+EBX.
cc_trampoline_mbr_buf:  dd ?
endg

; Tail-call thunk: same 5 stdcall args as disk_add_partition; set ESI/EBX then jmp.
align 4
cc_disk_add_partition_for_rust:
        mov     esi, [esp+20]           ; disk
        mov     ebx, [cc_trampoline_mbr_buf]
        jmp     disk_add_partition

; Mock disk_add_partition — naked stdcall (ret 20), no ebp frame.
align 4
cc_smoke_disk_add_partition:
        inc     dword [cc_smoke_add_count]
        mov     eax, [esp+4]            ; start_lo
        mov     [cc_smoke_add_lo], eax
        mov     eax, [esp+8]            ; start_hi
        mov     [cc_smoke_add_hi], eax
        mov     eax, [esp+12]           ; length_lo
        mov     [cc_smoke_add_len], eax
        mov     eax, [esp+20]           ; disk
        mov     [cc_smoke_add_disk], eax
        ret     20

; Called once from high_code after Cut Z smoke.
align 4
process_partition_table_entry_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Reset synthetic fixtures ---
        mov     edi, cc_smoke_entry
        mov     ecx, 4
        xor     eax, eax
        rep     stosd
        mov     edi, cc_smoke_disk
        mov     ecx, 16
        rep     stosd
        and     dword [cc_smoke_ext], 0
        and     dword [cc_smoke_add_count], 0

        ; Capacity = 0x10000 sectors at DISK.MediaInfo.Capacity (offset 56)
        mov     dword [cc_smoke_disk + 56], 0x10000
        mov     dword [cc_smoke_disk + 60], 0

        ; --- Extended type 0x0F via public trampoline → writes stack slot, no add ---
        mov     byte [cc_smoke_entry + 4], 0x0F
        mov     dword [cc_smoke_entry + 8], 0x4321
        mov     eax, 0xA1A1A1A1
        mov     ebx, 0xB2B2B2B2
        mov     ecx, cc_smoke_entry
        mov     edx, 0xD4D4D4D4
        mov     esi, cc_smoke_disk
        mov     edi, 0xE5E5E5E5
        mov     ebp, 100
        push    0
        call    process_partition_table_entry
        pop     eax
        cmp     eax, 0x4321
        jne     .fail
        cmp     dword [cc_smoke_add_count], 0
        jne     .fail
        cmp     ecx, cc_smoke_entry
        jne     .fail
        cmp     esi, cc_smoke_disk
        jne     .fail
        cmp     ebp, 100
        jne     .fail
        cmp     ebx, 0xB2B2B2B2
        jne     .fail

        ; --- Normal type 0x07 → direct rust_* + mock add (start=1100) ---
        ; stdcall only (callee ret 28) — never call+add esp,28 (REG-009).
        mov     byte [cc_smoke_entry + 4], 0x07
        mov     dword [cc_smoke_entry + 8], 100
        mov     dword [cc_smoke_entry + 12], 500
        and     dword [cc_smoke_add_count], 0
        lea     eax, [cc_smoke_ext]
        stdcall rust_process_partition_table_entry, cc_smoke_entry, 100, \
                dword [cc_smoke_disk + 56], dword [cc_smoke_disk + 60], \
                eax, cc_smoke_disk, cc_smoke_disk_add_partition
        cmp     dword [cc_smoke_add_count], 1
        jne     .fail
        cmp     dword [cc_smoke_add_lo], 1100
        jne     .fail
        cmp     dword [cc_smoke_add_hi], 0
        jne     .fail
        cmp     dword [cc_smoke_add_len], 500
        jne     .fail
        cmp     dword [cc_smoke_add_disk], cc_smoke_disk
        jne     .fail

        mov     dword [rust_process_partition_table_entry_smoke_result], 'PART'
        jmp     .done
.fail:
        mov     dword [rust_process_partition_table_entry_smoke_result], 'CCFL'
.done:
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

; Raw Rust stdcall body:
;   rust_process_partition_table_entry(...); ret 28
align 16
rust_process_partition_table_entry:
        file '../../rust_kernel/kolibri_utils/out/rust_process_partition_table_entry.bin'
