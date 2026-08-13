format PE DLL
entry START
include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'

section '.text' code readable executable
proc START stdcall, state:dword
        cmp     [state], 1
        jne     .ret1
        stdcall [RegService], sz_name, service_proc
        ret
.ret1:
        mov     eax, 1
        ret
endp
proc service_proc stdcall, ioctl:dword
        xor     eax, eax
        ret
endp
section '.data' data readable writeable
sz_name db 'ASOAKDRV',0
section '.idata' import data readable writeable
  library kernel,'KERNEL'
  import kernel,\
    RegService,'RegService'
