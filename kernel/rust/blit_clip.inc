;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; blit_clip — Rust embed + ABI smoke (Cut CD)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_blit_clip).
;;
;; Prerequisites:
;;   python scripts/build_rust.py  (extracts via project/build.toml)
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_blit_clip.bin
;;
;; Requires USE_RUST_BLIT_CLIP=1 so public blit_clip hits the trampoline
;; (not the FASM body). Independent of USE_RUST_BLOCK_CLIP.
;;
;; Critical ABI: ECX → BLITTER*; CF=0 draw / CF=1 reject; preserves
;; EBX/ESI/EDI. FASM `stdcall` only — rust_blit_clip ends `ret 4`;
;; never add esp after the call (REG-009).
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
rust_blit_clip_smoke_result dd 0
; Synthetic BLITTER fixtures (64 bytes each): dc, sc, dst, src, w, h, bitmap, stride
align 4
cd_smoke_blit_inside:
        dd 0, 0, 100, 100          ; dc
        dd 0, 0, 100, 100          ; sc
        dd 10, 20                  ; dst_x, dst_y
        dd 0, 0                    ; src_x, src_y
        dd 30, 40                  ; w, h
        dd 0xAABBCCDD              ; bitmap marker
        dd 0x11223344              ; stride marker
cd_smoke_blit_clamp:
        dd 0, 0, 200, 200          ; dc
        dd 10, 0, 100, 100         ; sc (clips left 10 of src)
        dd 50, 60                  ; dst
        dd 0, 0                    ; src
        dd 50, 40                  ; w, h
        dd 0, 0
cd_smoke_blit_reject_src:
        dd 0, 0, 100, 100          ; dc
        dd 50, 0, 100, 100         ; sc — src [0,40) rejects
        dd 0, 0
        dd 0, 0
        dd 40, 20
        dd 0x11111111, 0x22222222
cd_smoke_blit_reject_dst:
        dd 0, 0, 50, 50            ; dc — remapped dst outside
        dd 0, 0, 100, 100          ; sc
        dd 100, 100                ; dst
        dd 0, 0
        dd 20, 20
        dd 0x33333333, 0x44444444
endg

; Called once from high_code after Cut H smoke.
; Exercises: public blit_clip trampoline → Rust → CF + field mutate.
; Vectors: inside, src-clamp remap, reject-src, reject-dst; EBX/ESI/EDI/EBP.
align 4
blit_clip_rust_smoke_test:
        push    ebx
        push    esi
        push    edi
        push    ebp

        ; --- fully inside → CF clear, geometry unchanged, markers intact ---
        mov     ebx, 0xB1000001
        mov     esi, 0x51000002
        mov     edi, 0xD1000003
        mov     ebp, 0xE1000004
        mov     ecx, cd_smoke_blit_inside
        call    blit_clip
        jc      .fail
        cmp     dword [cd_smoke_blit_inside + 32], 10   ; dst_x
        jne     .fail
        cmp     dword [cd_smoke_blit_inside + 48], 30   ; w
        jne     .fail
        cmp     dword [cd_smoke_blit_inside + 56], 0xAABBCCDD
        jne     .fail
        cmp     dword [cd_smoke_blit_inside + 60], 0x11223344
        jne     .fail
        cmp     ebx, 0xB1000001
        jne     .fail
        cmp     esi, 0x51000002
        jne     .fail
        cmp     edi, 0xD1000003
        jne     .fail
        cmp     ebp, 0xE1000004
        jne     .fail

        ; --- src clamp → dst_x=60, src_x=10, w=40 ---
        mov     ebx, 0xB2000001
        mov     esi, 0x52000002
        mov     edi, 0xD2000003
        mov     ebp, 0xE2000004
        mov     ecx, cd_smoke_blit_clamp
        call    blit_clip
        jc      .fail
        cmp     dword [cd_smoke_blit_clamp + 32], 60    ; dst_x
        jne     .fail
        cmp     dword [cd_smoke_blit_clamp + 40], 10    ; src_x
        jne     .fail
        cmp     dword [cd_smoke_blit_clamp + 48], 40    ; w
        jne     .fail
        cmp     dword [cd_smoke_blit_clamp + 52], 40    ; h
        jne     .fail
        cmp     ebx, 0xB2000001
        jne     .fail
        cmp     esi, 0x52000002
        jne     .fail
        cmp     edi, 0xD2000003
        jne     .fail
        cmp     ebp, 0xE2000004
        jne     .fail

        ; --- reject src → CF set, fields unchanged ---
        mov     ebx, 0xB3000001
        mov     esi, 0x53000002
        mov     edi, 0xD3000003
        mov     ebp, 0xE3000004
        mov     ecx, cd_smoke_blit_reject_src
        call    blit_clip
        jnc     .fail
        cmp     dword [cd_smoke_blit_reject_src + 32], 0
        jne     .fail
        cmp     dword [cd_smoke_blit_reject_src + 48], 40
        jne     .fail
        cmp     dword [cd_smoke_blit_reject_src + 56], 0x11111111
        jne     .fail
        cmp     ebx, 0xB3000001
        jne     .fail
        cmp     esi, 0x53000002
        jne     .fail
        cmp     edi, 0xD3000003
        jne     .fail
        cmp     ebp, 0xE3000004
        jne     .fail

        ; --- reject dst → CF set, fields unchanged ---
        mov     ebx, 0xB4000001
        mov     esi, 0x54000002
        mov     edi, 0xD4000003
        mov     ebp, 0xE4000004
        mov     ecx, cd_smoke_blit_reject_dst
        call    blit_clip
        jnc     .fail
        cmp     dword [cd_smoke_blit_reject_dst + 32], 100
        jne     .fail
        cmp     dword [cd_smoke_blit_reject_dst + 48], 20
        jne     .fail
        cmp     dword [cd_smoke_blit_reject_dst + 56], 0x33333333
        jne     .fail
        cmp     ebx, 0xB4000001
        jne     .fail
        cmp     esi, 0x54000002
        jne     .fail
        cmp     edi, 0xD4000003
        jne     .fail
        cmp     ebp, 0xE4000004
        jne     .fail

        mov     [rust_blit_clip_smoke_result], 0x424C4954 ; 'BLIT'
        pop     ebp
        pop     edi
        pop     esi
        pop     ebx
        ret
.fail:
        mov     eax, 0xDEAD0CD0        ; Cut CD fail marker
        mov     ebx, 0x424C4954        ; 'BLIT'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_blit_clip(blitter) -> EAX 0/1; ret 4
align 16
rust_blit_clip:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_blit_clip.bin'
