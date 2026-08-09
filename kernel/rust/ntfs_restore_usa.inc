;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ntfs_restore_usa — Rust embed + runtime smoke (Cut J)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ntfs_restore_usa).
;; See docs/migration/cut-j-plan.md / cut-j-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-ntfs-restore-usa.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ntfs_restore_usa.bin
;;
;; kernel/fs/ntfs.inc (USE_RUST_NTFS_RESTORE_USA=1) trampolines
;; ntfs_restore_usa → rust_ntfs_restore_usa → CF.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust USA restore path ran.
rust_ntfs_restore_usa_smoke_result dd 0
endg

uglobal
align 4
usa_smoke_buf:  rb 1024
usa_smoke_buf2: rb 1024
endg

; Called once from high_code after Cut I smoke.
; Exercises: public symbol → trampoline → Rust → CF + memory mutate.
; On mismatch: distinctive hang (does not reach desktop).
align 4
ntfs_restore_usa_rust_smoke_test:

        ; --- vector 1: valid 2-sector → CF clear, both end-words restored ---
        mov     edi, usa_smoke_buf
        mov     ecx, 1024
        mov     al, 0xA5
        rep stosb
        mov     ebx, usa_smoke_buf
        mov     word [ebx+4], 0x0028          ; updateSequenceOffset
        mov     word [ebx+6], 3               ; updateSequenceSize = sectors+1
        mov     word [ebx+0x28], 0x55AA       ; USN
        mov     word [ebx+0x2A], 0x1111       ; orig sector0
        mov     word [ebx+0x2C], 0x2222       ; orig sector1
        mov     word [ebx+0x1FE], 0x55AA
        mov     word [ebx+0x3FE], 0x55AA

        mov     eax, 0xA9000001
        mov     ecx, 0xC9000003
        mov     edx, 0xD9000004
        mov     esi, 0xE9000005
        mov     edi, 0xF9000006
        mov     ebp, 0x19000007
        ; EBX already = usa_smoke_buf
        push    eax
        mov     eax, 1024
        call    ntfs_restore_usa
        jc      .fail
        pop     eax
        cmp     eax, 0xA9000001
        jne     .fail
        cmp     ebx, usa_smoke_buf
        jne     .fail
        cmp     ecx, 0xC9000003
        jne     .fail
        cmp     edx, 0xD9000004
        jne     .fail
        cmp     esi, 0xE9000005
        jne     .fail
        cmp     edi, 0xF9000006
        jne     .fail
        cmp     ebp, 0x19000007
        jne     .fail
        cmp     word [usa_smoke_buf+0x1FE], 0x1111
        jne     .fail
        cmp     word [usa_smoke_buf+0x3FE], 0x2222
        jne     .fail
        cmp     byte [usa_smoke_buf], 0xA5
        jne     .fail
        cmp     byte [usa_smoke_buf+0x100], 0xA5
        jne     .fail

        ; --- vector 2: USA size mismatch → CF set, buffer unchanged ---
        mov     edi, usa_smoke_buf2
        mov     ecx, 1024
        mov     al, 0x5A
        rep stosb
        mov     ebx, usa_smoke_buf2
        mov     word [ebx+4], 0x0028
        mov     word [ebx+6], 1               ; wrong (want 3)
        mov     word [ebx+0x28], 0x55AA
        mov     word [ebx+0x1FE], 0x55AA
        mov     word [ebx+0x3FE], 0x55AA
        mov     eax, 1024
        call    ntfs_restore_usa
        jnc     .fail
        cmp     byte [usa_smoke_buf2], 0x5A
        jne     .fail
        cmp     word [usa_smoke_buf2+0x1FE], 0x55AA
        jne     .fail

        ; --- vector 3: USN mismatch on 2nd sector → CF set, partial mutate ---
        mov     edi, usa_smoke_buf
        mov     ecx, 1024
        mov     al, 0xA5
        rep stosb
        mov     ebx, usa_smoke_buf
        mov     word [ebx+4], 0x0028
        mov     word [ebx+6], 3
        mov     word [ebx+0x28], 0x55AA
        mov     word [ebx+0x2A], 0xAAAA
        mov     word [ebx+0x2C], 0xBBBB
        mov     word [ebx+0x1FE], 0x55AA
        mov     word [ebx+0x3FE], 0x0000      ; bad USN
        mov     eax, 1024
        call    ntfs_restore_usa
        jnc     .fail
        cmp     word [usa_smoke_buf+0x1FE], 0xAAAA
        jne     .fail
        cmp     word [usa_smoke_buf+0x3FE], 0x0000
        jne     .fail

        ; --- vector 4: 1-sector OK ---
        mov     edi, usa_smoke_buf
        mov     ecx, 512
        mov     al, 0x3C
        rep stosb
        mov     ebx, usa_smoke_buf
        mov     word [ebx+4], 0x0028
        mov     word [ebx+6], 2
        mov     word [ebx+0x28], 0xBEEF
        mov     word [ebx+0x2A], 0xDEAD
        mov     word [ebx+0x1FE], 0xBEEF
        mov     eax, 512
        call    ntfs_restore_usa
        jc      .fail
        cmp     word [usa_smoke_buf+0x1FE], 0xDEAD
        jne     .fail

        mov     [rust_ntfs_restore_usa_smoke_result], 0x55534152 ; 'USAR'
        ret
.fail:
        mov     eax, 0xDEAD0C4A        ; Cut J fail marker
        mov     ebx, 0x55534152        ; 'USAR'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ntfs_restore_usa(record, size) -> EAX 0/1; ret 8
align 16
rust_ntfs_restore_usa:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_ntfs_restore_usa.bin'
