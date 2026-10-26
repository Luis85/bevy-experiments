@echo off
setlocal
cd /d "%~dp0"
where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust/Cargo fehlt. Installation: https://www.rust-lang.org/tools/install
  pause
  exit /b 1
)
cargo run --locked --release -p legion -- %*
if errorlevel 1 pause
endlocal
