@echo off
setlocal

cd /d "%~dp0"

set "APP_NAME=Leetcode"
set "RUSTUP_HOME=%CD%\.rustup"
set "CARGO_HOME=%CD%\.cargo"
set "PATH=%CARGO_HOME%\bin;%PATH%"

if not exist "%CARGO_HOME%\bin\cargo.exe" (
    echo Cargo was not found at:
    echo   %CARGO_HOME%\bin\cargo.exe
    echo.
    echo Install Rust or restore the local .cargo/.rustup toolchain, then run this file again.
    pause
    exit /b 1
)

set "COMMAND=%~1"
if "%COMMAND%"=="" set "COMMAND=run"

if /I "%COMMAND%"=="run" (
    echo Starting %APP_NAME% in development mode...
    cargo run
    goto done
)

if /I "%COMMAND%"=="check" (
    echo Checking %APP_NAME%...
    cargo check
    goto done
)

if /I "%COMMAND%"=="test" (
    echo Testing %APP_NAME%...
    cargo test
    goto done
)

if /I "%COMMAND%"=="build" (
    echo Building %APP_NAME%...
    cargo build
    goto done
)

if /I "%COMMAND%"=="release" (
    echo Building %APP_NAME% release binary...
    cargo build --release
    goto done
)

echo Unknown command: %COMMAND%
echo.
echo Usage:
echo   run-leetcode.cmd
echo   run-leetcode.cmd run
echo   run-leetcode.cmd check
echo   run-leetcode.cmd test
echo   run-leetcode.cmd build
echo   run-leetcode.cmd release
exit /b 2

:done
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
    echo.
    echo Command failed with exit code %EXIT_CODE%.
    pause
)
exit /b %EXIT_CODE%
