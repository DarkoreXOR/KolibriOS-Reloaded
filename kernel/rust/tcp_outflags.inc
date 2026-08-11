;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; tcp_outflags — Rust embed + ABI smoke (Cut BD)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_tcp_outflags).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_tcp_outflags.bin
;;
;; Requires USE_RUST_TCP_OUTFLAGS=1 so public tcp_outflags hits
;; the trampoline (not the FASM body).
;;
;; Critical ABI: EAX→socket in, EDX=flags out; preserve EAX/EBX/ECX
;; (REG-001 — tcp_output keeps socket/window/data live across call).
;; REG-003: smoke uses stack synthetic TCP_SOCKET only — never mutate
;; live socket list / net_device_list.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust tcp_outflags path ran.
rust_tcp_outflags_smoke_result dd 0
endg

; Called once during early init (with other diagnostic smokes).
; Exercises rust_* + public trampoline on a stack scratch socket only.
; On mismatch: distinctive hang (does not reach desktop).
align 4
tcp_outflags_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 128

        ; Zero synthetic TCP_SOCKET scratch at [ebp-128]
        push    edi ecx eax
        lea     edi, [ebp-128]
        mov     ecx, 128/4
        xor     eax, eax
        rep stosd
        pop     eax ecx edi

        lea     ebx, [ebp-128]

        ; --- Direct rust_*: ESTABLISHED (4) → TH_ACK (0x10) ---
        mov     dword [ebx+114], 4              ; t_state
        stdcall rust_tcp_outflags, ebx
        cmp     eax, 0x10
        jne     .fail

        ; --- Direct rust_*: CLOSED (0) → TH_RST|TH_ACK (0x14) ---
        mov     dword [ebx+114], 0
        stdcall rust_tcp_outflags, ebx
        cmp     eax, 0x14
        jne     .fail

        ; --- Direct rust_*: LISTEN (1) → 0 ---
        mov     dword [ebx+114], 1
        stdcall rust_tcp_outflags, ebx
        test    eax, eax
        jnz     .fail

        ; --- Direct rust_*: SYN_SENT (2) → TH_SYN (0x02) ---
        mov     dword [ebx+114], 2
        stdcall rust_tcp_outflags, ebx
        cmp     eax, 2
        jne     .fail

        ; --- Direct rust_*: FIN_WAIT_1 (6) → TH_FIN|TH_ACK (0x11) ---
        mov     dword [ebx+114], 6
        stdcall rust_tcp_outflags, ebx
        cmp     eax, 0x11
        jne     .fail

        ; --- Public trampoline: EDX out + EAX/EBX/ECX (+siblings) canaries ---
        mov     dword [ebx+114], 4              ; ESTABLISHED
        mov     eax, ebx                        ; socket
        mov     ecx, 0xC3C3C3C3
        mov     edx, 0xD4D4D4D4
        mov     esi, 0x51515151
        mov     edi, 0xE5E5E5E5
        ; EBX already scratch base — use a distinct canary via push
        push    ebx
        mov     ebx, 0xB2B2B2B2
        call    tcp_outflags
        cmp     eax, [esp]                      ; EAX must remain socket
        jne     .fail_pop
        cmp     ebx, 0xB2B2B2B2
        jne     .fail_pop
        cmp     ecx, 0xC3C3C3C3
        jne     .fail_pop
        cmp     edx, 0x10                       ; TH_ACK
        jne     .fail_pop
        cmp     esi, 0x51515151
        jne     .fail_pop
        cmp     edi, 0xE5E5E5E5
        jne     .fail_pop
        pop     ebx

        ; --- Public: CLOSED → EDX=0x14; EAX preserved ---
        mov     dword [ebx+114], 0
        mov     eax, ebx
        mov     ecx, 0xC1000001
        call    tcp_outflags
        cmp     eax, ebx
        jne     .fail
        cmp     edx, 0x14
        jne     .fail
        cmp     ecx, 0xC1000001
        jne     .fail

        ; --- Public: SYN_RECEIVED (3) → TH_SYN|TH_ACK (0x12) ---
        mov     dword [ebx+114], 3
        mov     eax, ebx
        call    tcp_outflags
        cmp     edx, 0x12
        jne     .fail
        cmp     eax, ebx
        jne     .fail

        mov     [rust_tcp_outflags_smoke_result], 0x46435054 ; 'TCPF'
        mov     esp, ebp
        pop     ebp
        ret

.fail_pop:
        pop     ebx
.fail:
        mov     eax, 0xDEAD0CBD                ; Cut BD fail marker
        mov     ebx, 0x46435054                ; 'TCPF'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_tcp_outflags(socket) -> EAX = flags; ret 4
align 16
rust_tcp_outflags:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_tcp_outflags.bin'
