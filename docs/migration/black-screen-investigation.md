# Black-screen bisect — investigation log

**Started:** 2026-08-09  
**Goal:** Find the first Rust migration/integration change after which the desktop goes black.

## Deterministic migration order

Order follows Stage 2 cut history (Phase C → Cuts A–O; later cuts P–AB are
outside this bisect log — see [`migration-plan.md`](migration-plan.md)). Each unit is one
`USE_RUST_*` trampoline switch (or Phase C smoke call). Smoke tests that go
through the public FASM symbol are part of the same unit when that switch is on.

| # | Unit | Switch / gate | Files | Rust symbol |
|---|------|---------------|-------|-------------|
| 0 | Phase C probe | `call phase_c_smoke_test` in `kernel.asm` | `kernel/kernel.asm`, `kernel/rust/phase_c.inc` | `rust_phase_c_probe` |
| 1 | Cut A CRC32 | `USE_RUST_CRC` | `kernel/crc.inc` | `rust_crc_32` |
| 2 | Cut A UTF-16 encode | `USE_RUST_UTF16` | `kernel/unicode.inc` | `rust_unicode_utf16_encode` |
| 3 | Cut A CP866 encode | `USE_RUST_CP866` | `kernel/unicode.inc` | `rust_unicode_cp866_encode` |
| 4 | Cut A UTF-8 decode | `USE_RUST_UTF8` | `kernel/unicode.inc` | `rust_unicode_utf8_decode` |
| 5 | Cut B cp866toUpper | `USE_RUST_CP866_UPPER` | `kernel/fs/parse_fn.inc` | `rust_cp866_to_upper` |
| 6 | Cut C utf16toUpper | `USE_RUST_UTF16_UPPER` | `kernel/fs/parse_fn.inc` | `rust_utf16_to_upper` |
| 7 | Cut D strncmp | `USE_RUST_STRNCMP` | `kernel/core/string.inc` | `rust_strncmp` |
| 8 | Cut E checksum_1 | `USE_RUST_CHECKSUM_1` | `kernel/network/stack.inc` | `rust_checksum_1` |
| 9 | Cut F checksum_2 | `USE_RUST_CHECKSUM_2` | `kernel/network/stack.inc` | `rust_checksum_2` |
| 10 | Cut G fsCalculateTime | `USE_RUST_FS_CALCULATE_TIME` | `kernel/fs/fs_common.inc` | `rust_fs_calculate_time` |
| 11 | Cut H block_clip | `USE_RUST_BLOCK_CLIP` | `kernel/video/blitter.inc` | `rust_block_clip` |
| 12 | Cut I ntfs_decode_mcb_entry | `USE_RUST_NTFS_DECODE_MCB_ENTRY` | `kernel/fs/ntfs.inc` | `rust_ntfs_decode_mcb_entry` |
| 13 | Cut J ntfs_restore_usa | `USE_RUST_NTFS_RESTORE_USA` | `kernel/fs/ntfs.inc` | `rust_ntfs_restore_usa` |
| 14 | Cut K fat_next_short_name | `USE_RUST_FAT_NEXT_SHORT_NAME` | `kernel/fs/fat.inc` | `rust_fat_next_short_name` |
| 15 | Cut L mouse_acceleration | `USE_RUST_MOUSE_ACCELERATION` | `kernel/hid/mousedrv.inc` | `rust_mouse_acceleration` |
| 16 | Cut M tcp_xmit_timer | `USE_RUST_TCP_XMIT_TIMER` | `kernel/network/tcp_subr.inc` | `rust_tcp_xmit_timer` |
| 17 | Cut N antiAliasing | `USE_RUST_ANTI_ALIASING` | `kernel/gui/font.inc` | `rust_anti_aliasing` |
| 18 | Cut O test_app_header | `USE_RUST_TEST_APP_HEADER` | `kernel/core/taskman.inc` | `rust_test_app_header` |

## Iteration log

### Iteration 0 — all Rust entry points disabled

