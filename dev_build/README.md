# `dev_build/`

Disposable development artifacts (gitignored except this README).

| Path | Role |
|------|------|
| `dev_build/test/` | CoW boot images (`scripts/prepare_image.py`) |
| `dev_build/last_image.txt` | Path of the latest prepared image |
| `dev_build/build-mode.txt` | Active build mode marker |

Delete unused temporary files here after use. Full wipe:
`python scripts/clean.py --full`. Persistent regression disks live in `images/`,
not here.

See `.cursor/rules/dev-build.mdc`.
