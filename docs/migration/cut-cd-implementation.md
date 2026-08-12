# Cut CD Implementation — `blit_clip`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-cd-plan.md`](cut-cd-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| Cut identifier | **CD** |
| FASM symbol | `blit_clip` |
| Source | [`kernel/video/blitter.inc`](../../kernel/video/blitter.inc) |
| Callers | 1× in `blit_32` (syscall 73 blit path) |
| Rust symbol | `rust_blit_clip` |
| Pure helper | `kolibri_utils::blit_clip` / `BlitterGeom` / `BlitClipResult` |
| Subsystem | Video / blit geometry compose |
| Stage | Stage 2 / Stage 5 video foothold (Cut H follow-on) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED** — video H+CD are geometry leaves; LFB/`blit_32` hot path remains FASM.

Selected `blit_clip` over `enable_irq`, `unpack`, and `window._.set_window_clientbox` for Cut H compose + strong oracle + manageable blast radius + desktop blit soak.

---

## Legacy ABI

```text
blit_clip() → void; plain ret
in:  ECX → BLITTER*
out: CF = 0 draw (mutates w,h,src_x,src_y,dst_x,dst_y)
     CF = 1 reject (geometry fields unchanged)
preserves: EBX, ESI, EDI
callees: block_clip ×2
```

**Legacy CF quirk:** FASM `.done` uses `add esp, 40` before `ret`, which writes CF and typically clears a reject carry. Cut CD trampoline restores the **documented** CF contract (`clc`/`stc` after pops). Mutation semantics match FASM exactly.

---

## Rust / trampoline

| Item | Detail |
|------|--------|
| Section | `.text.rust_blit_clip` |
| Blob | **389** bytes, **0 relocations** |
| SHA-256 | `e042423a265eeafbff243a6a39e135fc933570e2256097038dcb4144d27417a3` |
| Trampoline | `push edi/esi/ebx`; FASM **`stdcall rust_blit_clip, ecx`** (no `add esp`); `test eax` → pops → `clc`/`stc` |
| Gate | `USE_RUST_BLIT_CLIP` (prod 1) |
| Rust ABI | `stdcall rust_blit_clip(blitter); ret 4` → EAX 0/1 |
| Compose | Inlines pure `block_clip` (no reloc to `rust_block_clip`) |

---

## Differential

| Item | Result |
|------|--------|
| Oracle | Independent FASM-flow mirror (`fasm_oracle_blit_clip`) |
| PRNG seed | `0x424C4954` (`'BLIT'`) |
| PRNG cases | 50,000 |
| Cut CD host tests | focused geometry blit tests **PASS** |
| Full host suite | **658/658 PASS** |
| ABI smoke | **PASS** — marker `'BLIT'` (gated `if USE_RUST_BLIT_CLIP`) |

---

## QEMU validation

| Config | Gate | non-black | resets | Result |
|--------|------|-----------|--------|--------|
| OFF | `USE_RUST_BLIT_CLIP=0` | 779380 | 0 | PASS |
| ON | `USE_RUST_BLIT_CLIP=1` | 779380 | 0 | PASS |
| A/B | match | 779380 = 779380 | 0 | PASS |
| ON ×3 consecutive | 1 | 779380 each | 0 | PASS |

Harness: `python scripts/qmp_desktop_smoke.py --wait 90`  
Final image: `dev_build/test/kernel-20260812-155141.img`

---

## Subsystem soak

Desktop blit path (`blit_32` → `blit_clip` → syscall 73) exercised on every successful desktop smoke. OFF/ON non-black pixel counts match.

---

## Regressions

None new. Applied Cut CC lessons: FASM `stdcall` only (no double cleanup); ABI smoke asserts EBX/ESI/EDI/EBP; QMP `RESET` checked (`resets=0`).

---

## Production gate

`USE_RUST_BLIT_CLIP = 1` in `kernel/video/blitter.inc` (via `project/build.toml` migration registry).

---

## Rollback

Set `USE_RUST_BLIT_CLIP=0` / `enabled = false` for Cut CD in `project/build.toml`; rebuild kernel.

---

## Files changed

| Path | Change |
|------|--------|
| `rust_kernel/kolibri_utils/src/geometry.rs` | Cut CD logic + oracle + tests |
| `rust_kernel/kolibri_utils/src/ffi.rs` | `rust_blit_clip` export |
| `rust_kernel/kolibri_utils/src/lib.rs` | re-exports |
| `kernel/video/blitter.inc` | gate + trampoline + legacy body |
| `kernel/rust/blit_clip.inc` | blob embed + ABI smoke |
| `kernel/kernel32.inc` | include |
| `kernel/kernel.asm` | smoke call (gated) |
| `project/build.toml` | blob + migration entry |
| `docs/migration/cut-cd-plan.md` | plan |
| `docs/migration/cut-cd-implementation.md` | this report |
| `docs/migration/migration-todo.md` | inventory |
| `docs/migration/migration-plan.md` | Cut CD entry |

---

## Known limitations

* `blit_32` pixel hot path remains FASM.
* Legacy FASM `blit_clip` CF after `add esp,40` remains buggy when gate OFF; Rust path restores documented CF.

---

## Updated inventory

**85 / 135** (`85` enabled production migrations = Cut A four symbols + B–CD).
