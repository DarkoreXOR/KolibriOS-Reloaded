;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; Minimal bring-up PE driver — RegService only (no AllocPage yet)
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

format PE DLL
entry START

include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'

section '.text' code readable executable

proc START stdcall, state:dword
        cmp     [state], 1
        jne     .ok
        mov     esi, msg
        call    [SysMsgBoardStr]
.ok:
        stdcall [RegService], sz_name, service_proc
        ret
endp

proc service_proc stdcall, ioctl:dword
        xor     eax, eax
        ret
endp

section '.data' data readable writeable
sz_name db 'ASOAKDRV',0
msg     db 'ALLOCSOK START',13,10,0

section '.idata' import data readable writeable
  library kernel,'KERNEL'
  import kernel,\
    RegService,'RegService',\
    SysMsgBoardStr,'SysMsgBoardStr'
