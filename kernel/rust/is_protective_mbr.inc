;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; is_protective_mbr — Rust embed + ABI smoke (Cut AD)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_is_protective_mbr).
;; See docs/migration/cut-ad-plan.md / cut-ad-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-is-protective-mbr.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_is_protective_mbr.bin
;;
;; kernel/blkdev/disk.inc (USE_RUST_IS_PROTECTIVE_MBR=1) trampolines
;; is_protective_mbr → rust_is_protective_mbr → ZF.
;;
;; Capacity low dword is injected from [ESI+DISK.MediaInfo.Capacity] so
;; the Rust blob never references DISK layout globals.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust protective-MBR path ran.
rust_is_protective_mbr_smoke_result dd 0
endg

uglobal
align 4
; Synthetic MBR: 2-byte pre-word + 64-byte partition table (4×16).
; Layout: [0..1]=pre-word, [2..65]=entries 0..3  →  pt = protective_mbr_smoke+2
protective_mbr_smoke:   rb 66
; Minimal DISK stub: Capacity at offset 56 (MediaInfo @48 + Capacity @+8).
protective_disk_smoke:  rb 64
endg

; Called once from high_code (with Cuts Z/AC smokes).
; Exercises: register ABI ECX/ESI → ZF; canonical protective; rejects;
; capacity_lo-1 length; ECX/ESI/EDI preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
is_protective_mbr_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear buffers ---
        mov     edi, protective_mbr_smoke
        mov     ecx, 66/2
        xor     eax, eax
        rep     stosw
        mov     edi, protective_disk_smoke
        mov     ecx, 16
        rep     stosd

        ; Capacity = 1000 sectors (low dword only; high ignored by leaf)
        mov     dword [protective_disk_smoke + 56], 1000
        mov     dword [protective_disk_smoke + 60], 0

        ; --- Canonical protective: pre=0, boot=0, type=EE, first=1, len=-1 ---
        ; pt at protective_mbr_smoke+2; pre-word at +0
        mov     byte [protective_mbr_smoke + 2], 0          ; bootable
        mov     byte [protective_mbr_smoke + 2 + 4], 0xEE   ; type
        mov     dword [protective_mbr_smoke + 2 + 8], 1     ; first LBA
        mov     dword [protective_mbr_smoke + 2 + 12], -1   ; length

        mov     eax, 0xA90000AD
        mov     ebx, 0xB90000AD
        lea     ecx, [protective_mbr_smoke + 2]
        mov     edx, 0xD90000AD
        mov     esi, protective_disk_smoke
        mov     edi, 0xE90000AD
        mov     ebp, 0xF90000AD
        call    is_protective_mbr
        jnz     .fail                                      ; expect ZF set
        cmp     ecx, protective_mbr_smoke + 2
        jne     .fail
        cmp     esi, protective_disk_smoke
        jne     .fail
        cmp     edi, 0xE90000AD
        jne     .fail
        cmp     ebx, 0xB90000AD
        jne     .fail
        cmp     ebp, 0xF90000AD
        jne     .fail

        ; --- Length = Capacity_lo - 1 also protective ---
        mov     dword [protective_mbr_smoke + 2 + 12], 999
        lea     ecx, [protective_mbr_smoke + 2]
        mov     esi, protective_disk_smoke
        call    is_protective_mbr
        jnz     .fail

        ; --- Reject: wrong type ---
        mov     dword [protective_mbr_smoke + 2 + 12], -1
        mov     byte [protective_mbr_smoke + 2 + 4], 0x0B
        lea     ecx, [protective_mbr_smoke + 2]
        mov     esi, protective_disk_smoke
        call    is_protective_mbr
        jz      .fail                                      ; expect ZF clear
        mov     byte [protective_mbr_smoke + 2 + 4], 0xEE

        ; --- Reject: nonzero pre-word ---
        mov     word [protective_mbr_smoke], 1
        lea     ecx, [protective_mbr_smoke + 2]
        mov     esi, protective_disk_smoke
        call    is_protective_mbr
        jz      .fail
        mov     word [protective_mbr_smoke], 0

        ; --- Reject: nonzero entry 1 ---
        mov     byte [protective_mbr_smoke + 2 + 16], 1
        lea     ecx, [protective_mbr_smoke + 2]
        mov     esi, protective_disk_smoke
        call    is_protective_mbr
        jz      .fail
        mov     byte [protective_mbr_smoke + 2 + 16], 0

        ; --- Reject: bootable 0x80 ---
        mov     byte [protective_mbr_smoke + 2], 0x80
        lea     ecx, [protective_mbr_smoke + 2]
        mov     esi, protective_disk_smoke
        call    is_protective_mbr
        jz      .fail
        mov     byte [protective_mbr_smoke + 2], 0

        ; --- Restore canonical and confirm again ---
        mov     dword [protective_mbr_smoke + 2 + 12], -1
        lea     ecx, [protective_mbr_smoke + 2]
        mov     esi, protective_disk_smoke
        call    is_protective_mbr
        jnz     .fail

        mov     [rust_is_protective_mbr_smoke_result], 0x49504D42 ; 'IPMB'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0CAD                ; Cut AD fail marker
        mov     ebx, 0x49504D42                ; 'IPMB'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_is_protective_mbr(pt, cap_lo) -> EAX; ret 8
align 16
rust_is_protective_mbr:
        file '../../rust_kernel/kolibri_utils/out/rust_is_protective_mbr.bin'
