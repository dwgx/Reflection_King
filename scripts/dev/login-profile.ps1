param(
    [string]$BaseUrl = "http://154.40.36.22:8780",
    [string]$ProfileId = "admin_default",
    [ValidateSet("bilibili", "youtube", "douyin", "kuaishou", "pornhub", "acfun", "iqiyi", "youku")]
    [string]$Platform = "bilibili",
    [string]$ApiKey = "",
    [string]$ApiKeyFile = "",
    [string]$LoginToken = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Read-PlainTextSecret {
    param([string]$Prompt)
    $secure = Read-Host $Prompt -AsSecureString
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try {
        [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$browserDir = Join-Path $repoRoot "services\reflection-browser"
$nodeModules = Join-Path $browserDir "node_modules"
$localProfileRoot = Join-Path $repoRoot ".local\browser-profiles\$ProfileId\$Platform"
$nodeScript = Join-Path $PSScriptRoot "login-profile.mjs"

if (-not $ApiKey -and $ApiKeyFile) {
    $ApiKey = (Get-Content -LiteralPath $ApiKeyFile -Raw).Trim()
}
if (-not $ApiKey -and $env:RK_API_KEY) {
    $ApiKey = $env:RK_API_KEY.Trim()
}
if (-not $LoginToken -and $env:RK_LOGIN_TOKEN) {
    $LoginToken = $env:RK_LOGIN_TOKEN.Trim()
}
if (-not $ApiKey -and -not $LoginToken -and -not $DryRun) {
    $ApiKey = Read-PlainTextSecret "Admin API key"
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    throw "node was not found. Install Node.js or run scripts/dev/bootstrap.ps1 first."
}

if (-not (Test-Path $nodeModules)) {
    Push-Location $browserDir
    try {
        npm install
    } finally {
        Pop-Location
    }
}

New-Item -ItemType Directory -Force -Path $localProfileRoot | Out-Null

try {
    $env:RK_LOGIN_BASE_URL = $BaseUrl
    $env:RK_LOGIN_API_KEY = $ApiKey
    $env:RK_LOGIN_TOKEN = $LoginToken
    $env:RK_LOGIN_PROFILE_ID = $ProfileId
    $env:RK_LOGIN_PLATFORM = $Platform
    $env:RK_LOGIN_USER_DATA_DIR = $localProfileRoot
    $env:RK_LOGIN_PLAYWRIGHT = (Join-Path $browserDir "node_modules\playwright")
    if ($DryRun) {
        $env:RK_LOGIN_DRY_RUN = "1"
    } else {
        $env:RK_LOGIN_DRY_RUN = "0"
    }
    node $nodeScript
} finally {
    Remove-Item Env:\RK_LOGIN_BASE_URL -ErrorAction SilentlyContinue
    Remove-Item Env:\RK_LOGIN_API_KEY -ErrorAction SilentlyContinue
    Remove-Item Env:\RK_LOGIN_TOKEN -ErrorAction SilentlyContinue
    Remove-Item Env:\RK_LOGIN_PROFILE_ID -ErrorAction SilentlyContinue
    Remove-Item Env:\RK_LOGIN_PLATFORM -ErrorAction SilentlyContinue
    Remove-Item Env:\RK_LOGIN_USER_DATA_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:\RK_LOGIN_PLAYWRIGHT -ErrorAction SilentlyContinue
    Remove-Item Env:\RK_LOGIN_DRY_RUN -ErrorAction SilentlyContinue
}
