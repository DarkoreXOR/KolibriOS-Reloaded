//! Built-in English help text.

pub fn print_general_help() {
    println!(
        "\
orch — universal Rust/Rhai automation orchestrator

Usage:
  orch [GLOBAL_OPTIONS] [EXECUTION_UNITS...]

Execution units:
  $<source>       Inline Rhai (anonymous Action; same runtime as named Actions)
  @<name>         Named Action (':' is part of the logical name)
  <name>          Workflow (composes Actions / Workflows)

Unit terminator:
  ::              Explicitly end the current unit's arguments

Global options (must appear before the first execution unit):
  --quiet         Suppress progress / execution-tree output (fatal errors remain)
  --verbose       Extra execution diagnostics
  --json          Machine-readable JSON events on stdout (no mixed progress text)
  --no-progress   Disable progress-specific decoration
  --config PATH   Path to orch config.toml
  -V, --version   Print version
  --              End the global-option phase

Literal `::`:
  Bare `::` ends the current unit's arguments.
  After a within-unit `--`, `::` is a literal positional (shells strip quotes,
  so quoted :: and bare :: are the same argv token — use `-- ::` or `--key=::`).

Filesystem layout (namespace hierarchy; do not put ':' in filenames):
  actions/clean.rhai           → @clean
  actions/build/default.rhai   → @build
  actions/build/dev.rhai       → @build:dev
  workflows/run/default.rhai   → run
  workflows/run/dev.rhai       → run:dev

Examples:
  orch @build
  orch @build:dev
  orch run
  orch run:dev
  orch @build @clean
  orch run:dev @clean
  orch $ 'print(\"hello\")'
  orch @build:dev --target x86 :: @clean
  orch --quiet --json @build:dev
  orch --no-progress @clean

PowerShell note:
  Use --% so `$` and `@…` are not expanded by the shell:
    .\\orch --% @build:dev
    .\\orch --% $ \"log::info(\\\"ok\\\");\"
    cargo run --manifest-path tools/orch/Cargo.toml -- --% @build:dev

Exit statuses:
  0  success
  1  execution failure
  2  validation failure
  3  cancellation
  4  rollback failure

Extension model:
  1. Write a Rhai Action under .orch/actions/ (directory hierarchy = ':' names)
  2. Add a Workflow under .orch/workflows/ that composes via execution::run_action / run_workflow
  3. Add a specialized utility under ./tools/ and invoke it from Rhai
  4. Extend the Rust runtime only for generic primitives

Architecture:
  $ = anonymous Action · @name = named Action · name = Workflow
  Never put project-specific orchestration in the Rust runtime.
  Never recreate a second orchestrator under ./tools/.
  Prefer Actions / Workflows + focused tools.

Agent policy:
  Use orch as the only automation boundary (orch $ / @action / workflow).
  Invoke external programs from Rhai, not from the agent shell directly.
  If a generic primitive is missing, extend orch and expose it to Rhai.
"
    );
}