```text
Iteration:            0
Previous state:       (current tree defaults: all USE_RUST_*=1, Phase C smoke on)
Newly enabled:        none (baseline — all disabled)
Files modified:
  - all 18 USE_RUST_* switches → 0 (crc/unicode/parse_fn/string/stack/
    fs_common/blitter/ntfs/fat/mousedrv/tcp_subr/font/taskman)
  - kernel/kernel.asm: commented call phase_c_smoke_test
Symbols enabled:      none (FASM bodies; Phase C call skipped)
Build result:         OK — kernel.mnt 231448 bytes (FASM 8 passes)
Image generated:      build/test/kernel-20260809-155753-651.img
QEMU launched:        yes (qemu-system-i386 -fda … -boot a -m 256 -vga std)
User result:          BLACK SCREEN after bootloader (expected desktop).
                      Bisect baseline FAILED — cannot attribute to a single
                      USE_RUST_* cut yet. Awaiting user instructions.
```

### Iteration 0b — pure FASM boot (smokes disabled)

```text
Iteration:            0b
Previous state:       Iteration 0 (USE_RUST_*=0, Phase C off, smokes still on)
Newly enabled:        none
Disabled boot smokes: all migration *_rust_smoke_test + phase_c (already off)
Kept:                 test_cpu, mem_test (real hardware init)
Rust integration:     all USE_RUST_*=0; phase_c_smoke_test disabled
Files modified:       kernel/kernel.asm (comment out remaining smoke calls)
Build result:         OK — kernel.mnt 231352 bytes
Image generated:      build/test/kernel-20260809-160119-230.img
QEMU launched:        yes
User result:          WORKS — desktop visible; true known-good FASM baseline.
```

### Iteration 1 — enable #0 Phase C probe only

```text
Iteration:            1
Previous state:       Iteration 0b WORKS
Newly enabled:        #0 Phase C — call phase_c_smoke_test → rust_phase_c_probe
Still disabled:       all USE_RUST_*=0; all other migration smokes
Files modified:       kernel/kernel.asm (uncomment phase_c_smoke_test only)
Symbols enabled:      rust_phase_c_probe (via phase_c_smoke_test)
Build result:         OK — kernel.mnt 231352 bytes
Image generated:      build/test/kernel-20260809-160215-219.img
QEMU launched:        yes
User result:          WORKS
```

### Iteration 2 — enable #1 Cut A CRC32

```text
Iteration:            2
Previous state:       Iteration 1 WORKS (Phase C on)
Newly enabled:        #1 USE_RUST_CRC=1 + call crc_rust_smoke_test → rust_crc_32
Still disabled:       remaining USE_RUST_*; remaining migration smokes
Files modified:       kernel/crc.inc; kernel/kernel.asm
Symbols enabled:      rust_crc_32 (via crc_32 trampoline)
Build result:         OK — kernel.mnt 231320 bytes
Image generated:      build/test/kernel-20260809-160331-884.img
QEMU launched:        yes
User result:          WORKS
```

### Iteration 3 — enable #2 Cut A UTF-16 encode

```text
Iteration:            3
Previous state:       Iteration 2 WORKS (Phase C + CRC)
Newly enabled:        #2 USE_RUST_UTF16=1 + utf16_rust_smoke_test
Files modified:       kernel/unicode.inc; kernel/kernel.asm
Symbols enabled:      rust_unicode_utf16_encode
Build result:         OK — kernel.mnt 231304 bytes
Image generated:      build/test/kernel-20260809-160418-673.img
QEMU launched:        yes
User result:          WORKS
```

### Iteration 4 — enable #3 Cut A CP866 encode

```text
Iteration:            4
Previous state:       Iteration 3 WORKS (Phase C + CRC + UTF-16)
Newly enabled:        #3 USE_RUST_CP866=1 + cp866_rust_smoke_test
Files modified:       kernel/unicode.inc; kernel/kernel.asm
Symbols enabled:      rust_unicode_cp866_encode
Build result:         OK — kernel.mnt 231304 bytes
Image generated:      build/test/kernel-20260809-160504-925.img
QEMU launched:        yes
User result:          WORKS
```

### Iteration 5 — enable #4 Cut A UTF-8 decode

```text
Iteration:            5
Previous state:       Iteration 4 WORKS (Phase C + CRC + UTF-16 + CP866)
Newly enabled:        #4 USE_RUST_UTF8=1 + utf8_rust_smoke_test
Files modified:       kernel/unicode.inc; kernel/kernel.asm
Symbols enabled:      rust_unicode_utf8_decode
Build result:         OK — kernel.mnt 231128 bytes
Image generated:      build/test/kernel-20260809-160540-500.img
QEMU launched:        yes
User result:          WORKS
```

