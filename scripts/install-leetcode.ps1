param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Leetcode"),
    [switch]$NoShortcuts,
    [switch]$NoDesktopShortcut
)

$ErrorActionPreference = "Stop"

function Get-FullPath([string]$Path) {
    [System.IO.Path]::GetFullPath($Path)
}

function Test-PathInsideRoot([string]$Path, [string]$Root) {
    $fullPath = (Get-FullPath $Path).TrimEnd('\')
    $fullRoot = (Get-FullPath $Root).TrimEnd('\')
    $fullPath.Equals($fullRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        $fullPath.StartsWith("$fullRoot\", [System.StringComparison]::OrdinalIgnoreCase)
}

function Assert-InstallPath([string]$Path) {
    $fullPath = Get-FullPath $Path
    $allowedRoots = @(
        (Get-FullPath (Join-Path $env:LOCALAPPDATA "Programs")),
        (Get-FullPath $env:ProgramFiles)
    )
    foreach ($root in $allowedRoots) {
        if (Test-PathInsideRoot $fullPath $root) {
            return $fullPath
        }
    }
    throw "InstallDir must be inside LOCALAPPDATA\Programs or ProgramFiles. Got: $fullPath"
}

function New-LeetcodeShortcut([string]$Path, [string]$Target, [string]$WorkingDirectory) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    $shortcut.TargetPath = $Target
    $shortcut.WorkingDirectory = $WorkingDirectory
    $shortcut.IconLocation = $Target
    $shortcut.Save()
}

$sourceRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$exeSource = Join-Path $sourceRoot "leetcode.exe"
if (-not (Test-Path -LiteralPath $exeSource)) {
    throw "leetcode.exe was not found next to this installer script: $exeSource"
}

$installRoot = Assert-InstallPath $InstallDir
New-Item -ItemType Directory -Force -Path $installRoot | Out-Null

Get-ChildItem -LiteralPath $sourceRoot -Force | ForEach-Object {
    if ($_.Name -in @("target", "dist", ".git", ".cargo", ".rustup")) {
        return
    }
    Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $installRoot $_.Name) -Recurse -Force
}

$exeTarget = Join-Path $installRoot "leetcode.exe"
if (-not $NoShortcuts) {
    $startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Leetcode"
    New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
    New-LeetcodeShortcut -Path (Join-Path $startMenuDir "Leetcode.lnk") -Target $exeTarget -WorkingDirectory $installRoot

    if (-not $NoDesktopShortcut) {
        $desktop = [Environment]::GetFolderPath("Desktop")
        New-LeetcodeShortcut -Path (Join-Path $desktop "Leetcode.lnk") -Target $exeTarget -WorkingDirectory $installRoot
    }
}

$packageInfoPath = Join-Path $installRoot "package-info.json"
$version = "unknown"
if (Test-Path -LiteralPath $packageInfoPath) {
    try {
        $version = (Get-Content -Raw -LiteralPath $packageInfoPath | ConvertFrom-Json).version
    } catch {
        $version = "unknown"
    }
}

$metadata = [ordered]@{
    app = "Leetcode"
    version = $version
    install_dir = $installRoot
    installed_at = (Get-Date).ToUniversalTime().ToString("o")
}
$metadata | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 -Path (Join-Path $installRoot "install.json")

Write-Host "Leetcode installed:"
Write-Host $installRoot
