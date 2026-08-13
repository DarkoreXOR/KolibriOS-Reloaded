;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; asoakdrv_oom.asm — disposable early-OOM observation PE driver
;;
;; Retains AllocPage physical pages only (no VA mapping).
;;
;; Important legacy interaction:
;;   Sequential AllocPage advances page_start. Concurrent frees can leave
;;   free bits below page_start → AllocPage returns 0 (scan miss) while
;;   pages_free is still >> 1. That is NOT the early-OOM path.
;;
;; Protocol on EAX=0:
;;   1. Free oldest ledger page (may lower page_start)
;;   2. Retry exactly one AllocPage
;;   3. If still 0 → classify OOM HIT (candidate early-OOM / hard fail)
;;   4. At most one final failed probe is reported as OOM HIT
;;
;; Limits: tools/allocsoak/oom_limits.inc
;; Requires data fixups.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

format PE DLL
entry START

include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'
include 'oom_limits.inc'

DRV_ENTRY_STATE = 1
MAX_SCANMISS_REWIND = 4096   ; safety: max rewind cycles

section '.text' code readable executable

START:
        cmp     dword [esp+4], DRV_ENTRY_STATE
        jne     .exit0
        call    run_oom_experiment
.exit0:
        xor     eax, eax
        ret

;=======================================================================
run_oom_experiment:
        pushad
        call    report_init
        mov     esi, msg_start
        call    [SysMsgBoardStr]

        mov     ebx, 80
        call    [Delay]
        mov     esi, msg_a
        call    [SysMsgBoardStr]
        or      dword [report.flags], 1

        mov     esi, msg_b
        call    [SysMsgBoardStr]
        mov     dword [report.scanmiss], 0

.pressure:
        mov     ecx, [ledger_count]
        cmp     ecx, MAX_RETAIN
        jae     .pressure_ceiling

        call    [AllocPage]
        test    eax, eax
        jnz     .got_page

        call    handle_alloc_zero
        test    eax, eax
        jz      .alloc_fail_path
.got_page:
        call    ledger_push
        jc      .pressure_ceiling
        inc     dword [report.ap_ok]
        mov     ecx, [ledger_count]

        mov     eax, ecx
        and     eax, 4095
        jnz     .slow
        push    ecx
        mov     esi, msg_b_prog
        call    [SysMsgBoardStr]
        mov     eax, [esp]
        call    print_hex8
        mov     esi, msg_cr
        call    [SysMsgBoardStr]
        pop     ecx

.slow:
        mov     eax, ecx
        and     eax, 2047
        jnz     .yield_chk
        push    ecx
        mov     ebx, 2
        call    [Delay]
        pop     ecx
.yield_chk:
        test    ecx, YIELD_EVERY-1
        jnz     .pressure
        push    ecx
        call    [ChangeTask]
        pop     ecx
        jmp     .pressure

.alloc_fail_path:
        ; EAX=0 after rewind — either true OOM or scanmiss cap.
        test    dword [report.flags], 1024
        jnz     .scanmiss_blocked
        jmp     .oom_hit
.scanmiss_blocked:
        mov     eax, [ledger_count]
        mov     [report.pressure_ok], eax
        mov     dword [report.oom_ops], 0
        or      dword [report.flags], 512
        mov     esi, msg_oom_blk
        call    [SysMsgBoardStr]
        jmp     .oom_window

.pressure_ceiling:
        mov     ecx, [ledger_count]
        mov     [report.pressure_ok], ecx
        mov     esi, msg_b_done
        call    [SysMsgBoardStr]
        mov     eax, ecx
        call    print_hex8
        mov     esi, msg_cr
        call    [SysMsgBoardStr]
        or      dword [report.flags], 2
        mov     ebx, 100
        call    [Delay]

        mov     esi, msg_oom
        call    [SysMsgBoardStr]
        call    [AllocPage]
        test    eax, eax
        jz      .probe_zero
        call    ledger_push
        jnc     .probe_kept
        call    [FreePage]
.probe_kept:
        mov     dword [report.oom_ops], 1
        or      dword [report.flags], 512
        mov     esi, msg_oom_blk
        call    [SysMsgBoardStr]
        jmp     .oom_window

.probe_zero:
        call    handle_alloc_zero
        test    eax, eax
        jnz     .probe_retry_ok
        test    dword [report.flags], 1024
        jnz     .scanmiss_blocked
        jmp     .oom_hit_from_probe
.probe_retry_ok:
        call    ledger_push
        jnc     .probe_kept2
        call    [FreePage]
.probe_kept2:
        mov     dword [report.oom_ops], 1
        or      dword [report.flags], 512
        mov     esi, msg_oom_blk
        call    [SysMsgBoardStr]
        jmp     .oom_window

.oom_hit:
        mov     eax, [ledger_count]
        mov     [report.pressure_ok], eax
.oom_hit_from_probe:
        mov     dword [report.oom_ret], 0
        mov     dword [report.oom_ops], 1
        or      dword [report.flags], 256
        mov     esi, msg_oom_hit
        call    [SysMsgBoardStr]

.oom_window:
        ; Hold for host: pages_free / page_start / digest
        mov     ebx, 200                 ; 2.0s
        call    [Delay]
        call    print_stat_line

        mov     esi, msg_rec
        call    [SysMsgBoardStr]
        call    ledger_free_all
        mov     ebx, 80
        call    [Delay]
        call    [AllocPage]
        test    eax, eax
        jz      .rec_fail
        call    [FreePage]
        or      dword [report.flags], 8
        jmp     .rec_done
