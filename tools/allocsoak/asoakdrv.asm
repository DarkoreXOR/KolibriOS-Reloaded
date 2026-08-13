;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; asoakdrv.asm — disposable KolibriOS PE driver for Stage-4 allocator soak
;;
;; Calls exported KERNEL AllocPage / FreePage / AllocPages directly.
;; Loaded via syscall 68.21 from the MENUET loader (ALLOCSOK).
;; NOT a production driver — CoW / test-only.
;;
;; Seed: 0x5047424D ('PGBM')
;; Build: python scripts/build_asoakdrv.py
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

format PE DLL
entry START

include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'

; ---- limits ----------------------------------------------------------
MAX_LEDGER       = 2048          ; retain slots (safety — raise later for deep OOM)
PRESSURE_TARGET  = 512           ; AllocPage hammer retain count
MAX_OOM_EXTRA    = 256           ; bounded; full OOM may hang desktop bring-up
FRAG_N           = 64
SEED             = 0x5047424D

DRV_ENTRY_STATE  = 1
IOCTL_GET_REPORT = 1

; ---- report (copied to user via IOCTL) --------------------------------
; Layout must match host/Python struct documentation.
report_bytes = 512

section '.text' code readable executable

;=======================================================================
; load_pe_driver calls START as cdecl: push cmdline; push DRV_ENTRY; call;
; then pops three dwords. Use plain ret (not stdcall ret 4) to match.
; Return 0 after soak so fail_init frees the image (disposable; no RegService).
; Guest evidence is SysMsgBoardStr markers (ALLOCSOK …), not IOCTL.
START:
        cmp     dword [esp+4], DRV_ENTRY_STATE
        jne     .exit0
        call    run_soak
.exit0:
        xor     eax, eax
        ret

;=======================================================================
run_soak:
        pushad
        call    report_init
        mov     esi, msg_start
        call    [SysMsgBoardStr]

; --- PHASE A: baseline (host samples while we Delay) ------------------
        mov     ebx, 100                 ; 1.0s — host baseline window
        call    [Delay]
        or      dword [report.flags], 1  ; bit0 = baseline done
        mov     esi, msg_a
        call    [SysMsgBoardStr]

; --- Double-free early (safe, small) ----------------------------------
        mov     esi, msg_df
        call    [SysMsgBoardStr]
        call    test_double_free
        mov     ebx, 30
        call    [Delay]

; --- PHASE B: AllocPage hammer ----------------------------------------
        mov     esi, msg_b
        call    [SysMsgBoardStr]
        mov     ecx, PRESSURE_TARGET
.hammer:
        push    ecx
        call    [AllocPage]
        pop     ecx
        test    eax, eax
        jz      .hammer_fail
        call    ledger_push
        jc      .hammer_full
        inc     dword [report.ap_ok]
        ; yield every 64 allocs so host xp can sample
        test    ecx, 63
        jnz     .hammer_next
        call    [ChangeTask]
.hammer_next:
        loop    .hammer
        jmp     .hammer_done
.hammer_fail:
        inc     dword [report.ap_fail]
.hammer_full:
.hammer_done:
        or      dword [report.flags], 2  ; pressure done
        mov     eax, [report.ap_ok]
        mov     [report.pressure_ok], eax
        mov     esi, msg_b_done
        call    [SysMsgBoardStr]
        mov     ebx, 80                  ; host pressure sample window
        call    [Delay]

; --- PHASE E-ish: AllocPages table while free runs exist --------------
        mov     esi, msg_ap
        call    [SysMsgBoardStr]
        call    test_alloc_pages_table

; --- Fragmentation / reuse --------------------------------------------
        mov     esi, msg_frag
        call    [SysMsgBoardStr]
        call    test_fragmentation

; --- PHASE C: OOM hammer (bounded) ------------------------------------
        mov     esi, msg_oom
        call    [SysMsgBoardStr]
        call    test_oom

