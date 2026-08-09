# Subsystem: GUI / Window Manager

**Responsibilities:** windows, z-order, redraw, buttons, skins, background, fonts, mouse cursor.  
**Files:** `gui/*`, `video/*`.  
**Structs:** `WDATA`, `display_t`, `CURSOR`, skin structs.  
**Events:** bitmask path + syscall 72 messages.  
**Compat:** syscall GUI surface BEHAVIORAL+HARD layouts; fixed window arrays SHIM.  
**Risk:** timing-sensitive; migrate late.
