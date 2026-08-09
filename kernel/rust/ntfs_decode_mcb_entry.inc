;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfs_decode_mcb_entry — Rust embed + runtime smoke (Cut I)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ntfs_decode_mcb_entry).
;; See docs/migration/cut-i-plan.md / cut-i-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-ntfs-decode-mcb.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ntfs_decode_mcb_entry.bin
;;
;; kernel/fs/ntfs.inc (USE_RUST_NTFS_DECODE_MCB_ENTRY=1) trampolines
;; ntfs_decode_mcb_entry → rust_ntfs_decode_mcb_entry → CF.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust MCB decode path ran.
rust_ntfs_decode_mcb_smoke_result dd 0
; Packed MCB streams for smoke vectors.
mcb_smoke_end:
        db 0x00
mcb_smoke_simple:
        db 0x11, 0x05, 0x02          ; run=5, delta=+2
mcb_smoke_neg:
        db 0x11, 0x03, 0xFE          ; run=3, delta=-2
mcb_smoke_len_hi:
        db 0x11, 0x80, 0x01          ; length high-bit reject
mcb_smoke_chain:
        db 0x11, 0x01, 0x01          ; first entry
        db 0x00                      ; end
endg

; Called once from high_code after Cut H smoke.
; Exercises: FASM callers → trampoline → Rust body → CF + ESI + stack buf.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ntfs_decode_mcb_rust_smoke_test:
        mov     eax, 0xA8000001
        mov     ebx, 0xB8000002
        mov     ecx, 0xC8000003
        mov     edx, 0xD8000004
        mov     edi, 0xE8000005
        mov     ebp, 0xF8000006

        ; --- vector 1: header 0 → CF clear, buffer untouched ---
        sub     esp, 16
        mov     dword [esp], 0xA5A5A5A5
        mov     dword [esp+4], 0xA5A5A5A5
        mov     dword [esp+8], 0xA5A5A5A5
        mov     dword [esp+12], 0xA5A5A5A5
        mov     esi, mcb_smoke_end
        call    ntfs_decode_mcb_entry
        jc      .fail
        cmp     esi, mcb_smoke_end+1
        jne     .fail
        cmp     dword [esp], 0xA5A5A5A5
        jne     .fail
        add     esp, 16

        ; --- vector 2: simple positive → CF set, run=5, delta=2 ---
        sub     esp, 16
        mov     dword [esp], 0xA5A5A5A5
        mov     dword [esp+4], 0xA5A5A5A5
        mov     dword [esp+8], 0xA5A5A5A5
        mov     dword [esp+12], 0xA5A5A5A5
        mov     esi, mcb_smoke_simple
        call    ntfs_decode_mcb_entry
        jnc     .fail
        cmp     esi, mcb_smoke_simple+3
        jne     .fail
        cmp     dword [esp], 5
        jne     .fail
        cmp     dword [esp+4], 0
        jne     .fail
        cmp     dword [esp+8], 2
        jne     .fail
        cmp     dword [esp+12], 0
        jne     .fail
        add     esp, 16

        ; --- vector 3: negative delta → CF set, delta = -2 ---
        sub     esp, 16
        mov     esi, mcb_smoke_neg
        call    ntfs_decode_mcb_entry
        jnc     .fail
        cmp     dword [esp], 3
        jne     .fail
        cmp     dword [esp+8], -2
        jne     .fail
        cmp     dword [esp+12], -1
        jne     .fail
        add     esp, 16

        ; --- vector 4: length high-bit reject → CF clear, partial write ---
        sub     esp, 16
        mov     dword [esp], 0xA5A5A5A5
        mov     dword [esp+4], 0xA5A5A5A5
        mov     dword [esp+8], 0xA5A5A5A5
        mov     dword [esp+12], 0xA5A5A5A5
        mov     esi, mcb_smoke_len_hi
        call    ntfs_decode_mcb_entry
        jc      .fail
        cmp     esi, mcb_smoke_len_hi+2
        jne     .fail
        cmp     byte [esp], 0x80
        jne     .fail
        cmp     byte [esp+1], 0xA5
        jne     .fail
        add     esp, 16

        ; --- vector 5: chain then end ---
        sub     esp, 16
        mov     esi, mcb_smoke_chain
        call    ntfs_decode_mcb_entry
        jnc     .fail
        cmp     dword [esp], 1
        jne     .fail
        call    ntfs_decode_mcb_entry
        jc      .fail
        cmp     esi, mcb_smoke_chain+4
        jne     .fail
        add     esp, 16

        ; --- register preservation on a fresh simple decode ---
        mov     eax, 0xA8000001
        mov     ebx, 0xB8000002
        mov     ecx, 0xC8000003
        mov     edx, 0xD8000004
        mov     edi, 0xE8000005
        mov     ebp, 0xF8000006
        sub     esp, 16
        mov     esi, mcb_smoke_simple
        call    ntfs_decode_mcb_entry
        jnc     .fail
        cmp     eax, 0xA8000001
        jne     .fail
        cmp     ebx, 0xB8000002
        jne     .fail
        cmp     ecx, 0xC8000003
        jne     .fail
        cmp     edx, 0xD8000004
        jne     .fail
        cmp     edi, 0xE8000005
        jne     .fail
        cmp     ebp, 0xF8000006
        jne     .fail
        cmp     esi, mcb_smoke_simple+3
        jne     .fail
        add     esp, 16

        mov     [rust_ntfs_decode_mcb_smoke_result], 0x4D434245 ; 'MCBE'
        ret
.fail:
        mov     eax, 0xDEAD0C49        ; Cut I fail marker
        mov     ebx, 0x4D434245        ; 'MCBE'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ntfs_decode_mcb_entry(esi_inout, buffer) -> EAX 0/1; ret 8
align 16
rust_ntfs_decode_mcb_entry:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_decode_mcb_entry.bin'