### Iteration 6 — enable #5 Cut B cp866toUpper

```text
Iteration:            6
Previous state:       Iteration 5 WORKS (Phase C + Cut A complete)
Newly enabled:        #5 USE_RUST_CP866_UPPER=1 + cp866_upper_rust_smoke_test
Files modified:       kernel/fs/parse_fn.inc; kernel/kernel.asm
Symbols enabled:      rust_cp866_to_upper
Build result:         OK — kernel.mnt 231096 bytes
Image generated:      build/test/kernel-20260809-160617-210.img
QEMU launched:        yes
User result:          WORKS
```

### Iteration 7 — enable #6 Cut C utf16toUpper

```text
Iteration:            7
Previous state:       Iteration 6 WORKS
Newly enabled:        #6 USE_RUST_UTF16_UPPER=1 + utf16_upper_rust_smoke_test
Files modified:       kernel/fs/parse_fn.inc; kernel/kernel.asm
Symbols enabled:      rust_utf16_to_upper
Build result:         OK — kernel.mnt 231064 bytes
Image generated:      build/test/kernel-20260809-160651-157.img
QEMU launched:        yes
User result:          WORKS
```

### Iteration 8 — enable #7 Cut D strncmp

```text
Iteration:            8
Previous state:       Iteration 7 WORKS
Newly enabled:        #7 USE_RUST_STRNCMP=1 + strncmp_rust_smoke_test
Files modified:       kernel/core/string.inc; kernel/kernel.asm
Symbols enabled:      rust_strncmp
Build result:         OK — kernel.mnt 231032 bytes
Image generated:      build/test/kernel-20260809-160730-023.img
QEMU launched:        yes
User result:          OTHER — desktop apparently up; user unsure but internet
                      connection appears gone. Not classified as BLACK SCREEN.
                      Bisect paused per protocol; awaiting instructions.
```

### Iteration 7-recheck — rollback strncmp for network comparison

```text
Iteration:            7-recheck (rollback from 8)
Previous state:       Iteration 8 (strncmp on; possible net loss)
Action:               Disable USE_RUST_STRNCMP + strncmp smoke only
Still on:             Phase C + Cuts A–C
Files modified:       kernel/core/string.inc; kernel/kernel.asm
Build result:         OK — kernel.mnt 231064 bytes
Image generated:      build/test/kernel-20260809-161111-594.img
QEMU launched:        yes
User result:          WORKS — desktop + internet OK (confirms Iteration 7)
Next:                 Re-enable Iteration 8 and recheck net
```

### Iteration 8-recheck — re-enable #7 Cut D strncmp

```text
Iteration:            8-recheck
Previous state:       Iteration 7-recheck WORKS (internet OK)
Newly enabled:        #7 USE_RUST_STRNCMP=1 + strncmp_rust_smoke_test
Files modified:       kernel/core/string.inc; kernel/kernel.asm
Symbols enabled:      rust_strncmp
Build result:         OK — kernel.mnt 231032 bytes
Image generated:      build/test/kernel-20260809-161154-025.img
QEMU launched:        yes
User result:          NO INTERNET, no black screen (confirmed vs 7-recheck)
```

## REGRESSION FOUND (network — Cut D strncmp)

```text
Last known working state:
  Iteration 7 / 7-recheck — Phase C + Cuts A–C; desktop + internet OK

First failing state:
  Iteration 8 / 8-recheck — same + Cut D strncmp enabled

Newly enabled change:
  #7 Cut D strncmp
  USE_RUST_STRNCMP=1 + call strncmp_rust_smoke_test
  strncmp → rust_strncmp trampoline

Files changed:
  kernel/core/string.inc
  kernel/kernel.asm

Relevant symbols:
  strncmp, rust_strncmp

Why this change is the suspected boundary:
  A/B recheck: disabling strncmp restores internet; re-enabling removes it.
  Desktop still reaches (not black-screen). Network cuts E/F were never enabled.
  strncmp is used widely; a Rust ABI/semantics mismatch can break paths that
  affect stack/driver/config string compares without hanging at boot smokes.
```

