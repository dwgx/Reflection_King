$ErrorActionPreference = "Stop"

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    $userCargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path $userCargo) {
        $cargo = $userCargo
    }
}

if (-not $cargo) {
    Write-Error "cargo was not found. Install Rust stable first: https://rustup.rs/"
}

$vsDevCmd = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
if ($IsWindows -and (Test-Path $vsDevCmd) -and -not $env:VSCMD_VER) {
    $checkCmd = Join-Path $env:TEMP "reflection-king-check-rust.cmd"
    Set-Content -LiteralPath $checkCmd -Encoding ASCII -Value @"
@echo off
call "$vsDevCmd" -arch=x64 -host_arch=x64 >nul
"$cargo" fmt --all -- --check
if errorlevel 1 exit /b %errorlevel%
"$cargo" clippy --workspace --all-targets -- -D warnings
if errorlevel 1 exit /b %errorlevel%
"$cargo" test --workspace
"@
    cmd /c $checkCmd
    $rustExit = $LASTEXITCODE
    Remove-Item -Force -LiteralPath $checkCmd -ErrorAction SilentlyContinue
    if ($rustExit -ne 0) {
        exit $rustExit
    }
} else {
& $cargo fmt --all -- --check
& $cargo clippy --workspace --all-targets -- -D warnings
& $cargo test --workspace
}

Push-Location (Join-Path $PSScriptRoot "..\services\reflection-browser")
try {
    if (-not (Test-Path "node_modules")) {
        npm install
    }
    npm run check
}
finally {
    Pop-Location
}

$scriptPaths = @(
    "scripts\check.ps1",
    "scripts\dev\bootstrap.ps1",
    "scripts\dev\run-local.ps1"
)

foreach ($path in $scriptPaths) {
    $fullPath = Join-Path (Split-Path $PSScriptRoot -Parent) $path
    $tokens = $null
    $errors = $null
    [System.Management.Automation.Language.Parser]::ParseFile(
        (Resolve-Path $fullPath),
        [ref]$tokens,
        [ref]$errors
    ) | Out-Null

    if ($errors.Count -gt 0) {
        Write-Error "PowerShell syntax failed: $path"
    }
}
