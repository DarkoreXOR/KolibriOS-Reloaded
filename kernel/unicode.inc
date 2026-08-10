;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;                                                              ;;
;; Copyright (C) KolibriOS team 2020-2024. All rights reserved. ;;
;;  Distributed under terms of the GNU General Public License.  ;;
;;      Version 2, or (at your option) any later version.       ;;
;;                                                              ;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;


; UTF-8 decode migration: USE_RUST_UTF8=1 routes through Rust
; rust_unicode_utf8_decode (see rust/utf8.inc).
; Set USE_RUST_UTF8=0 to restore the original FASM body without deleting it.

USE_RUST_UTF8 = 1

if USE_RUST_UTF8

; Compatibility trampoline: existing ESI/ECX in-out, EAX out ABI → Rust stdcall.
; Callers keep `call unicode.utf8.decode` with buffer in ESI and length in ECX.
; Stack locals hold ESI/ECX so Rust can update them via pointer args.
unicode.utf8.decode:
        push    ecx
        mov     eax, esp      ; &length
        push    esi
        mov     edx, esp      ; &ptr
        stdcall rust_unicode_utf8_decode, edx, eax
        pop     esi
        pop     ecx
        ret

else

; --- original FASM implementation (rollback) ---
unicode.utf8.decode:
        test    ecx, ecx
        jz      .done
        movzx   eax, byte[esi]
        test    al, al
        jns     .read1
        shl     al, 2
        jnc     .error
        shl     al, 1
        jnc     .read2
        shl     al, 1
        jnc     .read3
        shl     al, 1
        jnc     .read4
        jmp     .error
.read1:
        sub     ecx, 1
        add     esi, 1
        jmp     .done
.read2:
        cmp     ecx, 2
        jb      .error
        shl     eax, 5
        mov     al, [esi+1]
        shl     al, 1
        jnc     .error
        shl     al, 1
        jc      .error
        shr     eax, 2
        sub     ecx, 2
        add     esi, 2
        jmp     .done
.read3:
        cmp     ecx, 3
        jb      .error
        shl     eax, 4
        mov     al, [esi+1]
        shl     al, 1
        jnc     .error
        shl     al, 1
        jc      .error
        shl     eax, 6
        mov     al, [esi+2]
        shl     al, 1
        jnc     .error
        shl     al, 1
        jc      .error
        shr     eax, 2
        sub     ecx, 3
        add     esi, 3
        jmp     .done
.read4:
        cmp     ecx, 4
        jb      .error
        shl     eax, 3
        mov     al, [esi+1]
        shl     al, 1
        jnc     .error
        shl     al, 1
        jc      .error
        shl     eax, 6
        mov     al, [esi+2]
        shl     al, 1
        jnc     .error
        shl     al, 1
        jc      .error
        shl     eax, 6
        mov     al, [esi+3]
        shl     al, 1
        jnc     .error
        shl     al, 1
        jc      .error
        shr     eax, 2
        sub     ecx, 4
        add     esi, 4
        jmp     .done
.error:
        mov     eax, 0xFFFD
        dec     ecx
        inc     esi
.done:
        ret

end if


; CP866 encode migration: USE_RUST_CP866=1 routes through Rust
; rust_unicode_cp866_encode (see rust/cp866.inc).
; Set USE_RUST_CP866=0 to restore the original FASM body without deleting it.
;
; Original body is a thin wrapper around uni2ansi_char (parse_fn.inc).

USE_RUST_CP866 = 1

if USE_RUST_CP866

; Compatibility trampoline: existing EAX/AX in, AL out ABI → Rust stdcall FFI.
; Callers keep `call unicode.cp866.encode` with code point in EAX (AX used).
; Preserve ECX: original uni2ansi_char leaves ECX intact (ASCII path untouched;
; special table push/pop). XFS/ext copy loops use `test ecx,ecx` after encode.
; EDX is also callee-clobbered by stdcall; XFS `copy_filename` must preserve it
; itself (`uses eax edx`) because dir_sf_read keeps the BDFE base in EDX across
; the call.
unicode.cp866.encode:
        push    ecx
        stdcall rust_unicode_cp866_encode, eax
        pop     ecx
        ret

else

; --- original FASM implementation (rollback) ---
unicode.cp866.encode:
        call    uni2ansi_char
        ret

end if


; UTF-16 encode migration: USE_RUST_UTF16=1 routes through Rust
; rust_unicode_utf16_encode (see rust/utf16.inc).
; Set USE_RUST_UTF16=0 to restore the original FASM body without deleting it.

USE_RUST_UTF16 = 1

if USE_RUST_UTF16

; Compatibility trampoline: existing EAX in/out ABI → Rust stdcall FFI.
; Callers keep `call unicode.utf16.encode` with code point in EAX.
; Preserve ECX: XFS/ext UTF-16 name loops test ECX after encode (FASM body
; does not clobber ECX).
unicode.utf16.encode:
        push    ecx
        stdcall rust_unicode_utf16_encode, eax
        pop     ecx
        ret

else

; --- original FASM implementation (rollback) ---
unicode.utf16.encode:
        cmp     eax, 0x110000
        jae     .error
        cmp     eax, 0x10000
        jae     .write2
        cmp     eax, 0xe000
        jae     .write1
        cmp     eax, 0xd800
        jb      .write1
        jmp     .error
.write1:
        jmp     .done
.write2:
        sub     eax, 0x10000
        shl     eax, 6
        shr     ax, 6
        ror     eax, 16
        or      eax, 0xdc00d800
        jmp     .done
.error:
        mov     eax, 0xFFFD
.done:
        ret

end if