**Bisect paused.** Awaiting user instructions (do not auto-fix).

### Cut D Test A — Rust strncmp ON, smoke OFF

```text
Iteration:            Cut-D Test A
Previous state:       Iteration 8-recheck (Rust strncmp + smoke; NO INTERNET)
Change:               USE_RUST_STRNCMP=1 kept; strncmp_rust_smoke_test disabled
Unchanged:            trampoline / rust_strncmp body (no edits)
Build result:         OK — kernel.mnt 231032 bytes
Image generated:      build/test/kernel-20260809-161350-844.img
QEMU launched:        yes
User result:          desktop OK; internet BROKEN
Interpretation:       Production FASM→Rust strncmp path implicated.
                      Smoke test NOT required to reproduce.
                      Test B skipped (only if Test A had internet OK).
```

### Cut D trampoline experiment — preserve EDX only

```text
Change:               push edx / stdcall rust_strncmp / pop edx in trampoline
Unchanged:            rust_strncmp body/blob; smoke OFF; all other cuts/switches
Build result:         OK (kernel.mnt 231032 bytes)
Image generated:      build/test/kernel-20260809-161954-609.img
QEMU launched:        yes
User result:          desktop OK, internet OK
```

```text
Cut D root cause CONFIRMED

EDX preservation in the FASM → Rust strncmp trampoline
restores the legacy ABI behavior required by get_service.
```

```text
Cut D — strncmp
Status: COMPLETE — FIXED

Regression:
    desktop OK
    internet BROKEN

Root cause:
    Rust strncmp clobbered EDX

Critical caller:
    get_service

Required compatibility behavior:
    EDX preserved across strncmp

Fix:
    FASM trampoline saves/restores EDX

Validation:
    desktop OK
    internet OK
```

### Iteration 9 — enable #8 Cut E checksum_1

```text
Iteration:            9
Previous state:       Cut D COMPLETE (Phase C + A–D; EDX trampoline; strncmp smoke OFF)
Newly enabled:        #8 USE_RUST_CHECKSUM_1=1 (production trampoline only)
Smoke:                checksum1_rust_smoke_test kept OFF
Still disabled:       Cut F+ (USE_RUST_CHECKSUM_2 and later = 0)
Files modified:       kernel/network/stack.inc (USE_RUST_CHECKSUM_1 → 1)
Symbols enabled:      rust_checksum_1 via checksum_1 trampoline
Build result:         OK (kernel.mnt 230952 bytes)
Image generated:      build/test/kernel-20260809-162208-075.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut E — checksum_1
Status: COMPLETE — OK (no regression)
```

### Iteration 10 — enable #9 Cut F checksum_2

```text
Iteration:            10
Previous state:       Iteration 9 WORKS (Phase C + A–E; Cut D EDX fix; smokes D/E OFF)
Newly enabled:        #9 USE_RUST_CHECKSUM_2=1 (production trampoline only)
Smoke:                checksum2_rust_smoke_test kept OFF
Still disabled:       Cut G+ (USE_RUST_FS_CALCULATE_TIME and later = 0)
Files modified:       kernel/network/stack.inc (USE_RUST_CHECKSUM_2 → 1)
Symbols enabled:      rust_checksum_2 via checksum_2 trampoline
Build result:         OK (kernel.mnt 230920 bytes)
Image generated:      build/test/kernel-20260809-162713-032.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut F — checksum_2
Status: COMPLETE — OK (no regression)
```

### Iteration 11 — enable #10 Cut G fsCalculateTime

```text
Iteration:            11
Previous state:       Iteration 10 WORKS (Phase C + A–F; Cut D EDX fix; smokes D–F OFF)
Newly enabled:        #10 USE_RUST_FS_CALCULATE_TIME=1 (production trampoline only)
Smoke:                fs_calculate_time_rust_smoke_test kept OFF
Still disabled:       Cut H+ (USE_RUST_BLOCK_CLIP and later = 0)
Files modified:       kernel/fs/fs_common.inc (USE_RUST_FS_CALCULATE_TIME → 1)
Symbols enabled:      rust_fs_calculate_time via fsCalculateTime trampoline
Build result:         OK (kernel.mnt 230840 bytes)
Image generated:      build/test/kernel-20260809-162812-535.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut G — fsCalculateTime
Status: COMPLETE — OK (no regression)
```

