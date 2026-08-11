;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; socket_check — Rust embed + ABI smoke (Cut AS)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_socket_check).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_socket_check.bin
;;
;; Requires USE_RUST_SOCKET_CHECK=1 so public socket_check hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EAX=ptr in → EAX=ptr/0 out; ZF set on miss/null.
;; Trampoline preserves EBX/ECX/EDX/ESI/EDI/EBP (REG-001) and restores
;; ZF with `test eax, eax` after stdcall (`pop` leaves EFLAGS).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust socket_check path ran.
rust_socket_check_smoke_result dd 0
endg

uglobal
align 4
; Synthetic socket-list arena: sentinel + 3 nodes (NextPtr at offset 0).
; Layout: [0]=sentinel, [16]=nodeA, [32]=nodeB, [48]=nodeC
socket_check_smoke_arena: rb 64
endg

; Called once after stack_init (net_sockets zeroed). Exercises synthetic
; rust_* list walk + public trampoline on live empty net_sockets.
; On mismatch: distinctive hang (does not reach desktop).
align 4
socket_check_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Build synthetic list: sentinel → A → B → C → 0 ---
        mov     edi, socket_check_smoke_arena
        mov     ecx, 64/4
        xor     eax, eax
        rep     stosd

        lea     eax, [socket_check_smoke_arena + 16]    ; A
        mov     [socket_check_smoke_arena], eax         ; sentinel.NextPtr = A
        lea     ebx, [socket_check_smoke_arena + 32]    ; B
        mov     [eax], ebx                              ; A.NextPtr = B
        lea     ecx, [socket_check_smoke_arena + 48]    ; C
        mov     [ebx], ecx                              ; B.NextPtr = C
        mov     dword [ecx], 0                          ; C.NextPtr = 0

        ; --- Direct rust_*: null → 0 ---
        stdcall rust_socket_check, 0, socket_check_smoke_arena
        test    eax, eax
        jnz     .fail

        ; --- Hit A ---
        lea     esi, [socket_check_smoke_arena + 16]
        stdcall rust_socket_check, esi, socket_check_smoke_arena
        cmp     eax, esi
        jne     .fail

        ; --- Hit B ---
        lea     esi, [socket_check_smoke_arena + 32]
        stdcall rust_socket_check, esi, socket_check_smoke_arena
        cmp     eax, esi
        jne     .fail

        ; --- Hit C ---
        lea     esi, [socket_check_smoke_arena + 48]
        stdcall rust_socket_check, esi, socket_check_smoke_arena
        cmp     eax, esi
        jne     .fail

        ; --- Miss ---
        stdcall rust_socket_check, 0xDEADBEEF, socket_check_smoke_arena
        test    eax, eax
        jnz     .fail

        ; --- Public trampoline: live empty net_sockets → miss + ZF + regs ---
        mov     eax, 0xA5A50001
        mov     ebx, 0xB5B50002
        mov     ecx, 0xC5C50003
        mov     edx, 0xD5D50004
        mov     esi, 0x51510005
        mov     edi, 0xE5E50006
        mov     ebp, 0xF5F50007
        ; Overwrite EAX with a non-member query
        mov     eax, 0xCAFEBABE
        call    socket_check
        jz      @f
        jmp     .fail
@@:
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB5B50002
        jne     .fail
        cmp     ecx, 0xC5C50003
        jne     .fail
        cmp     edx, 0xD5D50004
        jne     .fail
        cmp     esi, 0x51510005
        jne     .fail
        cmp     edi, 0xE5E50006
        jne     .fail
        cmp     ebp, 0xF5F50007
        jne     .fail

        ; --- Public trampoline: null → ZF ---
        mov     eax, 0
        mov     ebx, 0xB6B60012
        mov     ecx, 0xC6C60013
        mov     edx, 0xD6D60014
        call    socket_check
        jz      @f
        jmp     .fail
@@:
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB6B60012
        jne     .fail
        cmp     ecx, 0xC6C60013
        jne     .fail
        cmp     edx, 0xD6D60014
        jne     .fail

        ; --- Temporarily plant one live node on net_sockets; hit + clear ZF ---
        lea     esi, [socket_check_smoke_arena + 16]
        mov     [net_sockets], esi
        mov     dword [esi], 0
        mov     eax, esi
        mov     ebx, 0xB7B70022
        mov     ecx, 0xC7C70023
        mov     edx, 0xD7D70024
        mov     edi, 0xE7E70025
        mov     ebp, 0xF7F70026
        call    socket_check
        jnz     @f
        jmp     .fail
@@:
        cmp     eax, esi
        jne     .fail
        cmp     ebx, 0xB7B70022
        jne     .fail
        cmp     ecx, 0xC7C70023
        jne     .fail
        cmp     edx, 0xD7D70024
        jne     .fail
        cmp     edi, 0xE7E70025
        jne     .fail
        cmp     ebp, 0xF7F70026
        jne     .fail

        ; Restore empty live list
        mov     dword [net_sockets], 0

        mov     [rust_socket_check_smoke_result], 0x4B484353 ; 'SCHK'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C53                ; Cut AS fail marker
        mov     ebx, 0x4B484353                ; 'SCHK'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_socket_check(candidate, net_sockets) -> EAX; ret 8
align 16
rust_socket_check:
        file '../../rust_kernel/kolibri_utils/out/rust_socket_check.bin'
