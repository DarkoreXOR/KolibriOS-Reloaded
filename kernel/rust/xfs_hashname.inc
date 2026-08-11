;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; xfs_hashname — Rust embed + ABI smoke (Cut AP)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_xfs_hashname).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_xfs_hashname.bin
;;
;; Requires USE_RUST_XFS_HASHNAME=1 so public xfs_hashname hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: stdcall (_name,_len) / retn 8; EAX=hash; trampoline
;; preserves ECX+EDX+ESI (REG-001 / legacy `uses ecx esi` + untouched EDX).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust xfs_hashname path ran.
rust_xfs_hashname_smoke_result dd 0
xfs_hashname_smoke_a db 'a'
xfs_hashname_smoke_ab db 'a','b'
xfs_hashname_smoke_readme db 'README.TXT'
xfs_hashname_smoke_hi db 0x00,0x7F,0x80,0xFF
endg

; Called once from high_code after Cut AO smoke.
; Exercises: public trampoline + direct rust_*; named vectors; ECX/EDX canaries;
; empty len; multi-byte ROL7 chain.
; On mismatch: distinctive hang (does not reach desktop).
align 4
xfs_hashname_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- single 'a'; ECX/EDX canaries (REG-001) ---
        mov     ecx, 0xC0C00001
        mov     edx, 0xD0D00002
        mov     ebx, 0xB0B00003
        mov     esi, 0x51510004
        mov     edi, 0xE1E10005
        stdcall xfs_hashname, xfs_hashname_smoke_a, 1
        cmp     eax, 'a'
        jne     .fail
        cmp     ecx, 0xC0C00001
        jne     .fail
        cmp     edx, 0xD0D00002
        jne     .fail
        cmp     ebx, 0xB0B00003
        jne     .fail
        cmp     esi, 0x51510004
        jne     .fail
        cmp     edi, 0xE1E10005
        jne     .fail

        ; --- empty len → 0 via Rust direct (FASM public body hangs on len=0) ---
        stdcall rust_xfs_hashname, xfs_hashname_smoke_a, 0
        test    eax, eax
        jnz     .fail

        ; --- "ab" ROL7 chain ---
        ; eax=0; rol7; xor 'a'; rol7; xor 'b'
        stdcall xfs_hashname, xfs_hashname_smoke_ab, 2
        mov     ebx, eax
        ; Independent FASM-flow check in registers
        xor     eax, eax
        rol     eax, 7
        xor     al, 'a'
        rol     eax, 7
        xor     al, 'b'
        cmp     eax, ebx
        jne     .fail

        ; --- README.TXT (typical XFS lookup) ---
        stdcall xfs_hashname, xfs_hashname_smoke_readme, 10
        mov     ebx, eax
        xor     eax, eax
        mov     esi, xfs_hashname_smoke_readme
        mov     ecx, 10
@@:
        rol     eax, 7
        xor     al, [esi]
        add     esi, 1
        dec     ecx
        jnz     @b
        cmp     eax, ebx
        jne     .fail

        ; --- high bytes ---
        stdcall xfs_hashname, xfs_hashname_smoke_hi, 4
        mov     ebx, eax
        xor     eax, eax
        mov     esi, xfs_hashname_smoke_hi
        mov     ecx, 4
@@:
        rol     eax, 7
        xor     al, [esi]
        add     esi, 1
        dec     ecx
        jnz     @b
        cmp     eax, ebx
        jne     .fail

        ; --- direct rust_* entry (ret 8) ---
        stdcall rust_xfs_hashname, xfs_hashname_smoke_a, 1
        cmp     eax, 'a'
        jne     .fail

        ; --- loop ×8 canary stability ---
        mov     ebp, 8
.loop8:
        mov     ecx, 0xC0C00001
        mov     edx, 0xD0D00002
        stdcall xfs_hashname, xfs_hashname_smoke_ab, 2
        cmp     ecx, 0xC0C00001
        jne     .fail
        cmp     edx, 0xD0D00002
        jne     .fail
        dec     ebp
        jnz     .loop8

        mov     [rust_xfs_hashname_smoke_result], 0x58485348 ; 'XHSH'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0001        ; Cut AP fail marker
        mov     ebx, 0x41504641        ; 'APFA'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_xfs_hashname(name, len) -> EAX (hash); ret 8
align 16
rust_xfs_hashname:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_xfs_hashname.bin'
