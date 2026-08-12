;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;                                                              ;;
;; Copyright (C) KolibriOS team 2004-2024. All rights reserved. ;;
;;  Distributed under terms of the GNU General Public License.  ;;
;;                                                              ;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;


iglobal
full_file_name_table    dd  sysdir_name
.size                   dd  1
endg

uglobal
sysdir_name     rb  64  ; 'sys',0
sysdir_path     rb  64
sysdir_name1    rb  64
sysdir_path1    rb  64
endg
; Example:
; align 64
; sysdir_name1 db 'KolibriOS',0
; align 64
; sysdir_path1 db 'HD0/1',0

proc Parser_params
    locals
        buff    rb  4   ; for test cd
    endl
if defined extended_primary_loader
        mov     ecx, sysdir_path
        mov     [ecx-64], dword 'sys'
        mov     [ecx-2], byte 3
        mov     esi, BOOT.syspath
        mov     edi, sysdir_path-1
        mov     ecx, 20
        rep movsb
        ret
else
        mov     ax, [BOOT.sys_disk]
        mov     ecx, sysdir_path
        mov     [ecx-64], dword 'sys'
        mov     [ecx-2], byte 3
        mov     [ecx-1], byte '/'
        cmp     al, 'r' ; ram disk
        jnz     @f
        mov     [ecx], dword 'RD/?'
        mov     [ecx+3], byte ah
        mov     [ecx+4], byte 0
        ret

@@:
        cmp     al, 'm'
        jnz     .hard_disk
        mov     [ecx], dword 'CD?/'
        mov     [ecx+4], byte '1'
        mov     [ecx+5], dword '/KOL'
        mov     [ecx+9], dword 'IBRI'
        mov     [ecx+13], byte 0
.next_cd:
        mov     [ecx+2], byte ah
        inc     ah
        cmp     ah, '5'
        je      @f
        lea     edx, [buff]
        pushad
        stdcall read_file, read_firstapp, edx, 0, 4
        popad
        cmp     [edx], dword 'MENU'
        jne     .next_cd
@@:
        ret

.hard_disk:
        sub     al, '1'
        mov     [ecx], dword 'HD?/'
        mov     [ecx+2], byte al
        mov     [ecx+4], byte ah
        mov     [ecx+5], dword '/KOL'
        mov     [ecx+9], dword 'IBRI'
        mov     [ecx+13], byte 0
        ret
end if

endp

; CP866 uppercase migration: USE_RUST_CP866_UPPER=1 routes through Rust
; rust_cp866_to_upper (see rust/cp866_upper.inc).
; Set USE_RUST_CP866_UPPER=0 to restore the original FASM body without deleting it.

USE_RUST_CP866_UPPER = 1

if USE_RUST_CP866_UPPER

; Compatibility trampoline: existing AL/EAX in/out ABI → Rust stdcall.
; Callers keep `call cp866toUpper` with the CP866 character in AL.
cp866toUpper:
        stdcall rust_cp866_to_upper, eax
        ret

else

; --- original FASM implementation (rollback) ---
cp866toUpper:
; convert cp866 character in al to uppercase
        cmp     al, 'a'
        jb      .ret
        cmp     al, 'z'
        jbe     @f
        cmp     al, 0xA0
        jb      .ret
        cmp     al, 0xB0
        jb      @f
        cmp     al, 0xE0
        jb      .ret
        cmp     al, 0xF0
        jb      .rus
        cmp     al, 0xF7
        ja      .ret
        and     eax, -2
.ret:
        ret

@@:
        sub     eax, 32
        ret

.rus:
        sub     eax, 0xE0-0x90
        ret

end if

; UTF-16 uppercase migration: USE_RUST_UTF16_UPPER=1 routes through Rust
; rust_utf16_to_upper (see rust/utf16_upper.inc).
; Set USE_RUST_UTF16_UPPER=0 to restore the original FASM body without deleting it.

USE_RUST_UTF16_UPPER = 1

if USE_RUST_UTF16_UPPER

; Compatibility trampoline: existing AX/EAX in/out ABI → Rust stdcall.
; Callers keep `call utf16toUpper` with the UTF-16 character in AX.
; Critical: FASM leaf preserved ECX/EDX; stdcall may clobber them. FAT/exFAT/CD
; name compares stash the path unit in EDX across the second utf16toUpper call.
utf16toUpper:
        push    ecx
        push    edx
        stdcall rust_utf16_to_upper, eax
        pop     edx
        pop     ecx
        ret

else

; --- original FASM implementation (rollback) ---
utf16toUpper:
; convert UTF-16 character in ax to uppercase
        cmp     ax, 'a'
        jb      .ret
        cmp     ax, 'z'
        jbe     @f
        cmp     ax, 430h
        jb      .ret
        cmp     ax, 450h
        jb      @f
        cmp     ax, 460h
        jnc     .ret
        sub     eax, 80
