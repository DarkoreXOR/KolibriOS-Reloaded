;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; asoakdrv_ab.asm — disposable free_page vs release_pages page_start A/B
;;
;; free_page only lowers page_start when the freed page's bitmap dword is
;; STRICTLY below the current cursor. Two consecutive AllocPages often share
;; a dword, so this harness allocates FILL_N fillers after the target page
;; to force page_start into a later dword before the free/release.
;;
;; CASE A (free_page):
;;   AllocPage A → FILL_N fillers → FreePage(A)
;;   Expect: page_start may lower toward A's dword.
;;
;; CASE B (release_pages):
;;   KernelAlloc(4096) → lin/phys A'
;;   FILL_N fillers → ReleasePages(lin, 1)
;;   Expect: page_start unchanged (FASM discards local ebx).
;;
;; Seed: 0x5047424D ('PGBM'). Requires data fixups.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

format PE DLL
entry START

include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'

SEED            = 0x5047424D
DRV_ENTRY_STATE = 1
FILL_N          = 40            ; >32 bits/dword — forces cursor past target dword
HOLD_HS         = 150           ; 1.5s Delay windows for host xp latch

section '.text' code readable executable

START:
        cmp     dword [esp+4], DRV_ENTRY_STATE
        jne     .exit0
        call    run_ab
.exit0:
        xor     eax, eax
        ret

;=======================================================================
run_ab:
        pushad
        mov     esi, msg_start
        call    [SysMsgBoardStr]
        mov     ebx, 80
        call    [Delay]
        mov     esi, msg_a
        call    [SysMsgBoardStr]

; ----- CASE A: free_page ---------------------------------------------
        mov     esi, msg_free_setup
        call    [SysMsgBoardStr]

        call    [AllocPage]
        test    eax, eax
        jz      .fail
        mov     [phys_a], eax

        mov     ecx, FILL_N
        call    fill_alloc
        jc      .fail_free_a

        mov     esi, msg_free_pa
        call    [SysMsgBoardStr]
        mov     eax, [phys_a]
        call    print_hex8
        mov     esi, msg_cr
        call    [SysMsgBoardStr]

        mov     esi, msg_free_before
        call    [SysMsgBoardStr]
        mov     ebx, HOLD_HS
        call    [Delay]

        mov     eax, [phys_a]
        call    [FreePage]
        mov     dword [phys_a], 0

        mov     esi, msg_free_after
        call    [SysMsgBoardStr]
        mov     ebx, HOLD_HS
        call    [Delay]

        call    fill_free_all
        or      dword [flags], 1

; ----- CASE B: release_pages -----------------------------------------
        mov     esi, msg_rel_setup
        call    [SysMsgBoardStr]

        stdcall [KernelAlloc], 4096
        test    eax, eax
        jz      .fail
        mov     [lin_a], eax
        call    [GetPgAddr]
        mov     [phys_rel], eax

        mov     ecx, FILL_N
        call    fill_alloc
        jc      .fail_rel_cleanup

        mov     esi, msg_rel_pa
        call    [SysMsgBoardStr]
        mov     eax, [phys_rel]
        call    print_hex8
        mov     esi, msg_sp
        call    [SysMsgBoardStr]
        mov     eax, [lin_a]
        call    print_hex8
        mov     esi, msg_cr
        call    [SysMsgBoardStr]

        mov     esi, msg_rel_before
        call    [SysMsgBoardStr]
        mov     ebx, HOLD_HS
        call    [Delay]

        mov     eax, [lin_a]
        mov     ecx, 1
        call    [ReleasePages]

        mov     esi, msg_rel_after
        call    [SysMsgBoardStr]
        mov     ebx, HOLD_HS
        call    [Delay]

        ; VA bookkeeping only (bitmap already released) — not KernelFree
        stdcall [FreeKernelSpace], [lin_a]
        mov     dword [lin_a], 0

        call    fill_free_all
        or      dword [flags], 2

