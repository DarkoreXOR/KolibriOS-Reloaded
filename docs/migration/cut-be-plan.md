# Cut BE Plan

**Date:** 2026-08-12  
**Status:** complete — see [`cut-be-implementation.md`](cut-be-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut BE** migrates HID hotkey-match —
> `hotkey_do_test` in `kernel/hid/keyboard.inc`.  
> Cuts A–BD remain complete and must not be redone. Do not start Cut BF.

---

## Post-BD migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | **EAX** is live hotkey list node (must preserve); **CF is OUT**; preserve **EBX/ECX/EDX** across trampoline |
| REG-002 | FS empty-path / `bdfe.name` NUL | N/A — HID leaf |
| REG-003 | ABI smoke mutates live globals | Smoke uses **stack synthetic hotkey node** + **save/restore `kb_state`** — never wipe live hotkey lists |
| Cut L | HID mouse | Complete; this leaf is **hotkey predicate match**, not mouse deepen |

### Inventory baseline

[`migration-todo.md`](migration-todo.md) verified against `project/build.toml`:
**59 / 135** (`59` enabled `[[rust.migrations]]` = Cut A four symbols + B–BD).
Minor comment drift only (`build.toml` migrations header still said A–BC; header A–BD).
Optional: `strncat` is live/exported but absent from inventory — same export-only class as
`strchr`; add as thin/export row when updating inventory for BE (no count inflation).

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** |
| I+J+AE–AG+AX NTFS Path A? | **No** |
| AC/M/V/AS/AU/AY/BD network Path A? | **No** — leaves ≠ protocol ownership |
| Y+AT+`get_proc_ex` PE Path A? | **No** — PE ban stretch |
| AV AHCI Path A? | **No** |
| U+K+AO+BC FAT Path A? | **No** |
| D+BB+string leaves as Path A? | **No** |
| P+AZ+`is_string_userspace` Stage-3 Path A? | **No** — thin sibling ≠ façade ownership |
| L+`hotkey_do_test` HID Path A? | **No** — match leaf ≠ input ownership |
| Strongest remaining **live** leaf? | **Yes** — `hotkey_do_test` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| Path A clusters above | Leaves ≠ ownership |
| `strchr` / `strnlen` / `strncat` | Export-only — no kernel callers |
| `strlen` / `strncpy` | String deepen after BB; EXT-only / shmem-only soak |
| `is_string_userspace` | Thin P sibling (repeatedly ranked #2/#5) |
| `v86_get_lin_addr` | Stage-4 address math; BIOS/V86 soak weak |
| `swap_bytes_in_words` | AV trivial deepen |
| `coff_get_align` / `get_proc_ex` | PE thin / ban stretch |
| `iso9660_copy_name` | AJ glue + `uni2ansi` ban + REG-002 |
| `ext_*` / `fsGetTime` | No `--disk ext`; calendar/CMOS caution |
| `tcp_mss` / `tcp_output` | TCP deepen after BD |
| AO/AN/address-math/socket ban-list | Unchanged |

### Ranked top candidates (post-BD)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `hotkey_do_test` | HID / hotkey | 3 (one loop) | **Hotkey predicate match** | Excellent | Med | Desktop kbd PARTIAL | **SELECT** |
| `is_string_userspace` | Stage-3 | 1 | String NUL-scan gate | Good | Low | `load_library` | #2 thin |
| `v86_get_lin_addr` | Stage-4 / V86 | 14 | PTE→linear | Excellent | Low | BIOS/V86 weak | #3 address-math |
| `swap_bytes_in_words` | AHCI util | 1 | Endian word-swap | Excellent | Low | `--bus ahci` | #4 AV deepen |
| `coff_get_align` | PE / DLL | 2 | Align-mask decode | Excellent | Low | `.sys` load | #5 thin PE |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: hotkey_do_test
Source: kernel/hid/keyboard.inc
Subsystem: HID hotkey field match (scancode → system hotkey list)
Stage: Stage-2 / HID leaf (Cut L sibling class, new semantic)
Why selected:
    Post-BD audit: Path A rejected everywhere. Thin P sibling / Stage-4
    address math / PE leftovers / AV deepen stay weaker. Strongest remaining
    live leaf is hotkey_do_test — new HID semantic (kb_state × nibble
    predicate table), distinct from Cut L mouse acceleration. Prior
    “reloc-hostile” reject mitigated by inlining hotkey_test0..4 (no
    PC-relative call table in blob); kb_state injected by trampoline.
Why this is a genuine migration boundary:
    Deterministic (funcs dword, kb_state, CL) → CF. Reloc-free via inlined
    predicates. Complements Cut L without claiming HID ownership.
Why Path A / Path B:
    Path B — one match leaf. Hotkey list / scancode dispatch stay FASM.
Regression risks:
    REG-001: preserve EAX (list node); CF polarity for jc/jnc callers;
    preserve EBX/ECX/EDX.
    Reloc: must NOT leave hotkey_tests as PC-relative table — inline 0..4.
    REG-003: save/restore kb_state; synthetic node only (never mutate
    hotkey_list / hotkey_scancodes).
CPU/interrupt-state risks:
    None in leaf — pure compute; caller owns IRQ/keyboard path.
Shared-state risks:
    Read-only kb_state via trampoline inject; no list mutation.
Concurrency/locking risks:
    None in leaf.
Required differential tests:
    Independent FASM-flow oracle (5 inlined predicates); all CL∈{0,2,4};
    all test ids 0..4 + ≥5 fail; 50k PRNG seed 0x43554245 ('CUBE').
Required ABI tests:
    Marker HKDT; synthetic node; EAX/EBX/ECX/EDX/ESI/EDI canaries; CF
    polarity; kb_state save/restore.
Required A/B tests:
    Gate OFF vs ON desktop; same non-black ± clock noise; prior cut-bd-final.img.
Required real subsystem validation:
    Desktop keyboard path always runs; full hotkey matrix may be PARTIAL —
    report honestly.
Rejected alternatives:
    is_string_userspace (thin); v86_get_lin_addr; swap_bytes; coff_get_align;
    string deepen; Path A clusters; ban-list.
Expected legacy ABI:
    call with EAX→hotkey node, CL∈{0,2,4}; CF clear=pass / set=fail;
    preserves EAX/EBX/ESI/EDI/EBP; clobbers EDX and CL (legacy doubles CL);
    plain ret.
Expected Rust ABI:
    stdcall rust_hotkey_do_test(funcs, kb_state, cl) → EAX=0 pass / ≠0 fail;
    ret 12; trampoline injects [eax+4] and [kb_state], restores EAX, sets CF.
Differential-testing strategy:
    Independent oracle mirroring FASM shr/nibble/predicate; 50k PRNG.
ABI-risk assessment:
    Med — keyboard hotkey loop; REG-001 EAX + CF; REG-003 kb_state.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
register→stdcall trampoline with **EAX** preserve, **kb_state** inject,
**CF** out via `clc`/`stc`; `USE_RUST_HOTKEY_DO_TEST` rollback.

---

## Out of scope

* Claiming Path A for HID / hotkey dispatch
* Migrating `set_keyboard_data` / hotkey list management
* Migrating `is_string_userspace` / string leaves
* Beginning Cut BF
