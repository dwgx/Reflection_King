param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$launcher = Join-Path $PSScriptRoot "protocol-launch.ps1"
if (-not (Test-Path -LiteralPath $launcher)) {
    throw "protocol-launch.ps1 was not found at $launcher"
}

$protocolKey = "HKCU:\Software\Classes\reflection-king"
$commandKey = Join-Path $protocolKey "shell\open\command"
$command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$launcher`" `"%1`""

if ((Test-Path $protocolKey) -and -not $Force) {
    $current = (Get-ItemProperty -LiteralPath $commandKey -ErrorAction SilentlyContinue)."(default)"
    if ($current -eq $command) {
        Write-Host "reflection-king:// protocol is already registered."
        exit 0
    }
}

New-Item -Path $protocolKey -Force | Out-Null
New-ItemProperty -Path $protocolKey -Name "(default)" -Value "URL:Reflection King Login Protocol" -Force | Out-Null
New-ItemProperty -Path $protocolKey -Name "URL Protocol" -Value "" -Force | Out-Null
New-Item -Path $commandKey -Force | Out-Null
New-ItemProperty -Path $commandKey -Name "(default)" -Value $command -Force | Out-Null

Write-Host "Registered reflection-king:// protocol for this Windows user."
Write-Host "Repo: $repoRoot"
