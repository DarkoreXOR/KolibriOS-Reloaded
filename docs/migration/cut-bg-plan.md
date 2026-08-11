# Cut BG Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-bg-implementation.md`](cut-bg-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BG** migrates endian word-byte swap —
> `swap_bytes_in_words` in `kernel/blkdev/ahci.inc`.  
> Cuts A–BF remain complete and must not be redone. Do not start Cut BH.

---

## Post-BF migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Derive preserve from **legacy FASM body**: leaf **restores EAX/EBX/ECX** via push/pop; **EDX/ESI/EDI/EBP** untouched — trampoline must preserve all of them (Rust would clobber EAX/ECX/EDX) |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — endian buffer util (caller owns string NUL) |
| REG-003 | ABI smoke mutates live globals | Smoke uses **iglobal synthetic word buffer only** — never touches live `modelstr` / AHCI port state |
| Cut BF | string padded copy | Complete; this leaf is **endian word-swap**, not string deepen |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**61 / 135** (`61` enabled `[[rust.migrations]]` = Cut A four symbols + B–BF).
No gate drift. `strtoint_dec` still dead (`conf_lib.inc` not linked).
`strncat` remains export-only (special case; no count inflation).
`strchr` / `strnlen` remain export-only (zero in-kernel callers).

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY/BD network Path A? | **No** |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch |
| AV+this leaf AHCI Path A? | **No** — endian util ≠ controller ownership |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+BF string Path A? | **No** |
| L+BE+`set_mouse_data` HID Path A? | **No** — aggregator ≠ ownership |
| P+AZ+`is_string_userspace` Stage-3 Path A? | **No** — thin sibling |
| Strongest remaining **live** leaf? | **Yes** — `swap_bytes_in_words` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `set_mouse_data` | Post-BF #2: PE-export HID aggregator; strong mouse soak; **side-effect heavy** (`BTN_DOWN`/`MOUSE_*`/`wakeup_osloop` + display dims) — HID deepen after L+BE |
| `strlen` | Clean length leaf; **EXT-only callers**; no `--disk ext` |
| `iso9660_copy_name` | Real `--disk iso9660` soak; AJ glue + calls ban-listed `uni2ansi_char` + REG-002 adjacency |
| `is_string_userspace` | Thin P sibling (explicitly ranked below) |
| `ahci_is_sig_known` | Trivial 4-way CMP — AV sibling deepen, thinner than this leaf |
| `v86_get_lin_addr` | Stage-4 address math; BIOS/V86 soak weak |
| `coff_get_align` / `get_proc_ex` | PE thin / ban stretch |
| `strchr` / `strnlen` / `strncat` | Export-only — no kernel callers |
| `tcp_mss` / `tcp_output` | TCP deepen after BD (`tcp_mss` is a 1420 clamp) |
| `fsGetTime` | CMOS I/O + calendar cluster caution; time-dependent oracle |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BF)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `swap_bytes_in_words` | core / AHCI util | 1 (IDENTIFY model) | **Endian word-swap** | Excellent | Low | `--bus ahci` | **SELECT** |
| `set_mouse_data` | HID / mouse | PE export live | HID aggregator | Hard | Med–High | Desktop mouse | #2 deepen |
| `strlen` | core/string | 2 (EXT) | Length/`scasb` | Excellent | Low | No `--disk ext` | #3 soak |
| `iso9660_copy_name` | ISO9660 | 1 | Encoding dispatch | Good | Med | `--disk iso9660` | #4 glue+ban |
| `ahci_is_sig_known` | AHCI | 2 | Signature CMP | Excellent | Low | `--bus ahci` | #5 trivial |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: swap_bytes_in_words
Source: kernel/blkdev/ahci.inc
Subsystem: endian word-byte swap (AHCI ATA IDENTIFY model string)
Stage: Stage-2 / util leaf (new semantic vs AV free-slot scan)
Why selected:
    Post-BF audit: Path A rejected everywhere. HID deepen / thin P sibling /
    EXT-only strlen / ISO glue+ban / trivial AHCI sig CMP stay weaker.
    Strongest remaining live leaf is swap_bytes_in_words — first endian
    word-swap class, live AHCI IDENTIFY caller, excellent independent
    oracle, real --bus ahci soak (model-string path after identify).
Why this is a genuine migration boundary:
    Deterministic (base,len) → in-place word endian swap matching FASM
    xchg ah,al loop. Complements AV without claiming AHCI Path A.
Why Path A / Path B:
    Path B — one endian util leaf. AHCI controller / identify orchestration
    stay FASM.
Regression risks:
    REG-001: preserve EAX/EBX/ECX/EDX/ESI/EDI/EBP (legacy restores
    EAX/EBX/ECX; leaves EDX+ESI/EDI/EBP alone).
    REG-003: synthetic word buffer only; never mutate live modelstr.
    Edge: len=0; odd/even lens; overlapping high/low bytes.
CPU/interrupt-state risks:
    None in leaf — pure memory rewrite of len words.
Shared-state risks:
    Writes only caller-provided base[0..len); no globals.
Concurrency/locking risks:
    None in leaf (caller owns identify buffer).
Required differential tests:
    Independent FASM-flow oracle; len=0 / single / ATA model 20;
    50k PRNG seed 0x43554247 ('CUBG').
Required ABI tests:
    Marker SWBW; synthetic buffer; EAX/EBX/ECX/EDX/ESI/EDI/EBP canaries.
Required A/B tests:
    Gate OFF vs ON with --bus ahci; same non-black ± clock noise;
    prior cut-bf-final.img.
Required real subsystem validation:
    --bus ahci IDENTIFY path that calls swap_bytes_in_words on modelstr
    — report honestly if only boot/desktop without AHCI disk present.
Rejected alternatives:
    set_mouse_data (HID deepen); strlen; iso9660_copy_name;
    is_string_userspace; ahci_is_sig_known; Path A; ban-list.
Expected legacy ABI:
    stdcall swap_bytes_in_words(base,len); ret 8;
    preserves EAX/EBX/ECX (push/pop) + EDX/ESI/EDI/EBP (untouched);
    clobbers flags; no DF change; no meaningful return (EAX restored).
Expected Rust ABI:
    stdcall rust_swap_bytes_in_words(base,len); ret 8;
    trampoline preserves EAX/EBX/ECX/EDX (+ ESI/EDI/EBP canaries).
Differential-testing strategy:
    Independent oracle mirroring FASM ecx loop + xchg ah,al; 50k PRNG.
ABI-risk assessment:
    Low — buffer util; REG-001 full preserve; REG-003 synthetic only.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
stdcall trampoline with full register preserve; `USE_RUST_SWAP_BYTES_IN_WORDS`
rollback.

---

## Out of scope

* Claiming Path A for AHCI or endian ownership
* Migrating `ahci_is_sig_known` / `ahci_port_wait`
* Migrating `set_mouse_data` / `strlen` / `iso9660_copy_name`
* Beginning Cut BH
