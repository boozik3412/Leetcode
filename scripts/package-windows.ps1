$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$distRoot = Join-Path $repoRoot "dist"
$packageRoot = Join-Path $distRoot "leetcode-portable"
$exeSource = Join-Path $repoRoot "target\release\leetcode.exe"
$exeTarget = Join-Path $packageRoot "leetcode.exe"

Set-Location $repoRoot

if (Test-Path ".cargo\bin\cargo.exe") {
    $env:CARGO_HOME = Join-Path $repoRoot ".cargo"
    $env:RUSTUP_HOME = Join-Path $repoRoot ".rustup"
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}

cargo +stable build --release

New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
Copy-Item -Force $exeSource $exeTarget

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

Write-Host "Packaged Leetcode portable build:"
Write-Host $packageRoot
