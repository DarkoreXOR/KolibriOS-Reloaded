# Cut AX Plan

**Date:** 2026-08-11  
**Status:** complete — see [`cut-ax-implementation.md`](cut-ax-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).  
**Regression discipline:** [`regression-log.md`](regression-log.md), [`.cursor/rules/regression-log.mdc`](../../.cursor/rules/regression-log.mdc).

> **Nomenclature:** **Cut AX** migrates NTFS MCB VLE **encode** —
> `createMcbEntry` in `kernel/fs/ntfs.inc`.  
> Cuts A–AW remain complete and must not be redone. Do not start Cut AY.

---

## Post-AW migration audit (cluster readiness)

### Regression history (mandatory)

| ID | Class | Lesson applied here |
|----|-------|---------------------|
| REG-001 | Rust `stdcall` clobbers EDX across leaf calls | Preserve **EBX+EBP** (legacy untouched); EDI is inout; EAX/ECX/EDX/ESI legacy-clobbered; trampoline `cld` only when FRS slide path ran |
| REG-002 | FS empty-path / `bdfe.name` NUL | Not a volume-handler cut; still run `--disk ntfs` A/B |
| Cut D | `strncmp` EDX | N/A — no string helper |
| Cut I | MCB decode | Encode twin; do **not** claim NTFS write Path A |
| Cut AW | XFS AG→sector | Address-math foothold; do **not** pick another address-math sibling as default |

### Verdict: **Path B — no Path A cluster clears the raised bar**

| Question | Finding |
|----------|---------|
| AK+AM+AP+R+W+AW XFS Path A? | **No** — complementary leaves; FASM owns mount/AG/inode/dir/I/O/locks |
| AV + AHCI wait/IRQ/issue Path A? | **No** — controller/DMA/IRQ/recovery stay FASM |
| AC/M/V/AS/AU network Path A? | **No** — writers/mutex/alloc incomplete |
| Y + AT + `rebase_coff` Path A? | **No** — loader orchestration stays FASM |
| I + `createMcbEntry` Path A? | **No** — encode ≠ FRS/bitmap/space ownership |
| AQ + paging / X + AR? | **No** — unchanged rejects |
| Strongest remaining leaf? | **Yes** — `createMcbEntry` (MCB VLE encode; Cut I inverse) |

### Clusters / alternatives considered and rejected

| Cluster / candidate | Why not now |
|---------------------|-------------|
| XFS Path A (AK+AM+AP+R+W+AW) | Leaves ≠ ownership |
| `exFAT_get_sector` / `getInodeLocation` / `fat_get_sector` | Post-AW **address-math sibling ban**; EXT lacks `--disk ext` |
| `xfs._.get_last_dirblock` / inode packing extract | XFS fatigue after AW; thin R composition |
| AHCI wait / endian / sig | Impure / trivial |
| AU free-slot / socket_check_port | Anti-cluster / mutex |
| `rebase_coff` | Y mutate anti-cluster |
| `fat_date_to_bdfe` / `uni2ansi_char` | AO / AN ban-list |
| Ban-list FS/Unicode / H+blit / thin sysfn | Unchanged rejects |

### Ranked top candidates (post-AW)

| Candidate | Subsystem | Callers | New class | Diff | Blast | Soak | Verdict |
|-----------|-----------|--------:|-----------|------|-------|------|---------|
| `createMcbEntry` | NTFS write | 5 | **MCB VLE encode** (I inverse) | Hard (fixtures) | **High** | `--disk ntfs` write | **SELECT** |
| `getInodeLocation` | EXT | 2 | Inode→LBA math | Good | Low | **No EXT harness** | #2 (address-math + no soak) |
| `exFAT_get_sector` | exFAT | 3 | Cluster→LBA | Excellent | Low | `--disk exfat` | Reject (AW address-math sibling) |
| `xfs._.get_last_dirblock` | XFS | 2 | Dir last-offset | Good | Low | `--disk xfs` | Reject (XFS fatigue / thin over R) |
| `rebase_coff` | PE / DLL | 1 | DIR32 rebase | Good | Med | Rare | Defer (Y anti-cluster) |

