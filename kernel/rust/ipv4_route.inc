;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; ipv4_route — Rust embed + ABI smoke (Cut AC)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_ipv4_route).
;; See docs/migration/cut-ac-plan.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-ipv4-route.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_ipv4_route.bin
;;
;; kernel/network/IPv4.inc (USE_RUST_IPV4_ROUTE=1) trampolines
;; ipv4_route → rust_ipv4_route with injected IPv4_* / net_device_list.
;;
;; Table bases are trampoline-injected so the Rust blob stays reloc-free.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust ipv4_route path ran.
rust_ipv4_route_smoke_result dd 0
endg

uglobal
align 4
; Synthetic 16-slot IPv4 tables + device list for smoke.
ipv4_route_smoke_addr:          rd 16
ipv4_route_smoke_subnet:        rd 16
ipv4_route_smoke_gateway:       rd 16
ipv4_route_smoke_devlist:       rd 16
; Fake NET_DEVICE stubs (link_state at offset 24).
ipv4_route_smoke_dev0:          rb 32
ipv4_route_smoke_dev1:          rb 32
ipv4_route_smoke_dev2:          rb 32
; Live-table backup while public trampoline smoke plants synthetics.
ipv4_route_smoke_save_addr:     rd 16
ipv4_route_smoke_save_subnet:   rd 16
ipv4_route_smoke_save_gateway:  rd 16
ipv4_route_smoke_save_devlist:  rd 16
endg

