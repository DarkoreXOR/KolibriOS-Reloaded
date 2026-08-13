;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; asoakdrv_rbpb.asm — disposable §19 release-bitmap ABI smoke (RBPB)
;;
;; Validates the FUTURE call-out CONTRACT only:
;;   plain call; EAX=page_index → EAX=delta∈{0,1}
;;   preserves EBX/ECX/EDX/ESI/EDI/EBP; DF unchanged; ret 0
;;   test bitmap BTS polarity; page_start canary NEVER written
;;
;; Does NOT call production release_pages / free_page.
;; Does NOT implement a Rust blob or USE_RUST_* gate.
;; Seed/marker: RBPB (0x52504242). Requires data fixups.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

format PE DLL
entry START

include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'

DRV_ENTRY_STATE = 1
MAP_BYTES       = 16            ; 128 pages
CANARY_PS       = 0x51514151    ; page_start canary (never written by shim)
CANARY_EBX      = 0xB0B00001
CANARY_ECX      = 0xC0C00002
CANARY_EDX      = 0xD0D00003
CANARY_ESI      = 0x51510004
CANARY_EDI      = 0xE1E10005
CANARY_EBP      = 0xF1F10006

section '.text' code readable executable

START:
        cmp     dword [esp+4], DRV_ENTRY_STATE
        jne     .exit0
        call    run_rbpb
.exit0:
        xor     eax, eax
        ret

;=======================================================================
run_rbpb:
        pushad
        mov     esi, msg_start
        call    [SysMsgBoardStr]
        mov     ebx, 50
        call    [Delay]

        call    map_reset
        mov     dword [test_page_start], CANARY_PS
        mov     dword [test_pages_free], 0

; --- Vector 1: allocated bit → delta=1, canary intact ---------------
        mov     esi, msg_v1
        call    [SysMsgBoardStr]
        ; page 5 allocated (bit clear)
        mov     eax, 5
        call    expect_delta1
        jc      .fail

; --- Vector 2: already-free → delta=0 --------------------------------
        mov     esi, msg_v2
        call    [SysMsgBoardStr]
        mov     eax, 5
        call    expect_delta0
        jc      .fail

; --- Vector 3: allocated before cursor dword --------------------------
        call    map_reset
        mov     dword [test_page_start], CANARY_PS
        mov     dword [test_pages_free], 0
        ; cursor canary is unrelated to map offset; page 2 is "before"
        mov     esi, msg_v3
        call    [SysMsgBoardStr]
        mov     eax, 2
        call    expect_delta1
        jc      .fail
        cmp     dword [test_page_start], CANARY_PS
        jne     .fail

; --- Vector 4: allocated after / high index ---------------------------
        mov     esi, msg_v4
        call    [SysMsgBoardStr]
        mov     eax, 100
        call    expect_delta1
        jc      .fail
        cmp     dword [test_page_start], CANARY_PS
        jne     .fail

; --- Vector 5: repeated release --------------------------------------
        mov     esi, msg_v5
        call    [SysMsgBoardStr]
        mov     eax, 40
        call    expect_delta1
        jc      .fail
        mov     eax, 40
        call    expect_delta0
        jc      .fail

; --- Vector 6: dword boundary 31 then 32 ------------------------------
        mov     esi, msg_v6
        call    [SysMsgBoardStr]
        mov     eax, 31
        call    expect_delta1
        jc      .fail
        mov     eax, 32
        call    expect_delta1
        jc      .fail

; --- Vector 7: pages_free wrapping edge ------------------------------
        call    map_reset
        mov     dword [test_page_start], CANARY_PS
        mov     dword [test_pages_free], 0xFFFFFFFF
        mov     esi, msg_v7
        call    [SysMsgBoardStr]
        mov     eax, 7
        call    call_shim_checked
        jc      .fail
        cmp     eax, 1
        jne     .fail
        cmp     dword [test_pages_free], 0
        jne     .fail
        cmp     dword [test_page_start], CANARY_PS
        jne     .fail

        mov     esi, msg_pass
        call    [SysMsgBoardStr]
        mov     ebx, 100
        call    [Delay]
        jmp     .out
.fail:
        mov     esi, msg_fail
        call    [SysMsgBoardStr]
        mov     ebx, 100
        call    [Delay]
.out:
        popad
        ret