; ----- recovery ------------------------------------------------------
        mov     esi, msg_rec
        call    [SysMsgBoardStr]
        call    [AllocPage]
        test    eax, eax
        jz      .fail
        call    [FreePage]
        or      dword [flags], 4

        mov     esi, msg_stat
        call    [SysMsgBoardStr]
        mov     eax, [flags]
        call    print_hex8
        mov     esi, msg_cr
        call    [SysMsgBoardStr]

        mov     esi, msg_pass
        call    [SysMsgBoardStr]
        jmp     .out

.fail_free_a:
        mov     eax, [phys_a]
        test    eax, eax
        jz      .fail
        call    [FreePage]
        mov     dword [phys_a], 0
        call    fill_free_all
        jmp     .fail
.fail_rel_cleanup:
        call    fill_free_all
        mov     eax, [lin_a]
        test    eax, eax
        jz      .fail
        mov     ecx, 1
        call    [ReleasePages]
        stdcall [FreeKernelSpace], [lin_a]
        mov     dword [lin_a], 0
.fail:
        mov     esi, msg_fail
        call    [SysMsgBoardStr]
.out:
        popad
        ret

; ecx = count to allocate into fill_ledger; CF=1 on failure
fill_alloc:
        push    ebx edx
        mov     ebx, ecx
.fa_loop:
        test    ebx, ebx
        jz      .fa_ok
        call    [AllocPage]
        test    eax, eax
        jz      .fa_fail
        mov     edx, [fill_count]
        cmp     edx, FILL_N
        jae     .fa_fail
        mov     [fill_ledger + edx*4], eax
        inc     dword [fill_count]
        dec     ebx
        jmp     .fa_loop
.fa_ok:
        clc
        pop     edx ebx
        ret
.fa_fail:
        stc
        pop     edx ebx
        ret

fill_free_all:
        pushad
.ff:
        mov     edx, [fill_count]
        test    edx, edx
        jz      .ff_done
        dec     edx
        mov     eax, [fill_ledger + edx*4]
        mov     dword [fill_ledger + edx*4], 0
        mov     [fill_count], edx
        test    eax, eax
        jz      .ff
        call    [FreePage]
        jmp     .ff
.ff_done:
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

flags      dd 0
phys_a     dd 0
phys_rel   dd 0
lin_a      dd 0
fill_count dd 0
fill_ledger rd FILL_N

msg_start       db 'ALLOCSOK START',13,10,0
msg_a           db 'ALLOCSOK A',13,10,0
msg_free_setup  db 'ALLOCSOK FREE SETUP',13,10,0
msg_free_pa     db 'ALLOCSOK FREE PA ',0
msg_free_before db 'ALLOCSOK FREE BEFORE',13,10,0
msg_free_after  db 'ALLOCSOK FREE AFTER',13,10,0
msg_rel_setup   db 'ALLOCSOK REL SETUP',13,10,0
msg_rel_pa      db 'ALLOCSOK REL PA ',0
msg_rel_before  db 'ALLOCSOK REL BEFORE',13,10,0
msg_rel_after   db 'ALLOCSOK REL AFTER',13,10,0
msg_rec         db 'ALLOCSOK RECOVER',13,10,0
msg_stat        db 'ALLOCSOK STAT ',0
msg_pass        db 'ALLOCSOK PASS',13,10,0
msg_fail        db 'ALLOCSOK FAIL',13,10,0
msg_sp          db ' ',0
msg_cr          db 13,10,0
hexbuf          db 0,0

section '.idata' import data readable writeable
  library kernel,'KERNEL'
  import kernel,\
    AllocPage,'AllocPage',\
    FreePage,'FreePage',\
    ReleasePages,'ReleasePages',\
    KernelAlloc,'KernelAlloc',\
    FreeKernelSpace,'FreeKernelSpace',\
    GetPgAddr,'GetPgAddr',\
    SysMsgBoardStr,'SysMsgBoardStr',\
    Delay,'Delay'

data fixups
end data
