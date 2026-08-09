;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; tcp_xmit_timer — Rust embed + runtime smoke (Cut M)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_tcp_xmit_timer).
;; See docs/migration/cut-m-plan.md / cut-m-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-tcp-xmit-timer.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_tcp_xmit_timer.bin
;;
;; kernel/network/tcp_subr.inc (USE_RUST_TCP_XMIT_TIMER=1) trampolines
;; tcp_xmit_timer → rust_tcp_xmit_timer.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust TCP xmit timer path ran.
rust_tcp_xmit_timer_smoke_result dd 0
endg

; Called once from high_code after Cut L smoke.
; Exercises: public symbol → trampoline → Rust → socket field mutate.
; On mismatch: distinctive hang (does not reach desktop).
align 4
tcp_xmit_timer_rust_smoke_test:
        push    ebp
        mov     ebp, esp
        sub     esp, 256

        ; Zero fake TCP_SOCKET scratch at [ebp-256]
        push    edi ecx eax
        lea     edi, [ebp-256]
        mov     ecx, 256/4
        xor     eax, eax
        rep stosd
        pop     eax ecx edi

        lea     ebx, [ebp-256]

        ; --- Init path: t_rtt=0, rtt=5 → srtt=40, rttvar=10 ---
        mov     dword [ebx+202], 0      ; t_rtt
        mov     dword [ebx+210], 0xDEADBEEF
        mov     dword [ebx+214], 0xCAFEBABE
        push    dword [TCPS_rttupdated]
        mov     eax, 5
        call    tcp_xmit_timer
        cmp     dword [ebx+210], 40     ; t_srtt = 5<<3
        jne     .fail
        cmp     dword [ebx+214], 10     ; t_rttvar = 5<<1
        jne     .fail
        cmp     dword [ebx+202], 0      ; t_rtt unchanged
        jne     .fail
        pop     eax
        mov     ecx, [TCPS_rttupdated]
        sub     ecx, eax
        cmp     ecx, 1                  ; counter bumped once
        jne     .fail

        ; --- Update path + register preservation ---
        ; t_rtt=1, srtt=40, rttvar=10, rtt=5
        ; delta = 5 - (40>>3) - 1 = -1 = 0xFFFFFFFF
        ; FASM/Rust use unsigned ADD + JA: 40+(-1) sets CF → clamp srtt=1;
        ; abs(delta)=1; 1-(10>>2)=-1 → 10+(-1) sets CF → clamp rttvar=1.
        ; (Signed RFC-style 39/9 is NOT what add/ja implements.)
        mov     dword [ebx+202], 1
        mov     dword [ebx+210], 40
        mov     dword [ebx+214], 10
        mov     eax, 5
        mov     ecx, 0xC2000005
        mov     edx, 0xD2000006
        mov     esi, 0x52000007
        mov     edi, 0xE2000008
        call    tcp_xmit_timer
        cmp     ecx, 0xC2000005
        jne     .fail
        cmp     edx, 0xD2000006
        jne     .fail
        cmp     esi, 0x52000007
        jne     .fail
        cmp     edi, 0xE2000008
        jne     .fail
        cmp     dword [ebx+210], 1
        jne     .fail
        cmp     dword [ebx+214], 1
        jne     .fail

        ; --- Clamp path: t_rtt=1, srtt=1, rttvar=4, rtt=0 ---
        ; delta=-1; srtt 1+(-1)=0 → JA clamp 1; rttvar 4+(1-1)=4
        mov     dword [ebx+202], 1
        mov     dword [ebx+210], 1
        mov     dword [ebx+214], 4
        xor     eax, eax
        call    tcp_xmit_timer
        cmp     dword [ebx+210], 1
        jne     .fail
        cmp     dword [ebx+214], 4
        jne     .fail

        mov     [rust_tcp_xmit_timer_smoke_result], 0x5443504D ; 'TCPM'
        mov     esp, ebp
        pop     ebp
        ret
.fail:
        mov     eax, 0xDEAD0C4D        ; Cut M fail marker
        mov     ebx, 0x5443504D        ; 'TCPM'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_tcp_xmit_timer(rtt, socket) -> void; ret 8
align 16
rust_tcp_xmit_timer:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_tcp_xmit_timer.bin'
