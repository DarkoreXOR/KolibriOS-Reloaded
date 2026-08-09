;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; set_io_access_rights — Rust embed + ABI smoke (Cut X)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_set_io_access_rights).
;; See docs/migration/cut-x-plan.md / cut-x-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-set-io-access-rights.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_set_io_access_rights.bin
;;
;; kernel/kernel.asm (USE_RUST_SET_IO_ACCESS_RIGHTS=1) trampolines
;; set_io_access_rights → rust_set_io_access_rights.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust I/O-access path ran.
rust_set_io_access_rights_smoke_result dd 0
endg

; Called once from high_code after tss._io_map_0 is mapped/filled.
; Exercises: public ABI → trampoline → Rust → real tss._io_map_0 bit ops.
; Uses high unused ports (0xF000..) and restores prior bits.
; NOTE: Must NOT use EBP as a frame pointer — EBP is the legacy ABI input.
; On mismatch: distinctive hang (does not reach desktop).
align 4
set_io_access_rights_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Enable port 0xF010: clear bit; preserve EAX/EDI/EDX/EBP ---
        ; Save original bit state in BL (0/1)
        mov     eax, 0xF010
        mov     edi, tss._io_map_0
        bt      [edi], eax
        setc    bl                              ; BL = prior bit

        mov     eax, 0xF010
        xor     ebp, ebp                        ; enable
        mov     ecx, 0xC1000001
        mov     edx, 0xD1000002
        mov     esi, 0x51000003
        mov     edi, 0xE1000004
        call    set_io_access_rights
        cmp     eax, 0xF010
        jne     .fail
        cmp     ecx, 0xC1000001
        jne     .fail
        cmp     edx, 0xD1000002
        jne     .fail
        cmp     esi, 0x51000003
        jne     .fail
        cmp     edi, 0xE1000004
        jne     .fail
        test    ebp, ebp
        jnz     .fail

        ; Bit must be clear (enabled)
        mov     edi, tss._io_map_0
        bt      [edi], eax
        jc      .fail

        ; --- Disable same port: set bit; nonzero EBP ---
        mov     eax, 0xF010
        xor     ebp, ebp
        inc     ebp                             ; disable
        mov     ecx, 0xC1000011
        mov     edx, 0xD1000012
        mov     esi, 0x51000013
        mov     edi, 0xE1000014
        call    set_io_access_rights
        cmp     eax, 0xF010
        jne     .fail
        cmp     ebp, 1
        jne     .fail
        cmp     ecx, 0xC1000011
        jne     .fail
        cmp     edx, 0xD1000012
        jne     .fail
        cmp     esi, 0x51000013
        jne     .fail
        cmp     edi, 0xE1000014
        jne     .fail

        mov     edi, tss._io_map_0
        bt      [edi], eax
        jnc     .fail

        ; --- Nonzero EBP other than 1 still disables (BTS path) ---
        mov     eax, 0xF011
        ; ensure enabled first
        xor     ebp, ebp
        call    set_io_access_rights
        mov     edi, tss._io_map_0
        bt      [edi], eax
        jc      .fail

        mov     eax, 0xF011
        mov     ebp, 0xDEADBEEF                 ; any nonzero
        call    set_io_access_rights
        cmp     ebp, 0xDEADBEEF
        jne     .fail
        mov     edi, tss._io_map_0
        bt      [edi], eax
        jnc     .fail

        ; --- Range loop style: enable 0xF020..0xF027 ---
        mov     eax, 0xF020
        xor     ebp, ebp
.en_loop:
        call    set_io_access_rights
        inc     eax
        cmp     eax, 0xF027
        jbe     .en_loop
        mov     edi, tss._io_map_0
        mov     eax, 0xF020
.chk_en:
        bt      [edi], eax
        jc      .fail
        inc     eax
        cmp     eax, 0xF027
        jbe     .chk_en

        ; Disable that range
        mov     eax, 0xF020
        xor     ebp, ebp
        inc     ebp
.dis_loop:
        call    set_io_access_rights
        inc     eax
        cmp     eax, 0xF027
        jbe     .dis_loop
        mov     edi, tss._io_map_0
        mov     eax, 0xF020
.chk_dis:
        bt      [edi], eax
        jnc     .fail
        inc     eax
        cmp     eax, 0xF027
        jbe     .chk_dis

        ; --- Restore original bit for 0xF010 ---
        mov     eax, 0xF010
        test    bl, bl
        jz      .rest_clear
        xor     ebp, ebp
        inc     ebp                             ; was set → disable
        jmp     .rest_do
.rest_clear:
        xor     ebp, ebp                        ; was clear → enable
.rest_do:
        call    set_io_access_rights

        ; Restore 0xF011 to denied (boot default for unused high ports)
        mov     eax, 0xF011
        xor     ebp, ebp
        inc     ebp
        call    set_io_access_rights

        mov     [rust_set_io_access_rights_smoke_result], 0x494F4158 ; 'IOAX'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0C58                ; Cut X fail marker
        mov     ebx, 0x494F4158                ; 'IOAX'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_set_io_access_rights(port, clear_access, io_map) -> void; ret 12
align 16
rust_set_io_access_rights:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_set_io_access_rights.bin'
