;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; block_clip — Rust embed + runtime smoke (Cut H)
;;
;; Embeds reloc-free machine code extracted from libkolibri_utils.a
;; (section .text.rust_block_clip).
;; See docs/migration/cut-h-plan.md / cut-h-implementation.md.
;;
;; Prerequisites:
;;   powershell -File rust_kernel/kolibri_utils/build-block-clip.ps1
;; which writes:
;;   ../../rust_kernel/kolibri_utils/out/rust_block_clip.bin
;;
;; kernel/video/blitter.inc (USE_RUST_BLOCK_CLIP=1) trampolines
;; block_clip → rust_block_clip → CF.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

iglobal
; Survives boot: observable proof that Rust block_clip path ran.
rust_block_clip_smoke_result dd 0
; Static RECT pairs: clip then rect (left,top,right,bottom dwords).
; 1) full contain — draw, rect unchanged
bc_smoke_clip_contain:
        dd 0, 0, 100, 100
bc_smoke_rect_contain:
        dd 10, 20, 30, 40
; 2) clamp all edges — draw → {10,10,90,90}
bc_smoke_clip_clamp:
        dd 10, 10, 90, 90
bc_smoke_rect_clamp:
        dd 0, 0, 100, 100
; 3) reject outside X — CF=1, rect unchanged
bc_smoke_clip_reject_x:
        dd 50, 0, 100, 100
bc_smoke_rect_reject_x:
        dd 0, 10, 40, 20
; 4) Y-fail after X clamp — CF=1, X mutated to {10,0,90,40}
bc_smoke_clip_yfail:
        dd 10, 50, 90, 100
bc_smoke_rect_yfail:
        dd 0, 0, 100, 40
endg

; Called once from high_code after Cut G smoke.
; Exercises: FASM callers → block_clip trampoline → Rust body → CF.
; Vectors: contain, clamp, reject-X, Y-fail-after-X; ESI/EDI/EBP/EBX preserve.
; On mismatch: distinctive hang (does not reach desktop).
align 4
block_clip_rust_smoke_test:
        mov     ebx, 0xB8000001
        mov     esi, 0x58000002
        mov     edi, 0xD8000003
        mov     ebp, 0xE8000004

        ; contain → CF clear, rect unchanged
        mov     esi, bc_smoke_clip_contain
        mov     edi, bc_smoke_rect_contain
        call    block_clip
        jc      .fail
        cmp     dword [bc_smoke_rect_contain], 10
        jne     .fail
        cmp     dword [bc_smoke_rect_contain+4], 20
        jne     .fail
        cmp     dword [bc_smoke_rect_contain+8], 30
        jne     .fail
        cmp     dword [bc_smoke_rect_contain+12], 40
        jne     .fail

        ; clamp → CF clear, rect = {10,10,90,90}
        mov     esi, bc_smoke_clip_clamp
        mov     edi, bc_smoke_rect_clamp
        call    block_clip
        jc      .fail
        cmp     dword [bc_smoke_rect_clamp], 10
        jne     .fail
        cmp     dword [bc_smoke_rect_clamp+4], 10
        jne     .fail
        cmp     dword [bc_smoke_rect_clamp+8], 90
        jne     .fail
        cmp     dword [bc_smoke_rect_clamp+12], 90
        jne     .fail

        ; reject X → CF set, rect unchanged
        mov     esi, bc_smoke_clip_reject_x
        mov     edi, bc_smoke_rect_reject_x
        call    block_clip
        jnc     .fail
        cmp     dword [bc_smoke_rect_reject_x], 0
        jne     .fail
        cmp     dword [bc_smoke_rect_reject_x+8], 40
        jne     .fail

        ; Y-fail after X clamp → CF set; left=10, right=90
        mov     esi, bc_smoke_clip_yfail
        mov     edi, bc_smoke_rect_yfail
        call    block_clip
        jnc     .fail
        cmp     dword [bc_smoke_rect_yfail], 10
        jne     .fail
        cmp     dword [bc_smoke_rect_yfail+8], 90
        jne     .fail
        cmp     dword [bc_smoke_rect_yfail+4], 0
        jne     .fail
        cmp     dword [bc_smoke_rect_yfail+12], 40
        jne     .fail

        ; Register preservation (ESI/EDI/EBP/EBX) on a fresh contain call.
        ; Restore contain rect (already unchanged) and re-run with sentinels.
        mov     ebx, 0xB8000001
        mov     esi, bc_smoke_clip_contain
        mov     edi, bc_smoke_rect_contain
        mov     ebp, 0xE8000004
        call    block_clip
        jc      .fail
        cmp     esi, bc_smoke_clip_contain
        jne     .fail
        cmp     edi, bc_smoke_rect_contain
        jne     .fail
        cmp     ebp, 0xE8000004
        jne     .fail
        cmp     ebx, 0xB8000001
        jne     .fail

        mov     [rust_block_clip_smoke_result], 0x42434C50 ; 'BCLP'
        ret
.fail:
        mov     eax, 0xDEAD0C48        ; Cut H fail marker
        mov     ebx, 0x42434C50        ; 'BCLP'
        mov     ecx, 0x4641494C        ; 'FAIL'
        jmp     $

; Raw Rust stdcall body:
;   rust_block_clip(clip, rect) -> EAX 0/1; ret 8
align 16
rust_block_clip:
        ; Path is relative to this include (kernel/rust/), not kernel.asm.
        file '../../rust_kernel/kolibri_utils/out/rust_block_clip.bin'
