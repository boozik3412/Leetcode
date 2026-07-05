param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Leetcode Client"),
    [switch]$KeepUserData
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
    throw "Refusing to uninstall outside LOCALAPPDATA\Programs or ProgramFiles. Got: $fullPath"
}

$installRoot = Assert-InstallPath $InstallDir

$shortcuts = @(
    (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Leetcode\Leetcode Client.lnk"),
    (Join-Path ([Environment]::GetFolderPath("Desktop")) "Leetcode Client.lnk")
)
foreach ($shortcut in $shortcuts) {
    if (Test-Path -LiteralPath $shortcut) {
        Remove-Item -LiteralPath $shortcut -Force
    }
}

$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Leetcode"
if (Test-Path -LiteralPath $startMenuDir) {
    $remaining = Get-ChildItem -LiteralPath $startMenuDir -Force
    if (-not $remaining) {
        Remove-Item -LiteralPath $startMenuDir -Force
    }
}

if (Test-Path -LiteralPath $installRoot) {
    Remove-Item -LiteralPath $installRoot -Recurse -Force
}

if (-not $KeepUserData) {
    Write-Host "Client config is kept by default in APPDATA/LOCALAPPDATA leetcode-client folders."
    Write-Host "Remove it manually only when you no longer need saved remote URL and token."
}

Write-Host "Leetcode Client uninstalled:"
Write-Host $installRoot
