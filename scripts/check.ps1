$ErrorActionPreference = "Stop"

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    $candidateCargoPaths = @(
        "D:\Software\Rust\cargo\bin\cargo.exe",
        (Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe")
    )
    foreach ($candidate in $candidateCargoPaths) {
        if (Test-Path $candidate) {
            $cargo = $candidate
            break
        }
    }
}

if (-not $cargo) {
    Write-Error "cargo was not found. Install Rust stable first: https://rustup.rs/"
}

$cargoPath = if ($cargo.Source) { $cargo.Source } else { [string]$cargo }

$localRustRoot = "D:\Software\Rust"
if (Test-Path (Join-Path $localRustRoot "rustup")) {
    $env:RUSTUP_HOME = Join-Path $localRustRoot "rustup"
}
if (Test-Path (Join-Path $localRustRoot "cargo")) {
    $env:CARGO_HOME = Join-Path $localRustRoot "cargo"
}

$isWindowsHost = $env:OS -eq "Windows_NT" -or $PSVersionTable.Platform -eq "Win32NT"

$vsDevCmdCandidates = @("D:\Software\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat")
if (${env:ProgramFiles(x86)}) {
    $vsDevCmdCandidates += Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
}
if (${env:ProgramFiles}) {
    $vsDevCmdCandidates += Join-Path ${env:ProgramFiles} "Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat"
}
$vsDevCmd = $vsDevCmdCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($isWindowsHost -and $vsDevCmd -and (Test-Path $vsDevCmd) -and -not $env:VSCMD_VER) {
    $checkCmd = Join-Path $env:TEMP "reflection-king-check-rust.cmd"
    Set-Content -LiteralPath $checkCmd -Encoding ASCII -Value @"
@echo off
call "$vsDevCmd" -arch=x64 -host_arch=x64 >nul
if not "%RUSTUP_HOME%"=="" set RUSTUP_HOME=%RUSTUP_HOME%
if not "%CARGO_HOME%"=="" set CARGO_HOME=%CARGO_HOME%
"$cargoPath" fmt --all -- --check
if errorlevel 1 exit /b %errorlevel%
"$cargoPath" clippy --workspace --all-targets -- -D warnings
if errorlevel 1 exit /b %errorlevel%
"$cargoPath" test --workspace
"@
    cmd /c $checkCmd
    $rustExit = $LASTEXITCODE
    Remove-Item -Force -LiteralPath $checkCmd -ErrorAction SilentlyContinue
    if ($rustExit -ne 0) {
        exit $rustExit
    }
} else {
& $cargoPath fmt --all -- --check
& $cargoPath clippy --workspace --all-targets -- -D warnings
& $cargoPath test --workspace
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
