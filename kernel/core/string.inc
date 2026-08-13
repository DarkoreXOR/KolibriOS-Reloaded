;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;                                                              ;;
;; Copyright (C) KolibriOS team 2004-2024. All rights reserved. ;;
;; Distributed under terms of the GNU General Public License    ;;
;; Author: Kees J. Bot 1 Jan 1994                               ;;
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;


; size_t strncat(char *s1, const char *s2, size_t n)
; Append string s2 to s1.

; char *strchr(const char *s, int c)


; int strncmp(const char *s1, const char *s2, size_t n)
; Compare two strings.

; char *strncpy(char *s1, const char *s2, size_t n)
; Copy string s2 to s1.

; size_t strnlen(const char *s, size_t n)
; Return the length of a string.

; proc strrchr stdcall, s:dword, c:dword
; Look for the last occurrence a character in a string.

proc strncat stdcall, s1:dword, s2:dword, n:dword
        push    esi
        push    edi
        mov     edi, [s1]        ; String s1
        mov     edx, [n]         ; Maximum length

        mov     ecx, -1
        xor     al, al           ; Null byte
        cld
        repne scasb              ; Look for the zero byte in s1
        dec     edi              ; Back one up (and clear 'Z' flag)
        push    edi              ; Save end of s1
        mov     edi, [s2]        ; edi = string s2
        mov     ecx, edx         ; Maximum count
        repne scasb              ; Look for the end of s2
        jne     @F
        inc     ecx              ; Exclude null byte
@@:
        sub     edx, ecx         ; Number of bytes in s2
        mov     ecx, edx
        mov     esi, [s2]        ; esi = string s2
        pop     edi              ; edi = end of string s1
        rep movsb                ; Copy bytes
        stosb                    ; Add a terminating null
        mov     eax, [s1]        ; Return s1
        pop     edi
        pop     esi
        ret
endp

align 4
; strncmp migration: USE_RUST_STRNCMP=1 routes through Rust
; rust_strncmp (see rust/strncmp.inc).
; Set USE_RUST_STRNCMP=0 to restore the original FASM body without deleting it.

USE_RUST_STRNCMP = 1

if USE_RUST_STRNCMP

; Compatibility trampoline: existing stdcall ABI → Rust stdcall.
; Callers keep `stdcall strncmp, s1, s2, n` / manual push+call with ret 12.
; ABI requirement: preserve EDX — FASM body never touched it; get_service
; keeps SRV* in EDX across the call. rust_strncmp clobbers EDX as s2.
proc strncmp stdcall, s1:dword, s2:dword, n:dword
        push    edx
        stdcall rust_strncmp, [s1], [s2], [n]
        pop     edx
        ret
endp

else

; --- original FASM implementation (rollback) ---
proc strncmp stdcall, s1:dword, s2:dword, n:dword

        push    esi
        push    edi
        mov     ecx, [n]
        test    ecx, ecx         ; Max length is zero?
        je      .done

        mov     esi, [s1]        ; esi = string s1
        mov     edi, [s2]        ; edi = string s2
        cld
.compare:
        cmpsb                    ; Compare two bytes
        jne     .done
        cmp     byte [esi-1], 0  ; End of string?
        je      .done
        dec     ecx              ; Length limit reached?
        jne     .compare
.done:
        seta    al               ; al = (s1 > s2)
        setb    ah               ; ah = (s1 < s2)
        sub     al, ah
        movsx   eax, al          ; eax = (s1 > s2) - (s1 < s2), i.e. -1, 0, 1
        pop     edi
        pop     esi
        ret
endp

end if

align 4
; strncpy migration: USE_RUST_STRNCPY=1 routes through Rust
; rust_strncpy (see rust/strncpy.inc).
; Set USE_RUST_STRNCPY=0 to restore the original FASM body without deleting it.

USE_RUST_STRNCPY = 1

if USE_RUST_STRNCPY

; Compatibility trampoline: stdcall(s1,s2,n) → rust_strncpy → EAX=s1.
; Legacy FASM clobbers ECX/EDX — do NOT invent EDX preserve (Cut BE lesson).
; Legacy DF: FASM always `cld` before return — match that here.
proc strncpy stdcall, s1:dword, s2:dword, n:dword
        stdcall rust_strncpy, [s1], [s2], [n]
        cld
        ret
endp

else

