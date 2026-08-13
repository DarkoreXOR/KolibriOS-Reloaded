# Cut CN Plan

**Date:** 2026-08-13  
**Status:** complete — see [`cut-cn-implementation.md`](cut-cn-implementation.md)

---

## Fresh post-CM repository audit

### Baseline verification (2026-08-13)

| Check | Result |
|-------|--------|
| Inventory | **94 / 135** (`migration-todo.md`; 94 `[x]` + 41 `[ ]`) |
| Production gates | **94** `[[rust.migrations]]` with `enabled = true` |
| Cut CC–CM | intact (gates ON; CM blob SHA verified) |
| Cut CM blob SHA | `8dd04514d23f7448e300dfa833c33e6f2139683be8f7b80c515f741bd30a3b2a` (**101 B / 0 reloc**) |
| Cut CK/CL blob SHA | `766a371d747139c9f2520f4b6a55e18e6367fa9fdf6530637902d3a8be374572` (**25 B / 0 reloc** each) |
| `TMP_STACK_TOP` | **`0x008E000`** (`kernel/const.inc`; **not** `0x008F000` — user prompt stale) |
| `sys_proc` | **`OS_BASE+0x008E000`** (**not** `0x008F000`) |
| `SLOT_BASE` | **`OS_BASE+0x0090000`** (**not** `0x0091000`; REG-012 pack) |
| End `.bss` (build) | **`OS_BASE+0x8CFC3`** |
| Early-stack assert | `0x8DFC3 < 0x8E000` → **0x3D (~61 B)** headroom |
| `--disk ext` tooling | intact (`scripts/mkfs.py ext`, `images/ext-image.img`) |
| Docs vs tree | CM plan+impl agree with live gates and memory pack |

**Note:** Prompt memory baseline (`TMP_STACK_TOP`/`sys_proc` @ `0x8F000`, `SLOT_BASE` @ `0x91000`) does **not** match the repository after REG-012. Authoritative values are CM implementation pack above.

### Path A decision: **REJECTED**

Same clusters as Cut CM — no remaining pending symbol establishes Rust-owned subsystem state (PE/USB/video/exFAT-plugin/IRQ/string-export/allocator clusters fail ownership bar). Cut CN remains **Path B**.

---

## Complete candidate ranking (41 pending)

| Rank | Symbol | Oracle | Memory | Soak | Verdict |
|------|--------|--------|--------|------|---------|
| 1 | **`strchr`** | **5/5** chunk-growth forward search | ~60–90 B blob; +4 B smoke iglobal | PE export (0 kernel callers) | **SELECT** |
| 2 | `strnlen` | 5/5 | ~35 B | export-only | Reject — thin repne leaf |
| 3 | `tcp_mss` | 5/5 | ~30 B | 1 TCP caller | Reject — thin clamp+store / TCP deepen ban |
| 4 | `unpack` | 5/5 | KiB code + ~31 KiB heap LZMA | DLL load | Defer — Stage-2 architecture |
| 5 | `exFAT_find_lfn` | 3/5 | ~400–800 B + callbacks | `--disk exfat` | Defer — plugin island |
| 6 | `blit_32` | 3/5 | KiB+ | syscall blit | Defer — LFB blast |
| 7 | `drawChar` | 2/5 | KiB+ | GUI | Defer — Stage 7 |
| 8 | `mem_test` | 3/5 | ~60 B | boot-only | Defer — CR0/cache probe |
| 9 | `mutex_init` | 5/5 | ~25 B | ~33 callers | Reject — thin + fan-out |
| 10 | `enable_irq` / `irq_eoi` | 1/5 | ~80 B | IRQ path | Reject — I/O oracle |
| … | remaining 31 | varies | — | deferred/ban/unsuitable | unchanged |

### Special scrutiny outcomes

| Target | Outcome |
|--------|---------|
| `unpack` | ~500-line LZMA + heap `unpack.p`; no coherent smaller leaf without breaking production path |
| `blit_32` | LFB/cursor/bpp; pixel oracle required; Cut CD covers clip only |
| `exFAT_find_lfn` | Stack callbacks + `exFAT_get_name`; not a reloc-free leaf |
| `drawChar` | ~540 lines + smoothing; Stage 7 |
| IRQ | No mask/EOI state oracle |
| Thin/export | `strnlen`, `net_ptr_to_num`, `ntfs_restore_usa_frs`, `sysfn_getfreemem` rejected |

---

## Selected target: `strchr`

| Field | Value |
|-------|-------|
| Source | `kernel/core/string.inc` ~179–205 |
| Subsystem | core/string forward character search |
| Stage | Stage 2 / string leaf (complement to Cut BB `strrchr`) |
| Path | **B** |
| Callers | **0 in-kernel**; PE export (`exports.inc`) |
| Callees | none |
| Globals | none |
| Callbacks | none |

### Selection rationale

Post-CM, no pending in-kernel leaf combines oracle quality, reloc-free size, and subsystem fit. `getInodeLocation` closed the last strong FS address leaf. Remaining high-value targets (`unpack`, `blit_32`, `exFAT_find_lfn`, `drawChar`) fail memory, LFB, plugin, or Stage-7 bars. **`strchr`** is the forward-search twin of migrated **`strrchr`** (Cut BB): chunk-doubling `scasb` algorithm, excellent independent differential oracle, compact blob within ~61 B `.bss` headroom (smoke reuses BB synthetic strings + one `dd` result cell). Export-only soak is acknowledged; host oracle + PE export contract + desktop A/B parity match prior string-leaf evidence bar when kernel caller count is zero.

---

## Legacy ABI

```text
strchr  stdcall(s, c)
  in:  stack — s (ptr), c (int, low byte used)
  out: EAX = ptr to first c or NULL
  preserves: EDI (push/pop in leaf)
  clobbers: ECX, EDX, EAX, flags (repne/scasb loop)
  DF: cld at entry; unchanged at return
  stack: ret 8
```

## Rust ABI

```text
stdcall rust_strchr(s, c); ret 8
  → EAX = ptr as u32 (0 = NULL)
```

Trampoline: `stdcall rust_strchr`; `cld`; no double stack cleanup (REG-009).

---

## Oracle

| Item | Value |
|------|-------|
| Independent | Chunk-growth FASM-flow oracle (not derived from Rust body) |
| PRNG seed | `0x53434852` (`'SCHR'`) |
| PRNG cases | 50,000 |
| Edge cases | empty, NUL needle, first/mid/not-found, chunk boundaries, wide `c` truncation |

## Validation plan

| Layer | Plan |
|-------|------|
| Host tests | `schrc_*` focused + full suite |
| ABI smoke | Reuse `strrchr_smoke_*` strings; marker `'SCHR'`; hang `DEAD0C7E` |
| QEMU | OFF / ON / A/B / ON×3 |
| Subsystem soak | PE-export class — desktop parity (no in-kernel path) |
| Rollback | `USE_RUST_STRCHR = 0` |

## Memory impact

No layout change expected. Smoke adds **one** `dd` iglobal (+4 B). Blob in `.text` via `incbin`.

## Gate

`USE_RUST_STRCHR = 1` in `kernel/core/string.inc` / `project/build.toml`.

---

**Stop after Cut CN. Do not start Cut CO.**
