format PE DLL
entry START
include '../fasm/INCLUDE/MACRO/PROC32.INC'

section '.text' code readable executable
proc START stdcall, state:dword
        mov     eax, 1
        ret
endp
