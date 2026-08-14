;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ipv4_output — Rust embed + ABI smoke (Cut CX)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ipv4_output) when USE_RUST_IPV4_OUTPUT=1.
;;
;; Critical ABI: rust_ipv4_output(ctx) ret 4 (REG-009). Snapshot
;; AX/EBX/ECX/EDX/EDI before the 56-byte stack ctx (REG-010). Capture eth
;; ZF inside the blob (REG-018). Marker 'IPV4' / fail hang 0xDEAD0C78.
;;
;; REG-012: blob + smoke are gated so OFF builds do not consume pack slack.
;; Smoke fixtures are stack-local; only the result marker is iglobal.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_ipv4_output_smoke_result dd 0
; Points at the active smoke stack frame while vectors run (callbacks).
ipv4_smoke_frame dd 0
endg

; Smoke stack frame layout (relative to [ipv4_smoke_frame]):
; +0  nexthop
; +4  source
; +8  devidx
; +12 arp_fail
; +16 eth_fail
; +20 buf
; +24 header
; +28 device
; +32 tx[16]
; +96 devlist[16]
; +160 headerbuf[64]
; size 224

if USE_RUST_IPV4_OUTPUT

align 4
rust_ipv4_output:
        file '../../rust_kernel/kolibri_utils/out/rust_ipv4_output.bin'

ipv4_smoke_route:
        mov     eax, [ipv4_smoke_frame]
        mov     edx, [eax+4]
        mov     edi, [eax+8]
        mov     eax, [eax]
        ret

ipv4_smoke_arp:
        mov     eax, [ipv4_smoke_frame]
        cmp     dword [eax+12], 0
        je      .ok
        mov     eax, 0xffffffff
        ret
.ok:
        mov     eax, 0x00005552
        mov     ebx, 0x56341200
        ret

ipv4_smoke_eth:
        mov     eax, [ipv4_smoke_frame]
        cmp     dword [eax+16], 0
        je      .ok
        xor     eax, eax
        ret
.ok:
        mov     ebx, [eax+28]
        mov     edx, 60
        mov     edi, [eax+24]
        mov     eax, [eax+20]
        test    eax, eax
        ret

ipv4_smoke_loop:
        mov     eax, [ipv4_smoke_frame]
        mov     ebx, 0xB10CB10C
        mov     edi, [eax+24]
        mov     eax, [eax+20]
        ret

