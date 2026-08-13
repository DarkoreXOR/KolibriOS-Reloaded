;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; allocsoak — MENUET01 loader for PE asoakdrv
;;
;; Loads /sys/ASOAKDRV via 68.21 (driver runs soak in START, returns 0).
;; Evidence is kernel msg_board markers from the PE; then start LAUNCHER.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

use32
        org     0
        db      'MENUET01'
        dd      1, start, i_end, mem, mem, 0, 0

start:
        mov     eax, 68
        mov     ebx, 21
        mov     ecx, drv_path
        xor     edx, edx
        int     0x40
        mov     [drv_handle], eax

        mov     eax, 5
        mov     ebx, 30
        int     0x40

        mov     eax, 70
        mov     ebx, file70_launcher
        int     0x40

        or      eax, -1
        int     0x40

align 4
file70_launcher:
        dd 7,0,0,0,0
        db '/sys/LAUNCHER',0
drv_path:
        db '/sys/ASOAKDRV',0

align 4
drv_handle dd ?

i_end:
        rb 4096
mem = $ + 0x10000