.ret:
        ret

@@:
        sub     eax, 32
        ret

end if

; Cut BZ: USE_RUST_UNI2ANSI_CHAR=1 routes through Rust rust_uni2ansi_char
; (see rust/uni2ansi_char.inc). Set USE_RUST_UNI2ANSI_CHAR=0 to restore the
; original FASM body without deleting it. Independent of Cuts A–BY.
; Critical ABI: AX in → AL out; FAT/ISO/NTFS `loop` needs ECX; REG-001 —
; stdcall clobbers EDX, so trampoline preserves ECX+EDX.
; Production default ON after Cut BZ gates.

USE_RUST_UNI2ANSI_CHAR = 1

if USE_RUST_UNI2ANSI_CHAR

; Compatibility trampoline: AX Unicode → AL CP866; ECX/EDX preserved.
align 4
uni2ansi_char:
; convert UNICODE character in ax to ANSI character in al using cp866 encoding
        push    ecx
        push    edx
        stdcall rust_uni2ansi_char, eax
        pop     edx
        pop     ecx
        ret

else

uni2ansi_char:
; convert UNICODE character in ax to ANSI character in al using cp866 encoding
        cmp     ax, 0x80
        jb      .ret
        cmp     ax, 0xB6
        jz      .B6
        cmp     ax, 0x400
        jb      .unk
        cmp     ax, 0x410
        jb      @f
        cmp     ax, 0x440
        jb      .rus1
        cmp     ax, 0x450
        jb      .rus2
        cmp     ax, 0x460
        jb      @f
.unk:
        mov     al, '_'
.ret:
        ret

.B6:
        mov     al, 20
        ret

.rus1:  ; 0x410-0x43F -> 0x80-0xAF
        add     al, 0x70
        ret

.rus2:  ; 0x440-0x44F -> 0xE0-0xEF
        add     al, 0xA0
        ret

@@:
        push    ecx edi
        mov     ecx, 8
        mov     edi, uni2ansi_char.table
        repnz scasb
        mov     ah, cl
        pop     edi ecx
        jnz     .unk
        mov     al, 0xF7
        sub     al, ah
        ret

end if

; Shared CP866 special table (FASM ansi2uni rollback + uni2ansi rollback).
align 1
uni2ansi_char.table  db  1, 51h, 4, 54h, 7, 57h, 0Eh, 5Eh

; Cut AN: USE_RUST_ANSI2UNI_CHAR=1 routes through Rust rust_ansi2uni_char
; (see rust/ansi2uni_char.inc). Set USE_RUST_ANSI2UNI_CHAR=0 to restore the
; original FASM body without deleting it. Independent of Cuts A–AM.
; Critical ABI: AL in → AX out; FAT/ISO `loop` needs ECX; REG-001 — stdcall
; clobbers EDX, so trampoline preserves ECX+EDX.
; Production default ON after Cut AN gates.

USE_RUST_ANSI2UNI_CHAR = 1

if USE_RUST_ANSI2UNI_CHAR

; Compatibility trampoline: AL/EAX register ABI → Rust stdcall → AX.
; Callers keep `call ansi2uni_char` with CP866 byte in AL.
; Preserve ECX (FAT/ISO name loops) and EDX (REG-001 / Cut D class).
align 4
ansi2uni_char:
; convert ANSI character in al to UNICODE character in ax, using cp866 encoding
        push    ecx
        push    edx
        stdcall rust_ansi2uni_char, eax
        pop     edx
        pop     ecx
        ret

else

; --- original FASM implementation (rollback) ---
ansi2uni_char:
; convert ANSI character in al to UNICODE character in ax, using cp866 encoding
        movzx   eax, al
        cmp     al, 0x80
        jb      @f      ; 0x00-0x7F - trivial map
        cmp     al, 0xB0
        jb      .rus    ; 0x80-0xAF -> 0x410-0x43F
        cmp     al, 0xE0
        jb      .unk
        cmp     al, 0xF0
        jb      .rus2   ; 0xE0-0xEF -> 0x440-0x44F
        cmp     al, 0xF8
        jnc     .unk
        mov     al, [eax+uni2ansi_char.table-0xF0]
        add     ax, 400h
        ret

@@:
        cmp     al, 20
        jnz     .ret
        mov     al, 0xB6
.ret:
        ret

.rus:
        add     ax, 0x410-0x80
        ret

.rus2:
        add     ax, 0x440-0xE0
        ret

.unk:
        mov     al, '_'
        ret

end if

; Cut BQ: USE_RUST_CP866_TO_UTF8_STRING=1 routes through Rust rust_cp866_to_utf8_string
; (see rust/cp866_to_utf8_string.inc). Set USE_RUST_CP866_TO_UTF8_STRING=0 to restore
; the original FASM body without deleting it. Independent of Cuts A–BP.

