# Cut AB Implementation — `utf8to16`

**Date:** 2026-08-10  
**Status:** complete (audited)  
**Plan:** [`cut-ab-plan.md`](cut-ab-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `utf8to16` |
| Source | [`kernel/fs/parse_fn.inc`](../../kernel/fs/parse_fn.inc) |
| Callers | 16 live (taskman, fat×4, font, ntfs×4, iso9660×2, fs_lfn×2, exfat×2) |
| Rust symbol | `rust_utf8to16` |
| Pure helper | `kolibri_utils::utf8to16` |
| Subsystem | FS/GUI path & string decode |

---

## Candidate comparison (post-AA audit)

| Candidate | Outcome |
|-----------|---------|
| `utf8to16` | **Selected** — ESI-advancing UTF-8→UTF-16 stream (Q inverse; Cut-A leftover) |
| `ntfs_test_bootsec` | Deferred #2 — FS bootsec validate+CF; prefer streaming ABI novelty |
| `ipv4_route` | Deferred #3 — Stage-5 net routing foothold after AB |
| `memmove` | Deferred Stage-4 — forward-only; 24-caller fanout |
| `get_pg_addr` / `v86_get_lin_addr` | Deferred — Stage-4 memory class; high blast |
| `uni2ansi_char` | Deferred — same `parse_fn` family; scalar map behind streaming |

---

## Legacy ABI

FASM leaf in `parse_fn.inc` (retained under `USE_RUST_UTF8TO16=0`):

```text
call / ret
in:  ESI → UTF-8 byte stream (advances)
out: AX = UTF-16 code unit (EAX high bits algorithm-dependent)
preserves: EBX, ECX, EDX, EDI, EBP (untouched)
clobbers: EAX, ESI, flags
quirks: invalid-lead restart; mid-stream ASCII → .got (xor ah,ah);
        continuation gather; 2-byte vs 3-byte via shl ax,3 CF;
        incoming EAX high bits affect 3-byte shl eax,3 path
```

Flags are unspecified to callers (clobbered).

---

## Rust ABI

```text
stdcall rust_utf8to16(esi_inout, initial_eax) -> EAX
ret 8
```

Trampoline passes `&ESI` stack slot + live `EAX`; restores ESI from slot; preserves EBX/ECX/EDX/EDI/EBP.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `utf8to16.rs` + `ffi.rs` section `.text.rust_utf8to16` |
| Extract | `extract_reloc_free_text.py` → `rust_utf8to16.bin` |
| Embed | `kernel/rust/utf8to16.inc` `file` directive |
| Trampoline | `parse_fn.inc` under `USE_RUST_UTF8TO16` |
| Gate | `USE_RUST_UTF8TO16` (dev 0 → prod 1) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_utf8to16` |
| Blob/object size | 215 bytes |
| Relocations | 0 |
| SHA-256 | `555230D311B977C925EE4E80AE4E840F70D9EA7B1963E98F03ECCCC6CC6EB175` |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow bit oracle vs Rust | **PASS** |
| Named vectors | ASCII; U+0080/U+00E9/U+07FF; U+0800/U+20AC/U+FFFF; invalid-lead restart; mid-ASCII abort; chained `initial_eax` |
| Boundary | high `initial_eax` with `.got`; overlong quirky leads; NUL after restart |
| Exhaustive | all single-byte; all valid 2-byte leads C2..DF×80..BF |
| PRNG | 50 000 vectors, seed `0x43555442` (`'CUTB'`) |
| Host tests | **284/284** cargo tests |

---

## ABI smoke

| Item | Result |
|------|--------|
| `utf8to16_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0CAB` hang) |
| Vectors | ASCII; U+00E9; U+0800; restart `80'Z'`; public euro U+20AC; chained C3 A9→`!`; mid-abort C3`X`; EBX/ECX/EDX/EDI/EBP preserve |
| Marker | `rust_utf8to16_smoke_result = 'U8T6'` on success |

---

## QEMU validation

Kernels built with Cuts A–AA production gates intact (`USE_RUST_PID_TO_SLOT=1`, etc.).

Images: CoW from `cut-aa-final.img`, replace `KERNEL.MNT`.

| Gate | Setting | Desktop | Network |
|------|---------|---------|---------|
| OFF | `USE_RUST_UTF8TO16=0` | **OK** (QMP `running` + screendump `tmp_images/cut-ab-off.ppm`, 779380 non-black samples) | **OK** (e1000 + user net) |
| ON | `USE_RUST_UTF8TO16=1` | **OK** (screendump `tmp_images/cut-ab-on.ppm`, 779380 non-black samples) | **OK** |

Smoke (ON): **PASS** (no `0xDEAD0CAB`; boot continued to desktop).

Real subsystem soak: **PASS** — boot smoke exercises public trampoline with multi-byte/restart vectors; font UTF-8 draw path (`font.inc` `.drawUTF8`) remains linked on stock desktop.

Production image: `tmp_images/cut-ab-final.img`.

---

## Production gate

```text
USE_RUST_UTF8TO16 = 1
```

Rollback: `USE_RUST_UTF8TO16 = 0` (or `enabled = false` in `tools/build/config.toml`).

---

## Files changed

* `rust_kernel/kolibri_utils/src/utf8to16.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `rust_kernel/kolibri_utils/build-utf8to16.ps1` (new)
* `rust_kernel/kolibri_utils/out/rust_utf8to16.bin` (generated)
* `kernel/rust/utf8to16.inc` (new)
* `kernel/fs/parse_fn.inc` (trampoline + gate)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call)
* `tools/build/config.toml`
* `tools/build/README.md`
* `docs/migration/cut-ab-plan.md`
* `docs/migration/cut-ab-implementation.md`
* `docs/migration/migration-plan.md`

---

## Known limitations

* Not the same algorithm as Cut A `unicode.utf8.decode` (no length bound, no U+FFFD; restart/quirky invalid behavior retained).
* Does not emit UTF-16 surrogate pairs for supplementary-plane UTF-8 (legacy leaf BMP-oriented bit packing).
* Incoming `EAX` high bits are preserved through the trampoline (FASM fidelity for chained calls).
* Flags after return are unspecified.
* `memmove` / Stage-4 VA→PA / `ipv4_route` / `ntfs_test_bootsec` remain deferred.