; --- Recovery: free everything, prove AllocPage works -----------------
        mov     esi, msg_rec
        call    [SysMsgBoardStr]
        call    ledger_free_all
        call    [AllocPage]
        test    eax, eax
        jz      .rec_fail
        call    [FreePage]
        or      dword [report.flags], 8  ; recovery ok
        jmp     .rec_done
.rec_fail:
        or      dword [report.flags], 16 ; recovery fail
.rec_done:

; --- finish ------------------------------------------------------------
        test    dword [report.flags], 16
        jnz     .fail
        or      dword [report.flags], 32 ; PASS bit
        call    print_stat_line
        mov     esi, msg_pass
        call    [SysMsgBoardStr]
        jmp     .out
.fail:
        call    print_stat_line
        mov     esi, msg_fail
        call    [SysMsgBoardStr]
.out:
        popad
        ret

; Print one machine-scrapeable line to the kernel message board.
print_stat_line:
        pushad
        mov     esi, msg_stat
        call    [SysMsgBoardStr]
        ; flags
        mov     eax, [report.flags]
        call    print_hex8
        mov     esi, msg_sp
        call    [SysMsgBoardStr]
        mov     eax, [report.ap_ok]
        call    print_hex8
        mov     esi, msg_sp
        call    [SysMsgBoardStr]
        mov     eax, [report.apages_ok]
        call    print_hex8
        mov     esi, msg_sp
        call    [SysMsgBoardStr]
        mov     eax, [report.oom_ops]
        call    print_hex8
        mov     esi, msg_cr
        call    [SysMsgBoardStr]
        popad
        ret

; eax = value → 8 hex digits to board
print_hex8:
        pushad
        mov     ecx, 8
        mov     ebx, eax
.ph:
        rol     ebx, 4
        mov     eax, ebx
        and     eax, 0xF
        cmp     al, 10
        jb      .dec
        add     al, 'A'-10
        jmp     .put
.dec:
        add     al, '0'
.put:
        mov     [hexbuf], al
        mov     esi, hexbuf
        ; SysMsgBoardStr expects ASCIIZ — emit one char via temporary
        mov     byte [hexbuf+1], 0
        call    [SysMsgBoardStr]
        loop    .ph
        popad
        ret

;=======================================================================
report_init:
        push    edi ecx eax
        mov     edi, report
        mov     ecx, report_bytes / 4
        xor     eax, eax
        rep stosd
        mov     dword [report.magic], 'AOSK'
        mov     dword [report.version], 1
        mov     dword [report.seed], SEED
        mov     dword [report.ledger_cap], MAX_LEDGER
        mov     dword [report.pressure_target], PRESSURE_TARGET
        pop     eax ecx edi
        ret

;=======================================================================
; CF=1 if full
ledger_push:
        ; eax = phys page
        push    ebx
        mov     ebx, [ledger_count]
        cmp     ebx, MAX_LEDGER
        jae     .full
        mov     [ledger + ebx*4], eax
        inc     dword [ledger_count]
        mov     [report.last_pa], eax
        mov     [report.outstanding], ebx
        inc     dword [report.outstanding]
        clc
        pop     ebx
        ret
.full:
        ; free immediately — cannot track
        call    [FreePage]
        stc
        pop     ebx
        ret

ledger_free_all:
        pushad
.loop:
        mov     ebx, [ledger_count]
        test    ebx, ebx
        jz      .done
        dec     ebx
        mov     eax, [ledger + ebx*4]
        mov     dword [ledger + ebx*4], 0
        mov     [ledger_count], ebx
        test    eax, eax
        jz      .loop
        call    [FreePage]
        inc     dword [report.free_ok]
        jmp     .loop
.done:
        mov     dword [report.outstanding], 0
        call    [ChangeTask]
        popad
        ret

;=======================================================================
test_double_free:
        pushad
        call    [AllocPage]
        test    eax, eax
        jz      .skip
        mov     [report.df_pa], eax
        push    eax
        call    [FreePage]
        pop     eax
        ; second free — legacy BTS polarity: pages_free must NOT increase
        call    [FreePage]
        or      dword [report.flags], 64  ; df executed
