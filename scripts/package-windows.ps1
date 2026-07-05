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
$manifestPath = Join-Path $distRoot "latest.json"
$exeSource = Join-Path $repoRoot "target\release\leetcode.exe"
$exeTarget = Join-Path $packageRoot "leetcode.exe"
$cargoToml = Get-Content -Raw -Path (Join-Path $repoRoot "Cargo.toml")
$versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
$version = if ($versionMatch.Success) { $versionMatch.Groups[1].Value } else { "0.0.0" }

Set-Location $repoRoot

if (Test-Path ".cargo\bin\cargo.exe") {
    $env:CARGO_HOME = Join-Path $repoRoot ".cargo"
    $env:RUSTUP_HOME = Join-Path $repoRoot ".rustup"
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}

cargo +stable build --release
if ($LASTEXITCODE -ne 0) {
    Write-Warning "cargo +stable build failed, retrying with the default cargo toolchain."
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

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

foreach ($file in @("install-leetcode.ps1", "uninstall-leetcode.ps1")) {
    $source = Join-Path $PSScriptRoot $file
    if (Test-Path $source) {
        Copy-Item -Force $source (Join-Path $packageRoot $file)
    }
}

$assetDir = Join-Path $repoRoot "assets"
if (Test-Path $assetDir) {
    Copy-Item -Recurse -Force $assetDir (Join-Path $packageRoot "assets")
}

$packageInfo = [ordered]@{
    schema_version = 1
    app = "Leetcode"
    version = $version
    channel = "stable"
    platform = "windows-x64"
    packaged_at = (Get-Date).ToUniversalTime().ToString("o")
}
$packageInfo | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 -Path (Join-Path $packageRoot "package-info.json")

if (-not $SkipArchive) {
    if (Test-Path $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    Compress-Archive -Path (Join-Path $packageRoot "*") -DestinationPath $archivePath -CompressionLevel Optimal
    $hash = Get-FileHash -Algorithm SHA256 -Path $archivePath
    "$($hash.Hash)  $(Split-Path -Leaf $archivePath)" | Set-Content -Encoding ASCII -Path $hashPath

    $manifest = [ordered]@{
        schema_version = 1
        app = "Leetcode"
        version = $version
        channel = "stable"
        platform = "windows-x64"
        package = Split-Path -Leaf $archivePath
        sha256 = $hash.Hash
        size_bytes = (Get-Item -LiteralPath $archivePath).Length
        installer = "install-leetcode.ps1"
        uninstaller = "uninstall-leetcode.ps1"
        published_at = (Get-Date).ToUniversalTime().ToString("o")
    }
    $manifest | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 -Path $manifestPath
}

Write-Host "Packaged Leetcode portable build:"
Write-Host $packageRoot
if (-not $SkipArchive) {
    Write-Host "Archive:"
    Write-Host $archivePath
    Write-Host "SHA256:"
    Write-Host $hashPath
    Write-Host "Update manifest:"
    Write-Host $manifestPath
}