```text
Next cut:
Kind: single-function cut (Path B)
Path: B
Target: createMcbEntry
Source: kernel/fs/ntfs.inc
Subsystem: NTFS MCB (data-run) VLE encode
Stage: Stage-5/FS foothold (encode leaf; NTFS write ownership stays FASM)
Why selected:
    Post-AW audit: Path A rejected everywhere. Address-math siblings
    (exFAT_get_sector / getInodeLocation) lose to the raised bar after AW.
    XFS next leaves deepen fatigue. createMcbEntry has been the permanent #2
    through AU–AW; safer low-blast leaves shipped. Completing Cut I's codec
    pair is the strongest remaining semantic class (VLE encode + optional FRS
    slide), with 5 write-path callers and real --disk ntfs coverage.
Why this is a genuine migration boundary:
    Deterministic width/header/byte packing from fileDataStart/Size; optional
    attribute/FRS extend with legacy sizeWithHeader-before-space-check quirk;
    EDI advances to terminator byte. Distinct from decode (I) without claiming
    bitmap/space/write orchestration.
Why Path A / Path B:
    Path B — one encode leaf. Allocation, FRS ownership, attr orchestration,
    and disk write remain FASM.
Regression risks:
    REG-001 on EBX across write paths; DF (cld only after FRS slide);
    sizeWithHeader +8 before FRS full early-out; EDI points AT terminator
    (not past); partial FRS mutate on no-space.
CPU/interrupt-state risks:
    DF flip on extend path only; no cli/sti; no locks in leaf.
Shared-state risks:
    Mutates caller attr header + FRS buffer; reads NTFS.fileData*.
Concurrency/locking risks:
    None in leaf (NTFS.Lock held by callers outside this leaf).
Required differential tests:
    Independent FASM-flow oracle; width zig-zag/shl quirks; no-extend encode;
    extend+slide; FRS-full early-out with sizeWithHeader quirk; roundtrip vs
    Cut I decode; 50k PRNG seed 0x43555458 ('CUTX').
Required ABI tests:
    Marker CMCE; fake NTFS+FRS+attr; EBX/EBP/EDI canaries; DF after extend;
    no 0xDEAD0C58 hang.
Required A/B tests:
    Gate OFF vs ON desktop; --disk ntfs boot+attach.
Required real subsystem validation:
    python scripts/run_qemu.py --disk ntfs (write/create paths reach
    createMcbEntry; browse soak at minimum proves mount+read unaffected).
Rejected alternatives:
    getInodeLocation; exFAT_get_sector; get_last_dirblock; rebase_coff;
    XFS/AHCI/network/PE Path A; AO/AN ban-list.
Expected legacy ABI:
    register call; in [ebp+NTFS.fileDataStart/Size], edi→dest, esi→attr;
    out edi advanced to terminator; may mutate attr+FRS; no uses;
    clobbers eax/ecx/edx/esi; ebx/ebp untouched; DF→0 iff FRS slide.
Expected Rust ABI:
    stdcall rust_ntfs_create_mcb_entry(start, size, dest, attr, frs, out_dest)
      → EAX bit0=need_cld; *out_dest=new edi; ret 24;
    trampoline extracts NTFS fields; cld when EAX bit0 set; preserves EBX.
Differential-testing strategy:
    Independent oracle mirroring FASM width/extend/slide/stos; 50k PRNG 'CUTX'.
ABI-risk assessment:
    High — FRS mutate + DF + REG-001 EBX; omit-FP trampoline with EBP→NTFS.
```

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin
omit-FP trampoline that materializes NTFS fields then calls Rust;
`USE_RUST_NTFS_CREATE_MCB_ENTRY` rollback; trampoline `cld` when slide ran.

---

## Out of scope

* Claiming Path A for NTFS / XFS / AHCI / PE / network
* Migrating `getInodeLocation` / `exFAT_get_sector` / `rebase_coff`
* Beginning Cut AY