.rec_fail:
        or      dword [report.flags], 16
.rec_done:
        test    dword [report.flags], 16
        jnz     .fail
        or      dword [report.flags], 32
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

;=======================================================================
; On AllocPage EAX=0: free oldest ledger page (rewind page_start), retry once.
; Returns EAX=new page or 0.
handle_alloc_zero:
        push    ebx ecx
        mov     eax, [report.scanmiss]
        cmp     eax, MAX_SCANMISS_REWIND
        jae     .give_up
        inc     dword [report.scanmiss]
        ; board marker only every 256 rewinds (avoid flood)
        mov     eax, [report.scanmiss]
        dec     eax
        test    eax, 255
        jnz     .no_msg
        mov     esi, msg_scanmiss
        call    [SysMsgBoardStr]
.no_msg:

        mov     ebx, [ledger_count]
        test    ebx, ebx
        jz      .give_up
        ; free oldest = ledger[0]
        mov     eax, [ledger]
        call    [FreePage]
        ; shift ledger down
        mov     ecx, ebx
        dec     ecx
        mov     [ledger_count], ecx
        test    ecx, ecx
        jz      .shifted
        mov     esi, ledger
        lea     edi, [ledger]
        ; manual shift: for i in 0..count-1: ledger[i]=ledger[i+1]
        xor     ebx, ebx
.shift:
        cmp     ebx, ecx
        jae     .shifted
        mov     eax, [ledger + ebx*4 + 4]
        mov     [ledger + ebx*4], eax
        inc     ebx
        jmp     .shift
.shifted:
        dec     dword [report.outstanding]
        inc     dword [report.free_ok]
        ; retry once
        call    [AllocPage]
        pop     ecx ebx
        ret
.give_up:
        mov     esi, msg_scanmiss_cap
        call    [SysMsgBoardStr]
        or      dword [report.flags], 1024  ; scanmiss cap
        xor     eax, eax
        pop     ecx ebx
        ret

;=======================================================================
report_init:
        push    edi ecx eax
        mov     edi, report
        mov     ecx, 40
        xor     eax, eax
        rep stosd
        mov     dword [report.magic], 'AOOM'
        mov     dword [report.version], 2
        mov     dword [report.seed], SEED
        mov     dword [report.ledger_cap], MAX_LEDGER
        mov     dword [report.pressure_target], MAX_RETAIN
        mov     dword [ledger_count], 0
        pop     eax ecx edi
        ret

ledger_push:
        push    ebx
        mov     ebx, [ledger_count]
        cmp     ebx, MAX_LEDGER
        jae     .full
        mov     [ledger + ebx*4], eax
        inc     dword [ledger_count]
        mov     [report.last_pa], eax
        mov     eax, ebx
        inc     eax
        mov     [report.outstanding], eax
        clc
        pop     ebx
        ret
.full:
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
        test    ebx, 255
        jnz     .loop
        call    [ChangeTask]
        jmp     .loop
.done:
        mov     dword [report.outstanding], 0
        popad
        ret

print_stat_line:
        pushad
        mov     esi, msg_stat
        call    [SysMsgBoardStr]
        mov     eax, [report.flags]
        call    print_hex8
        mov     esi, msg_sp
        call    [SysMsgBoardStr]
        mov     eax, [report.ap_ok]
        call    print_hex8
        mov     esi, msg_sp
        call    [SysMsgBoardStr]
        mov     eax, [report.pressure_ok]
        call    print_hex8
        mov     esi, msg_sp
        call    [SysMsgBoardStr]
        mov     eax, [report.scanmiss]
        call    print_hex8
        mov     esi, msg_cr
        call    [SysMsgBoardStr]
        popad
        ret

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
        mov     byte [hexbuf+1], 0
        mov     esi, hexbuf
        call    [SysMsgBoardStr]
        loop    .ph
        popad
        ret

;=======================================================================
section '.data' data readable writeable

msg_start    db 'ALLOCSOK START',13,10,0
msg_a        db 'ALLOCSOK A',13,10,0
msg_b        db 'ALLOCSOK B',13,10,0
msg_b_prog   db 'ALLOCSOK B ',0
msg_b_done   db 'ALLOCSOK B DONE ',0
msg_scanmiss db 'ALLOCSOK SCANMISS',13,10,0
msg_scanmiss_cap db 'ALLOCSOK SCANMISS CAP',13,10,0
msg_oom      db 'ALLOCSOK OOM',13,10,0
msg_oom_hit  db 'ALLOCSOK OOM HIT',13,10,0
msg_oom_blk  db 'ALLOCSOK OOM BLOCKED',13,10,0
msg_rec      db 'ALLOCSOK RECOVER',13,10,0
msg_pass     db 'ALLOCSOK PASS',13,10,0
msg_fail     db 'ALLOCSOK FAIL',13,10,0
msg_stat     db 'ALLOCSOK STAT ',0
msg_sp       db ' ',0
msg_cr       db 13,10,0
hexbuf       db 0,0

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
.scanmiss         dd ?
                  rb 64

align 4
ledger_count dd 0
ledger       rd MAX_LEDGER

section '.idata' import data readable writeable
  library kernel,'KERNEL'
  import kernel,\
    AllocPage,'AllocPage',\
    FreePage,'FreePage',\
    SysMsgBoardStr,'SysMsgBoardStr',\
    Delay,'Delay',\
    ChangeTask,'ChangeTask'

data fixups
end data
