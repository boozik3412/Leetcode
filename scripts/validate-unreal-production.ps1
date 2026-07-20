[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Workspace,
    [string]$EngineRoot = $env:UE_ENGINE_ROOT,
    [string]$TestFilter = "Project",
    [ValidatePattern('^[A-Za-z0-9_-]+$')]
    [string]$Platform = "Win64",
    [ValidatePattern('^[A-Za-z0-9_-]+$')]
    [string]$Configuration = "Development",
    [switch]$IncludePackage
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($TestFilter -notmatch '^[A-Za-z0-9._:+\- ]+$') {
    throw "Unsafe Automation test filter"
}

$workspacePath = (Resolve-Path -LiteralPath $Workspace).Path
$project = Get-ChildItem -LiteralPath $workspacePath -Filter *.uproject -File | Select-Object -First 1
if (-not $project) {
    throw "No .uproject found in $workspacePath"
}

if ([string]::IsNullOrWhiteSpace($EngineRoot)) {
    $standard = "C:\Program Files\Epic Games\UE_5.8"
    if (Test-Path -LiteralPath $standard) {
        $EngineRoot = $standard
    }
}
if ([string]::IsNullOrWhiteSpace($EngineRoot)) {
    throw "UE_ENGINE_ROOT is not configured"
}
$enginePath = (Resolve-Path -LiteralPath $EngineRoot).Path
$editorCmd = Join-Path $enginePath "Engine\Binaries\Win64\UnrealEditor-Cmd.exe"
$uat = Join-Path $enginePath "Engine\Build\BatchFiles\RunUAT.bat"
foreach ($tool in @($editorCmd, $uat)) {
    if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) {
        throw "Required Unreal tool not found: $tool"
    }
}

$outputRoot = Join-Path $workspacePath "assets\generated\leetcode\unreal\production"
$automationReport = Join-Path $outputRoot "automation"
$packageRoot = Join-Path $outputRoot "package"
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
New-Item -ItemType Directory -Path $automationReport -Force | Out-Null

$steps = [System.Collections.Generic.List[object]]::new()

function Invoke-ValidationStep {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [Parameter(Mandatory = $true)][string]$Program,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $logPath = Join-Path $outputRoot "$Id.log"
    $started = Get-Date
    $output = & $Program @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    $output | Set-Content -LiteralPath $logPath -Encoding UTF8
    $steps.Add([ordered]@{
        id = $Id
        ok = ($exitCode -eq 0)
        exit_code = $exitCode
        duration_ms = [math]::Round(((Get-Date) - $started).TotalMilliseconds)
        log = $logPath.Substring($workspacePath.Length).TrimStart('\').Replace('\', '/')
        command = $Program
        arguments = $Arguments
    })
    if ($exitCode -ne 0) {
        throw "Unreal step '$Id' failed with exit code $exitCode. See $logPath"
    }
}

$failure = $null
try {
    Invoke-ValidationStep -Id "automation" -Program $editorCmd -Arguments @(
        $project.FullName,
        "-Unattended",
        "-NoSplash",
        "-NullRHI",
        "-ExecCmds=Automation RunTest $TestFilter;Quit",
        "-TestExit=Automation Test Queue Empty",
        "-ReportExportPath=$automationReport",
        "-log",
        "-stdout",
        "-UTF8Output"
    )
    Invoke-ValidationStep -Id "cook" -Program $editorCmd -Arguments @(
        $project.FullName,
        "-run=Cook",
        "-TargetPlatform=$Platform",
        "-Unversioned",
        "-Unattended",
        "-NullRHI",
        "-stdout",
        "-UTF8Output"
    )
    if ($IncludePackage) {
        New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
        Invoke-ValidationStep -Id "package" -Program $uat -Arguments @(
            "BuildCookRun",
            "-project=$($project.FullName)",
            "-noP4",
            "-platform=$Platform",
            "-clientconfig=$Configuration",
            "-build",
            "-cook",
            "-stage",
            "-pak",
            "-package",
            "-archive",
            "-archivedirectory=$packageRoot",
            "-utf8output"
        )
    }
}
catch {
    $failure = $_.Exception.Message
}

$reportPath = Join-Path $workspacePath "assets\generated\leetcode\unreal\production-validation.json"
$report = [ordered]@{
    schema_version = 1
    created_at = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    workspace = $workspacePath
    project = $project.FullName
    engine_root = $enginePath
    platform = $Platform
    configuration = $Configuration
    test_filter = $TestFilter
    package_requested = [bool]$IncludePackage
    ok = ($null -eq $failure)
    error = $failure
    steps = $steps
    artifacts = @(
        if (Test-Path -LiteralPath $automationReport) { $automationReport.Substring($workspacePath.Length).TrimStart('\').Replace('\', '/') }
        if ($IncludePackage -and (Test-Path -LiteralPath $packageRoot)) { $packageRoot.Substring($workspacePath.Length).TrimStart('\').Replace('\', '/') }
    )
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding UTF8
Write-Output ($report | ConvertTo-Json -Depth 8)

if ($failure) {
    exit 1
}