.skip:
        popad
        ret

;=======================================================================
; AllocPages table — records per-case results in report.ap_cases[i]
; Each case: 4 dwords { N, ret_pa, status, reserved }
; status: 0=fail_expected_or_got0, 1=success, 2=unexpected
test_alloc_pages_table:
        pushad
        xor     ebx, ebx                 ; case index

        ; case 0: N=0
        stdcall run_ap_case, 0, ebx
        inc     ebx
        ; case 1: N=1 (rounds to 8 pages)
        stdcall run_ap_case, 1, ebx
        inc     ebx
        ; case 2: N=8
        stdcall run_ap_case, 8, ebx
        inc     ebx
        ; case 3: N=16
        stdcall run_ap_case, 16, ebx
        inc     ebx
        ; case 4: N=9 (rounds to 16)
        stdcall run_ap_case, 9, ebx
        inc     ebx
        ; case 5: N=7 (rounds to 8)
        stdcall run_ap_case, 7, ebx
        inc     ebx

        mov     [report.ap_cases_n], ebx
        or      dword [report.flags], 4  ; ap table done
        popad
        ret

; stdcall N, index
proc run_ap_case stdcall, n:dword, idx:dword
        push    ebx esi
        mov     ebx, [idx]
        mov     esi, report.ap_cases
        mov     eax, ebx
        shl     eax, 4
        add     esi, eax
        mov     eax, [n]
        mov     [esi], eax               ; N
        stdcall [AllocPages], eax
        mov     [esi+4], eax             ; ret
        test    eax, eax
        jz      .fail
        mov     dword [esi+8], 1
        ; free the run: count was rounded up to multiple of 8
        mov     ecx, [n]
        add     ecx, 7
        shr     ecx, 3
        shl     ecx, 3                   ; pages allocated
        mov     edx, eax
.free_run:
        test    ecx, ecx
        jz      .ok
        mov     eax, edx
        call    [FreePage]
        add     edx, 4096
        dec     ecx
        jmp     .free_run
.ok:
        inc     dword [report.apages_ok]
        jmp     .out
.fail:
        mov     dword [esi+8], 0
        inc     dword [report.apages_fail]
.out:
        mov     dword [esi+12], 0
        call    [ChangeTask]
        pop     esi ebx
        ret
endp

;=======================================================================
test_fragmentation:
        pushad
        ; allocate FRAG_N singles into frag_buf
        xor     ebx, ebx
.alloc:
        cmp     ebx, FRAG_N
        jae     .alloc_done
        call    [AllocPage]
        test    eax, eax
        jz      .alloc_done
        mov     [frag_buf + ebx*4], eax
        inc     ebx
        jmp     .alloc
.alloc_done:
        mov     [frag_count], ebx
        ; free every second
        xor     ebx, ebx
.free_odd:
        cmp     ebx, [frag_count]
        jae     .free_done
        test    ebx, 1
        jz      .skip
        mov     eax, [frag_buf + ebx*4]
        test    eax, eax
        jz      .skip
        call    [FreePage]
        mov     dword [frag_buf + ebx*4], 0
        inc     dword [report.frag_holes]
.skip:
        inc     ebx
        jmp     .free_odd
.free_done:
        ; try AllocPages 8 — may succeed if holes coalesced to FF byte
        stdcall [AllocPages], 8
        mov     [report.frag_ap8_ret], eax
        test    eax, eax
        jz      .no_ap
        mov     edx, eax
        mov     ecx, 8
.fap:
        mov     eax, edx
        call    [FreePage]
        add     edx, 4096
        loop    .fap
        inc     dword [report.frag_ap8_ok]
.no_ap:
        ; free remaining
        xor     ebx, ebx
.fr:
        cmp     ebx, [frag_count]
        jae     .fr_done
        mov     eax, [frag_buf + ebx*4]
        test    eax, eax
        jz      .fr_next
        call    [FreePage]
.fr_next:
        inc     ebx
        jmp     .fr
