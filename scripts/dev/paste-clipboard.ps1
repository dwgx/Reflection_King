param(
    [string]$BaseUrl = "http://154.40.36.22:8780",
    [Parameter(Mandatory = $true)]
    [string]$Token,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Windows.Forms
$text = [System.Windows.Forms.Clipboard]::GetText()

if ($DryRun) {
    Write-Host (@{
        ok = $true
        baseUrl = $BaseUrl
        tokenPrefix = if ($Token.Length -gt 12) { $Token.Substring(0, 12) } else { $Token }
        textLength = $text.Length
    } | ConvertTo-Json -Depth 3)
    exit 0
}

if (-not $text.Trim()) {
    throw "Clipboard is empty."
}

$endpoint = "$($BaseUrl.TrimEnd('/'))/api/clipboard-paste-tokens/$Token/submit"
$body = @{ text = $text } | ConvertTo-Json -Depth 3
$response = Invoke-RestMethod -Method Post -Uri $endpoint -ContentType "application/json" -Body $body
Write-Host ($response | ConvertTo-Json -Depth 3)
