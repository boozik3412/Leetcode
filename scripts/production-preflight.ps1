[CmdletBinding()]
param(
    [ValidateSet("Quick", "Release", "Live")]
    [string]$Mode = "Quick",
    [switch]$SkipTests,
    [switch]$SkipPackaging
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$reportRoot = Join-Path $repoRoot "assets\generated\leetcode\production-validation"
$reportPath = Join-Path $reportRoot "preflight-run.json"
New-Item -ItemType Directory -Path $reportRoot -Force | Out-Null
Set-Location $repoRoot

if (Test-Path -LiteralPath ".cargo\bin\cargo.exe") {
    $cargo = (Resolve-Path -LiteralPath ".cargo\bin\cargo.exe").Path
} else {
    $cargo = "cargo"
}

$steps = [System.Collections.Generic.List[object]]::new()
$stepStatus = @{}
$failed = $false

function Add-SkippedStep {
    param([string]$Id, [string]$Detail)
    $steps.Add([ordered]@{
        id = $Id
        status = "skipped"
        duration_ms = 0
        detail = $Detail
        output = @()
    })
    $script:stepStatus[$Id] = "skipped"
}

function Invoke-PreflightStep {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [Parameter(Mandatory = $true)][scriptblock]$Action
    )
    $started = Get-Date
    $captured = [System.Collections.Generic.List[string]]::new()
    try {
        & $Action | ForEach-Object {
            $line = $_.ToString()
            $captured.Add($line)
            $line
        } | Out-Null
        $steps.Add([ordered]@{
            id = $Id
            status = "passed"
            duration_ms = [math]::Round(((Get-Date) - $started).TotalMilliseconds)
            detail = "ok"
            output = @($captured | Select-Object -Last 80)
        })
        $script:stepStatus[$Id] = "passed"
    }
    catch {
        $script:failed = $true
        $steps.Add([ordered]@{
            id = $Id
            status = "failed"
            duration_ms = [math]::Round(((Get-Date) - $started).TotalMilliseconds)
            detail = $_.Exception.Message
            output = @($captured | Select-Object -Last 80)
        })
        $script:stepStatus[$Id] = "failed"
    }
}

function Invoke-Cargo {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    & $cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Test-UpdateManifest {
    param([Parameter(Mandatory = $true)][string]$ManifestPath)
    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        throw "Manifest not found: $ManifestPath"
    }
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
    foreach ($field in @("app", "version", "package", "sha256")) {
        if ([string]::IsNullOrWhiteSpace([string]$manifest.$field)) {
            throw "Manifest field '$field' is empty: $ManifestPath"
        }
    }
    if ([string]$manifest.sha256 -notmatch '^[a-fA-F0-9]{64}$') {
        throw "Manifest SHA256 is invalid: $ManifestPath"
    }
    if ([string]$manifest.package -match '^https?://') {
        return
    }
    $package = Join-Path (Split-Path -Parent $ManifestPath) ([string]$manifest.package)
    if (-not (Test-Path -LiteralPath $package -PathType Leaf)) {
        throw "Manifest package not found: $package"
    }
    $actualHash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne ([string]$manifest.sha256).ToLowerInvariant()) {
        throw "Manifest SHA256 mismatch for $package"
    }
    if ($null -ne $manifest.size_bytes -and [int64]$manifest.size_bytes -ne (Get-Item -LiteralPath $package).Length) {
        throw "Manifest size mismatch for $package"
    }
}

function Invoke-SmokeBinary {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Binary,
        [int]$TimeoutSeconds = 15
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Binary not found: $Path"
    }
    $stdout = [IO.Path]::GetTempFileName()
    $stderr = [IO.Path]::GetTempFileName()
    try {
        $process = Start-Process `
            -FilePath $Path `
            -ArgumentList "--production-smoke" `
            -RedirectStandardOutput $stdout `
            -RedirectStandardError $stderr `
            -PassThru `
            -WindowStyle Hidden
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            throw "$Binary production smoke timed out after $TimeoutSeconds seconds"
        }
        # Завершаем асинхронное чтение redirected stdout/stderr до проверки ExitCode.
        $process.WaitForExit()
        $process.Refresh()
        $output = Get-Content -LiteralPath $stdout -Raw
        $errorOutput = Get-Content -LiteralPath $stderr -Raw
        if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) {
            throw "$Binary production smoke failed with exit code $($process.ExitCode): $errorOutput"
        }
        if ([string]::IsNullOrWhiteSpace($output)) {
            throw "$Binary production smoke returned empty output"
        }
        $payload = $output | ConvertFrom-Json
        if (-not $payload.ok) {
            throw "$Binary returned an invalid smoke payload"
        }
        $output.Trim()
    }
    finally {
        Remove-Item -LiteralPath $stdout, $stderr -Force -ErrorAction SilentlyContinue
    }
}

Invoke-PreflightStep -Id "cargo-fmt" -Action {
    Invoke-Cargo fmt -- --check
}
Invoke-PreflightStep -Id "cargo-check" -Action {
    Invoke-Cargo check --all-targets
}
Invoke-PreflightStep -Id "stage45-game-task-constructor" -Action {
    foreach ($requiredPath in @(
        "src\game_task_builder.rs",
        "docs\GAME_TASK_CONSTRUCTOR.md",
        "scripts\unreal\export_asset_registry.py"
    )) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "Stage 45 contract file is missing: $requiredPath"
        }
    }
    Invoke-Cargo test --bin leetcode game_task_builder::tests::catalog_has_eight_by_ten_by_ten_operations
    Invoke-Cargo test --bin leetcode game_task_builder::tests::manifest_blocks_explicit_object_outside_confirmed_scope
    Invoke-Cargo test --bin leetcode setup_wizard
    "Stage 45 catalog and immutable target scope are valid"
}
if ($SkipTests) {
    Add-SkippedStep -Id "cargo-test" -Detail "SkipTests requested"
} else {
    Invoke-PreflightStep -Id "cargo-test" -Action {
        Invoke-Cargo test
    }
}