USE_RUST_CP866_TO_UTF8_STRING = 1

if USE_RUST_CP866_TO_UTF8_STRING

align 4
cp866toUTF8_string:
;   in:
; esi -> cp866 string (could be zero terminated)
; edi -> buffer for UTF-8 string
; ecx = buffer size (signed)
        push    ebx
        push    edx
        push    ebp

        mov     ebx, esi
        mov     ebp, edi

        sub     esp, 12
        lea     eax, [esp]
        lea     esi, [esp+4]
        lea     edi, [esp+8]
        stdcall rust_cp866_to_utf8_string, ebx, ebp, ecx, eax, esi, edi

        mov     edx, eax                ; packed (SF<<31)|(ZF<<30)|code_unit
        mov     esi, [esp]
        mov     edi, [esp+4]
        mov     ecx, [esp+8]
        add     esp, 12

        mov     ebx, edx                ; ebx = packed (caller EBX on stack)
        mov     eax, edx
        and     eax, 0x3FFFFFFF         ; EAX = code unit for test eax,eax
        test    ebx, ebx                ; SF = bit 31 of packed

        pop     ebp
        pop     edx
        pop     ebx
        ret

else

; --- original FASM implementation (rollback) ---
cp866toUTF8_string:
;   in:
; esi -> cp866 string (could be zero terminated)
; edi -> buffer for UTF-8 string
; ecx = buffer size (signed)
        lodsb
        call    ansi2uni_char
        push    eax
        call    UTF16to8
        pop     eax
        js      @f
        test    eax, eax
        jnz     cp866toUTF8_string
@@:
        ret

end if

; SF=1 -> counter
; ZF=1 -> zero char

; Cut BP: USE_RUST_UTF16_TO_8_STRING=1 routes through Rust rust_utf16_to_8_string
; (see rust/utf16_to_8_string.inc). Set USE_RUST_UTF16_TO_8_STRING=0 to restore
; the original FASM body without deleting it. Independent of Cuts A–BO.

USE_RUST_UTF16_TO_8_STRING = 1

if USE_RUST_UTF16_TO_8_STRING

align 4
UTF16to8_string:
;   in:
; esi -> UTF-16 string (could be zero terminated)
; edi -> buffer for UTF-8 string
; ecx = buffer size (signed)
        push    ebx
        push    edx
        push    ebp

        mov     ebx, esi
        mov     ebp, edi

        sub     esp, 12
        lea     eax, [esp]
        lea     esi, [esp+4]
        lea     edi, [esp+8]
        stdcall rust_utf16_to_8_string, ebx, ebp, ecx, eax, esi, edi

        mov     edx, eax                ; packed (SF<<31)|(ZF<<30)|code_unit
        mov     esi, [esp]
        mov     edi, [esp+4]
        mov     ecx, [esp+8]
        add     esp, 12

        mov     ebx, edx                ; ebx = packed (caller EBX on stack)
        mov     eax, edx
        and     eax, 0x3FFFFFFF         ; EAX = code unit for test eax,eax
        test    ebx, ebx                ; SF = bit 31 of packed

        pop     ebp
        pop     edx
        pop     ebx
        ret

else

UTF16to8_string:
;   in:
; esi -> UTF-16 string (could be zero terminated)
; edi -> buffer for UTF-8 string
; ecx = buffer size (signed)
        xor     eax, eax
@@:
        lodsw
        push    eax
        call    UTF16to8
        pop     eax
        js      @f
        test    eax, eax
        jnz     @b
@@:
        ret

end if

; Cut Q: USE_RUST_UTF16_TO_8=1 routes through Rust rust_utf16_to_8
; (see rust/utf16_to_8.inc). Set USE_RUST_UTF16_TO_8=0 to restore the
; original FASM body without deleting it. Independent of Cuts A–P.
; Production ON (Cut Q complete). Set =0 to restore the original FASM body.

USE_RUST_UTF16_TO_8 = 1

if USE_RUST_UTF16_TO_8

