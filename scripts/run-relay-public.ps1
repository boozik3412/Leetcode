param(
    [string]$Bind = "0.0.0.0:17990",
    [string]$PublicUrl = $env:LEETCODE_RELAY_PUBLIC_URL,
    [ValidateSet("plain", "edge")]
    [string]$TlsMode = "edge",
    [int]$HostSessionTtlSecs = 30,
    [int]$ClientSessionTtlSecs = 900,
    [int]$ClientPollMs = 2000
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$candidates = @(
    (Join-Path $scriptDir "leetcode-relay.exe"),
    (Join-Path $repoRoot "target\release\leetcode-relay.exe"),
    (Join-Path $repoRoot "target\debug\leetcode-relay.exe")
)

$relayExe = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $relayExe) {
    throw "leetcode-relay.exe not found. Build it with: cargo build --release --bin leetcode-relay"
}

if ([string]::IsNullOrWhiteSpace($PublicUrl)) {
    $PublicUrl = "http://$Bind"
}

Write-Host "Starting Leetcode Relay"
Write-Host "  Bind:       $Bind"
Write-Host "  Public URL: $PublicUrl"
Write-Host "  TLS mode:   $TlsMode"
Write-Host ""
Write-Host "For public HTTPS/WSS, terminate TLS at Cloudflare, Caddy, Nginx, or Tailscale Funnel"
Write-Host "and point it to this local HTTP relay."

& $relayExe `
    --bind $Bind `
    --public-url $PublicUrl `
    --tls-mode $TlsMode `
    --host-session-ttl-secs $HostSessionTtlSecs `
    --client-session-ttl-secs $ClientSessionTtlSecs `
    --client-poll-ms $ClientPollMs
