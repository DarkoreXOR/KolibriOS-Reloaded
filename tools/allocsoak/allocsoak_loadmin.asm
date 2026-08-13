use32
        org     0
        db      'MENUET01'
        dd      1, start, i_end, mem, mem, 0, 0
start:
        mov     eax, 68
        mov     ebx, 21
        mov     ecx, path
        xor     edx, edx
        int     0x40
        mov     [h], eax
        mov     eax, 5
        mov     ebx, 20
        int     0x40
        mov     eax, 70
        mov     ebx, L
        int     0x40
        or      eax, -1
        int     0x40
align 4
L:      dd 7,0,0,0,0
        db '/sys/LAUNCHER',0
path:   db '/sys/ASOAKDRV',0
h       dd ?
i_end:
        rb 1024
mem = $ + 0x4000
