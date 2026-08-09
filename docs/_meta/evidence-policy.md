# Evidence Policy

This documentation suite reverse-engineers the KolibriOS FASM kernel in
[`kernel/`](../../kernel/) for a future freestanding Rust reimplementation.

## Labels

Every important claim must carry one of:

| Label | Meaning |
|-------|---------|
| **LOCAL FACT** | Verified in this workspace (cite path under `kernel/`, `rust_kernel/`, or `tools/`). Prefer `path:Symbol` (and line range when useful). |
| **UPSTREAM FACT** | Taken from official KolibriOS trunk only to reconstruct missing/corrupted local sources. Cite URL/revision and mirror path under `docs/_upstream/`. |
| **INFERENCE** | Reasonable conclusion from local facts; not proven by a single authoritative site. |
| **UNKNOWN** | Cannot be determined from sources available here. Requires runtime investigation or additional sources. |

## Hybrid upstream rule (locked)

1. Prefer **local tree** evidence everywhere.
2. Use official upstream **only** to reconstruct [`kernel/init.inc`](../../kernel/init.inc) and symbols it defines that local [`kernel/kernel.asm`](../../kernel/kernel.asm) calls.
3. Never silently assume upstream matches this tree version.
4. Record differences in [`upstream-init-diff.md`](upstream-init-diff.md).
5. Do **not** patch `kernel/`. Upstream mirrors live under `docs/_upstream/` only.

## Conflict resolution

- Prefer live code (`const.inc`, handlers) over comments in [`kernel/memmap.inc`](../../kernel/memmap.inc) when they disagree.
- Prefer code over [`kernel/docs/*.txt`](../../kernel/docs/) when docs and code disagree; document the conflict.
- Stale documentation is still useful as **historical ABI hints** but must be labeled.

## Scope of this tree

**LOCAL FACT:** Workspace root contains `kernel/` (FASM), `rust_kernel/` (Rust Cut A), `tools/`, `docs/`, vendored `fasm/`, and the immutable reference floppy image.

**LOCAL FACT:** [`kernel/init.inc`](../../kernel/init.inc) was restored 2026-08-09 from upstream matching the reference image (`944d74f01`). Historical corruption notes: [`upstream-init-diff.md`](upstream-init-diff.md), [`../migration/fasm-baseline-restoration.md`](../migration/fasm-baseline-restoration.md).

## Citation format

```
LOCAL FACT — kernel/core/syscall.inc:servetable2
UPSTREAM FACT — docs/_upstream/init.inc:mem_test (KolibriOS trunk)
INFERENCE — apps may poke SLOT_BASE; not proven from this tree alone
UNKNOWN — requires runtime investigation
```

## Goal reminder

Preserve **externally observable** KolibriOS ABI and behavior.
Do **not** preserve the FASM internal module structure unless it is ABI-visible.