.fr_done:
        or      dword [report.flags], 128
        popad
        ret

;=======================================================================
test_oom:
        pushad
        xor     ecx, ecx
.oom_loop:
        cmp     ecx, MAX_OOM_EXTRA
        jae     .ceiling
        mov     eax, [ledger_count]
        cmp     eax, MAX_LEDGER
        jae     .ceiling
        push    ecx
        call    [AllocPage]
        pop     ecx
        test    eax, eax
        jz      .hit
        call    ledger_push
        jc      .ceiling
        inc     dword [report.ap_ok]
        inc     ecx
        test    ecx, 127
        jnz     .oom_loop
        push    ecx
        call    [ChangeTask]
        pop     ecx
        jmp     .oom_loop
.hit:
        mov     dword [report.oom_ret], 0
        mov     [report.oom_ops], ecx
        or      dword [report.flags], 256  ; OOM observed (EAX=0)
        mov     esi, msg_oom_hit
        call    [SysMsgBoardStr]
        jmp     .oom_done
.ceiling:
        mov     [report.oom_ops], ecx
        or      dword [report.flags], 512  ; OOM blocked by ceiling
        mov     esi, msg_oom_blk
        call    [SysMsgBoardStr]
.oom_done:
        mov     ebx, 50
        call    [Delay]
        popad
        ret

;=======================================================================
section '.data' data readable writeable

sz_name db 'ASOAKDRV',0  ; unused (no RegService); kept for docs/grep

msg_start db 'ALLOCSOK START',13,10,0
msg_a     db 'ALLOCSOK A',13,10,0
msg_b     db 'ALLOCSOK B',13,10,0
msg_b_done db 'ALLOCSOK B DONE',13,10,0
msg_ap    db 'ALLOCSOK AP',13,10,0
msg_frag  db 'ALLOCSOK FRAG',13,10,0
msg_oom   db 'ALLOCSOK OOM',13,10,0
msg_oom_hit db 'ALLOCSOK OOM HIT',13,10,0
msg_oom_blk db 'ALLOCSOK OOM BLOCKED',13,10,0
msg_rec   db 'ALLOCSOK RECOVER',13,10,0
msg_pass  db 'ALLOCSOK PASS',13,10,0
msg_fail  db 'ALLOCSOK FAIL',13,10,0
msg_stat  db 'ALLOCSOK STAT ',0
msg_df    db 'ALLOCSOK DF',13,10,0
msg_sp    db ' ',0
msg_cr    db 13,10,0
hexbuf    db 0,0

align 16
report:
.magic            dd ?
.version          dd ?
.seed             dd ?
.flags            dd ?
.ap_ok            dd ?
.ap_fail          dd ?
.apages_ok        dd ?
.apages_fail      dd ?
.free_ok          dd ?
.outstanding      dd ?
.last_pa          dd ?
.pressure_ok      dd ?
.pressure_target  dd ?
.ledger_cap       dd ?
.oom_ret          dd ?
.oom_ops          dd ?
.df_pa            dd ?
.frag_holes       dd ?
.frag_ap8_ret     dd ?
.frag_ap8_ok      dd ?
.ap_cases_n       dd ?
.reserved0        dd ?
.ap_cases:        rb 16*16       ; up to 16 cases * 16 bytes
report_end:
                  rb report_bytes - (report_end - report)

align 4
ledger_count dd 0
ledger       rd MAX_LEDGER
frag_count   dd 0
frag_buf     rd FRAG_N

;=======================================================================
section '.idata' import data readable writeable
  library kernel,'KERNEL'
  import kernel,\
    AllocPage,'AllocPage',\
    AllocPages,'AllocPages',\
    FreePage,'FreePage',\
    SysMsgBoardStr,'SysMsgBoardStr',\
    Delay,'Delay',\
    ChangeTask,'ChangeTask'

; Required: Kolibri load_PE maps away from ImageBase (0x400000). Without
; BASE_RELOCATION fixups, call [Import] uses stale 0x40xxxx addresses and hangs.
data fixups
end data