### Iteration 12 — enable #11 Cut H block_clip

```text
Iteration:            12
Previous state:       Iteration 11 WORKS (Phase C + A–G; Cut D EDX fix; smokes D–G OFF)
Newly enabled:        #11 USE_RUST_BLOCK_CLIP=1 (production trampoline only)
Smoke:                block_clip_rust_smoke_test kept OFF
Still disabled:       Cut I+ (USE_RUST_NTFS_DECODE_MCB_ENTRY and later = 0)
Files modified:       kernel/video/blitter.inc (USE_RUST_BLOCK_CLIP → 1)
Symbols enabled:      rust_block_clip via block_clip trampoline
Build result:         OK (kernel.mnt 230776 bytes)
Image generated:      build/test/kernel-20260809-162947-474.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut H — block_clip
Status: COMPLETE — OK (no regression)
```

### Iteration 13 — enable #12 Cut I ntfs_decode_mcb_entry

```text
Iteration:            13
Previous state:       Iteration 12 WORKS (Phase C + A–H; Cut D EDX fix; smokes D–H OFF)
Newly enabled:        #12 USE_RUST_NTFS_DECODE_MCB_ENTRY=1 (production trampoline only)
Smoke:                ntfs_decode_mcb_rust_smoke_test kept OFF
Still disabled:       Cut J+ (USE_RUST_NTFS_RESTORE_USA and later = 0)
Files modified:       kernel/fs/ntfs.inc (USE_RUST_NTFS_DECODE_MCB_ENTRY → 1)
Symbols enabled:      rust_ntfs_decode_mcb_entry via ntfs_decode_mcb_entry trampoline
Build result:         OK (kernel.mnt 230728 bytes)
Image generated:      build/test/kernel-20260809-163048-586.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut I — ntfs_decode_mcb_entry
Status: COMPLETE — OK (no regression)
```

### Iteration 14 — enable #13 Cut J ntfs_restore_usa

```text
Iteration:            14
Previous state:       Iteration 13 WORKS (Phase C + A–I; Cut D EDX fix; smokes D–I OFF)
Newly enabled:        #13 USE_RUST_NTFS_RESTORE_USA=1 (production trampoline only)
Smoke:                ntfs_restore_usa_rust_smoke_test kept OFF
Still disabled:       Cut K+ (USE_RUST_FAT_NEXT_SHORT_NAME and later = 0)
Files modified:       kernel/fs/ntfs.inc (USE_RUST_NTFS_RESTORE_USA → 1)
Symbols enabled:      rust_ntfs_restore_usa via ntfs_restore_usa trampoline
Build result:         OK (kernel.mnt 230696 bytes)
Image generated:      build/test/kernel-20260809-163145-265.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut J — ntfs_restore_usa
Status: COMPLETE — OK (no regression)
```

### Iteration 15 — enable #14 Cut K fat_next_short_name

```text
Iteration:            15
Previous state:       Iteration 14 WORKS (Phase C + A–J; Cut D EDX fix; smokes D–J OFF)
Newly enabled:        #14 USE_RUST_FAT_NEXT_SHORT_NAME=1 (production trampoline only)
Smoke:                fat_next_short_name_rust_smoke_test kept OFF
Still disabled:       Cut L+ (USE_RUST_MOUSE_ACCELERATION and later = 0)
Files modified:       kernel/fs/fat.inc (USE_RUST_FAT_NEXT_SHORT_NAME → 1)
Symbols enabled:      rust_fat_next_short_name via fat_next_short_name trampoline
Build result:         OK (kernel.mnt 230584 bytes)
Image generated:      build/test/kernel-20260809-163239-740.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut K — fat_next_short_name
Status: COMPLETE — OK (no regression)
```

### Iteration 16 — enable #15 Cut L mouse_acceleration

