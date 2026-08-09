# Cut R Plan

**Date:** 2026-08-09  
**Status:** complete — see [`cut-r-implementation.md`](cut-r-implementation.md)  
**Evidence policy:** [`../_meta/evidence-policy.md`](../_meta/evidence-policy.md).

> **Nomenclature:** **Cut R** is the first migration of an **omit-frame-pointer stdcall leaf with EBP-as-object** — `xfs._.extent_unpack`, which unpacks a big-endian XFS `xfs_bmbt_rec` into `EBP+XFS.extent` while **EBP must remain the XFS partition pointer**.  
> Cuts A–Q remain complete and must not be redone.

---

## Selected target

| Field | Value |
|-------|-------|
| **Function** | `xfs._.extent_unpack` |
| **Source** | [`kernel/fs/xfs.asm:1466–1533`](../../kernel/fs/xfs.asm) |
| **Subsystem** | Filesystem / XFS extent map |
| **Purpose** | Unpack one 16-byte BE `xfs_bmbt_rec` into `XFS.extent` (`xfs_bmbt_irec`) |

---

## Candidate comparison

| Candidate | Outcome |
|-----------|---------|
| `xfs._.extent_unpack` | **Selected** — stdcall + omit-FP + EBP-as-object + MOVBE BE bitfields |
| `window._.check_window_position` | Deferred — best live desktop path; less ABI novelty |
| `fsTime2bdfe` | Deferred — EDI+=8 follow-on to G; calendar-family overlap |
| `blit_clip` | Deferred — CF composition after H |
| `memmove` | Deferred — Stage-4 memory class; ~24 callers |

---

## Why Cut R is a meaningful next step

Cuts A–Q proved utils / FS / video / HID / TCP / GUI / process header / ZF / SF streaming.

Cut R answers:

> Does Strategy A + C remain viable for an **omit-frame-pointer stdcall leaf** whose callers keep **EBP → XFS partition object**, with MOVBE big-endian bitfield unpack into `XFS.extent`, as a reloc-free blob?

---

## Strategy

**A + C:** freestanding Rust → reloc-free extract → FASM `file` embed; thin omit-FP trampoline passes `(extent_data, &XFS.extent)` derived from stack arg + EBP; `USE_RUST_XFS_EXTENT_UNPACK` rollback switch.

---

## ABI (locked)

| Item | Contract |
|------|----------|
| Convention | stdcall, **omit frame pointer**, `retn 4` |
| Stack in | `_extent_data` → 16-byte `xfs_bmbt_rec` |
| Implicit in | **EBP → `struct XFS`** (must not be clobbered) |
| Out | writes `XFS.extent`: `br_state`, `br_startoff`, `br_startblock`, `br_blockcount` |
| Preserved | EAX/EBX/ECX/EDX (FASM `uses`); **EBP** |
| Flags | unspecified / unused by callers |

### Layout (`xfs_bmbt_irec`)

```text
+0  br_startoff.lo / .hi     (8)
+8  br_startblock.lo / .hi   (8)
+16 br_blockcount            (4)
+20 br_state                 (4)
sizeof = 24
```

Rust does not hardcode `offsetof(XFS)`; the trampoline uses FASM `lea ecx, [ebp+XFS.extent]`.

---

## Out of scope

* Migrating other XFS helpers (`xfs_hashname`, extent walk, etc.)  
* Building an XFS test volume / live soak pipeline  
* “Fixing” XFS bitfield layout  
* Cut S  

---

## Completion rule

Complete Cut R gates → document → **STOP**. Do not start Cut S.