; Called once from high_code near other network smokes.
; Exercises: rust_* on synthetic tables + public ipv4_route with register ABI
; after planting live IPv4_* / net_device_list (works for FASM or Rust gate).
; On mismatch: distinctive hang (does not reach desktop).
align 4
ipv4_route_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic tables / devices ---
        mov     edi, ipv4_route_smoke_addr
        mov     ecx, (16*4)/4
        xor     eax, eax
        rep     stosd
        mov     edi, ipv4_route_smoke_subnet
        mov     ecx, (16*4)/4
        rep     stosd
        mov     edi, ipv4_route_smoke_gateway
        mov     ecx, (16*4)/4
        rep     stosd
        mov     edi, ipv4_route_smoke_devlist
        mov     ecx, (16*4)/4
        rep     stosd
        mov     edi, ipv4_route_smoke_dev0
        mov     ecx, (32*3)/4
        rep     stosd

        ; Device list: slots 0..2 → fake stubs; link_state @ +24
        mov     dword [ipv4_route_smoke_devlist + 0], ipv4_route_smoke_dev0
        mov     dword [ipv4_route_smoke_devlist + 4], ipv4_route_smoke_dev1
        mov     dword [ipv4_route_smoke_devlist + 8], ipv4_route_smoke_dev2
        mov     dword [ipv4_route_smoke_dev0 + 24], 0xffffffff   ; loopback-style
        mov     dword [ipv4_route_smoke_dev1 + 24], 1
        mov     dword [ipv4_route_smoke_dev2 + 24], 1

        ; Base addrs: lo 127.0.0.1/8; eth0 10.0.0.2/8 gw 10.0.0.1
        mov     dword [ipv4_route_smoke_addr + 0], 0x0100007f
        mov     dword [ipv4_route_smoke_subnet + 0], 0x000000ff
        mov     dword [ipv4_route_smoke_addr + 4], 0x0200000a
        mov     dword [ipv4_route_smoke_subnet + 4], 0x000000ff
        mov     dword [ipv4_route_smoke_gateway + 4], 0x0100000a

        ; --- Direct rust_*: on-link fill source when EDX was 0 ---
        sub     esp, 8
        mov     esi, esp
        lea     ebp, [esp+4]
        stdcall rust_ipv4_route, 0x0300000a, 0, 0, \
                ipv4_route_smoke_addr, ipv4_route_smoke_subnet, \
                ipv4_route_smoke_gateway, ipv4_route_smoke_devlist, esi, ebp
        cmp     eax, 0x0300000a
        jne     .fail_stack
        cmp     dword [esp], 0x0200000a
        jne     .fail_stack
        cmp     dword [esp+4], 4
        jne     .fail_stack
        add     esp, 8

        ; --- Direct rust_*: off-link → first gateway (skip loopback) ---
        sub     esp, 8
        mov     esi, esp
        lea     ebp, [esp+4]
        stdcall rust_ipv4_route, 0x01000008, 0, 0, \
                ipv4_route_smoke_addr, ipv4_route_smoke_subnet, \
                ipv4_route_smoke_gateway, ipv4_route_smoke_devlist, esi, ebp
        cmp     eax, 0x0100000a
        jne     .fail_stack
        cmp     dword [esp], 0x0200000a
        jne     .fail_stack
        cmp     dword [esp+4], 4
        jne     .fail_stack
        add     esp, 8

        ; --- Broadcast prefers link without IP (slot 2) ---
        mov     dword [ipv4_route_smoke_addr + 8], 0
        sub     esp, 8
        mov     esi, esp
        lea     ebp, [esp+4]
        stdcall rust_ipv4_route, 0xffffffff, 0, 0, \
                ipv4_route_smoke_addr, ipv4_route_smoke_subnet, \
                ipv4_route_smoke_gateway, ipv4_route_smoke_devlist, esi, ebp
        cmp     eax, 0xffffffff
        jne     .fail_stack
        cmp     dword [esp+4], 8
        jne     .fail_stack
        add     esp, 8

        ; --- Explicit device on-link; source overwritten from table ---
        sub     esp, 8
        mov     esi, esp
        lea     ebp, [esp+4]
        stdcall rust_ipv4_route, 0x0300000a, ipv4_route_smoke_dev1, 0x11111111, \
                ipv4_route_smoke_addr, ipv4_route_smoke_subnet, \
                ipv4_route_smoke_gateway, ipv4_route_smoke_devlist, esi, ebp
        cmp     eax, 0x0300000a
        jne     .fail_stack
        cmp     dword [esp], 0x0200000a
        jne     .fail_stack
        cmp     dword [esp+4], 4
        jne     .fail_stack
        add     esp, 8

        ; --- Unknown device fail: EAX=0, EDI=-1; source preserved ---
        sub     esp, 8
        mov     esi, esp
        lea     ebp, [esp+4]
        stdcall rust_ipv4_route, 0x01000008, 0xDEADBEEF, 0xCAFEBABE, \
                ipv4_route_smoke_addr, ipv4_route_smoke_subnet, \
                ipv4_route_smoke_gateway, ipv4_route_smoke_devlist, esi, ebp
        test    eax, eax
        jnz     .fail_stack
        cmp     dword [esp], 0xCAFEBABE
        jne     .fail_stack
        cmp     dword [esp+4], -1
        jne     .fail_stack
        add     esp, 8

        ; --- Plant synthetics into live globals for public ipv4_route ---
        mov     esi, IPv4_address
        mov     edi, ipv4_route_smoke_save_addr
        mov     ecx, 16
        rep     movsd
        mov     esi, IPv4_subnet
        mov     edi, ipv4_route_smoke_save_subnet
        mov     ecx, 16
        rep     movsd
        mov     esi, IPv4_gateway
        mov     edi, ipv4_route_smoke_save_gateway
        mov     ecx, 16
        rep     movsd
        mov     esi, net_device_list
        mov     edi, ipv4_route_smoke_save_devlist
        mov     ecx, 16
        rep     movsd

        mov     esi, ipv4_route_smoke_addr
        mov     edi, IPv4_address
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_subnet
        mov     edi, IPv4_subnet
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_gateway
        mov     edi, IPv4_gateway
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_devlist
        mov     edi, net_device_list
        mov     ecx, 16
        rep     movsd

        ; Public: on-link fill source; ESI/EBP canaries (EBX/ECX may clobber)
        mov     eax, 0x0300000a
        xor     ebx, ebx
        xor     edx, edx
        mov     esi, 0x51AC0001
        mov     ebp, 0xF1AC0002
        call    ipv4_route
        cmp     eax, 0x0300000a
        jne     .fail_restore
        cmp     edx, 0x0200000a
        jne     .fail_restore
        cmp     edi, 4
        jne     .fail_restore
        cmp     esi, 0x51AC0001
        jne     .fail_restore
        cmp     ebp, 0xF1AC0002
        jne     .fail_restore

        ; Public: off-link gateway
        mov     eax, 0x01000008
        xor     ebx, ebx
        xor     edx, edx
        mov     esi, 0x51AC0011
        mov     ebp, 0xF1AC0012
        call    ipv4_route
        cmp     eax, 0x0100000a
        jne     .fail_restore
        cmp     edx, 0x0200000a
        jne     .fail_restore
        cmp     edi, 4
        jne     .fail_restore
        cmp     esi, 0x51AC0011
        jne     .fail_restore
        cmp     ebp, 0xF1AC0012
        jne     .fail_restore

        ; Public: broadcast pick (slot 2, no IP)
        mov     eax, 0xffffffff
        xor     ebx, ebx
        xor     edx, edx
        mov     esi, 0x51AC0021
        mov     ebp, 0xF1AC0022
        call    ipv4_route
        cmp     eax, 0xffffffff
        jne     .fail_restore
        cmp     edi, 8
        jne     .fail_restore
        cmp     esi, 0x51AC0021
        jne     .fail_restore
        cmp     ebp, 0xF1AC0022
        jne     .fail_restore

        ; Public: explicit device
        mov     eax, 0x0300000a
        mov     ebx, ipv4_route_smoke_dev1
        mov     edx, 0x11111111
        mov     esi, 0x51AC0031
        mov     ebp, 0xF1AC0032
        call    ipv4_route
        cmp     eax, 0x0300000a
        jne     .fail_restore
        cmp     edx, 0x0200000a
        jne     .fail_restore
        cmp     edi, 4
        jne     .fail_restore
        cmp     esi, 0x51AC0031
        jne     .fail_restore
        cmp     ebp, 0xF1AC0032
        jne     .fail_restore

        ; Public: unknown device fail
        mov     eax, 0x01000008
        mov     ebx, 0xDEADBEEF
        mov     edx, 0xCAFEBABE
        mov     esi, 0x51AC0041
        mov     ebp, 0xF1AC0042
        call    ipv4_route
        test    eax, eax
        jnz     .fail_restore
        cmp     edi, -1
        jne     .fail_restore
        cmp     edx, 0xCAFEBABE
        jne     .fail_restore
        cmp     esi, 0x51AC0041
        jne     .fail_restore
        cmp     ebp, 0xF1AC0042
        jne     .fail_restore

        ; Restore live tables
        mov     esi, ipv4_route_smoke_save_addr
        mov     edi, IPv4_address
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_save_subnet
        mov     edi, IPv4_subnet
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_save_gateway
        mov     edi, IPv4_gateway
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_save_devlist
        mov     edi, net_device_list
        mov     ecx, 16
        rep     movsd

        mov     [rust_ipv4_route_smoke_result], 0x49505254 ; 'IPRT'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail_stack:
        add     esp, 8
        jmp     .fail
.fail_restore:
        mov     esi, ipv4_route_smoke_save_addr
        mov     edi, IPv4_address
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_save_subnet
        mov     edi, IPv4_subnet
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_save_gateway
        mov     edi, IPv4_gateway
        mov     ecx, 16
        rep     movsd
        mov     esi, ipv4_route_smoke_save_devlist
        mov     edi, net_device_list
        mov     ecx, 16
        rep     movsd
.fail:
        mov     eax, 0xDEAD0CAC                ; Cut AC fail marker
        mov     ebx, 0x49505254                ; 'IPRT'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_ipv4_route(dest, device, source, addr, subnet, gw, devlist,
;                   source_out, device_idx_out) -> EAX; ret 36
align 16
rust_ipv4_route:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_ipv4_route.bin'
