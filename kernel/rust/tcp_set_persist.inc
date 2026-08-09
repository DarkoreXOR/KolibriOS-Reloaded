;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; tcp_set_persist — Rust embed + runtime smoke (Cut V)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_tcp_set_persist).
;; See docs/migration/cut-v-plan.md / cut-v-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-tcp-set-persist.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_tcp_set_persist.bin
;;
;; kernel/network/tcp_subr.inc (USE_RUST_TCP_SET_PERSIST=1) trampolines
;; tcp_set_persist → rust_tcp_set_persist.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust TCP persist path ran.
rust_tcp_set_persist_smoke_result dd 0
endg

; Called once from high_code after Cut U smoke.
; Exercises: public symbol → trampoline → Rust → socket field mutate.
; On mismatch: distinctive hang (does not reach desktop).
align 4
tcp_set_persist_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 288

        ; Zero fake TCP_SOCKET scratch at [ebp-288]
        push    edi ecx eax
        lea     edi, [ebp-288]
        mov     ecx, 288/4
        xor     eax, eax
        rep stosd
        pop     eax ecx edi

        lea     ebx, [ebp-288]

        ; --- Retransmit gate: flags bit0 set → no mutation ---
        mov     dword [ebx+210], 40             ; t_srtt
        mov     dword [ebx+214], 10             ; t_rttvar
        mov     byte  [ebx+118], 0              ; t_rxtshift
        mov     dword [ebx+254], 1              ; timer_flag_retransmission
        mov     dword [ebx+262], 0xDEADBEEF     ; timer_persist
        mov     eax, ebx
        mov     ecx, 0xC2000005
        mov     edx, 0xD2000006
        mov     esi, 0x52000007
        mov     edi, 0xE2000008
        call    tcp_set_persist
        cmp     eax, ebx                        ; EAX must remain socket
        jne     .fail
        cmp     ecx, 0xC2000005
        jne     .fail
        cmp     edx, 0xD2000006
        jne     .fail
        cmp     esi, 0x52000007
        jne     .fail
        cmp     edi, 0xE2000008
        jne     .fail
        cmp     dword [ebx+262], 0xDEADBEEF
        jne     .fail
        cmp     dword [ebx+254], 1
        jne     .fail
        cmp     byte  [ebx+118], 0
        jne     .fail

        ; --- First arm: srtt=40, rttvar=10, shift=0 ---
        ; ((40>>2)+10)>>1 = 10 → persist=10; flags|=8; rxtshift=1
        mov     dword [ebx+210], 40
        mov     dword [ebx+214], 10
        mov     byte  [ebx+118], 0
        mov     dword [ebx+254], 0
        mov     dword [ebx+262], 0
        mov     eax, ebx
        call    tcp_set_persist
        cmp     eax, ebx
        jne     .fail
        cmp     dword [ebx+262], 10
        jne     .fail
        cmp     dword [ebx+254], 8              ; timer_flag_persist
        jne     .fail
        cmp     byte  [ebx+118], 1
        jne     .fail

        ; --- Min clamp: srtt=0,rttvar=0 → raw 0 → persist=8 ---
        mov     dword [ebx+210], 0
        mov     dword [ebx+214], 0
        mov     byte  [ebx+118], 0
        mov     dword [ebx+254], 0
        mov     dword [ebx+262], 99
        mov     eax, ebx
        call    tcp_set_persist
        cmp     dword [ebx+262], 8
        jne     .fail
        cmp     dword [ebx+254], 8
        jne     .fail
        cmp     byte  [ebx+118], 1
        jne     .fail

        ; --- Max clamp + shift: srtt=64,rttvar=0,shift=4 → 8<<4=128 → 94 ---
        mov     dword [ebx+210], 64
        mov     dword [ebx+214], 0
        mov     byte  [ebx+118], 4
        mov     dword [ebx+254], 0
        mov     dword [ebx+262], 0
        mov     eax, ebx
        call    tcp_set_persist
        cmp     dword [ebx+262], 94
        jne     .fail
        cmp     byte  [ebx+118], 5
        jne     .fail

        ; --- rxtshift saturate at 12 ---
        mov     dword [ebx+210], 40
        mov     dword [ebx+214], 10
        mov     byte  [ebx+118], 12
        mov     dword [ebx+254], 0
        mov     dword [ebx+262], 0
        mov     eax, ebx
        call    tcp_set_persist
        cmp     byte  [ebx+118], 12
        jne     .fail
        cmp     dword [ebx+254], 8
        jne     .fail

        ; --- EBX preserve across call (legacy push/pop) ---
        mov     dword [ebx+210], 40
        mov     dword [ebx+214], 10
        mov     byte  [ebx+118], 0
        mov     dword [ebx+254], 0
        mov     eax, ebx
        mov     ecx, ebx                        ; save expected EBX in ECX briefly
        push    ecx
        mov     ebx, 0xB2000009
        call    tcp_set_persist
        cmp     ebx, 0xB2000009
        jne     .fail
        pop     ecx
        mov     ebx, ecx                        ; restore scratch base
        cmp     eax, ebx
        jne     .fail

        mov     [rust_tcp_set_persist_smoke_result], 0x54435056 ; 'TCPV'
        mov     esp, ebp
        pop     ebp
        ret
.fail:
        mov     eax, 0xDEAD0C56                ; Cut V fail marker
        mov     ebx, 0x54435056                ; 'TCPV'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_tcp_set_persist(socket) -> void; ret 4
align 16
rust_tcp_set_persist:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_tcp_set_persist.bin'
