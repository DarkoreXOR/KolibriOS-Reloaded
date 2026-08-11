# Cut AT Implementation — `get_coff_sym`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-at-plan.md`](cut-at-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `get_coff_sym` |
| Source | [`kernel/core/dll.inc`](../../kernel/core/dll.inc) |
| Callers | 3 live (`load_library` ×2 `EXPORTS`/`_EXPORTS`; `ext_lib.inc` — currently not included) |
| Rust symbol | `rust_get_coff_sym` |
| Pure helper | `kolibri_utils::get_coff_sym` |
| Subsystem | PE/COFF symbol lookup (Stage-8 foothold) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AS audit: socket lifecycle Path A rejected
(mutex/insert/remove still FASM); Y+`get_coff_sym`/`rebase_coff` do not own
the PE loader; I+`createMcbEntry` encode ≠ FRS ownership (high write blast);
AQ/X+AR unchanged rejects. Selected **`get_coff_sym`** — strongest remaining
leaf: Stage-8 PE name→Value scan with clear stdcall ABI and excellent
differential domain. Preferred over `createMcbEntry` (blast) and
`rebase_coff` (Y anti-cluster).

REG-001: trampoline preserves **EBX+ECX+EDX+ESI+EDI+EBP** (`uses`). Legacy
FASM leaves EBX/ESI/EDI/EBP untouched (no `uses`); ECX/EDX may be clobbered
by `strncmp`. ABI smoke asserts legacy-visible preserves only.

---

## Candidate comparison (post-AS audit)

| Candidate | Outcome |
|-----------|---------|
| `get_coff_sym` | **Selected** — Stage-8 PE name→Value |
| `createMcbEntry` | #2 — NTFS MCB encode; high FRS blast |
| `rebase_coff` | Defer — Y anti-cluster / mutate |
| `ipv4_find_fragment_slot` / `memmove` | Defer — weak soak / high fanout |
| Socket Path A / `socket_check_port` | Reject — artificial AS extension |
| Y+sym Path A / AQ / X+AR | Reject — ownership incomplete |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_GET_COFF_SYM=0`:

```text
stdcall get_coff_sym(pSym, count, sz_sym) → EAX
  ret 12
in:  pSym = COFF_SYM*; count = nSymbols; sz_sym = name (strncmp len 8)
out: EAX = Value dword on first name match / 0 on miss
does not touch EBX/ESI/EDI/EBP; ECX/EDX may change via strncmp
```

Quirk: `count == 0` still runs one compare, then `dec` wraps — host oracle
excludes this case; production passes `nSymbols > 0`.

---

## Rust ABI

```text
stdcall rust_get_coff_sym(pSym, count, sz_sym) → EAX
  ret 12
```

Trampoline: same stack args; preserves EBX/ECX/EDX/ESI/EDI/EBP. Inline
8-byte strncmp-equivalent compare (no call to `rust_strncmp` — reloc-free).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `get_coff_sym.rs` + `ffi.rs` section `.text.rust_get_coff_sym` |
| Extract | `extract_reloc_free_text.py` → `rust_get_coff_sym.bin` |
| Embed | `kernel/rust/get_coff_sym.inc` `file` directive |
| Trampoline | `dll.inc` under `USE_RUST_GET_COFF_SYM` |
| Gate | `USE_RUST_GET_COFF_SYM` (prod 1) |
| Smoke | `get_coff_sym_rust_smoke_test` (after LTR / near Cut Y) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_get_coff_sym` |
| Blob/object size | 204 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `836C3F67EAD20AF26617CDF6A2F791AE6208C4E9B3C36A72922377F07DCD7B0F` |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs `get_coff_sym` | **PASS** |
| Named vectors | hit first/mid/last; miss; EXPORTS/_EXPORTS; 8-byte exact; NUL stop; count edge; first-match-wins |
| Name-eq vs Cut D `strncmp` oracle | **PASS** |
| PRNG | 50 000 vectors, seed `0x43555454` (`'CUTT'`) |
| Host tests | **441/441** cargo tests (incl. get_coff_sym suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `get_coff_sym_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C54` hang) |
| Vectors | Direct `rust_*` EXPORTS/_EXPORTS; public hit last + miss; count=1 miss / count=2 hit; EBX/ESI/EDI/EBP canaries |
| Marker | `rust_get_coff_sym_smoke_result = 'GCSY'` on success |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_GET_COFF_SYM=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_GET_COFF_SYM=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

---

## A/B validation

| Workload | OFF | ON | Verdict |
|----------|-----|----|---------|
| Desktop | 779380 | 779380 | **match** |

---

## Real subsystem soak

```text
Real subsystem soak: NOT AVAILABLE
```

Forced COFF DLL / `load_library` symbol lookup was not scripted. Boot-time
`dll.Load` for kernel `.obj` is under `if 0`; `ext_lib.inc` is not included.
Stock image may load drivers later, but a dedicated `.sys`/DLL reload harness
was not run (same class as Cut Y). Leaf validated by differential + ABI smoke
on synthetic `COFF_SYM` tables; desktop OFF/ON is integration regression.

---

## Regressions

```text
NONE
```

(No live REG-NNN append for this cut.)

---

## Production gate

```text
USE_RUST_GET_COFF_SYM = 1
```

Rollback: `USE_RUST_GET_COFF_SYM = 0` (or `enabled = false` in
`project/build.toml` for cut AT).

Image: `dev_build/cut-at-final.img`

---

## Files changed

* `rust_kernel/kolibri_utils/src/get_coff_sym.rs` — pure walk + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_get_coff_sym` section
* `rust_kernel/kolibri_utils/src/lib.rs` — module export
* `rust_kernel/kolibri_utils/out/rust_get_coff_sym.bin` — extracted blob
* `kernel/rust/get_coff_sym.inc` — embed + ABI smoke
* `kernel/core/dll.inc` — trampoline + gate + FASM rollback body
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call after Cut Y
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-at-plan.md` / `cut-at-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* No forced `.sys` / DLL `load_library` soak harness
* PE loader / `fix_coff_relocs` / `rebase_coff` remain FASM
* No Path A claim with Cut Y
* `count == 0` FASM wrap quirk retained but excluded from host PRNG
* `ext_lib.inc` caller site exists but is not currently included in the build