; --- original FASM implementation (rollback) ---
proc strncpy stdcall, s1:dword, s2:dword, n:dword

        push    esi
        push    edi

        mov     ecx, [n]         ; Maximum length
        mov     edi, [s2]        ; edi = string s2
        xor     al, al           ; Look for a zero byte
        mov     edx, ecx         ; Save maximum count
        cld
        repne scasb              ; Look for end of s2
        sub     edx, ecx         ; Number of bytes in s2 including null
        xchg    ecx, edx
        mov     esi, [s2]        ; esi = string s2
        mov     edi, [s1]        ; edi = string s1
        rep movsb                ; Copy bytes

        mov     ecx, edx         ; Number of bytes not copied
        rep stosb                ; strncpy always copies n bytes by null padding
        mov     eax, [s1]        ; Return s1
        pop     edi
        pop     esi
        ret
endp

end if

align 4
proc strnlen stdcall, s:dword, n:dword

        push    edi
        mov     ecx, [n]
        mov     edi, [s]         ; edi = string
        xor     al, al           ; Look for a zero byte
        mov     edx, ecx         ; Save maximum count
        cmp     cl, 1            ; 'Z' bit must be clear if ecx = 0
        cld
        repne scasb              ; Look for zero
        jne     @F
        inc     ecx              ; Don't count zero byte
@@:
        mov     eax, edx
        sub     eax, ecx         ; Compute bytes scanned
        pop     edi
        ret
endp

align 4
; strchr migration: USE_RUST_STRCHR=1 routes through Rust
; rust_strchr (see rust/strchr.inc).
; Set USE_RUST_STRCHR=0 to restore the original FASM body without deleting it.

USE_RUST_STRCHR = 1

if USE_RUST_STRCHR

; Compatibility trampoline: stdcall(s,c) → rust_strchr → EAX.
; Legacy DF: FASM always `cld` at entry — match at return.
proc strchr stdcall, s:dword, c:dword
        stdcall rust_strchr, [s], [c]
        cld
        ret
endp

align 4
rust_strchr:
        file '../../rust_kernel/kolibri_utils/out/rust_strchr.bin'

else

; --- original FASM implementation (rollback) ---
proc strchr stdcall, s:dword, c:dword
        push    edi
        cld
        mov     edi, [s]         ; edi = string
        mov     edx, 16          ; Look at small chunks of the string
.next:
        shl     edx, 1           ; Chunks become bigger each time
        mov     ecx, edx
        xor     al, al           ; Look for the zero at the end
        repne scasb
        pushf                    ; Remember the flags
        sub     ecx, edx
        neg     ecx              ; Some or all of the chunk
        sub     edi, ecx         ; Step back
        mov     eax, [c]         ; The character to look for
        repne scasb
        je      .found
        popf                     ; Did we find the end of string earlier?
        jne     .next            ; No, try again
        xor     eax, eax         ; Return NULL
        pop     edi
        ret
.found:
        pop     eax              ; Get rid of those flags
        lea     eax, [edi-1]     ; Address of byte found
        pop     edi
        ret

endp

end if


align 4
; strrchr migration: USE_RUST_STRRCHR=1 routes through Rust
; rust_strrchr (see rust/strrchr.inc).
; Set USE_RUST_STRRCHR=0 to restore the original FASM body without deleting it.

USE_RUST_STRRCHR = 1

if USE_RUST_STRRCHR

; Compatibility trampoline: stdcall(s,c) → rust_strrchr → EAX.
; REG-001: preserve EDX (FASM leaf never touched it; Rust stdcall clobbers EDX).
; Legacy DF: FASM always `cld` before return — match that here.
proc strrchr stdcall, s:dword, c:dword
        push    edx
        stdcall rust_strrchr, [s], [c]
        pop     edx
        cld
        ret
endp

else

; --- original FASM implementation (rollback) ---
proc strrchr stdcall, s:dword, c:dword
        push    edi
        mov     edi, [s]     ; edi = string
        mov     ecx, -1
        xor     al, al
        cld
        repne scasb              ; Look for the end of the string
        not     ecx              ; -1 - ecx = Length of the string + null
        dec     edi              ; Put edi back on the zero byte
        mov     eax, [c]         ; The character to look for
        std                      ; Downwards search
        repne scasb
        cld                      ; Direction bit back to default
        jne     .fail
        lea     eax, [edi+1]     ; Found it
        pop     edi
        ret
.fail:
        xor     eax, eax         ; Not there
        pop     edi
        ret
endp

end if



