;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; exFAT_find_lfn — Rust embed + ABI smoke (Cut CQ)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_exfat_find_lfn).
;;
;; Critical ABI: rust_exfat_find_lfn(ctx) ret 4; never add esp after the
;; call (REG-009). Snapshot callbacks before stdcall (REG-010).
;; Marker 'FLFN' / fail DEAD0C71.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_exfat_find_lfn_smoke_result dd 0
cq_smoke_next_count dd 0
cq_smoke_entry rb 32
endg

align 4
rust_exfat_find_lfn:
        file '../../rust_kernel/kolibri_utils/out/rust_exfat_find_lfn.bin'

align 4
cq_smoke_first:
        mov     edi, cq_smoke_entry
        clc
        ret

align 4
cq_smoke_next:
        ; REG-019: CLC once then STC so smoke walks two get_name/next pairs.
        inc     dword [cq_smoke_next_count]
        cmp     dword [cq_smoke_next_count], 1
        ja      .end
        clc
        ret
.end:
        mov     eax, 5
        stc
        ret

align 4
cq_smoke_get_name:
        stc
        ret

align 4
exfat_find_lfn_rust_smoke_test:
if ~ USE_RUST_EXFAT_FIND_LFN
        ret
end if
        push    ebx
        push    esi
        push    edi
        push    ebp
        cld
        sub     esp, 52 + 64            ; ctx + mini-fs (injected fields + path)

        lea     edi, [esp]
        mov     ecx, (52 + 64) / 4
        xor     eax, eax
        rep stosd

        lea     eax, [esp]
        lea     ebp, [esp+52]           ; mini-fs: 7 dwords then path
        mov     [eax+0], ebp
        lea     esi, [ebp+0]
        mov     [eax+4], esi            ; secondary
        lea     esi, [ebp+4]
        mov     [eax+8], esi            ; need_hash
        lea     esi, [ebp+8]
        mov     [eax+12], esi           ; LFN_reserve_place
        lea     esi, [ebp+12]
        mov     [eax+16], esi           ; path_in_utf8
        lea     esi, [ebp+16]
        mov     [eax+20], esi           ; current_hash
        lea     esi, [ebp+20]
        mov     [eax+24], esi           ; valid_data_length
        mov     dword [ebp+20], 0xffffffff
        lea     esi, [ebp+24]
        mov     [eax+40], esi           ; pair
        lea     esi, [ebp+32]
        mov     byte [esi], 'X'
        mov     byte [esi+1], 0
        mov     [eax+44], esi           ; path

        ; Vector 0: first=0 → EAX=5, no UTF-8 walk.
        mov     ebx, 0xB10000C1
        stdcall rust_exfat_find_lfn, eax
        cmp     ebx, 0xB10000C1
        jne     .fail
        cmp     eax, 5
        jne     .fail

        ; Vector 1: first CF=0 then UTF-8/hash + get_name STC + next CLC then
        ; get_name STC + next STC (REG-018 + REG-019 two-callback walk).
        and     dword [cq_smoke_next_count], 0
        lea     eax, [esp]
        mov     dword [eax+28], cq_smoke_first
        mov     dword [eax+32], cq_smoke_next
        mov     dword [eax+36], cq_smoke_get_name
        mov     ebx, 0xB10000C1
        stdcall rust_exfat_find_lfn, eax
        cmp     ebx, 0xB10000C1
        jne     .fail
        cmp     eax, 5
        jne     .fail

        mov     [rust_exfat_find_lfn_smoke_result], 0x4E464C46 ; 'FLFN'
        add     esp, 52 + 64
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     [rust_exfat_find_lfn_smoke_result], 0xDEAD0C71
@@:
        jmp     @b
