# Development and test artifacts

Disposable boot images, screendumps, QEMU logs, and agent workspace output
belong here — not in `./build/` (production artifacts) or `./images/`
(persistent filesystem regression disks).

The orchestrator writes timestamped hybrid-kernel boot images to
`dev_build/test/` by default (see `orch/config.toml`).

Safe to delete:

```powershell
cargo run --manifest-path orch/Cargo.toml -- clean
```

This removes `./dev_build/` and `./build/` but preserves `./images/`.

Migration cut checkpoints that previously lived under `tmp_images/` should
also use `dev_build/` for new work.
