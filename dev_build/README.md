# `dev_build/`

Disposable development artifacts (gitignored except this README).

| Path | Role |
|------|------|
| `dev_build/test/` | Orchestrator CoW boot images (`@prepare_image`) |
| `dev_build/last_image.txt` | Path of the latest prepared image |
| `dev_build/orch-mode.txt` | Active build mode marker |

Delete unused temporary files here after use. Full wipe: `orch @clean`.
Persistent regression disks live in `images/`, not here.

See `.cursor/rules/dev-build.mdc`.
