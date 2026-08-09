# Syscall Audit

Cross-check: `servetable2` × `sysfuncs.txt` × implementations.

## Entry mechanisms

| Entry | Documented? | Implemented? | Class |
|-------|-------------|--------------|-------|
| `int 0x40` | Yes | Yes (`i40`) | HARD |
| SYSENTER | **No** in sysfuncs | Yes | HARD undocumented |
| AMD SYSCALL | **No** | Yes | HARD undocumented |

**SYSENTER convention (LOCAL FACT `syscall.entry`):**

- Kernel loads `esp` from `tss._esp0`
- User passes `ebp` = pointer to user stack area used for sysexit restore
- `push ebp`; `mov ebp,[ebp]`; after handler, reconstructs return via `sysexit` (edx=eip, ecx=esp)

**UNKNOWN:** exact user-mode stub instruction sequence (not in this tree).

## Global doc claims vs reality

| Claim (`sysfuncs.txt`) | Reality | Class |
|------------------------|---------|-------|
| Only `int 0x40` | Also SYSENTER/SYSCALL | Doc incomplete |
| All regs + eflags preserved | APM sets CF; SYSENTER doesn’t restore eflags via iret | HARD exceptions |
| Returns via EAX | Handlers must write `SYSCALL_STACK.eax` or old EAX (fn#) remains | Implementation hazard |

## Top-level table integrity

- Documented live functions match implemented slots (see prior inventory).
- **Fn 58:** documented remnants vs `undefined_syscall` — keep −1.
- **Fn 6,19,…:** undefined → −1 HARD.
- Table comments “74–76 reserved” are **stale**; code+docs live.

## Nested: function 18

`sys_system_table` in `kernel.asm`; EBX subfn after `dec ebx`. Subfn 12 removed → `undefined_syscall`.

## Nested: function 68

**LOCAL FACT dispatch (`memory.inc:f68`):**

| EBX | Path |
|-----|------|
| 0–4 | `sys_sheduler` (switch counter, yield, rdpmc/cache, rdmsr, wrmsr) |
| 5–10 | `undefined_syscall` (−1) |
| 11–31 | `f68call[ebx-11]` |
| >31 | −1 |

**Doc bugs / gaps:**

| Issue | Evidence | Rust action |
|-------|----------|-------------|
| f68.15 missing in docs; returns **0** via fail stub | `f68call` | Return 0 |
| f68.25 docs mention edx; code uses ecx bit ops only | `memory.inc` | Match **code** |
| f68.4 blocks SYSENTER/STAR MSRs silently | `kernel.asm` | Match code |
| f68.1 “no return value” — eax unrestored specially | yield | Match |
| Fn77 docs label some ops `SF_FUTEX` wrongly | sysfuncs | Match posix.inc |

## Function 9 specifics

| Item | Docs | Code | Preserve |
|------|------|------|----------|
| Buffer | 1KB recommended | validates **0x4C** only | Fill 0x4C; ignore extra |
| Name | 11 bytes | copies 11; struct pad 12 | 11 chars |
| Slot −1 | current | yes | yes |
| Kernel ptr reject | yes | `is_region_userspace` | yes |

## Return convention checklist for implementers

1. Decode args from `SYSCALL_STACK` after `pushad`.
2. Write results to stacked EAX/EDX/… as existing handler does.
3. Do not assume C ABI return in EAX alone without stack write.
4. Preserve eflags for `int 0x40` except documented CF cases.

## Undocumented but implemented behaviors to treat as ABI

1. SYSENTER/SYSCALL entries.
2. f68 ebx 5–10 → −1.
3. f68.15 → 0.
4. MSR write filter for sysenter MSRs.
5. `undefined_syscall` → −1 for sparse holes.

## BEHAVIORAL (not bit-identical required)

- Exact task-switch count timing vs f68.0
- GUI redraw coalescing
- Network timing

## Prior suite gaps closed here

- Complete f68 0–4 vs 11–31 split
- SYSENTER stack/EBP detail
- Doc/code conflicts list for 58, 68.15, 68.25, 77 labels