$releaseMode = $Mode -in @("Release", "Live")
if ($releaseMode) {
    Invoke-PreflightStep -Id "release-build" -Action {
        Invoke-Cargo build --release --bins
    }
    $binaryRoot = Join-Path $repoRoot "target\release"
    $buildStepId = "release-build"
} else {
    Invoke-PreflightStep -Id "debug-build" -Action {
        Invoke-Cargo build --bins
    }
    $binaryRoot = Join-Path $repoRoot "target\debug"
    $buildStepId = "debug-build"
}

if ($stepStatus[$buildStepId] -eq "passed") {
    foreach ($binary in @("leetcode.exe", "leetcode-client.exe", "leetcode-relay.exe")) {
        $id = "smoke-" + [IO.Path]::GetFileNameWithoutExtension($binary)
        $path = Join-Path $binaryRoot $binary
        Invoke-PreflightStep -Id $id -Action {
            Invoke-SmokeBinary -Path $path -Binary $binary
        }
    }
    Invoke-PreflightStep -Id "relay-health" -Action {
        $relayPath = Join-Path $binaryRoot "leetcode-relay.exe"
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
        $listener.Start()
        $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
        $listener.Stop()
        $relay = Start-Process -FilePath $relayPath -ArgumentList @("--bind", "127.0.0.1:$port") -PassThru -WindowStyle Hidden
        try {
            $health = $null
            for ($attempt = 0; $attempt -lt 30; $attempt++) {
                Start-Sleep -Milliseconds 100
                try {
                    $health = Invoke-RestMethod -Uri "http://127.0.0.1:$port/health" -TimeoutSec 2
                    break
                } catch {}
            }
            if ($null -eq $health -or -not $health.ok) {
                throw "Relay health endpoint did not become ready"
            }
            $health | ConvertTo-Json -Depth 6
        }
        finally {
            if (-not $relay.HasExited) {
                Stop-Process -Id $relay.Id
            }
        }
    }
} else {
    foreach ($id in @("smoke-leetcode", "smoke-leetcode-client", "smoke-leetcode-relay", "relay-health")) {
        Add-SkippedStep -Id $id -Detail "$buildStepId failed"
    }
}

if ($releaseMode -and -not $SkipPackaging) {
    Invoke-PreflightStep -Id "package-host" -Action {
        & (Join-Path $PSScriptRoot "package-windows.ps1")
        if ($LASTEXITCODE -ne 0) { throw "host packaging failed" }
    }
    Invoke-PreflightStep -Id "package-client" -Action {
        & (Join-Path $PSScriptRoot "package-client-windows.ps1")
        if ($LASTEXITCODE -ne 0) { throw "client packaging failed" }
    }
    Invoke-PreflightStep -Id "updater-manifests" -Action {
        Test-UpdateManifest -ManifestPath (Join-Path $repoRoot "dist\latest.json")
        Test-UpdateManifest -ManifestPath (Join-Path $repoRoot "dist\client-latest.json")
        "host and client update manifests are valid"
    }
} else {
    Add-SkippedStep -Id "packaging" -Detail "Use -Mode Release or -Mode Live without -SkipPackaging"
}

if ($Mode -eq "Live") {
    if ($env:LEETCODE_LIVE_PROVIDER_CONTRACTS -eq "1") {
        Invoke-PreflightStep -Id "live-providers" -Action {
            Invoke-Cargo test validates_configured_chat_providers_when_opted_in -- --ignored --nocapture
        }
    } else {
        Add-SkippedStep -Id "live-providers" -Detail "Set LEETCODE_LIVE_PROVIDER_CONTRACTS=1"
    }
    if ($env:LEETCODE_LIVE_MCP_CONTRACTS -eq "1" -and $env:LEETCODE_LIVE_MCP_WORKSPACE) {
        Invoke-PreflightStep -Id "live-mcp" -Action {
            Invoke-Cargo test validates_configured_mcp_servers_when_opted_in -- --ignored --nocapture
        }
    } else {
        Add-SkippedStep -Id "live-mcp" -Detail "Set LEETCODE_LIVE_MCP_CONTRACTS=1 and LEETCODE_LIVE_MCP_WORKSPACE"
    }
    if ($env:LEETCODE_UE_VALIDATION_WORKSPACE) {
        Invoke-PreflightStep -Id "live-unreal" -Action {
            & (Join-Path $PSScriptRoot "validate-unreal-production.ps1") -Workspace $env:LEETCODE_UE_VALIDATION_WORKSPACE -IncludePackage
            if ($LASTEXITCODE -ne 0) { throw "Unreal production validation failed" }
        }
    } else {
        Add-SkippedStep -Id "live-unreal" -Detail "Set LEETCODE_UE_VALIDATION_WORKSPACE"
    }
}

$passedCount = @($steps | Where-Object status -eq "passed").Count
$failedCount = @($steps | Where-Object status -eq "failed").Count
$skippedCount = @($steps | Where-Object status -eq "skipped").Count
$report = [ordered]@{
    schema_version = 1
    created_at = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    mode = $Mode.ToLowerInvariant()
    ok = (-not $failed)
    passed = $passedCount
    failed = $failedCount
    skipped = $skippedCount
    steps = $steps
}
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath -Encoding UTF8
Write-Output ($report | ConvertTo-Json -Depth 10)

if ($failed) {
    exit 1
}
