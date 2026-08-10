@echo off
setlocal EnableExtensions
cd /d "%~dp0"

rem Keep the package-local target dir so the launcher never picks up a stale
rem binary from another CARGO_TARGET_DIR (e.g. IDE/sandbox caches).
set "CARGO_TARGET_DIR=%CD%\tools\orch\target"
set "ORCH_BIN=%CARGO_TARGET_DIR%\debug\orch.exe"

cargo build -q --manifest-path tools\orch\Cargo.toml
if errorlevel 1 exit /b %ERRORLEVEL%

if not exist "%ORCH_BIN%" (
  echo error: orch binary missing after build: %ORCH_BIN% 1>&2
  exit /b 1
)

"%ORCH_BIN%" %*
exit /b %ERRORLEVEL%
