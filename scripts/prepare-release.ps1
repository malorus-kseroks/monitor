param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$Dist = "dist"
)

$ErrorActionPreference = "Stop"
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version must be semver" }

$root = Split-Path -Parent $PSScriptRoot
$distPath = Join-Path $root $Dist
New-Item -ItemType Directory -Path $distPath -Force | Out-Null

Push-Location $root
try {
    cargo build --release --locked
    cargo cyclonedx --format json --output-cdx

    $binary = Join-Path $root "target\release\kernox-monitor.exe"
    if (-not (Test-Path -LiteralPath $binary)) { throw "Windows release binary is missing" }
    $archive = Join-Path $distPath "kernox-monitor-$Version-x86_64-pc-windows-msvc.zip"
    Compress-Archive -LiteralPath $binary, "LICENSE", "README.md" -DestinationPath $archive -Force

    Get-FileHash -Algorithm SHA256 -LiteralPath $archive |
        ForEach-Object { "{0}  {1}" -f $_.Hash.ToLowerInvariant(), (Split-Path $_.Path -Leaf) } |
        Set-Content -LiteralPath (Join-Path $distPath "SHA256SUMS") -Encoding ascii

    if ($env:MINISIGN_SECRET_KEY_FILE) {
        minisign -S -s $env:MINISIGN_SECRET_KEY_FILE -m (Join-Path $distPath "SHA256SUMS")
    } else {
        Write-Warning "MINISIGN_SECRET_KEY_FILE is unset; artifacts were not signed"
    }
} finally {
    Pop-Location
}
