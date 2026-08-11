# Cut AZ Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-az-implementation.md`](cut-az-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AZ** migrates syscall-70/80 buffer-size → userspace
> region gate — `file_system_is_operation_safe` in `kernel/fs/fs_lfn.inc`.  
> Cuts A–AY remain complete and must not be redone. Do not start Cut BA.

---

## Post-AY migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX/ECX | Preserve **EAX+EBX+ECX+EDX**; reconstruct **ZF** after Rust (`cmp eax,1`); flag-neutral pops |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; still soak `--disk` browse |
| REG-003 | ABI smoke mutates live globals | Smoke uses **stack/uglobal synthetic** `inf` structs only — never touch live FS mounts |
| Cut P | ZF-out userspace gate | Compose via **inlined** pure `is_region_userspace` (no cross-section call) |
| Cut AY | Network device index | Complete; do **not** deepen AS/AU/AY network cluster |

### Verdict: **Path B — no Path A cluster clears the raised bar**

**Path A: REJECTED**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** — complementary leaves; FASM owns mount/AG/inode/dir/I/O |
| I+J+AE–AG+AX NTFS Path A? | **No** — encode/decode/time ≠ FRS/bitmap/space ownership |
| AC/M/V/AS/AU/AY network Path A? | **No** — index/lookup ≠ stack/mutex/alloc ownership |
| Y+AT+`get_proc_ex`/`rebase_coff` PE Path A? | **No** — loader orchestration stays FASM |
| AV AHCI Path A? | **No** — controller/DMA/IRQ stay FASM |
| AQ / X+AR Path A? | **No** — unchanged rejects |
| P + this leaf as Path A? | **No** — one syscall-70 size→gate leaf ≠ syscall façade / FS ownership |
| Strongest remaining leaf? | **Yes** — `file_system_is_operation_safe` |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| XFS / NTFS / network / AHCI / PE Path A | Leaves ≠ ownership |
| `get_proc_ex` | PE ban stretch after Y+AT (AY #2 deferred again) |
| `tcp_outflags` | Mild M/V TCP deepen; stock net soak partial |
| `bdfe_to_fat_*` / `fat_date_to_bdfe` | AO calendar ban |
| `uni2ansi_char` | AN inverse ban |
| Address-math siblings | AW ban |
| `socket_check_port` / `socket_num_to_ptr` | Mutex + AS/AY anti-cluster |
| `rebase_coff` / `usb_td_to_virt` | Y mutate / AQ compose + weak USB |
| `fat_name_is_legal` | Charset table; mild FAT deepen |
| `is_string_userspace` | Thin P sibling |
| Ban-list thin (`strnlen`, `coff_get_align`, …) | Unchanged rejects |

### Ranked top candidates (post-AY)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `file_system_is_operation_safe` | Stage-3 / sysfn70 | 2 | **Buffer-size→userspace ZF gate** | Excellent | Med (all FS syscalls) | `--disk` + desktop | **SELECT** |
| `get_proc_ex` | PE / DLL | 1 | Import name→VA | Excellent | Med | `.sys` load | #2 (PE ban) |
| `tcp_outflags` | TCP | 1 | State→flags table | Excellent | Low | Partial net | #3 |
| `fat_name_is_legal` | FAT | 1 | Charset validate | Good | Low | `--disk exfat` | #4 |
| `is_string_userspace` | Stage-3 | 1 | String region gate | Good | Low | path syscalls | #5 thin |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: file_system_is_operation_safe
Source: kernel/fs/fs_lfn.inc
Subsystem: Syscall-70/80 FS operation safety gate (Stage-3)
Stage: Stage-3 foothold (size math + userspace compose; FS plugins stay FASM)
Why selected:
    Post-AY audit: Path A rejected everywhere. Network deepen after AY is
    weak; PE get_proc_ex stays ban-stretched; calendar/USB/address-math stay
    ban-listed. Strongest remaining leaf is file_system_is_operation_safe:
    new Stage-3 semantic class (sysfn70 subfn → buffer-byte length → ZF
    userspace gate), composes Cut P via inlined pure helper (AE/T pattern),
    reloc-free, and exercises every filesystem path through sysfn70/80.
Why this is a genuine migration boundary:
    Deterministic length switch (subfn 0/1/2–3/5/6 + unknown→accept quirk)
    then Cut P region check. Distinct from P alone (no size math) without
    claiming FS plugin or syscall-table ownership.
Why Path A / Path B:
    Path B — one security-gate leaf. LFN/Unicode FS dispatch, mount, and
    disk I/O remain FASM. Composing P is helper reuse inside one blob, not
    Rust-owned subsystem.
Regression risks:
    REG-001: EAX/EBX/ECX/EDX preserve + ZF reconstruction (Cut P class).
    REG-003: smoke must use synthetic inf structs only.
    Unknown subfn → ZF=1 without region check (legacy quirk — retain).
CPU/interrupt-state risks:
    None in leaf — no cli/sti; pure compute + compose.
Shared-state risks:
    Read-only of caller-provided inf struct; no globals.
Concurrency/locking risks:
    None.
Required differential tests:
    Independent FASM-flow oracle; all subfns; encoding ≤1 vs >1; count=0;
    overflow length; unknown subfn accept; P quirk vectors; 50k PRNG
    seed 0x4355545A ('CUTZ').
Required ABI tests:
    Marker FSOS; synthetic inf; EAX/EBX/ECX/EDX canaries; ZF polarity;
    no live FS global mutation.
Required A/B tests:
    Gate OFF vs ON desktop; --disk exfat and/or xfs/ntfs browse.
Required real subsystem validation:
    sysfn70/80 on attached disks (Eolite browse).
Rejected alternatives:
    get_proc_ex; tcp_outflags; fat_name_is_legal; is_string_userspace;
    Path A clusters; AO/AN/address-math/socket/PE/USB ban-list.
Expected legacy ABI:
    stdcall(inf_struct_ptr); ZF=1 safe / ZF=0 unsafe;
    preserves EAX/EBX/ECX/EDX; ret 4.
Expected Rust ABI:
    stdcall rust_file_system_is_operation_safe(inf) → EAX∈{0,1}; ret 4;
    trampoline cmp eax,1 + restore EAX/EBX/ECX/EDX (flag-neutral pops).
Differential-testing strategy:
    Independent oracle mirroring FASM switch + inlined P oracle; 50k 'CUTZ'.
ABI-risk assessment:
    High — all FS syscalls; ZF + REG-001; retain unknown-subfn accept quirk.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
stdcall→stdcall trampoline with **EAX+EBX+ECX+EDX** preserve and ZF
reconstruction; `USE_RUST_FILE_SYSTEM_IS_OPERATION_SAFE` rollback.

---

## Out of scope

* Claiming Path A for Stage-3 syscall façade / FS plugins
* Migrating `is_string_userspace` / `get_proc_ex` / `tcp_outflags`
* Changing the unknown-subfn → accept quirk
* Beginning Cut BA