;-----------------------------------------------------------------------
; rbpb_shim — FUTURE ABI shape (test-only implementation)
; IN:  EAX = page_index
; OUT: EAX = delta ∈ {0,1}
; Preserves: EBX,ECX,EDX,ESI,EDI,EBP
; Clobbers: EAX, EFLAGS
; Stack: plain ret (0)
;-----------------------------------------------------------------------
rbpb_shim:
        push    ebx
        push    ecx
        push    edx

        mov     ecx, eax                ; page index
        cmp     ecx, MAP_BYTES*8
        jae     .oob

        mov     eax, ecx
        shr     eax, 3                  ; byte index
        mov     edx, ecx
        and     edx, 7                  ; bit
        mov     ebx, 1
        mov     cl, dl
        shl     ebx, cl                 ; bit mask in ebx

        mov     edx, test_map
        add     edx, eax                ; &byte
        mov     al, [edx]
        mov     ah, al
        or      al, bl                  ; set free bit
        mov     [edx], al
        ; OLD = (ah >> bit) & 1  — recover via test
        mov     edx, ebx
        and     edx, 0xFF
        test    ah, dl
        jnz     .already                ; OLD=1 → delta=0
        ; OLD=0 → delta=1
        add     dword [test_pages_free], 1
        mov     eax, 1
        jmp     .done
.already:
        xor     eax, eax
        jmp     .done
.oob:
        xor     eax, eax
.done:
        ; MUST NOT touch test_page_start
        pop     edx
        pop     ecx
        pop     ebx
        ret

; call shim with register canaries; CF=1 on preserve/DF failure
; IN: EAX=page_index  OUT: EAX=delta  CF=fail
call_shim_checked:
        push    esi
        push    edi
        push    ebp
        push    ebx
        push    ecx
        push    edx

        cld
        pushfd
        pop     dword [saved_flags]

        mov     ebx, CANARY_EBX
        mov     ecx, CANARY_ECX
        mov     edx, CANARY_EDX
        mov     esi, CANARY_ESI
        mov     edi, CANARY_EDI
        mov     ebp, CANARY_EBP

        ; EAX already = page_index
        call    rbpb_shim

        cmp     ebx, CANARY_EBX
        jne     .bad
        cmp     ecx, CANARY_ECX
        jne     .bad
        cmp     edx, CANARY_EDX
        jne     .bad
        cmp     esi, CANARY_ESI
        jne     .bad
        cmp     edi, CANARY_EDI
        jne     .bad
        cmp     ebp, CANARY_EBP
        jne     .bad

        pushfd
        pop     ebx
        mov     ecx, [saved_flags]
        xor     ebx, ecx
        and     ebx, 0x400              ; DF bit only
        jnz     .bad

        cmp     dword [test_page_start], CANARY_PS
        jne     .bad

        pop     edx
        pop     ecx
        pop     ebx
        pop     ebp
        pop     edi
        pop     esi
        clc
        ret
.bad:
        pop     edx
        pop     ecx
        pop     ebx
        pop     ebp
        pop     edi
        pop     esi
        stc
        ret

expect_delta1:
        call    call_shim_checked
        jc      .e1f
        cmp     eax, 1
        jne     .e1f
        clc
        ret
.e1f:
        stc
        ret

expect_delta0:
        call    call_shim_checked
        jc      .e0f
        test    eax, eax
        jnz     .e0f
        clc
        ret
.e0f:
        stc
        ret

map_reset:
        pushad
        cld
        mov     edi, test_map
        mov     ecx, MAP_BYTES/4
        xor     eax, eax
        rep     stosd                   ; all bits 0 = allocated
        popad
        ret

;=======================================================================
section '.data' data readable writeable

saved_flags     dd 0
test_pages_free dd 0
test_page_start dd CANARY_PS
test_map        rb MAP_BYTES

msg_start db 'RBPB START',13,10,0
msg_v1    db 'RBPB V1',13,10,0
msg_v2    db 'RBPB V2',13,10,0
msg_v3    db 'RBPB V3',13,10,0
msg_v4    db 'RBPB V4',13,10,0
msg_v5    db 'RBPB V5',13,10,0
msg_v6    db 'RBPB V6',13,10,0
msg_v7    db 'RBPB V7',13,10,0
msg_pass  db 'RBPB PASS',13,10,0
msg_fail  db 'RBPB FAIL',13,10,0

section '.idata' import data readable writeable
  library kernel,'KERNEL'
  import kernel,\
    SysMsgBoardStr,'SysMsgBoardStr',\
    Delay,'Delay'

data fixups
end data
