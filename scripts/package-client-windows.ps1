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
$packageRoot = Join-Path $distRoot "leetcode-client-portable"
$archivePath = Join-Path $distRoot "leetcode-client-portable.zip"
$hashPath = Join-Path $distRoot "leetcode-client-portable.sha256.txt"
$manifestPath = Join-Path $distRoot "client-latest.json"
$exeSource = Join-Path $repoRoot "target\release\leetcode-client.exe"
$exeTarget = Join-Path $packageRoot "leetcode-client.exe"
$relaySource = Join-Path $repoRoot "target\release\leetcode-relay.exe"
$relayTarget = Join-Path $packageRoot "leetcode-relay.exe"
$cargoToml = Get-Content -Raw -Path (Join-Path $repoRoot "Cargo.toml")
$versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
$version = if ($versionMatch.Success) { $versionMatch.Groups[1].Value } else { "0.0.0" }

Set-Location $repoRoot

if (Test-Path ".cargo\bin\cargo.exe") {
    $env:CARGO_HOME = Join-Path $repoRoot ".cargo"
    $env:RUSTUP_HOME = Join-Path $repoRoot ".rustup"
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}

cargo +stable build --release --bin leetcode-client
if ($LASTEXITCODE -ne 0) {
    Write-Warning "cargo +stable build failed, retrying with the default cargo toolchain."
    cargo build --release --bin leetcode-client
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

cargo +stable build --release --bin leetcode-relay
if ($LASTEXITCODE -ne 0) {
    Write-Warning "cargo +stable build leetcode-relay failed, retrying with the default cargo toolchain."
    cargo build --release --bin leetcode-relay
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
Copy-Item -Force $relaySource $relayTarget

if ($Sign) {
    if ([string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
        throw "Signing requested, but CertificateThumbprint is empty. Set LEETCODE_SIGN_CERT_THUMBPRINT or pass -CertificateThumbprint."
    }
    if ([string]::IsNullOrWhiteSpace($SignToolPath)) {
        $SignToolPath = "signtool.exe"
    }
    & $SignToolPath sign /fd SHA256 /sha1 $CertificateThumbprint /tr $TimestampServer /td SHA256 $exeTarget
    & $SignToolPath sign /fd SHA256 /sha1 $CertificateThumbprint /tr $TimestampServer /td SHA256 $relayTarget
}

foreach ($file in @("README.md", "PACKAGING.md")) {
    $source = Join-Path $repoRoot $file
    if (Test-Path $source) {
        Copy-Item -Force $source (Join-Path $packageRoot $file)
    }
}

foreach ($file in @("install-leetcode-client.ps1", "uninstall-leetcode-client.ps1")) {
    $source = Join-Path $PSScriptRoot $file
    if (Test-Path $source) {
        Copy-Item -Force $source (Join-Path $packageRoot $file)
    }
}

foreach ($file in @("run-relay-public.ps1")) {
    $source = Join-Path $PSScriptRoot $file
    if (Test-Path $source) {
        Copy-Item -Force $source (Join-Path $packageRoot $file)
    }
}

$packageInfo = [ordered]@{
    schema_version = 1
    app = "Leetcode Client"
    version = $version
    channel = "stable"
    platform = "windows-x64"
    binary = "leetcode-client.exe"
    relay_binary = "leetcode-relay.exe"
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
        app = "Leetcode Client"
        version = $version
        channel = "stable"
        platform = "windows-x64"
        package = Split-Path -Leaf $archivePath
        sha256 = $hash.Hash
        size_bytes = (Get-Item -LiteralPath $archivePath).Length
        installer = "install-leetcode-client.ps1"
        uninstaller = "uninstall-leetcode-client.ps1"
        published_at = (Get-Date).ToUniversalTime().ToString("o")
        signature_algorithm = if ($Sign) { "windows-authenticode+sha256" } else { "sha256" }
        signature = if ($Sign) { "authenticode-signed-binaries" } else { "" }
        rollout_percent = 100
        rollout_seed = $version
        rollback_version = ""
        rollback_package = ""
        rollback_sha256 = ""
        minimum_supported_version = "0.1.0"
        notes = "Stable Leetcode thin client update."
    }
    $manifest | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 -Path $manifestPath
}

Write-Host "Packaged Leetcode Client portable build:"
Write-Host $packageRoot
if (-not $SkipArchive) {
    Write-Host "Archive:"
    Write-Host $archivePath
    Write-Host "SHA256:"
    Write-Host $hashPath
    Write-Host "Client update manifest:"
    Write-Host $manifestPath
}
