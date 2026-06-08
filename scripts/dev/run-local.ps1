$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$browserDir = Join-Path $root "services\reflection-browser"

$env:RK_BROWSER_PROBE_URL = if ($env:RK_BROWSER_PROBE_URL) {
    $env:RK_BROWSER_PROBE_URL
} else {
    "http://127.0.0.1:8791"
}

$browser = Start-Process `
    -FilePath "cmd.exe" `
    -ArgumentList "/c", "npm run dev" `
    -WorkingDirectory $browserDir `
    -PassThru `
    -WindowStyle Hidden

try {
    Start-Sleep -Seconds 2
    Push-Location $root
    cargo run -p reflection-api
}
finally {
    Pop-Location
    if ($browser -and -not $browser.HasExited) {
        Stop-Process -Id $browser.Id -Force -ErrorAction SilentlyContinue
    }
}
