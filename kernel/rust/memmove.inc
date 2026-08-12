;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; memmove — Rust embed + ABI smoke (Cut CJ)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_memmove).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_memmove.bin
;;
;; Requires USE_RUST_MEMMOVE=1 so public memmove hits the trampoline
;; (not the FASM body). Independent of Cuts CC–CI.
;;
;; Critical ABI: register (EAX=from, EBX=to, ECX=nbytes) → same registers
;; restored; forward-only copy; signed ecx≤0 no-op.
;; rust_memmove stdcall(from, to, nbytes); ret 12.
;; Trampoline preserves EAX/EBX/ECX/EDX/ESI/EDI/EBP (REG-001/011).
;; Smoke uses stack synthetic buffer only (REG-003; no large .bss).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust memmove path ran.
rust_memmove_smoke_result dd 0
endg

; Called once from early smoke cluster.
; Stack layout after prologue:
;   [esp+0 .. esp+63]  synthetic buffer
; On mismatch: distinctive hang (does not reach desktop).
align 4
memmove_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp
        sub     esp, 64

        ; Fill buf[i] = i
        xor     ecx, ecx
.fill:
        cmp     ecx, 64
        jae     .filled
        mov     [esp + ecx], cl
        inc     ecx
        jmp     .fill
.filled:

        ; Direct rust_*: copy 16 bytes from buf+0 to buf+32
        mov     eax, esp
        lea     edx, [esp + 32]
        stdcall rust_memmove, eax, edx, 16
        xor     ecx, ecx
.check16:
        cmp     ecx, 16
        jae     .ok16
        mov     al, [esp + ecx]
        cmp     al, [esp + 32 + ecx]
        jne     .fail
        inc     ecx
        jmp     .check16
.ok16:

        ; Direct rust_*: nbytes=0 leaves buf[0] unchanged
        mov     byte [esp], 0xA5
        lea     eax, [esp + 1]
        mov     edx, esp
        stdcall rust_memmove, eax, edx, 0
        cmp     byte [esp], 0xA5
        jne     .fail

        ; Forward-overlap left-shift: from=buf+1, to=buf, n=8
        xor     ecx, ecx
.refill:
        cmp     ecx, 32
        jae     .refilled
        mov     al, cl
        add     al, 0x40
        mov     [esp + ecx], al
        inc     ecx
        jmp     .refill
.refilled:
        lea     eax, [esp + 1]
        mov     edx, esp
        stdcall rust_memmove, eax, edx, 8
        cmp     byte [esp], 0x41
        jne     .fail
        cmp     byte [esp + 7], 0x48
        jne     .fail

        ; Public trampoline: non-overlap + register canaries
        xor     ecx, ecx
.refill2:
        cmp     ecx, 64
        jae     .refilled2
        mov     [esp + ecx], cl
        inc     ecx
        jmp     .refill2
.refilled2:
        mov     eax, esp
        lea     ebx, [esp + 40]
        mov     ecx, 12
        mov     edx, 0xD3D30001
        mov     esi, 0xE4E40002
        mov     edi, 0xF5F50003
        mov     ebp, 0xA6A60004
        call    memmove
        ; EAX must still be `esp` (from), EBX `esp+40` (to), ECX 12
        cmp     eax, esp
        jne     .fail
        lea     eax, [esp + 40]
        cmp     ebx, eax
        jne     .fail
        cmp     ecx, 12
        jne     .fail
        cmp     edx, 0xD3D30001
        jne     .fail
        cmp     esi, 0xE4E40002
        jne     .fail
        cmp     edi, 0xF5F50003
        jne     .fail
        cmp     ebp, 0xA6A60004
        jne     .fail
        xor     ecx, ecx
.check12:
        cmp     ecx, 12
        jae     .ok12
        mov     al, [esp + ecx]
        cmp     al, [esp + 40 + ecx]
        jne     .fail
        inc     ecx
        jmp     .check12
.ok12:

        ; Public trampoline: forward-overlap + canaries (EAX/EBX/ECX already
        ; proven in the prior trampoline case; re-check EDX/ESI/EDI/EBP + data)
        xor     ecx, ecx
.refill3:
        cmp     ecx, 32
        jae     .refilled3
        mov     al, cl
        xor     al, 0x5A
        mov     [esp + ecx], al
        inc     ecx
        jmp     .refill3
.refilled3:
        lea     eax, [esp + 1]
        mov     ebx, esp
        mov     ecx, 16
        mov     edx, 0x11112222
        mov     esi, 0x33334444
        mov     edi, 0x55556666
        mov     ebp, 0x77778888
        call    memmove
        cmp     edx, 0x11112222
        jne     .fail
        cmp     esi, 0x33334444
        jne     .fail
        cmp     edi, 0x55556666
        jne     .fail
        cmp     ebp, 0x77778888
        jne     .fail
        cmp     ecx, 16
        jne     .fail
        mov     al, 1
        xor     al, 0x5A
        cmp     [esp], al
        jne     .fail

        mov     [rust_memmove_smoke_result], 'MMOV'
        add     esp, 64
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret

.fail:
        mov     [rust_memmove_smoke_result], 'DEAD'
        mov     eax, 0xDEAD0C6A
@@:
        cli
        hlt
        jmp     @b

; ---------------------------------------------------------------------------
; Reloc-free Rust blob (extracted). Path relative to this include (kernel/rust/).
; ---------------------------------------------------------------------------
align 4
rust_memmove:
        file '../../rust_kernel/kolibri_utils/out/rust_memmove.bin'
