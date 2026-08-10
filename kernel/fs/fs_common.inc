;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;                                                              ;;
;; Copyright (C) KolibriOS team 2004-2024. All rights reserved. ;;
;;  Distributed under terms of the GNU General Public License.  ;;
;;                                                              ;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;


fsReadCMOS:
        out     70h, al
        in      al, 71h
        xor     ah, ah
        shl     ax, 4
        shr     al, 4
        aad
        ret

fsGetTime:
        mov     al, 7
        call    fsReadCMOS
        ror     eax, 8
        mov     al, 8
        call    fsReadCMOS
        ror     eax, 8
        mov     al, 9
        call    fsReadCMOS
        add     eax, 2000
        ror     eax, 16
        push    eax
        xor     eax, eax
        call    fsReadCMOS
        ror     eax, 8
        mov     al, 2
        call    fsReadCMOS
        ror     eax, 8
        mov     al, 4
        call    fsReadCMOS
        ror     eax, 16
        push    eax
        mov     esi, esp
        add     esp, 8
; Cut G: USE_RUST_FS_CALCULATE_TIME=1 routes through Rust rust_fs_calculate_time
; (see rust/fs_calculate_time.inc). Set USE_RUST_FS_CALCULATE_TIME=0 to restore
; the original FASM body without deleting it. Independent of Cuts A–F.
; Note: fsGetTime falls through into this label after CMOS read + stack BDFE.

USE_RUST_FS_CALCULATE_TIME = 1

if USE_RUST_FS_CALCULATE_TIME

; Compatibility trampoline: register ABI → Rust stdcall → EAX.
; Callers keep `call fsCalculateTime` with ESI → BDFE block / EAX out.
; ESI must be preserved (ext/ntfs advance ESI by 8 between calls).
; EBX/ECX/EDX may be clobbered (matches original FASM body).
align 4
fsCalculateTime:
        stdcall rust_fs_calculate_time, esi
        ret

else

fsCalculateTime:
; in: esi -> data block
; out: eax = seconds since 01.01.2001
        movzx   eax, word [esi+6]
        sub     eax, 2001
        jnc     @f
        xor     eax, eax
@@:
        mov     edx, months
        mov     ebx, eax
        inc     eax
        test    eax, 3
        jnz     @f
        add     edx, 12
@@:
        movzx   eax, byte [esi+5]
        dec     eax
        xor     ecx, ecx
@@:
        dec     eax
        js      @f
        add     cl, [edx+eax]
        adc     ch, 0
        jmp     @b
@@:
        mov     eax, ebx    ; years
        mov     edx, 365
        mul     edx
        shr     ebx, 2
        add     eax, ebx
        add     eax, ecx
        mov     bl, [esi+4]
        dec     eax
        add     eax, ebx    ; days
        mov     dl, 24
        mul     edx
        mov     bl, [esi+2]
        add     eax, ebx    ; hours
        mov     ecx, 60
        mul     ecx
        mov     bl, [esi+1]
        add     eax, ebx    ; minutes
        mul     ecx
        mov     bl, [esi]
        add     eax, ebx
        ret

end if

iglobal
months  db  31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31
months2 db  31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31
endg

; Cut T: USE_RUST_FS_TIME2BDFE=1 routes through Rust rust_fs_time2bdfe
; (see rust/fs_time2bdfe.inc). Set USE_RUST_FS_TIME2BDFE=0 to restore the
; original FASM body without deleting it. Independent of Cuts A–S.
; Critical ABI: EDI advances by exactly 8; hour stored as word at +2.

USE_RUST_FS_TIME2BDFE = 1

if USE_RUST_FS_TIME2BDFE

; Compatibility trampoline: register ABI → Rust stdcall → EDI+=8.
; Callers keep `call fsTime2bdfe` with EAX=secs / EDI→BDFE out.
; EDI must advance by 8 (ext/ntfs/xfs chain timestamps).
; EBX/ECX/EDX may be clobbered (matches original FASM body).
; ESI/EBP preserved (untouched by FASM; stdcall callee-saved).
; Explicit push/pop EDI so add edi,8 stays correct if the blob
; failed to restore EDI (stdcall callee-saved).
align 4
fsTime2bdfe:
        push    edi
        stdcall rust_fs_time2bdfe, eax, edi
        pop     edi
        add     edi, 8
        ret

else

fsTime2bdfe:
; in: eax = seconds since 01.01.2001
; edi -> data block
; out: edi = edi+8
        xor     edx, edx
        mov     ecx, 60
        div     ecx
        mov     [edi], dl
        xor     edx, edx
        div     ecx
        mov     [edi+1], dl
        xor     edx, edx
        mov     cl, 24
        div     ecx
        mov     [edi+2], dx
        xor     edx, edx
        mov     cx, 365
        div     ecx
        mov     ebx, eax
        add     ebx, 2001
        shr     eax, 2
        sub     edx, eax
        jns     @f
        dec     ebx
        add     edx, 365
        test    ebx, 3
        jnz     @f
        inc     edx
@@:
        xor     eax, eax
        mov     ecx, months-1
        test    ebx, 3
        jnz     @f
        add     ecx, 12
@@:
        inc     ecx
        inc     eax
        sub     dl, [ecx]
        jnc     @b
        dec     dh
        jns     @b
        add     dl, [ecx]
        inc     edx
        mov     [edi+4], dl
        mov     [edi+5], al
        mov     [edi+6], bx
        add     edi, 8
        ret

end if
