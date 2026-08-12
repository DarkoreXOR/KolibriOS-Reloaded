# Cut BU Implementation — `fix_coff_symbols`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bu-plan.md`](cut-bu-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BU** |
| FASM symbol | `fix_coff_symbols` |
| Source | [`kernel/core/dll.inc`](../../kernel/core/dll.inc) |
| Callers | 2× `stdcall fix_coff_symbols` (`load_library`, `ext_lib.inc`) |
| Rust symbol | `rust_fix_coff_symbols` |
| Pure helper | `kolibri_utils::fix_coff_symbols` |
| Subsystem | PE/COFF symbol table resolve |
| Stage | Stage 8 PE loader foothold |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — Y/AT/BK/BU leaves do not establish Rust-owned PE loader.

Selected `fix_coff_symbols` over `fsGetTime`, `tcp_mss`, and `fsReadCMOS` for
first COFF symbol-table resolve semantic class + two live callers + strong
synthetic oracle after fresh post-BT audit.

---

## Legacy ABI

```text
stdcall fix_coff_symbols(sec, symbols, sym_count, strings, imports)
  walk sym_count × COFF_SYM (18 B)
  SectionNumber == 0: resolve name → get_proc_ex → mov Value
  SectionNumber in {0xFFFF, 0xFFFE}: skip
  else: Value += sec[secnum-1].VirtualAddress
  EAX = 1 unless any external resolve returned 0 → 0
clobbers: EAX, EBX, EDI (proc uses ebx esi)
stack: ret 20
```

Quirks retained:

* External inline name (`Name` dword ≠ 0): `get_proc_ex(edi→sym, imports)`
* Long external name: offset in `[sym+4]` + `strings`
* Internal section index: `(secnum-1)*40` byte stride (FASM `dec/shl/lea`)
* External replaces `Value`; internal **adds** to existing `Value`
* Unresolved external sets `retval=0` but loop continues

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_fix_coff_symbols` |
| Blob | **177** bytes, **0 relocations** |
| SHA-256 | `96566d16030bd4a2e4f2e8fea991dc3ddc55947688649c40bbcd24c49bc19308` |
| Trampoline | injects `get_proc_ex` as 6th arg → `rust_fix_coff_symbols` |
| Gate | `USE_RUST_FIX_COFF_SYMBOLS` (prod 1) |
| Rust ABI | `stdcall(..., get_proc_ex); ret 24` → EAX |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent buffered FASM-flow mirror (`fix_coff_symbols_oracle`) |
| Host tests | **PASS** — `615/615` (includes 9 Cut BU tests + 50k PRNG) |
| Seed | `0x43554255` (`'CUBU'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `fix_coff_symbols_rust_smoke_test` | **PASS** |
| Marker | `rust_fix_coff_symbols_smoke_result = 'FCFS'` |
| Coverage | direct Rust + smoke `get_proc_ex` stub (internal+external+unresolved); public trampoline internal-only (real `get_proc_ex` needs live imports) |
| Fixture lesson | Vector 2 must not call public path with external syms + `imports=0` — fixture defect, not kernel regression |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` | **OK** (`running`, 779380 non-black) | A/B capture |
| ON | `USE_RUST_FIX_COFF_SYMBOLS=1` | **OK** (`running`, 779380 non-black) | A/B capture |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |

---

## Real subsystem soak

| Harness | Result |
|---------|--------|
| Desktop boot | **PASS** (QMP smoke) |
| PE/DLL load via `.sys` | **PARTIAL** — no dedicated COFF resolve harness; exercised indirectly at desktop |
| `--disk ntfs` / `--disk xfs` | **NOT AVAILABLE** — unrelated to PE sym resolve |

---

## Regressions

None. Initial smoke fixture (public trampoline + external + `imports=0`) corrected before close — **not** logged as REG (fixture defect).

---

## Production / rollback

| Item | Value |
|------|-------|
| Production gate | `USE_RUST_FIX_COFF_SYMBOLS = 1` |
| Rollback | `USE_RUST_FIX_COFF_SYMBOLS = 0` in `kernel/core/dll.inc` (or `enabled = false` in `build.toml`) |
| Final image | `dev_build/test/kernel-20260812-122004.img` |

---

## Files changed

| Path | Role |
|------|------|
| `rust_kernel/kolibri_utils/src/fix_coff_symbols.rs` | Algorithm + host tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_fix_coff_symbols` |
| `rust_kernel/kolibri_utils/src/lib.rs` | module export |
| `kernel/rust/fix_coff_symbols.inc` | embed + smoke |
| `kernel/core/dll.inc` | gate + trampoline |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | smoke call |
| `project/build.toml` | blob + migration registry |
| `docs/migration/cut-bu-plan.md` | audit + selection |
| `docs/migration/cut-bu-implementation.md` | this file |
| `docs/migration/migration-todo.md` | inventory 76/135 |
| `docs/migration/migration-plan.md` | Cut BU entry |
| `docs/migration/boundaries.md` | LOCAL FACT update |

---

## Known limitations

* `get_proc_ex` remains FASM — Rust receives injected callback only.
* Smoke public-trampoline vector tests internal path only (externals need live import table).
* No dedicated PE symbol-resolve QEMU harness beyond desktop partial soak.

---

## Updated inventory

**76 / 135** (post-Cut BU).

Cut BT (`ntfsGetTime`) remains **closed** — untouched.

**Stop; do not start Cut BV.**
