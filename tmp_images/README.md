# `tmp_images/` (deprecated)

Do **not** use this directory for new work.

| Need | Use instead |
|------|-------------|
| Disposable CoW / boot images | `dev_build/test/` via `python scripts/prepare_image.py` |
| Persistent FS regression disks | `images/` via `python scripts/mkfs.py` |
| Full wipe of disposables | `python scripts/clean.py --full` |

```bash
python scripts/mkfs.py exfat 4M
python scripts/run.py
python scripts/clean.py --full
```

See `.cursor/rules/dev-build.mdc` and `.cursor/rules/image-handling.mdc`.
