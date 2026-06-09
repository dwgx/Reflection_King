$ErrorActionPreference = "Stop"

if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Rust via winget..."
    winget install --id Rustlang.Rustup --exact --accept-package-agreements --accept-source-agreements
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$hasVcTools = $false
if (Test-Path $vswhere) {
    $vcInstall = & $vswhere `
        -latest `
        -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
    $hasVcTools = -not [string]::IsNullOrWhiteSpace($vcInstall)
}

if (-not $hasVcTools) {
    Write-Host "Installing Visual Studio C++ Build Tools via winget..."
    winget install `
        --id Microsoft.VisualStudio.2022.BuildTools `
        --exact `
        --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --norestart" `
        --accept-package-agreements `
        --accept-source-agreements
}

if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
    Write-Host "Installing FFmpeg via winget..."
    winget install --id Gyan.FFmpeg --exact --accept-package-agreements --accept-source-agreements
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Node.js via winget..."
    winget install --id OpenJS.NodeJS --exact --accept-package-agreements --accept-source-agreements
}

if (-not (Get-Command yt-dlp -ErrorAction SilentlyContinue)) {
    if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
        Write-Host "Installing Python via winget for yt-dlp..."
        winget install --id Python.Python.3.12 --exact --accept-package-agreements --accept-source-agreements
    }

    Write-Host "Installing yt-dlp via pip..."
    python -m pip install --user --upgrade yt-dlp==2026.03.17
}

Write-Host "Installing browser sidecar dependencies..."
Push-Location (Join-Path $PSScriptRoot "..\..\services\reflection-browser")
npm install
npx playwright install chromium
Pop-Location

Write-Host "Restart this terminal if Rust, FFmpeg, or Build Tools were just installed, then run:"
Write-Host "  cargo check --workspace"
