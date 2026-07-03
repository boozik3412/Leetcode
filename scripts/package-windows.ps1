param(
    [switch]$SkipArchive,
    [switch]$Sign,
    [string]$CertificateThumbprint = $env:LEETCODE_SIGN_CERT_THUMBPRINT,
    [string]$TimestampServer = "http://timestamp.digicert.com",
    [string]$SignToolPath = $env:SIGNTOOL_PATH
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$distRoot = Join-Path $repoRoot "dist"
$packageRoot = Join-Path $distRoot "leetcode-portable"
$archivePath = Join-Path $distRoot "leetcode-portable.zip"
$hashPath = Join-Path $distRoot "leetcode-portable.sha256.txt"
$exeSource = Join-Path $repoRoot "target\release\leetcode.exe"
$exeTarget = Join-Path $packageRoot "leetcode.exe"

Set-Location $repoRoot

if (Test-Path ".cargo\bin\cargo.exe") {
    $env:CARGO_HOME = Join-Path $repoRoot ".cargo"
    $env:RUSTUP_HOME = Join-Path $repoRoot ".rustup"
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}

cargo +stable build --release

$distRootFull = [System.IO.Path]::GetFullPath($distRoot)
$packageRootFull = [System.IO.Path]::GetFullPath($packageRoot)
if (-not $packageRootFull.StartsWith($distRootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to package outside dist: $packageRootFull"
}

if (Test-Path $packageRoot) {
    Remove-Item -LiteralPath $packageRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
Copy-Item -Force $exeSource $exeTarget

if ($Sign) {
    if ([string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
        throw "Signing requested, but CertificateThumbprint is empty. Set LEETCODE_SIGN_CERT_THUMBPRINT or pass -CertificateThumbprint."
    }
    if ([string]::IsNullOrWhiteSpace($SignToolPath)) {
        $SignToolPath = "signtool.exe"
    }
    & $SignToolPath sign /fd SHA256 /sha1 $CertificateThumbprint /tr $TimestampServer /td SHA256 $exeTarget
}

foreach ($file in @("README.md", "BACKLOG.md", "PACKAGING.md", "run-leetcode.cmd")) {
    $source = Join-Path $repoRoot $file
    if (Test-Path $source) {
        Copy-Item -Force $source (Join-Path $packageRoot $file)
    }
}

$assetDir = Join-Path $repoRoot "assets"
if (Test-Path $assetDir) {
    Copy-Item -Recurse -Force $assetDir (Join-Path $packageRoot "assets")
}

if (-not $SkipArchive) {
    if (Test-Path $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    Compress-Archive -Path (Join-Path $packageRoot "*") -DestinationPath $archivePath -CompressionLevel Optimal
    $hash = Get-FileHash -Algorithm SHA256 -Path $archivePath
    "$($hash.Hash)  $(Split-Path -Leaf $archivePath)" | Set-Content -Encoding ASCII -Path $hashPath
}

Write-Host "Packaged Leetcode portable build:"
Write-Host $packageRoot
if (-not $SkipArchive) {
    Write-Host "Archive:"
    Write-Host $archivePath
    Write-Host "SHA256:"
    Write-Host $hashPath
}
