param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Url,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Parse-QueryString {
    param([string]$Query)
    $result = @{}
    if (-not $Query) {
        return $result
    }
    $trimmed = $Query.TrimStart("?")
    foreach ($part in $trimmed.Split("&", [System.StringSplitOptions]::RemoveEmptyEntries)) {
        $pair = $part.Split("=", 2)
        $key = [Uri]::UnescapeDataString($pair[0].Replace("+", " "))
        $value = ""
        if ($pair.Length -gt 1) {
            $value = [Uri]::UnescapeDataString($pair[1].Replace("+", " "))
        }
        $result[$key] = $value
    }
    return $result
}

$uri = [Uri]$Url
if ($uri.Scheme -ne "reflection-king") {
    throw "Unsupported protocol: $($uri.Scheme)"
}
$query = Parse-QueryString $uri.Query
$baseUrl = $query["base"]
$token = $query["token"]

switch ($uri.Host) {
    "login" {
        $profileId = $query["profile"]
        $platform = $query["platform"]
        if (-not $baseUrl -or -not $profileId -or -not $platform -or -not $token) {
            throw "reflection-king login URL is missing base/profile/platform/token"
        }
        $script = Join-Path $PSScriptRoot "login-profile.ps1"
        $args = @(
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", $script,
            "-BaseUrl", $baseUrl,
            "-ProfileId", $profileId,
            "-Platform", $platform,
            "-LoginToken", $token
        )
        if ($DryRun) {
            $args += "-DryRun"
        }
        Start-Process -FilePath "powershell.exe" -ArgumentList $args -WindowStyle Normal
    }
    "paste" {
        if (-not $baseUrl -or -not $token) {
            throw "reflection-king paste URL is missing base/token"
        }
        $script = Join-Path $PSScriptRoot "paste-clipboard.ps1"
        $args = @(
            "-STA",
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", $script,
            "-BaseUrl", $baseUrl,
            "-Token", $token
        )
        if ($DryRun) {
            $args += "-DryRun"
        }
        Start-Process -FilePath "powershell.exe" -ArgumentList $args -WindowStyle Hidden
    }
    default {
        throw "Unsupported reflection-king action: $($uri.Host)"
    }
}