```text
Iteration:            16
Previous state:       Iteration 15 WORKS (Phase C + A–K; Cut D EDX fix; smokes D–K OFF)
Newly enabled:        #15 USE_RUST_MOUSE_ACCELERATION=1 (production trampoline only)
Smoke:                mouse_acceleration_rust_smoke_test kept OFF
Still disabled:       Cut M+ (USE_RUST_TCP_XMIT_TIMER and later = 0)
Files modified:       kernel/hid/mousedrv.inc (USE_RUST_MOUSE_ACCELERATION → 1)
Symbols enabled:      rust_mouse_acceleration via mouse_acceleration trampoline
Build result:         OK (kernel.mnt 230584 bytes)
Image generated:      build/test/kernel-20260809-163322-763.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut L — mouse_acceleration
Status: COMPLETE — OK (no regression)
```

### Iteration 17 — enable #16 Cut M tcp_xmit_timer

```text
Iteration:            17
Previous state:       Iteration 16 WORKS (Phase C + A–L; Cut D EDX fix; smokes D–L OFF)
Newly enabled:        #16 USE_RUST_TCP_XMIT_TIMER=1 (production trampoline only)
Smoke:                tcp_xmit_timer_rust_smoke_test kept OFF
Still disabled:       Cut N+ (USE_RUST_ANTI_ALIASING and later = 0)
Files modified:       kernel/network/tcp_subr.inc (USE_RUST_TCP_XMIT_TIMER → 1)
Symbols enabled:      rust_tcp_xmit_timer via tcp_xmit_timer trampoline
Build result:         OK (kernel.mnt 230488 bytes)
Image generated:      build/test/kernel-20260809-163425-938.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut M — tcp_xmit_timer
Status: COMPLETE — OK (no regression)
```

### Iteration 18 — enable #17 Cut N antiAliasing

```text
Iteration:            18
Previous state:       Iteration 17 WORKS (Phase C + A–M; Cut D EDX fix; smokes D–M OFF)
Newly enabled:        #17 USE_RUST_ANTI_ALIASING=1 (production trampoline only)
Smoke:                anti_aliasing_rust_smoke_test kept OFF
Still disabled:       Cut O (USE_RUST_TEST_APP_HEADER = 0)
Files modified:       kernel/gui/font.inc (USE_RUST_ANTI_ALIASING → 1)
Symbols enabled:      rust_anti_aliasing via antiAliasing trampoline
Build result:         OK (kernel.mnt 230472 bytes)
Image generated:      build/test/kernel-20260809-163549-095.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut N — antiAliasing
Status: COMPLETE — OK (no regression)
```

### Iteration 19 — enable #18 Cut O test_app_header

```text
Iteration:            19
Previous state:       Iteration 18 WORKS (Phase C + A–N; Cut D EDX fix; smokes D–N OFF)
Newly enabled:        #18 USE_RUST_TEST_APP_HEADER=1 (production trampoline only)
Smoke:                test_app_header_rust_smoke_test kept OFF
Still disabled:       none remaining in Stage 2 cut order
Files modified:       kernel/core/taskman.inc (USE_RUST_TEST_APP_HEADER → 1)
Symbols enabled:      rust_test_app_header via test_app_header trampoline
Build result:         OK (kernel.mnt 230392 bytes)
Image generated:      build/test/kernel-20260809-163644-391.img
QEMU launched:        yes
User result:          WORKS — desktop OK, internet OK
```

```text
Cut O — test_app_header
Status: COMPLETE — OK (no regression)
```

## BISECT COMPLETE (Stage 2 Cuts A–O)

```text
Final known-good state:
  Phase C + Cuts A–O all USE_RUST_*=1
  Cut D: EDX-preserving FASM trampoline (required fix)
  Migration diagnostic smokes D–O: OFF
  Earlier Cut A–C / Phase C smokes: still ON (as during bisect)

Sole regression found:
  Cut D strncmp — internet BROKEN (desktop OK)
  Root cause: rust_strncmp clobbered EDX; get_service needed EDX=SRV*
  Fix: push/pop edx in FASM strncmp trampoline
  Validation: desktop OK, internet OK

All other cuts (#0 Phase C, #1–#6 A–C, #8–#18 E–O):
  No desktop/internet regression observed under this protocol.
```

## Stage 3 — diagnostic smoke re-enable (D→O, one at a time)

Production stays Phase C + A–O `USE_RUST_*=1` with Cut D EDX trampoline.
Only the `*_rust_smoke_test` call in `kernel.asm` changes per iteration.