align 4
ipv4_output_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp
        cld
        ; frame224 + ctx56 = 280
        sub     esp, 280
        lea     eax, [esp]
        mov     [ipv4_smoke_frame], eax
        mov     edi, eax
        mov     ecx, 280/4
        xor     eax, eax
        rep     stosd
        lea     eax, [esp+160]
        mov     [esp+24], eax                   ; header
        mov     dword [esp+20], 0xB1000001      ; buf
        mov     dword [esp+28], 0x0DEC0DE0      ; device
        mov     dword [esp+96+4], 0x0DEC0DE0    ; devlist[1]

        ; V1: UDP-like success
        mov     dword [esp], 0x0202000a
        mov     dword [esp+4], 0x0f02000a
        mov     dword [esp+8], 4
        lea     edi, [esp+224]
        mov     ecx, 56/4
        xor     eax, eax
        rep     stosd
        mov     word [esp+224], 0x1180
        mov     dword [esp+224+8], 18
        mov     dword [esp+224+16], 0x0202000a
        mov     dword [esp+224+20], ipv4_smoke_route
        mov     dword [esp+224+24], ipv4_smoke_arp
        mov     dword [esp+224+28], ipv4_smoke_eth
        mov     dword [esp+224+32], ipv4_smoke_loop
        lea     eax, [esp+32]
        mov     [esp+224+36], eax
        lea     eax, [esp+96]
        mov     [esp+224+40], eax
        mov     ebx, 0xC20000D1
        mov     esi, 0xC20000D2
        mov     ebp, 0xC20000D3
        lea     eax, [esp+224]
        stdcall rust_ipv4_output, eax
        cmp     ebx, 0xC20000D1
        jne     .fail
        cmp     esi, 0xC20000D2
        jne     .fail
        cmp     ebp, 0xC20000D3
        jne     .fail
        cmp     eax, 0xB1000001
        jne     .fail
        cmp     dword [esp+32+4], 1
        jne     .fail
        mov     esi, [esp+24]
        cmp     byte [esi], 0x45
        jne     .fail
        cmp     byte [esi+8], 128
        jne     .fail
        cmp     byte [esi+9], 17
        jne     .fail
        cmp     word [esi+10], 0xb722
        jne     .fail

        ; V2: too large
        mov     dword [esp+32+4], 0
        lea     edi, [esp+224]
        mov     ecx, 56/4
        xor     eax, eax
        rep     stosd
        mov     word [esp+224], 0x1180
        mov     dword [esp+224+8], 65501
        mov     dword [esp+224+16], 0x08080808
        mov     dword [esp+224+20], ipv4_smoke_route
        mov     dword [esp+224+24], ipv4_smoke_arp
        mov     dword [esp+224+28], ipv4_smoke_eth
        mov     dword [esp+224+32], ipv4_smoke_loop
        lea     eax, [esp+32]
        mov     [esp+224+36], eax
        lea     eax, [esp+96]
        mov     [esp+224+40], eax
        lea     eax, [esp+224]
        stdcall rust_ipv4_output, eax
        test    eax, eax
        jnz     .fail
        cmp     dword [esp+32+4], 0
        jne     .fail

        ; V3: no route
        mov     dword [esp], 0
        lea     edi, [esp+224]
        mov     ecx, 56/4
        xor     eax, eax
        rep     stosd
        mov     word [esp+224], 0x1180
        mov     dword [esp+224+8], 8
        mov     dword [esp+224+16], 0x08080808
        mov     dword [esp+224+20], ipv4_smoke_route
        mov     dword [esp+224+24], ipv4_smoke_arp
        mov     dword [esp+224+28], ipv4_smoke_eth
        mov     dword [esp+224+32], ipv4_smoke_loop
        lea     eax, [esp+32]
        mov     [esp+224+36], eax
        lea     eax, [esp+96]
        mov     [esp+224+40], eax
        lea     eax, [esp+224]
        stdcall rust_ipv4_output, eax
        test    eax, eax
        jnz     .fail

        ; V4: ARP fail
        mov     dword [esp], 0x0202000a
        mov     dword [esp+4], 0x0f02000a
        mov     dword [esp+8], 4
        mov     dword [esp+12], 1
        lea     edi, [esp+224]
        mov     ecx, 56/4
        xor     eax, eax
        rep     stosd
        mov     word [esp+224], 0x1180
        mov     dword [esp+224+8], 8
        mov     dword [esp+224+16], 0x08080808
        mov     dword [esp+224+20], ipv4_smoke_route
        mov     dword [esp+224+24], ipv4_smoke_arp
        mov     dword [esp+224+28], ipv4_smoke_eth
        mov     dword [esp+224+32], ipv4_smoke_loop
        lea     eax, [esp+32]
        mov     [esp+224+36], eax
        lea     eax, [esp+96]
        mov     [esp+224+40], eax
        lea     eax, [esp+224]
        stdcall rust_ipv4_output, eax
        test    eax, eax
        jnz     .fail
        cmp     dword [esp+32+4], 0
        jne     .fail
        and     dword [esp+12], 0

        ; V5: eth ZF, counter incremented
        mov     dword [esp+16], 1
        lea     edi, [esp+224]
        mov     ecx, 56/4
        xor     eax, eax
        rep     stosd
        mov     word [esp+224], 0x1180
        mov     dword [esp+224+8], 8
        mov     dword [esp+224+16], 0x08080808
        mov     dword [esp+224+20], ipv4_smoke_route
        mov     dword [esp+224+24], ipv4_smoke_arp
        mov     dword [esp+224+28], ipv4_smoke_eth
        mov     dword [esp+224+32], ipv4_smoke_loop
        lea     eax, [esp+32]
        mov     [esp+224+36], eax
        lea     eax, [esp+96]
        mov     [esp+224+40], eax
        lea     eax, [esp+224]
        stdcall rust_ipv4_output, eax
        test    eax, eax
        jnz     .fail
        cmp     dword [esp+32+4], 1
        jne     .fail
        and     dword [esp+16], 0

        ; V6: loopback
        mov     dword [esp+32], 0
        mov     dword [esp+32+4], 0
        mov     dword [esp], 0x0100007f
        mov     dword [esp+4], 0x0f02000a
        mov     dword [esp+8], 0
        mov     dword [esp+20], 0xB1000010
        lea     edi, [esp+224]
        mov     ecx, 56/4
        xor     eax, eax
        rep     stosd
        mov     word [esp+224], 0x0140
        mov     dword [esp+224+8], 0
        mov     dword [esp+224+16], 0x0100007f
        mov     dword [esp+224+20], ipv4_smoke_route
        mov     dword [esp+224+24], ipv4_smoke_arp
        mov     dword [esp+224+28], ipv4_smoke_eth
        mov     dword [esp+224+32], ipv4_smoke_loop
        lea     eax, [esp+32]
        mov     [esp+224+36], eax
        lea     eax, [esp+96]
        mov     [esp+224+40], eax
        lea     eax, [esp+224]
        stdcall rust_ipv4_output, eax
        cmp     eax, 0xB1000010
        jne     .fail
        cmp     dword [esp+32], 1
        jne     .fail
        mov     esi, [esp+24]
        cmp     dword [esi+12], 0x0100007f
        jne     .fail

        ; V7: AL/AH
        mov     dword [esp], 0x0202000a
        mov     dword [esp+4], 0x0f02000a
        mov     dword [esp+8], 4
        mov     dword [esp+20], 0xB1000001
        lea     edi, [esp+224]
        mov     ecx, 56/4
        xor     eax, eax
        rep     stosd
        mov     word [esp+224], 0x1107
        mov     dword [esp+224+8], 8
        mov     dword [esp+224+16], 0x08080808
        mov     dword [esp+224+20], ipv4_smoke_route
        mov     dword [esp+224+24], ipv4_smoke_arp
        mov     dword [esp+224+28], ipv4_smoke_eth
        mov     dword [esp+224+32], ipv4_smoke_loop
        lea     eax, [esp+32]
        mov     [esp+224+36], eax
        lea     eax, [esp+96]
        mov     [esp+224+40], eax
        lea     eax, [esp+224]
        stdcall rust_ipv4_output, eax
        test    eax, eax
        jz      .fail
        mov     esi, [esp+24]
        cmp     byte [esi+8], 7
        jne     .fail
        cmp     byte [esi+9], 17
        jne     .fail

        and     dword [ipv4_smoke_frame], 0
        mov     [rust_ipv4_output_smoke_result], 'IPV4'
        add     esp, 280
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        and     dword [ipv4_smoke_frame], 0
        mov     [rust_ipv4_output_smoke_result], 0xDEAD0C78
@@:
        jmp     @b

else

align 4
ipv4_output_rust_smoke_test:
        ret

end if
