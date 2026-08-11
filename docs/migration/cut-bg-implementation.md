# Cut BG Implementation — `swap_bytes_in_words`

**Date:** 2026-08-12  
**Status:** complete (audited)  
**Plan:** [`cut-bg-plan.md`](cut-bg-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `swap_bytes_in_words` |
| Source | [`kernel/blkdev/ahci.inc`](../../kernel/blkdev/ahci.inc) |
| Callers | 1 kernel (`ahci_port_identify` model-string path) |
| Rust symbol | `rust_swap_bytes_in_words` |
| Pure helper | `kolibri_utils::swap_bytes_in_words` |
| Subsystem | endian word-byte swap (AHCI ATA IDENTIFY model string) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

**Path A: REJECTED.** Post-BF audit: XFS/NTFS/network/AHCI/PE/FAT/Stage-3/HID
Path A still fail the raised bar; AO/AN/address-math/socket/USB leftovers stay
ban-listed. Selected **`swap_bytes_in_words`** — first **endian word-swap**
class; live AHCI IDENTIFY caller; excellent independent oracle; real
`--bus ahci` soak (with optional `--disk` for IDENTIFY).

REG-001: trampoline preserves **EAX/EBX/ECX/EDX/ESI/EDI/EBP** — legacy restores
EAX/EBX/ECX via push/pop and leaves EDX+ESI/EDI/EBP untouched; Rust stdcall
would clobber EAX/ECX/EDX.

REG-003: ABI smoke uses **iglobal synthetic word buffer only** — never mutates
live `modelstr` / AHCI port state.

---

## Candidate comparison (post-BF audit)

| Candidate | Outcome |
|-----------|---------|
| `swap_bytes_in_words` | **Selected** — endian word-swap |
| `set_mouse_data` | #2 — HID deepen after L+BE; side-effect heavy |
| `strlen` | #3 — EXT-only; no `--disk ext` |
| `iso9660_copy_name` | #4 — AJ glue + `uni2ansi` ban |
| `ahci_is_sig_known` | #5 — trivial 4-way CMP |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_SWAP_BYTES_IN_WORDS=0`:

```text
stdcall swap_bytes_in_words(base, len)
in:  base → u16 words; len = word count
out: (none meaningful — EAX restored)
preserves: EAX, EBX, ECX (push/pop); EDX, ESI, EDI, EBP (untouched)
clobbers: flags
DF: unchanged (no cld/std)
ret 8
```

Quirk: `len` is a **word** count; indexing is `ecx*2`.

---

## Rust ABI

```text
stdcall rust_swap_bytes_in_words(base, len)
  ret 8
```

Trampoline: `push eax ebx ecx edx esi edi ebp` → `stdcall rust_*` →
`pop` restore (no meaningful return; EAX restored like legacy).

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `swap_bytes_in_words.rs` + `ffi.rs` section `.text.rust_swap_bytes_in_words` |
| Extract | `extract_reloc_free_text.py` → `rust_swap_bytes_in_words.bin` |
| Embed | `kernel/rust/swap_bytes_in_words.inc` `file` directive |
| Trampoline | `ahci.inc` under `USE_RUST_SWAP_BYTES_IN_WORDS` |
| Gate | `USE_RUST_SWAP_BYTES_IN_WORDS` (prod 1) |
| Smoke | `swap_bytes_in_words_rust_smoke_test` (after `ahci_init`) |

---

## Blob

| Field | Value |
|-------|-------|
| Section | `.text.rust_swap_bytes_in_words` |
| Size | **111 bytes** |
| Relocations | **0** |
| SHA-256 | `8ADE714E5DE69E9224AD0CA9D3C4D7B186CCF81BD32A108A295378A3A26B2924` |
| Epilogue | `ret 8` (`c2 08 00`) |

---

## Differential

| Item | Result |
|------|--------|
| Host `cargo test` | **PASS** (525 tests; Cut BG suite included) |
| Independent oracle | FASM-flow `xchg ah,al` loop (not derived from Rust body) |
| Coverage | len=0; single word; ATA model 20 words; double-swap identity; **50k PRNG** seed `0x43554247` (`'CUBG'`) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `swap_bytes_in_words_rust_smoke_test` | **PASS** (boot reached desktop; no `DEAD` hang) |
| Vectors | rust_* two-word / len=0 / single; public 20-word ATA-sized + full EAX..EBP canaries + sentinel |
| Marker | `rust_swap_bytes_in_words_smoke_result = 'SWBW'` on success |
| Live state | Synthetic `swap_bytes_smoke_buf` only (REG-003) |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_SWAP_BYTES_IN_WORDS=0` | **OK** (QMP `running` + screendump, 7358 non-black) | FASM body; `--bus ahci` |
| ON | `USE_RUST_SWAP_BYTES_IN_WORDS=1` | **OK** (QMP `running` + screendump, 7358 non-black) | Final production gate; `--bus ahci` |
| ON + disk | `=1` | **OK** (QMP `running` + screendump, 7358 non-black) | `--bus ahci --disk xfs` |

---

## A/B validation

| Check | Result |
|-------|--------|
| OFF vs ON screendump | **Identical** (byte-for-byte PPM match; 7358 non-black) |
| `--bus ahci` | **PASS** both OFF and ON |

---

## Real subsystem soak

| Path | Result |
|------|--------|
| `--bus ahci` boot (AHCI init + smoke; PCI config path) | **PASS** |
| `--bus ahci --disk xfs` (SATA attach → IDENTIFY → `swap_bytes_in_words` on `modelstr` when port succeeds) | **PASS** (desktop reached; AHCI+XFS disk attached) |
| Interactive Eolite browse of model string / full AHCI I/O matrix | **PARTIAL** — desktop smoke confirms boot; model-string DEBUGF not captured in QMP |

---

## Regressions

| Item | Result |
|------|--------|
| Regressions discovered | **none** |
| Regression log entry | N/A (no live regression) |

---

## Production / packaging

| Field | Value |
|-------|-------|
| Production gate | `USE_RUST_SWAP_BYTES_IN_WORDS = 1` (`project/build.toml` `enabled = true`) |
| Image | `dev_build/cut-bg-final.img` |
| Rollback | `USE_RUST_SWAP_BYTES_IN_WORDS = 0` or `[[rust.migrations]]` `cut = "BG"` `enabled = false` |

---

## Files changed

* `rust_kernel/kolibri_utils/src/swap_bytes_in_words.rs` (new)
* `rust_kernel/kolibri_utils/src/lib.rs`
* `rust_kernel/kolibri_utils/src/ffi.rs`
* `kernel/blkdev/ahci.inc` (gate + trampoline)
* `kernel/rust/swap_bytes_in_words.inc` (new)
* `kernel/kernel32.inc` (include)
* `kernel/kernel.asm` (smoke call after `ahci_init`)
* `project/build.toml` (blob + migration)
* `docs/migration/cut-bg-plan.md`
* `docs/migration/cut-bg-implementation.md`
* `docs/migration/migration-plan.md`
* `docs/migration/boundaries.md`
* `docs/migration/migration-todo.md`

---

## Known limitations

* Leaf does not own AHCI IDENTIFY orchestration — only the endian swap of the
  caller-provided word buffer.
* DF is left as-found (matches legacy; no `cld`).
* Interactive verification of the printed ATA model string is not part of the
  automated QMP harness.
