# Cut BR Implementation — `ext_read_all_times`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-br-plan.md`](cut-br-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **BR** |
| FASM symbol | `ext_read_all_times` |
| Source | [`kernel/fs/ext.inc`](../../kernel/fs/ext.inc) |
| Callers | 2 live (`ext_ReadFolder`, `ext_GetFileInfo`) |
| Rust symbol | `rust_ext_read_all_times` |
| Pure helper | `kolibri_utils::ext_read_all_times_ptr` (inlined AL+T) |
| Subsystem | EXT / inode triple-timestamp → BDFE |
| Stage | Stage-2/5 Path B (EXT read deepen after Cut AL) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — EXT triple-timestamp fan-out does not establish Rust-owned
EXT mount/inode subsystem. Cut AL + BR remain complementary Path B leaves.

Selected `ext_read_all_times` over `fix_coff_symbols`, `ext_write_time`, and
`fsGetTime` for inode-scale EXT semantic class + two live callers + strong
synthetic differential.

---

## Candidate comparison (post-BQ audit)

| Rank | Candidate | Outcome |
| ---- | --------- | ------- |
| 1 | `ext_read_all_times` | **SELECT** — EXT 3× timestamp inode fan-out |
| 2 | `fix_coff_symbols` | Defer — PE deepen / `get_proc_ex` |
| 3 | `ext_write_time` | Defer — `fsGetTime` / no `--disk ext` |
| 4 | `fsGetTime` | Defer — CMOS/calendar caution |
| 5 | `strchr` | Reject — export-only |

---

## Legacy ABI

```text
register call ext_read_all_times
in:  ESI -> inode; EDI -> 3× BDFE out
out: cr/cTime, aTime, mTime BDFE blocks written (+24 bytes)
preserves: ESI; ECX clobbered (callers push/pop where live)
clobbers: EAX, EDX, ECX, EDI
```

Quirks retained:

* Fast path when `extraISize >= 24`: cr/a/m with all Extra fields.
* Slow path: `ecx = (extraISize - 4) / 4` (0 when `extraISize < 4`).
* First slot: `crTime` when `ecx >= 4`, else `cTime` (+ optional `cTimeExtra`).
* `aTimeExtra` when `ecx >= 3`; `mTimeExtra` when `ecx >= 2`.
* Each slot via Cut AL epoch convert + Cut T calendar (inlined in Rust blob).

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_ext_read_all_times` |
| Blob | **2822** bytes, **0 relocations** |
| SHA-256 | `AD10806CFEB760ED5CFBE547B1FED3B0DFAAA69341E097CDDE3D53EC26B0EC69` |
| Trampoline | `kernel/fs/ext.inc` under `USE_RUST_EXT_READ_ALL_TIMES` |
| Gate | `USE_RUST_EXT_READ_ALL_TIMES` (prod 1) |
| Rust ABI | `stdcall rust_ext_read_all_times(inode, out); ret 8` |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow partial/fast path + per-slot `fasm_oracle_ext_read_time` |
| Host tests | **PASS** — `593/593` (includes 8 Cut BR tests + 50k PRNG) |
| Seed | `0x43554252` (`'CUBR'`) |
| Exact PRNG count | **50,000** |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ext_read_all_times_rust_smoke_test` | **PASS** |
| Marker | `rust_ext_read_all_times_smoke_result = 'EXBR'` |
| Coverage | no-extra + fast-path public trampoline; partial direct `rust_*`; ESI/EBX/EBP canaries |
| Live state | isolated synthetic `iglobal` inode/out buffers only (REG-003 safe) |

Initial smoke failure (non-zero TimeExtra triggering AL clamp_max) fixed before gate ON.

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `enabled = false` | **OK** (`running`, 779380 non-black) | `dev_build/br-off.ppm` (deleted after A/B) |
| ON | `USE_RUST_EXT_READ_ALL_TIMES=1` | **OK** (`running`, 779380 non-black) | `dev_build/br-on.ppm` (deleted after A/B) |

Tooling: `python scripts/qmp_desktop_smoke.py --wait 25`.

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON desktop non-black count | **PASS** — 779380 vs 779380 |
| OFF vs ON attach-only exFAT secondary disk | **PASS** — both `running`, 779380 non-black |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| Attach-only exFAT secondary disk | **PASS** — OFF/ON desktop equivalence |
| `--disk ext` EXT readdir/GetFileInfo timestamp path | **NOT AVAILABLE** — no persistent EXT regression image |
| Scripted EXT folder browse | **NOT AVAILABLE** |

Precision: callers live in `ext_ReadFolder` / `ext_GetFileInfo`, but no `--disk ext`
harness exists (same class as Cut AL/BH).

---

## Regressions

| Item | Result |
|------|--------|
| Live regressions discovered | **none** |
| Regression-log entry | none |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_EXT_READ_ALL_TIMES = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/test/kernel-20260812-111124.img` |
| Rollback | `USE_RUST_EXT_READ_ALL_TIMES = 0` or `[[rust.migrations]]` `cut = "BR"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/ext_read_all_times.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/src/lib.rs`
* `kernel/rust/ext_read_all_times.inc`
* `kernel/fs/ext.inc`
* `kernel/kernel.asm`
* `kernel/kernel32.inc`
* `project/build.toml`
* `docs/migration/cut-br-plan.md`
* `docs/migration/cut-br-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/migration-todo.md`
* `docs/migration/boundaries.md`

## Known limitations

* Read-only inode fan-out — does not migrate `ext_write_time` / `fsGetTime`.
* FASM advances `EDI` by +24 through the `ext_read_time` chain; Rust writes via
  pointer but does not update caller `EDI` (callers do not observe `EDI` post-call).
* No `--disk ext` soak; attach-only exFAT A/B only.
* Large blob (2822 B) due to triple inlined AL+T calendar path — acceptable for reloc-free discipline.

---

## Inventory

**73 / 135** — one new `[x]` (`ext_read_all_times`).
