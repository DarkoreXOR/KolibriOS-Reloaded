# `orch`

Universal Rust/Rhai automation orchestrator — **generic runtime only**.

```text
                         orch
                          │
              ┌───────────┼───────────┐
              │           │           │
              $          @name      workflow
              │           │           │
         inline Rhai    Action      Workflow
                          │           │
                          │      composes Actions
                          │      and Workflows
                          │
                          └──────┬────┘
                                 │
                                Rhai
                                 │
                     generic orch runtime
```

```text
Rust       = generic runtime primitives
Rhai       = project automation (Actions / Workflows)
./tools/   = focused specialized utilities
project/   = CONFIG_DATA (not executable orchestration)
```

## Agent automation policy (mandatory)

**`orch` is the only automation boundary used directly by the AI agent.**

> Do **not** directly invoke PowerShell, cmd, bash, sh, Python, cargo, QEMU, FASM
> or other external programs from the agent shell when performing repository
> automation. Use `orch $` / `@action` / workflows, and invoke external programs
> from Rhai when necessary.

```text
AI agent → orch → Rhai → external process
```

If a generic capability is missing: extend orch → expose to Rhai → continue via
`orch $`. Project-specific work stays in Rhai; focused I/O stays in `./tools/`.
Never recreate a second orchestrator under `./tools/`.

## Architecture rule

> Never implement project-specific orchestration inside the generic `orch`
> runtime. Prefer Rhai Actions and Workflows. Use focused standalone tools under
> `./tools/` for operations that should be external executables.

Kolibri-specific build/image/QEMU logic lives in Rhai under `.orch/actions` and
`.orch/workflows`, reading **CONFIG_DATA** from
[`project/build.toml`](../../project/build.toml). Specialized work uses
`tools/kolibri_img`, `tools/mkfs_utils`, `tools/migration_gates`, and extract
scripts — not a second orchestrator binary.

## What belongs where

| Layer | Role |
|-------|------|
| **Rust (`orch`)** | Generic primitives: process, FS, CWD, env, pipes, sockets, HTTP, crypto, RNG, timers, cancel, rollback, registry, CLI |
| **`$`** | Anonymous one-off Action (inline Rhai; may `import` from `lib_dirs`) |
| **`@action`** | Named Action — a Rhai program (no Rhai `import`; compose via `execution::*`) |
| **`workflow`** | Composition of Actions / Workflows (no Rhai `import`) |
| **`./tools/*`** | Narrow utilities — one focused contract each |
| **`project/*.toml`** | Project data/configuration consumed by Rhai |

Actions and Workflows occupy **separate namespaces** by design (`@build` ≠ `build`).

## Usage

```powershell
.\orch --% run:dev
.\orch --% @build:dev
.\orch --% @mkfs exfat 4M
.\orch --% @doctor
.\orch --% $ "log::info(\"ok\");"
```

Repo-root launchers: `orch.cmd` (Windows) and `orch` (Unix). They prefer a
built `tools/orch/target/{release,debug}/orch` binary, else `cargo run -q …`.
Optional: `cargo install --path tools/orch` for a PATH-wide `orch`.

### CLI namespaces

```text
orch [GLOBAL_OPTIONS] [EXECUTION_UNITS...]

$ source      Inline Rhai (anonymous Action; in-memory)
@name         Named Action (`:` is part of the name — `@build:dev` is one name)
name          Workflow
::            Optional unit-argument terminator
```

Global flags (`--quiet`, `--verbose`, `--json`, `--no-progress`, `--config`) must
appear **before** the first execution unit.

Composition: `orch run:dev @clean` — the execution graph deduplicates identical
Actions already performed in this invocation (identity-based, not string matching).

### Filesystem layout

`:` is not written into filenames. Directory hierarchy maps to logical names;
`default.rhai` omits the final name component:

```text
actions/clean.rhai              → @clean
actions/build/default.rhai      → @build
actions/build/dev.rhai          → @build:dev
actions/build/kernel/release.rhai → @build:kernel:release
workflows/run/default.rhai      → run
workflows/run/dev.rhai          → run:dev
```

Duplicate logical names are fatal.

### Extending (AI agent / human)

No Rust changes required for project automation:

1. Add an Action under `.orch/actions/` (hierarchy = `:` names)
2. Add a Workflow under `.orch/workflows/` that calls `execution::run_action(...)` / `execution::run(...)`
3. Or add a focused tool under `./tools/` and invoke it via `process::run`

Extend Rust **only** for generic primitives missing from the runtime.

### String API (Rhai 1.25)

Prefer **method syntax**. Most operations are Rhai built-ins from
`MoreStringPackage` (enabled by `Engine::new()`). orch only adds a few gaps.

**Rhai built-in (do not reimplement):**

```rhai
let name = "kernel-release.bin";

name.starts_with("kernel")   // bool
name.ends_with(".bin")       // bool
name.contains("release")     // bool
name.index_of("rel")         // INT, or -1
name.len()                   // character count (not bytes)
name.is_empty()
name.to_lower() / name.to_upper()
name.sub_string(start, len)  // character indices
name.split("-")              // Array

// In-place (mutate the string variable):
name.trim();
name.replace("release", "debug");
```

**orch additions** (`runtime/string.rs`):

```rhai
let s = "  ab  ";
s.trim_start();              // in-place
s.trim_end();                // in-place

"foo:bar".strip_prefix("foo:")   // "bar", or () if absent
"foo:bar".strip_suffix(":bar")   // "foo", or () if absent

"abc".is_ascii()             // true
"a\nb\nc".lines()            // Array (strips `\r` via Rust `lines`)
["a","b"].join(":")          // "a:b"
```

Unicode: `len`, `index_of`, `sub_string`, and indexing use **characters**, not
UTF-8 bytes (`"Привет".len() == 6`, `"🙂".len() == 1`).

Path/filesystem helpers (`path::…`, basename/extension, …) stay in the path/FS
API — they are not string methods.

### CWD semantics

- **Invocation CWD**: OS working directory when `orch` started.
- **Script CWD**: logical overlay (`path::cwd` / `path::chdir`); starts as invocation CWD.
- **Child processes**: inherit Script CWD unless `process::run_cwd` overrides.
- Relative paths in FS helpers resolve against Script CWD.
- `path::chdir` does not change the OS process CWD.

### Process model

```rhai
let p = process::run("cargo", ["build"]);
while p.poll_running() {
    // other work
}
let code = p.wait();
if !success(code) { fail("build failed"); }
```

Stdout and stderr are distinct (inherit or capture). Cancellation (Ctrl+C)
propagates and terminates tracked child processes.

### Cancellation and rollback

- Ctrl+C sets a cancel token; Workflow → Action → children are stopped.
- Actions may register optional compensation with `execution::on_rollback("…")`.
- Rollback runs LIFO on the execution tree; failures are reported separately from
  the original error. Irreversible ops need explicit backups — the runtime does
  not invent fake undo.

### Literal `::`

Bare `::` terminates the current unit. After a within-unit `--`, or via
`--key=::`, `::` is literal data (shells strip quotes).

### Exit statuses

| Code | Meaning |
|------|---------|
| 0 | success |
| 1 | execution failure |
| 2 | validation failure |
| 3 | cancellation |
| 4 | rollback failure |
