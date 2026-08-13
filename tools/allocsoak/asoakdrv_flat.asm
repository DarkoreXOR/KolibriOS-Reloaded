format PE DLL
entry START

include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'

section '.flat' readable writeable executable

proc START stdcall, state:dword
        cmp     [state], 1
        jne     .done
        ; Exercise AllocPage/FreePage without RegService.
        call    [AllocPage]
        test    eax, eax
        jz      .done
        mov     [last], eax
        call    [FreePage]
        mov     esi, msg
        call    [SysMsgBoardStr]
.done:
        mov     eax, 1
        ret
endp

last dd 0
msg db 'ALLOCSOK AP-OK',13,10,0

data import
  library kernel,'KERNEL'
  import kernel,\
    AllocPage,'AllocPage',\
    FreePage,'FreePage',\
    SysMsgBoardStr,'SysMsgBoardStr'
end data

; Kolibri maps drivers away from ImageBase — fixups are mandatory for IAT calls.
data fixups
end data
