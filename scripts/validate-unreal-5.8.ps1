param(
    [Parameter(Mandatory = $true)]
    [string]$Workspace
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$resolvedWorkspace = (Resolve-Path -LiteralPath $Workspace).Path
$env:RUSTUP_HOME = Join-Path $repoRoot ".rustup"
$env:CARGO_HOME = Join-Path $repoRoot ".cargo"
$env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
$env:LEETCODE_UE_VALIDATION_WORKSPACE = $resolvedWorkspace

Push-Location $repoRoot
try {
    & cargo test validates_configured_unreal_5_8_workspace -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "Unreal Engine 5.8 validation failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
    Remove-Item Env:LEETCODE_UE_VALIDATION_WORKSPACE -ErrorAction SilentlyContinue
}
