#!/usr/bin/env python3
"""Merge heuristic PTE hits with hand-audited authority into stage4-pte-writers.json."""

from __future__ import annotations

import json
from pathlib import Path

from common import PROJECT_ROOT

OUT = PROJECT_ROOT / "docs" / "migration" / "stage4-pte-writers.json"
HITS = PROJECT_ROOT / "docs" / "migration" / "stage4-pte-writers.hits.json"

# Hand-audited authority (LOCAL FACT from source reading 2026-08-14).
AUDITED = {
    "summary": {
        "audited_runtime_page_tabs_or_pde_writers": 23,
        "audited_runtime_writer_notes": (
            "23 audited records in audited_runtime_writers; the heap.inc record "
            "covers multiple symbols (user_alloc/free/realloc/unmap/normalize). "
            "Expanding that group yields 30+ distinct mutating loci."
        ),
        "audited_boot_writers": 3,
        "audited_cr3_switch_sites": 8,
        "audited_invlpg_helper_classes": 6,
        "audited_read_only_footholds": 3,
        "unresolved_suspicious": 1,
        "unresolved_notes": (
            "PCIe.inc references undeclared `sys_pgdir` / `PG_LARGE` in-tree; "
            "site is a PDE writer intent but symbol binding is unresolved — "
            "classified under unresolved_suspicious, not as clean ownership."
        ),
        "sole_runtime_writer_feasible_today": False,
        "sole_runtime_writer_notes": (
            "page_tabs is dual-use: (1) hardware PTE/PDE recursive window at "
            "0xFDC00000; (2) user-heap soft descriptors (MEM_BLOCK_* with "
            "present bit clear). Hardware map helpers, heap soft writers, DLL, "
            "v86, fault, and sched I/O maps all mutate the same array. "
            "Migrating map_page alone cannot claim ownership."
        ),
        "dual_use_page_tabs": True,
        "app_page_tabs_alias": "app_page_tabs == page_tabs == 0xFDC00000",
        "master_tab": "master_tab = page_tabs + (page_tabs shr 10) — PDE self-map",
    },
    "architecture_facts": {
        "page_tabs": "0xFDC00000",
        "app_page_tabs": "0xFDC00000 (alias)",
        "kernel_tabs": "page_tabs + (OS_BASE shr 10) = 0xFDE00000",
        "master_tab": "page_tabs + (page_tabs shr 10)",
        "paging_mode": "32-bit non-PAE PDE/PTE; optional PSE large pages; optional PGE",
        "nx": "not used as a first-class kernel PTE bit; pte_valid_mask set at boot",
        "present_bit": "PG_READ = 0x001",
        "writable_bit": "PG_WRITE = 0x002",
        "user_bit": "PG_USER = 0x004",
        "soft_heap_tags": {
            "MEM_BLOCK_RESERVED": "0x02 (== PG_WRITE; used for lazy alloc with present=0)",
            "MEM_BLOCK_FREE": "0x04",
            "MEM_BLOCK_USED": "0x08",
            "MEM_BLOCK_DONT_FREE": "0x10",
        },
        "pte_valid_mask": "data32.inc; initialized in kernel.asm high_code from CPU caps",
    },
    "audited_runtime_writers": [
        {
            "symbol": "map_page",
            "file": "kernel/core/memory.inc",
            "category": "runtime_map",
            "direct": True,
            "states": ["page_tabs PTE", "TLB via invlpg"],
            "operation": "store (phys|flags) & pte_valid_mask; invlpg [lin]",
            "present": "set if flags include PG_READ; PG_UNMAP clears",
            "writable": "via PG_WRITE in flags",
            "user": "via PG_USER in flags",
            "page_table_level": "PTE",
            "invlpg": True,
            "cr3": False,
            "mutex": False,
            "allocates_phys": False,
            "could_become_rust": "only inside a sole page_tabs owner cluster — not alone",
            "notes": "stdcall(lin,phys,flags); ret 12; push/pop ebx",
        },
        {
            "symbol": "map_io_mem",
            "file": "kernel/core/memory.inc",
            "category": "runtime_map",
            "direct": True,
            "states": ["page_tabs PTE", "TLB"],
            "operation": "loop store phys+flags advancing by PAGE_SIZE",
            "page_table_level": "PTE",
            "invlpg": True,
            "cr3": False,
            "mutex": False,
            "allocates_phys": False,
            "could_become_rust": "same cluster as map_page",
            "notes": "alloc_kernel_space first; callers: acpi, ahci, apic, hpet, …",
        },
        {
            "symbol": "commit_pages",
            "file": "kernel/core/memory.inc",
            "category": "runtime_map",
            "direct": True,
            "states": ["page_tabs PTE", "TLB"],
            "operation": "stosd loop under pg_data.mutex; phys advances",
            "page_table_level": "PTE",
            "invlpg": True,
            "cr3": False,
            "mutex": "pg_data.mutex",
            "could_become_rust": "same cluster; mutex ownership question",
            "notes": "ABI: eax=phys|flags, ebx=lin, ecx=count; PE CommitPages",
        },
        {
            "symbol": "unmap_pages",
            "file": "kernel/core/memory.inc",
            "category": "runtime_unmap",
            "direct": True,
            "states": ["page_tabs PTE", "TLB"],
            "operation": "stosd zeros; invlpg; does NOT free phys pages",
            "page_table_level": "PTE",
            "invlpg": True,
            "cr3": False,
            "mutex": False,
            "could_become_rust": "same cluster",
            "notes": "ABI: eax=base, ecx=count; distinct from release_pages",
        },
        {
            "symbol": "release_pages",
            "file": "kernel/core/memory.inc",
            "category": "runtime_unmap",
            "direct": True,
            "states": ["page_tabs PTE", "TLB", "sys_pgmap via Rust Mode-B"],
            "operation": "xchg PTE→0; invlpg; if present call Rust bitmap helper",
            "page_table_level": "PTE",
            "invlpg": True,
            "cr3": False,
            "mutex": "pg_data.mutex",
            "could_become_rust": "PTE half only with sole page_tabs owner; bitmap already Rust",
            "notes": "Must not call free_page (page_start)",
        },
        {
            "symbol": "map_page_table",
            "file": "kernel/core/memory.inc",
            "category": "page_table_allocation",
            "direct": True,
            "states": ["master_tab PDE", "TLB of page_tabs window"],
            "operation": "store PDE PG_UWR into master_tab[lin>>22]; invlpg page_tabs slot",
            "page_table_level": "PDE",
            "invlpg": True,
            "cr3": False,
            "could_become_rust": "requires PDE ownership; couples to AS growth",
            "notes": "Used by new_mem_resize / fault table growth paths",
        },
        {
            "symbol": "safe_map_page",
            "file": "kernel/core/memory.inc",
            "category": "runtime_map",
            "direct": False,
            "states": ["page_tabs via map_page"],
            "operation": "indirect → map_page",
            "could_become_rust": "follows map_page",
            "notes": "IPC/map helpers",
        },
        {
            "symbol": "page_fault_handler",
            "file": "kernel/core/memory.inc",
            "category": "page_fault",
            "direct": True,
            "states": ["page_tabs via map_page", "zero-fill new page", "CoW DLL"],
            "operation": "lazy alloc; map_page PG_UWR; CoW path remaps writable",
            "page_table_level": "PTE (+ may require PDE present)",
            "invlpg": "via map_page",
            "cr3": False,
            "allocates_phys": True,
            "could_become_rust": "NO without fault/policy ownership acceptance",
            "notes": "Uses MEM_BLOCK_RESERVED/PG_WRITE encoding for lazy user pages",
        },
        {
            "symbol": "new_mem_resize",
            "file": "kernel/core/memory.inc",
            "category": "process_address_space",
            "direct": True,
            "states": ["app_page_tabs", "map_page_table", "free_page"],
            "operation": "shrink clears PTEs+free_page; expand alloc_page+map_page_table+zero PT",
            "mutex": "pg_data.mutex",
            "could_become_rust": "NO — process mem_used + PDE growth",
            "notes": "app_page_tabs is alias of page_tabs",
        },
        {
            "symbol": "create_ring_buffer",
            "file": "kernel/core/memory.inc",
            "category": "runtime_map",
            "direct": True,
            "states": ["page_tabs dual mapping"],
            "operation": "map same phys at buf and buf+0x10000",
            "invlpg": True,
            "could_become_rust": "only with page_tabs owner",
            "notes": "uses alloc_pages",
        },
        {
            "symbol": "sys_ipc_send",
            "file": "kernel/core/memory.inc",
            "category": "runtime_unmap",
            "direct": True,
            "states": ["page_tabs PTE clear for ipc_tmp/pdir/ptab"],
            "operation": "mov 0; invlpg",
            "could_become_rust": "only with page_tabs owner",
            "notes": "teardown of temporary IPC maps",
        },
        {
            "symbol": "user_alloc / user_alloc_at / user_free / user_unmap / user_realloc / user_normalize / init_heap(user)",
            "file": "kernel/core/heap.inc",
            "category": "heap_mapping",
            "direct": True,
            "states": ["page_tabs soft MEM_BLOCK_* AND hardware PTEs"],
            "operation": "soft freelist in PTE slots; xchg clear+free_page+invlpg on present",
            "present": "soft entries present=0; mapped pages present=1",
            "mutex": "PROC.heap_lock",
            "could_become_rust": "requires heap+page_tabs joint ownership — huge",
            "notes": "PRIMARY dual-use co-owner of page_tabs; bypasses map_page",
        },
        {
            "symbol": "map_shared_memory / SMEM path",
            "file": "kernel/core/heap.inc",
            "category": "heap_mapping",
            "direct": True,
            "states": ["page_tabs PTE copy with PG_SHARED|PG_UR"],
            "operation": "lodsd/stosd from owner PTEs",
            "could_become_rust": "with heap/page_tabs cluster",
            "notes": "bypasses map_page",
        },
        {
            "symbol": "dll packed load pre-map + HDLL map_pages_loop",
            "file": "kernel/core/dll.inc",
            "category": "dll_pe_mapping",
            "direct": True,
            "states": ["page_tabs PTE", "MEM_BLOCK_DONT_FREE"],
            "operation": "stosd PG_UWR; later xchg PG_UR CoW; or DONT_FREE on header",
            "invlpg": True,
            "could_become_rust": "NO without DLL loader ownership",
            "notes": "bypasses map_page; CoW couples to page_fault_handler",
        },
        {
            "symbol": "nosb5 / nosb6 background image map",
            "file": "kernel/gui/background.inc",
            "category": "runtime_map",
            "direct": True,
            "states": ["page_tabs PTE", "MEM_BLOCK_DONT_FREE"],
            "operation": "copy PTE with PG_UWR; free_page old; invlpg",
            "could_become_rust": "only with page_tabs owner",
            "notes": "bypasses map_page",
        },
        {
            "symbol": "v86_create / v86_start / v86 I/O map updaters",
            "file": "kernel/core/v86.inc",
            "category": "v86_mapping",
            "direct": True,
            "states": ["page_tabs low-MB BIOS maps", "CR3", "tss io_map PTEs"],
            "operation": "direct PTE stores + mov cr3",
            "cr3": True,
            "could_become_rust": "NO without v86+CR3 ownership",
            "notes": "switches to V86 process CR3 then writes page_tabs",
        },
        {
            "symbol": "do_change_task",
            "file": "kernel/core/sched.inc",
            "category": "process_address_space",
            "direct": True,
            "states": ["page_tabs TSS I/O map PTEs", "CR3"],
            "operation": "remap tss._io_map_* pages; maybe mov cr3",
            "cr3": True,
            "could_become_rust": "NO — boundaries non-cut / Stage 6",
            "notes": "process switch",
        },
        {
            "symbol": "create_process / taskman tmp_task_ptab maps",
            "file": "kernel/core/taskman.inc",
            "category": "process_address_space",
            "direct": False,
            "states": ["page_tabs via map_page"],
            "operation": "map_page / unmap helpers around process PDT build",
            "could_become_rust": "NO — Stage 6",
            "notes": "also copies kernel PDEs from sys_proc",
        },
        {
            "symbol": "framebuffer LFB page-table fill",
            "file": "kernel/video/framebuffer.inc",
            "category": "runtime_map",
            "direct": True,
            "states": ["mapped PT via map_page then stosd PTE pattern", "CR3 flush"],
            "operation": "map_page scratch; stosd phys run; mov cr3,cr3 flush",
            "could_become_rust": "video/LFB ownership — out of early Path A",
            "notes": "also touches PROC.pdt_0 LFB PDEs",
        },
        {
            "symbol": "kernel.asm TSS / idle I/O map_page sites",
            "file": "kernel/kernel.asm",
            "category": "runtime_map",
            "direct": False,
            "states": ["page_tabs via map_page"],
            "operation": "stdcall map_page",
            "could_become_rust": "follows map_page",
            "notes": "boot/runtime setup callers",
        },
        {
            "symbol": "kernel.asm master_tab TLB games",
            "file": "kernel/kernel.asm",
            "category": "error_recovery",
            "direct": True,
            "states": ["master_tab[0]", "CR3 flush"],
            "operation": "xchg master_tab entry; mov cr3,cr3",
            "cr3": True,
            "could_become_rust": "NO",
            "notes": "diagnostic/error path around master_tab",
        },
        {
            "symbol": "mtrr_* CR3 flush",
            "file": "kernel/core/mtrr.inc",
            "category": "tlb_only",
            "direct": False,
            "states": ["TLB via CR3 reload"],
            "operation": "mov eax,cr3; mov cr3,eax",
            "cr3": True,
            "could_become_rust": "N/A — not a PTE content writer",
            "notes": "classified as TLB invalidate via CR3, not page_tabs owner",
        },
        {
            "symbol": "shutdown / APM CR3 paths",
            "file": "kernel/boot/shutdown.inc",
            "category": "teardown",
            "direct": False,
            "states": ["CR3"],
            "operation": "mov cr3",
            "cr3": True,
            "could_become_rust": "NO",
            "notes": "boot/teardown",
        },
    ],
    "audited_boot_writers": [
        {
            "symbol": "init_mem",
            "file": "kernel/init.inc",
            "category": "boot_initialization",
            "states": ["sys_proc.pdt_0", "tmp page tables", "recursive page_tabs PDE"],
            "notes": "PSE optional 4MiB kernel map; builds initial AS",
        },
        {
            "symbol": "init_page_map",
            "file": "kernel/init.inc",
            "category": "boot_initialization",
            "states": ["sys_pgmap bitmap — not page_tabs"],
            "notes": "phys bitmap boot; already documented in Stage-4 bitmap program",
        },
        {
            "symbol": "high_code pte_valid_mask + PGE",
            "file": "kernel/kernel.asm",
            "category": "boot_initialization",
            "states": ["pte_valid_mask", "sys_proc.pdt_0 PGE bits", "CR3 flush"],
            "notes": "sets feature mask used by all map helpers",
        },
    ],
    "audited_non_writers": [
        {
            "symbol": "get_pg_addr (AQ)",
            "file": "kernel/core/memory.inc + rust",
            "role": "read_only_translate",
            "notes": "injects page_tabs; does not own",
        },
        {
            "symbol": "v86_get_lin_addr (BL)",
            "role": "read_only_translate",
        },
        {
            "symbol": "usb_td_to_virt (CI)",
            "role": "read_only_compose",
        },
    ],
    "unresolved_suspicious": [
        {
            "symbol": "pci_ext_config / PCIe MMIO map",
            "file": "kernel/bus/pci/PCIe.inc",
            "issue": "writes dword[sys_pgdir+…] with PG_SHARED+PG_LARGE+PG_USER; sys_pgdir and PG_LARGE not defined in scanned const.inc — unresolved binding",
            "classification": "intended PDE large-page writer; do not count as owned clean runtime API until symbol audit resolves",
        }
    ],
    "bypasses_of_map_page": [
        "map_io_mem (own loop)",
        "commit_pages (stosd)",
        "unmap_pages (stosd zero)",
        "release_pages (xchg)",
        "heap.inc user_* soft+hard stores",
        "heap.inc shared-memory lodsd/stosd",
        "dll.inc unpack pre-map stosd",
        "dll.inc HDLL map_pages_loop xchg",
        "gui/background.inc nosb5/6",
        "v86.inc BIOS identity maps",
        "sched.inc do_change_task I/O maps",
        "memory.inc create_ring_buffer",
        "memory.inc sys_ipc_send clears",
        "memory.inc new_mem_resize",
        "framebuffer.inc stosd into mapped PT",
        "map_page_table → master_tab PDE",
    ],
    "callers_of_map_page": [
        "page_fault_handler",
        "safe_map_page / map_mem* IPC helpers",
        "heap.inc (selected paths)",
        "taskman.inc create_process helpers",
        "framebuffer.inc",
        "kernel.asm TSS / I/O map setup",
    ],
    "ownership_graph": {
        "page_tabs_array": "FASM multi-writer (hardware + soft heap)",
        "master_tab_pde": "FASM (map_page_table + boot + error paths)",
        "pte_permissions_policy": "FASM callers choose PG_* flags",
        "tlb_invlpg": "FASM co-located with each writer",
        "tlb_cr3_reload": "FASM sched/v86/mtrr/framebuffer/shutdown",
        "page_table_allocation": "FASM (alloc_page Rust + map_page_table FASM)",
        "fault_repair": "FASM page_fault_handler",
        "cr3_switching": "FASM do_change_task / v86 / boot",
        "phys_bitmap": "Rust (Cut CU) — consumed by fault/heap/release",
    },
}


def main() -> int:
    hits_doc = {}
    if HITS.exists():
        hits_doc = json.loads(HITS.read_text(encoding="utf-8"))

    payload = {
        "schema": "stage4-pte-writers/v1",
        "generated_by": "scripts/merge_pte_writers_inventory.py",
        "authority": (
            "audited_* sections are authoritative for ownership. "
            "summary_heuristic / raw_hits are scanner aids only."
        ),
        **AUDITED,
        "summary_heuristic": hits_doc.get("summary_heuristic", {}),
        "raw_hit_count": hits_doc.get("summary_heuristic", {}).get("raw_hits"),
        "raw_hits_path": "docs/migration/stage4-pte-writers.hits.json",
    }
    OUT.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(
        f"Wrote {OUT} audited_runtime={AUDITED['summary']['audited_runtime_page_tabs_or_pde_writers']} "
        f"unresolved={len(AUDITED['unresolved_suspicious'])}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
