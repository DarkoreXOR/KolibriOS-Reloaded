# Cut AU Implementation — `ipv4_find_fragment_slot`

**Date:** 2026-08-11  
**Status:** complete (audited)  
**Plan:** [`cut-au-plan.md`](cut-au-plan.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

---

## Target

| Field | Value |
|-------|-------|
| FASM symbol | `ipv4_find_fragment_slot` |
| Source | [`kernel/network/IPv4.inc`](../../kernel/network/IPv4.inc) |
| Callers | 2 live (`ipv4_input` middle fragment; `ipv4_input` last fragment) |
| Rust symbol | `rust_ipv4_find_fragment_slot` |
| Pure helper | `kolibri_utils::ipv4_find_fragment_slot` |
| Subsystem | IPv4 fragment reassembly slot lookup (Stage-5 foothold) |
| Migration kind | **Single-function cut** (Path B; Path A rejected) |

---

## Cluster audit summary

Path A was **rejected**. Post-AT audit: Y+AT+`rebase_coff` do not own the PE
loader; I+`createMcbEntry` encode ≠ FRS ownership (high write blast); socket
lifecycle Path A unchanged reject. Selected **`ipv4_find_fragment_slot`** —
strongest remaining leaf: Stage-5 IPv4 reassembly keyed table scan with clear
register ABI, excellent differential domain, and low blast. Preferred over
`createMcbEntry` (FRS blast) and `rebase_coff` (Y mutate anti-cluster).

REG-001: trampoline preserves **EAX+EBX+ECX+EDX** (legacy push/pop) plus
EDI/EBP canaries; `ipv4_input` keeps EDX→packet / EBX→device across the call.
ESI is the sole intentional return.

---

## Candidate comparison (post-AT audit)

| Candidate | Outcome |
|-----------|---------|
| `ipv4_find_fragment_slot` | **Selected** — Stage-5 fragment-slot scan |
| `createMcbEntry` | #2 — NTFS MCB encode; high FRS blast |
| `ahci_find_cmdslot` | #3 — driver free-slot scan |
| `rebase_coff` | Defer — Y anti-cluster / mutate |
| `memmove` / `blit_clip` / socket Path A / PE Path A | Defer / reject |

---

## Legacy ABI

FASM leaf retained under `USE_RUST_IPV4_FIND_FRAGMENT_SLOT=0`:

```text
call ipv4_find_fragment_slot
in:  EDX → IPv4_header
out: ESI = slot pointer | -1
preserves EAX/EBX/ECX/EDX via push/pop
plain ret (no stack args)
```

Quirk: empty (zeroed) slots match Identification=0 + Src/Dst=0 — TTL is not
checked (legacy TODO notes missing protocol match as well).

---

## Rust ABI

```text
stdcall rust_ipv4_find_fragment_slot(packet, fragments, count) → EAX
  ret 12
```

Trampoline: injects `IPv4_fragments` + `IPv4_MAX_FRAGMENTS`; `mov esi, eax`;
preserves EAX/EBX/ECX/EDX/EDI/EBP.

---

## Strategy A + C

| Stage | Detail |
|-------|--------|
| Freestanding | `ipv4_find_fragment_slot.rs` + `ffi.rs` section `.text.rust_ipv4_find_fragment_slot` |
| Extract | `extract_reloc_free_text.py` → `rust_ipv4_find_fragment_slot.bin` |
| Embed | `kernel/rust/ipv4_find_fragment_slot.inc` `file` directive |
| Trampoline | `IPv4.inc` under `USE_RUST_IPV4_FIND_FRAGMENT_SLOT` |
| Gate | `USE_RUST_IPV4_FIND_FRAGMENT_SLOT` (prod 1) |
| Smoke | `ipv4_find_fragment_slot_rust_smoke_test` (after `stack_init`) |

---

## Artifact

| Field | Value |
|-------|-------|
| Rust section | `.text.rust_ipv4_find_fragment_slot` |
| Blob/object size | 82 bytes |
| Relocations | 0 (extractor rejects any REL/RELA targeting the section) |
| SHA-256 | `A298BB7E5A293DF7DFCD4147023BE87ED78D898971CA649A6415ED96DE00786E` |

---

## Differential/oracle

| Suite | Result |
|-------|--------|
| Independent FASM-flow oracle vs `ipv4_find_fragment_slot_from_keys` | **PASS** |
| Named vectors | empty; miss; hit first/mid/last; first-match duplicates; partial-key miss; zeroed-slot quirk |
| Pointer-form synthetic table | **PASS** |
| PRNG | 50 000 vectors, seed `0x43555455` (`'CUTU'`) |
| Host tests | **449/449** cargo tests (incl. ipv4_find_fragment_slot suite) |

---

## ABI smoke

| Item | Result |
|------|--------|
| `ipv4_find_fragment_slot_rust_smoke_test` | **PASS** (boot reached desktop; no `0xDEAD0C55` hang) |
| Vectors | Direct `rust_*` hit mid + miss; public hit + miss; EAX/EBX/ECX/EDX/EDI/EBP canaries |
| Marker | `rust_ipv4_find_fragment_slot_smoke_result = 'FRAG'` on success |

---

## QEMU regression

| Config | Gate | Result | Notes |
|--------|------|--------|-------|
| OFF | `USE_RUST_IPV4_FIND_FRAGMENT_SLOT=0` | **OK** (QMP `running` + screendump, 779380 non-black) | FASM body |
| ON | `USE_RUST_IPV4_FIND_FRAGMENT_SLOT=1` | **OK** (QMP `running` + screendump, 779380 non-black) | Final production gate |

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

Forced fragmented IPv4 traffic (middle/last fragment against a populated
`IPv4_fragments` slot) was not scripted. Stock desktop traffic typically does
not exercise reassembly. Leaf validated by differential + ABI smoke on synthetic
headers/tables; desktop OFF/ON is integration regression. Complements Cuts
AC/M/V/AS without claiming reassembly ownership.

---

## Regressions

```text
NONE
```

(No live REG-NNN append for this cut.)

---

## Production gate

```text
USE_RUST_IPV4_FIND_FRAGMENT_SLOT = 1
```

Rollback: `USE_RUST_IPV4_FIND_FRAGMENT_SLOT = 0` (or `enabled = false` in
`project/build.toml` for cut AU).

Image: `dev_build/cut-au-final.img`

---

## Files changed

* `rust_kernel/kolibri_utils/src/ipv4_find_fragment_slot.rs` — pure walk + oracle tests
* `rust_kernel/kolibri_utils/src/ffi.rs` — `rust_ipv4_find_fragment_slot` section
* `rust_kernel/kolibri_utils/src/lib.rs` — module export
* `rust_kernel/kolibri_utils/out/rust_ipv4_find_fragment_slot.bin` — extracted blob
* `kernel/rust/ipv4_find_fragment_slot.inc` — embed + ABI smoke
* `kernel/network/IPv4.inc` — trampoline + gate + FASM rollback body
* `kernel/kernel32.inc` — include
* `kernel/kernel.asm` — smoke call after `stack_init`
* `project/build.toml` — blob + migration registry
* `docs/migration/cut-au-plan.md` / `cut-au-implementation.md`
* `docs/migration/migration-plan.md` / `boundaries.md`

---

## Known limitations

* No forced fragmented-IPv4 soak harness
* First-fragment free-slot fill / chain link / rebuild / TTL sweep remain FASM
* No Path A claim with AC/AS/M/V or full reassembly
* Empty-slot id=0/IP=0 false-hit quirk retained (legacy)
* Protocol number not matched (legacy TODO retained)
