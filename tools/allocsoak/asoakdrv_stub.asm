format PE DLL
entry START
include '../fasm/INCLUDE/MACRO/PROC32.INC'
include '../fasm/INCLUDE/MACRO/IMPORT32.INC'

section '.text' code readable executable
proc START stdcall, state:dword
        ; Return nonzero non-SRV → load_pe_driver treats as fail_init and frees.
        ; Proves START was reached without hanging in imports/load.
        mov     eax, 1
        ret
endp

section '.idata' import data readable writeable
  library kernel,'KERNEL'
  import kernel,\
    RegService,'RegService'
