;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; net_ptr_to_num4 — Rust embed + ABI smoke (Cut AY)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_net_ptr_to_num4).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_net_ptr_to_num4.bin
;;
;; Requires USE_RUST_NET_PTR_TO_NUM4=1 so public net_ptr_to_num4 hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: EBX=device in → EDI=index×4/−1 out; preserve
;; EAX/EBX/ECX/EDX/ESI/EBP (REG-001 — TCP send keeps EAX→socket).
;;
;; REG-003: this smoke runs AFTER stack_init/loop_init. Do NOT plant over
;; or zero net_device_list[0] — that destroys the live loopback entry.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust net_ptr_to_num4 path ran.
rust_net_ptr_to_num4_smoke_result dd 0
endg

uglobal
align 4
; Synthetic device-list arena: 16 dword slots (matches NET_DEVICES_MAX).
net_ptr_to_num4_smoke_list: rd 16
endg

; Called once after stack_init (loopback already in net_device_list[0]).
; Exercises synthetic rust_* walk + public trampoline without mutating the
; live device list. On mismatch: distinctive hang (does not reach desktop).
align 4
net_ptr_to_num4_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic list ---
        cld
        mov     edi, net_ptr_to_num4_smoke_list
        mov     ecx, 16
        xor     eax, eax
        rep     stosd

        ; Plant devices at slots 0, 3, 15
        mov     dword [net_ptr_to_num4_smoke_list], 0x11110001
        mov     dword [net_ptr_to_num4_smoke_list + 12], 0x33330003
        mov     dword [net_ptr_to_num4_smoke_list + 60], 0xFFFF000F

        ; --- Direct rust_*: null → −1 ---
        stdcall rust_net_ptr_to_num4, 0, net_ptr_to_num4_smoke_list, 16
        cmp     eax, -1
        jne     .fail

        ; --- Hit slot 0 → 0 ---
        stdcall rust_net_ptr_to_num4, 0x11110001, net_ptr_to_num4_smoke_list, 16
        test    eax, eax
        jnz     .fail

        ; --- Hit slot 3 → 12 ---
        stdcall rust_net_ptr_to_num4, 0x33330003, net_ptr_to_num4_smoke_list, 16
        cmp     eax, 12
        jne     .fail

        ; --- Hit slot 15 → 60 ---
        stdcall rust_net_ptr_to_num4, 0xFFFF000F, net_ptr_to_num4_smoke_list, 16
        cmp     eax, 60
        jne     .fail

        ; --- Miss ---
        stdcall rust_net_ptr_to_num4, 0xDEADBEEF, net_ptr_to_num4_smoke_list, 16
        cmp     eax, -1
        jne     .fail

        ; --- Public trampoline: miss on live list + regs (no list mutate) ---
        mov     eax, 0xA5A50001
        mov     ebx, 0xCAFEBABE          ; non-member device
        mov     ecx, 0xC5C50003
        mov     edx, 0xD5D50004
        mov     esi, 0x51510005
        mov     ebp, 0xF5F50007
        call    net_ptr_to_num4
        cmp     edi, -1
        jne     .fail
        cmp     eax, 0xA5A50001
        jne     .fail
        cmp     ebx, 0xCAFEBABE
        jne     .fail
        cmp     ecx, 0xC5C50003
        jne     .fail
        cmp     edx, 0xD5D50004
        jne     .fail
        cmp     esi, 0x51510005
        jne     .fail
        cmp     ebp, 0xF5F50007
        jne     .fail

        ; --- Public trampoline: null device → −1; preserve EAX ---
        mov     eax, 0xA6A60011
        xor     ebx, ebx
        mov     ecx, 0xC6C60013
        mov     edx, 0xD6D60014
        mov     esi, 0x51610015
        mov     ebp, 0xF6F60017
        call    net_ptr_to_num4
        cmp     edi, -1
        jne     .fail
        cmp     eax, 0xA6A60011
        jne     .fail
        test    ebx, ebx
        jnz     .fail
        cmp     ecx, 0xC6C60013
        jne     .fail
        cmp     edx, 0xD6D60014
        jne     .fail
        cmp     esi, 0x51610015
        jne     .fail
        cmp     ebp, 0xF6F60017
        jne     .fail

        ; --- Public trampoline: hit live loopback at slot 0 (REG-003) ---
        ; After stack_init/loop_init, net_device_list[0] = LOOPBACK_DEVICE.
        ; Do not overwrite or zero that slot.
        mov     ebx, [net_device_list]
        test    ebx, ebx
        jz      .fail
        mov     eax, 0xA7A70021
        mov     ecx, 0xC7C70023
        mov     edx, 0xD7D70024
        mov     esi, 0x51710025
        mov     ebp, 0xF7F70026
        push    ebx                      ; save expected device ptr
        call    net_ptr_to_num4
        pop     ebx
        test    edi, edi                 ; slot 0 → EDI=0
        jnz     .fail
        cmp     eax, 0xA7A70021
        jne     .fail
        cmp     ebx, [net_device_list]   ; EBX preserved as device ptr
        jne     .fail
        cmp     ecx, 0xC7C70023
        jne     .fail
        cmp     edx, 0xD7D70024
        jne     .fail
        cmp     esi, 0x51710025
        jne     .fail
        cmp     ebp, 0xF7F70026
        jne     .fail
        ; Live list untouched — loopback still at slot 0
        cmp     dword [net_device_list], ebx
        jne     .fail

        mov     [rust_net_ptr_to_num4_smoke_result], 0x4E505434 ; 'NPT4'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD0C59                ; Cut AY fail marker
        mov     ebx, 0x4E505434                ; 'NPT4'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_net_ptr_to_num4(device, list_base, max) -> EAX; ret 12
align 16
rust_net_ptr_to_num4:
        file '../../rust_kernel/kolibri_utils/out/rust_net_ptr_to_num4.bin'
