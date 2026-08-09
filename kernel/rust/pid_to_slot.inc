;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; pid_to_slot — Rust embed + ABI smoke (Cut AA)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_pid_to_slot).
;; See docs/migration/cut-aa-plan.md / cut-aa-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-pid-to-slot.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_pid_to_slot.bin
;;
;; kernel/core/taskman.inc (USE_RUST_PID_TO_SLOT=1) trampolines
;; pid_to_slot → rust_pid_to_slot.
;;
;; SLOT_BASE and thread_count are injected by the trampoline so the Rust
;; blob never references those globals.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust pid_to_slot path ran.
rust_pid_to_slot_smoke_result dd 0
endg

uglobal
align 4
; Synthetic APPDATA table: 4 slots × 256 bytes.
pid_slot_smoke_table:   rb 256*4
endg

; Called once from high_code after OS/IDLE SLOT_BASE setup (thread_count=2).
; Exercises: synthetic rust_* table walk + public trampoline on live SLOT_BASE.
; On mismatch: distinctive hang (does not reach desktop).
align 4
pid_to_slot_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- Clear synthetic table to TSTATE_FREE ---
        mov     edi, pid_slot_smoke_table
        mov     ecx, (256*4)/4
        xor     eax, eax
        rep     stosd
        mov     byte [pid_slot_smoke_table + 0*256 + 124], 9   ; FREE
        mov     byte [pid_slot_smoke_table + 1*256 + 124], 9
        mov     byte [pid_slot_smoke_table + 2*256 + 124], 9
        mov     byte [pid_slot_smoke_table + 3*256 + 124], 9

        ; Plant: slot1 tid=0xA1, slot2 tid=0xA2 (running), slot3 free with decoy tid
        mov     dword [pid_slot_smoke_table + 1*256 + 112], 0xA1
        mov     byte  [pid_slot_smoke_table + 1*256 + 124], 0
        mov     dword [pid_slot_smoke_table + 2*256 + 112], 0xA2
        mov     byte  [pid_slot_smoke_table + 2*256 + 124], 0
        mov     dword [pid_slot_smoke_table + 3*256 + 112], 0xA3
        mov     byte  [pid_slot_smoke_table + 3*256 + 124], 9   ; FREE — must skip
        ; Slot 0 decoy (must never match)
        mov     dword [pid_slot_smoke_table + 0*256 + 112], 0xA1
        mov     byte  [pid_slot_smoke_table + 0*256 + 124], 0

        ; --- Direct rust_*: find 0xA1 → slot 1 ---
        stdcall rust_pid_to_slot, 0xA1, pid_slot_smoke_table, 3
        cmp     eax, 1
        jne     .fail

        ; --- Find 0xA2 → slot 2 ---
        stdcall rust_pid_to_slot, 0xA2, pid_slot_smoke_table, 3
        cmp     eax, 2
        jne     .fail

        ; --- Free decoy 0xA3 → 0 ---
        stdcall rust_pid_to_slot, 0xA3, pid_slot_smoke_table, 3
        test    eax, eax
        jnz     .fail

        ; --- Missing → 0 ---
        stdcall rust_pid_to_slot, 0xDEAD, pid_slot_smoke_table, 3
        test    eax, eax
        jnz     .fail

        ; --- thread_count=1 excludes slot 2 ---
        stdcall rust_pid_to_slot, 0xA2, pid_slot_smoke_table, 1
        test    eax, eax
        jnz     .fail

        ; --- Public trampoline on live SLOT_BASE (after OS/IDLE setup) ---
        ; Read OS slot tid (slot 2) and look it up; expect EAX=2.
        ; Preserve-register adversarial canaries around the call.
        mov     eax, [SLOT_BASE + 2*256 + 112]          ; APPDATA.tid
        mov     ebx, 0xB1AA0001
        mov     ecx, 0xC1AA0002
        mov     edx, 0xD1AA0003
        mov     esi, 0x51AA0004
        mov     edi, 0xE1AA0005
        mov     ebp, 0xF1AA0006
        call    pid_to_slot
        cmp     eax, 2
        jne     .fail
        cmp     ebx, 0xB1AA0001
        jne     .fail
        cmp     ecx, 0xC1AA0002
        jne     .fail
        cmp     edx, 0xD1AA0003
        jne     .fail
        cmp     esi, 0x51AA0004
        jne     .fail
        cmp     edi, 0xE1AA0005
        jne     .fail
        cmp     ebp, 0xF1AA0006
        jne     .fail

        ; --- Missing live TID → 0; registers still preserved ---
        mov     eax, 0xEEEEEEEE
        mov     ebx, 0xB1AA0011
        mov     ecx, 0xC1AA0012
        mov     edx, 0xD1AA0013
        mov     esi, 0x51AA0014
        mov     edi, 0xE1AA0015
        mov     ebp, 0xF1AA0016
        call    pid_to_slot
        test    eax, eax
        jnz     .fail
        cmp     ebx, 0xB1AA0011
        jne     .fail
        cmp     ecx, 0xC1AA0012
        jne     .fail
        cmp     edx, 0xD1AA0013
        jne     .fail
        cmp     esi, 0x51AA0014
        jne     .fail
        cmp     edi, 0xE1AA0015
        jne     .fail
        cmp     ebp, 0xF1AA0016
        jne     .fail

        ; --- IDLE slot 1 tid lookup ---
        mov     eax, [SLOT_BASE + 1*256 + 112]
        call    pid_to_slot
        cmp     eax, 1
        jne     .fail

        mov     [rust_pid_to_slot_smoke_result], 0x5054534C ; 'PTSL'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0CAA                ; Cut AA fail marker
        mov     ebx, 0x5054534C                ; 'PTSL'
        mov     ecx, 0x4641494C                ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_pid_to_slot(pid, slot_base, thread_count) -> EAX; ret 12
align 16
rust_pid_to_slot:
        file '../../rust_kernel/kolibri_utils/out/rust_pid_to_slot.bin'
