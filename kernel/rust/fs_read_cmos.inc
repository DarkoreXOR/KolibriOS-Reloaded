;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; fsReadCMOS — Rust embed + ABI smoke (Cut CA)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_fs_read_cmos).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_fs_read_cmos.bin
;;
;; Requires USE_RUST_FS_READ_CMOS=1 so public fsReadCMOS hits the
;; trampoline (not the FASM body).
;;
;; Critical ABI: AL reg in → AX decoded out; upper EAX preserved;
;; ECX/EDX preserved (REG-001); ret (not stdcall) at public symbol.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_fs_read_cmos_smoke_result dd 0
endg

; Mock raw reader: returns fixed BCD nibbles without port I/O.
align 4
fs_read_cmos_smoke_raw_read:
        mov     eax, [esp+4]
        movzx   ecx, al
        cmp     ecx, 7
        je      .day
        cmp     ecx, 8
        je      .month
        cmp     ecx, 9
        je      .year
        cmp     ecx, 0
        je      .sec
        xor     eax, eax
        ret     4
.day:   mov     eax, 0x15
        ret     4
.month: mov     eax, 0x06
        ret     4
.year:  mov     eax, 0x26
        ret     4
.sec:   mov     eax, 0x30
        ret     4

; Called once from high_code after Cut BZ smoke.
align 4
fs_read_cmos_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- upper-EAX preservation quirk (live CMOS reg 7; AX in 0–99) ---
        mov     eax, 0xAABB0007
        mov     ebx, 0xE1E10001
        mov     ecx, 0xC2C20002
        mov     edx, 0xD3D30003
        call    fsReadCMOS
        test    ah, ah
        jnz     .fail
        mov     esi, eax
        and     esi, 0xFFFF0000
        cmp     esi, 0xAABB0000
        jne     .fail
        cmp     ebx, 0xE1E10001
        jne     .fail
        cmp     ecx, 0xC2C20002
        jne     .fail
        cmp     edx, 0xD3D30003
        jne     .fail

        ; --- fat_Write pattern: EBX live across get_time_for_file (REG-004) ---
        mov     ebx, 0xDEADBEEF
        mov     al, 0
        call    fsReadCMOS
        cmp     ebx, 0xDEADBEEF
        jne     .fail
        cmp     ax, 60
        jae     .fail

        ; --- direct rust_* with mock raw reader (deterministic) ---
        stdcall rust_fs_read_cmos, fs_read_cmos_smoke_raw_read, 7
        cmp     ax, 15
        jne     .fail
        stdcall rust_fs_read_cmos, fs_read_cmos_smoke_raw_read, 9
        cmp     ax, 26
        jne     .fail

        ; --- fs_read_cmos_stdcall wrapper (Cut BV compose; live CMOS) ---
        stdcall fs_read_cmos_stdcall, 8
        cmp     eax, 13
        ja      .fail

        mov     [rust_fs_read_cmos_smoke_result], 0x534D4F43 ; 'CMOS'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     eax, 0xDEAD02A1        ; Cut CA fail marker
        mov     ebx, 0x43415352        ; 'RCAS'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_fs_read_cmos(raw_read, reg) -> EAX (AX = decoded); ret 8
align 16
rust_fs_read_cmos:
        file '../../rust_kernel/kolibri_utils/out/rust_fs_read_cmos.bin'
