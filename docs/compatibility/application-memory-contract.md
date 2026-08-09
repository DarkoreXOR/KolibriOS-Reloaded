# Application Memory Contract

Adversarial answer: what kernel memory/structures can applications observe?

## Summary verdict

| Region | Direct ring-3 load/store? | Official API? | Classification |
|--------|---------------------------|---------------|----------------|
| User image / heap / stacks | Yes | App owns | HARD (user AS) |
| IPC receive buffer (user VA) | Yes | fn60 | HARD |
| Named shared memory | Yes (mapped user pages) | 68.22/23 | HARD |
| LFB via **GS** | Yes (documented) | fn61 params | **HARD** |
| Background mapped pages | Yes when mapped | fn15.6/7 | HARD |
| `process_information` buffer | User-supplied | fn9 | HARD |
| IOCTL in/out buffers | User-supplied | 68.17 | HARD |
| FS 70/80 parameter blocks | User-supplied | 70/80 | HARD |
| `SLOT_BASE` / `APPDATA` | **No** (no `PG_USER` on kernel PDEs) | fn9 abstracts | **INTERNAL** (not app-direct) |
| `window_data` / `WDATA` | **No** (same) | fn0/9/GUI syscalls | **INTERNAL** |
| `KEY_BUFF` / `BTN_BUFF` | **No** via paging | fn2/17 | **INTERNAL** (likely); UNKNOWN if any trick |
| Kernel heap / code / IDT | **No** | — | INTERNAL |
| WinMap bitmap | Via fn34 (and internal `_display.win_map`) | fn34 | HARD via syscall; VA of map INTERNAL |
| SRV object | Opaque handle; **fields dumped by 68.31** | 68.16/17/31 | HARD handle + ACCIDENTAL dump |

---

## 1. Paging barrier (apps vs kernel)

**UPSTREAM FACT:** `init_mem` maps kernel with `PG_SWR` (read/write, supervisor) — no `PG_USER` (`docs/_upstream/init.inc`).

**LOCAL FACT:** `create_process` copies kernel PDEs from `sys_proc` (`taskman.inc:337–339`).

**LOCAL FACT:** Many syscalls reject pointers `>= OS_BASE` (`cmp … OS_BASE` / `is_region_userspace`).

**Conclusion:** Flat linear access to `0x80000000+` from CPL3 faults. Prior docs that treated `SLOT_BASE` as app-visible **overstated** direct access.

**Exception — LFB:** Framebuffer mapping uses **`PG_UWR`** and is also exposed via **GDT graphics segment (`gs`)**. This is intentional app-visible memory.

---

## 2. Documented direct graphics memory

**LOCAL FACT** — `kernel/docs/sysfuncs.txt`:

- Fn35 remarks: direct videomemory via selector `gs`.
- Fn61: parameters for direct graphics access; `mov eax,[gs:0]` / stores to LFB in LFB modes.

**HARD ABI:**

- Presence of usable `gs` graphics view for apps
- Fn61 subfunctions 1–3 (resolution, bpp, pitch)
- LFB modes: stores affect real pixels

**BEHAVIORAL:** non-LFB modes may use shadow buffer (docs describe double write / ignore on write).

---

## 3. Syscall-mediated structure copies

### Function 9 — `process_information`

- Apps provide buffer; kernel fills **0x4C** bytes (`sizeof.process_information`).
- Docs recommend 1KB for future fields — **BEHAVIORAL/forward-compat**, not validated size.
- Intersection with kernel memory → −1 (**LOCAL FACT** docs + `is_region_userspace`).

### Function 60 — IPC

- User buffer registered in `APPDATA.ipc_start/size`.
- Messages copied into **user** memory; event bit `EVENT_IPC`.

### Function 68.22 — named shared memory

- Maps shared pages into user space with user-accessible PTEs (`PG_SHARED|PG_UR` path in `heap.inc`).

### Function 68.31 — driver info

- Copies kernel `SRV` fields into user buffer including **code pointers** — accidental observation of kernel addresses.

### Function 34 — pixel owner

- Reads `_display.win_map` (kernel allocation); returns slot numbers — not raw map VA to apps.

---

## 4. Fixed VAs: app relevance

| VA | App-relevant? | Why |
|----|---------------|-----|
| `0xFE000000` LFB | **Yes** (via GS / mapping) | Direct pixels |
| `0x80090000` slots | **No** direct | Use fn9 |
| `0x80001000` windows | **No** direct | Use GUI syscalls |
| `0x8000F400` keys | **No** direct expected | Use fn2 |
| `0x8000F500` buttons | **No** direct expected | Use fn17 |
| `0x9000` boot_data | **No** (boot only) | |

**UNKNOWN:** Whether any shipping app uses CPL0 tricks, V86, or maps kernel phys differently. No app binaries in tree.

---

## 5. What Rust must preserve for apps (memory angle)

1. User AS model: low 2 GiB private; kernel high not user-dereferenceable (except intentional LFB/GS and explicit maps).
2. GS direct framebuffer contract + fn61.
3. Exact copy-out layouts for fn9 / IPC / shmem / IOCTL / 68.31 (until proven unused).
4. Rejection of user pointers into kernel range where current code checks `OS_BASE`.

## 6. What Rust need **not** preserve for apps

- Exact VA of `SLOT_BASE`, `window_data`, key/button rings, IDT, heap, recursive PT — **provided** paging still denies CPL3 and syscalls keep semantics.