| Smoke | Result |
|-------|--------|
| D `strncmp_rust_smoke_test` | OK |
| E `checksum1_rust_smoke_test` | OK (soft: sometimes slower) |
| F `checksum2_rust_smoke_test` | OK |
| G `fs_calculate_time_rust_smoke_test` | OK |
| H `block_clip_rust_smoke_test` | OK |
| I `ntfs_decode_mcb_rust_smoke_test` | OK |
| J `ntfs_restore_usa_rust_smoke_test` | OK |
| K `fat_next_short_name_rust_smoke_test` | OK |
| L `mouse_acceleration_rust_smoke_test` | OK |
| M `tcp_xmit_timer_rust_smoke_test` | **FAIL** then **fixed** — see below → **OK** |
| N `anti_aliasing_rust_smoke_test` | **OK** |
| O `test_app_header_rust_smoke_test` | **OK** |

## Stage 3 COMPLETE (diagnostic smokes D–O)

```text
Final state:
  Phase C + Cuts A–O production USE_RUST_*=1
  Cut D: EDX-preserving FASM trampoline (unchanged)
  All migration diagnostic smokes D–O: ON
  Cut M smoke: expectations corrected for unsigned ADD+JA (1/1 not 39/9)

Validated: desktop OK, internet OK (user) through full D–O smoke stack
Image (Cut O): build/test/kernel-20260809-170120-841.img

Do not start Cut P.
```

```text
Stage 3 / Cut M smoke — root cause
Symptom:              black screen; QMP EAX=0xDEAD0C4D at smoke .fail
Isolation:            M alone (D–L OFF) still FAIL — not D–L interaction
Socket at fail:       t_rtt=1, t_srtt=1, t_rttvar=1 (update path result)
Cause:                smoke expected signed-style srtt=39/rttvar=9 after
                      delta=-1 from (40,10,rtt=5); FASM/Rust use unsigned
                      ADD + JA, so 40+(-1) sets CF → clamp to 1 (same for rttvar)
Fix:                  tcp_xmit_timer.inc smoke expects 1/1; clamp also checks rttvar=4
Production:           unchanged (Rust already matched FASM ja semantics)
```

```text
Stage 3 / Cut M smoke
Previous state:       D–L smokes ON; M–O OFF; production A–O ON
Change:               enable only tcp_xmit_timer_rust_smoke_test
Image (fail):         build/test/kernel-20260809-165158-746.img
User result:          BLACK SCREEN after bootloader
Action:               disable Cut M smoke again (production USE_RUST_TCP_XMIT_TIMER=1 kept)
Note:                 Stage 2 production Cut M was OK with this smoke OFF —
                      treat as diagnostic-smoke failure, not production rollback
Open:                 investigate smoke vs leave M smoke OFF and continue N→O
```

## REGRESSION FOUND (Iteration 0 baseline — inconclusive)

```text
Last known working state:
  Original/reference floppy (stock kernel) — desktop works (per investigation brief)

First failing state:
  Iteration 0 — all USE_RUST_* = 0 + Phase C smoke call disabled

Newly enabled change:
  none (this iteration disabled Rust trampolines; black screen persists)

Files changed for Iteration 0:
  - kernel/crc.inc, unicode.inc, fs/parse_fn.inc, core/string.inc,
    network/stack.inc, fs/fs_common.inc, video/blitter.inc, fs/ntfs.inc,
    fs/fat.inc, hid/mousedrv.inc, network/tcp_subr.inc, gui/font.inc,
    core/taskman.inc  (USE_RUST_* → 0)
  - kernel/kernel.asm (commented call phase_c_smoke_test)

Relevant symbols:
  Public FASM paths still exercised by boot smokes:
  crc_32, unicode.*, cp866toUpper, utf16toUpper, strncmp, checksum_1/2,
  fsCalculateTime, block_clip, ntfs_*, fat_next_short_name,
  mouse_acceleration, tcp_xmit_timer, antiAliasing, test_app_header
  (Phase C rust_phase_c_probe NOT called)

Why this is the suspected boundary:
  Black screen after bootloader with every Rust trampoline off means the
  fault is not (yet) isolated to one USE_RUST_* cut. Remaining hybrid
  boot path still includes hang-on-fail smoke tests in high_code that call
  those public symbols; a smoke fail would jmp $ and never reach desktop.
  Rust blobs remain embedded but unused by trampolines.
```
