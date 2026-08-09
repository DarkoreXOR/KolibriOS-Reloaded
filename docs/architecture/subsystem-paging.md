# Subsystem: Paging

**Responsibilities:** PDT/PT layout, recursive map at `page_tabs`, kernel half sharing, TLB.  
**Files:** `memory.inc`, `taskman.inc` `create_process`, upstream `init_mem`.  
**Invariants:** user `< OS_BASE`; kernel mappings shared; `cr3` per `PROC`.  
**Compat:** address split HARD; recursive VA INTERNAL.
