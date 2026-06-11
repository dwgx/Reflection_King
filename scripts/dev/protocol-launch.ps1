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
if ($uri.Host -ne "login") {
    throw "Unsupported reflection-king action: $($uri.Host)"
}

$query = Parse-QueryString $uri.Query
$baseUrl = $query["base"]
$profileId = $query["profile"]
$platform = $query["platform"]
$token = $query["token"]

if (-not $baseUrl -or -not $profileId -or -not $platform -or -not $token) {
    throw "reflection-king login URL is missing base/profile/platform/token"
}

$loginScript = Join-Path $PSScriptRoot "login-profile.ps1"
$args = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", $loginScript,
    "-BaseUrl", $baseUrl,
    "-ProfileId", $profileId,
    "-Platform", $platform,
    "-LoginToken", $token
)
if ($DryRun) {
    $args += "-DryRun"
}

Start-Process -FilePath "powershell.exe" -ArgumentList $args -WindowStyle Normal