; Compatibility trampoline: register ABI → Rust stdcall → SF/EAX/EDI/ECX.
; Callers keep `call UTF16to8` with AX/EDI/ECX; plain ret.
; Proven requirements: EBX/EDX/ESI/EBP untouched; DF unchanged;
; SF = packed bit 31; EAX = residue; ECX/EDI = Rust finals (incl. INT_MIN).
; Flag rule: reconstruct SF with `test` AFTER residue extract; pops are
; flag-neutral; use `lea esp` not `add esp` for slot cleanup.
align 4
UTF16to8:
;   in:
; ax = UTF-16 char
; edi -> buffer for UTF-8 char (increasing)
; ecx = byte counter (decreasing)
        push    ebx
        push    edx
        push    esi

        push    ecx                     ; [esp+4] ecx_inout storage
        push    edi                     ; [esp]   dest_inout storage

        movzx   eax, ax
        lea     edx, [esp]              ; &dest
        lea     ecx, [esp+4]            ; &ecx
        stdcall rust_utf16_to_8, eax, edx, ecx
        ; EAX = (SF << 31) | residue

        mov     edi, [esp]
        mov     ecx, [esp+4]
        lea     esp, [esp+8]            ; flag-neutral drop slots

        mov     ebx, eax                ; ebx = packed (caller EBX on stack)
        and     eax, 0x7FFFFFFF         ; EAX = residue (flags clobbered)
        test    ebx, ebx                ; SF = bit 31 of packed

        pop     esi                     ; flag-neutral restore
        pop     edx
        pop     ebx
        ret

else

; --- original FASM implementation (rollback) ---
align 4
UTF16to8:
;   in:
; ax = UTF-16 char
; edi -> buffer for UTF-8 char (increasing)
; ecx = byte counter (decreasing)
        movzx   eax, ax
        dec     ecx
        js      .ret
        cmp     eax, 80h
        jnc     @f
        stosb
        test    eax, eax    ; SF=0
.ret:
        ret

@@:
        dec     ecx
        js      .ret
        cmp     eax, 800h
        jnc     @f
        shl     eax, 2
        shr     al, 2
        or      eax, 1100000010000000b
        xchg    al, ah
        stosw
        ret

@@:
        dec     ecx
        js      .ret
        shl     eax, 4
        shr     ax, 2
        shr     al, 2
        or      eax, 111000001000000010000000b
        bswap   eax
        shr     eax, 8
        stosb
        shr     eax, 8
        stosw
        ret

end if

; Cut AB: USE_RUST_UTF8TO16=1 routes through Rust rust_utf8to16
; (see rust/utf8to16.inc). Set USE_RUST_UTF8TO16=0 to restore the
; original FASM body without deleting it. Independent of Cuts A–AA.
; Production ON (Cut AB complete).

USE_RUST_UTF8TO16 = 1

if USE_RUST_UTF8TO16

; Compatibility trampoline: ESI/EAX register ABI → Rust stdcall → EAX/ESI.
; Callers keep `call utf8to16` with ESI→UTF-8; AX out; ESI advanced.
; Proven requirements: EBX/ECX/EDX/EDI/EBP untouched; DF unchanged;
; incoming EAX passed through (high bits affect 3-byte path).
align 4
utf8to16:
; in: esi -> UTF-8 char (increasing)
; out: ax = UTF-16 char
        push    ebx
        push    ecx
        push    edx
        push    edi
        push    ebp

        push    esi                     ; [esp] = esi_inout storage
        mov     ecx, esp
        stdcall rust_utf8to16, ecx, eax
        pop     esi

        pop     ebp
        pop     edi
        pop     edx
        pop     ecx
        pop     ebx
        ret

else

; --- original FASM implementation (rollback) ---
utf8to16:
; in: esi -> UTF-8 char (increasing)
; out: ax = UTF-16 char
        lodsb
        test    al, al
        jns     .got
        shl     al, 2
        jnc     utf8to16
@@:
        shl     ax, 8
        lodsb
        test    al, al
        jns     .got
        shl     al, 2
        jc      @b
        shr     ah, 2
        shl     ax, 3
        jnc     @f
        shl     eax, 3
        lodsb
        test    al, al
        jns     .got
        shl     al, 2
        jc      @b
        shr     eax, 2
        ret

@@:
        shr     ax, 5
        ret

.got:
        xor     ah, ah
        ret

end if

; Cut BH: USE_RUST_STRLEN=1 routes through Rust rust_strlen
; (see rust/strlen.inc). Set USE_RUST_STRLEN=0 to restore the
; original FASM body without deleting it. Independent of Cuts A–BG.
; Legacy register ABI: ESI→string in; ECX=length out; plain ret.
; Preserves EAX/EDI (push/pop) + ESI/EBX/EDX/EBP (untouched).
; DF unchanged (no cld). REG-001: trampoline restores EAX after
; moving length EAX→ECX (Rust stdcall returns in EAX).

USE_RUST_STRLEN = 1

if USE_RUST_STRLEN

align 4
strlen:
        push    eax
        push    ebx
        push    edx
        push    esi
        push    edi
        push    ebp
        stdcall rust_strlen, esi
        mov     ecx, eax
        pop     ebp
        pop     edi
        pop     esi
        pop     edx
        pop     ebx
        pop     eax
        ret

else

strlen:
; in: esi -> source
; out: ecx = length
        push    edi eax
        or      ecx, -1
        mov     edi, esi
        xor     eax, eax
        repnz scasb
        inc     ecx
        not     ecx
        pop     eax edi
        ret

end if