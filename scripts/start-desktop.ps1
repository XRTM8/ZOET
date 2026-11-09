param(
    [int]$Port = 3000,
    [switch]$NoBrowser
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir

# Find server binary
$serverBin = Join-Path $repoRoot "rust\otium-core\target\release\otium-server.exe"
if (-not (Test-Path $serverBin)) {
    $serverBin = Join-Path $repoRoot "rust\otium-core\target\debug\otium-server.exe"
}

if (-not (Test-Path $serverBin)) {
    Write-Host "Building otium-server binary first..." -ForegroundColor Cyan
    & powershell -ExecutionPolicy Bypass -File (Join-Path $scriptDir "build-rust-ui.ps1")
    $localToolRoot = "E:\PROJECTS\ARGON\release\update-tools"
    if (Test-Path $localToolRoot) {
        $env:RUSTUP_HOME = Join-Path $localToolRoot "rustup"
        $env:CARGO_HOME = Join-Path $localToolRoot "cargo"
        $env:PATH = "$(Join-Path $localToolRoot 'mingw64\bin');$(Join-Path $localToolRoot 'cargo\bin');$env:PATH"
    }
    cargo build --bin otium-server --release --manifest-path (Join-Path $repoRoot "rust\otium-core\Cargo.toml")
    $serverBin = Join-Path $repoRoot "rust\otium-core\target\release\otium-server.exe"
}

$env:PORT = "$Port"
$env:ZOET_BIND = "127.0.0.1:$Port"
$env:ZOET_PUBLIC = Join-Path $repoRoot "public"

Write-Host "Starting ZOET Desktop Core Server on http://127.0.0.1:$Port..." -ForegroundColor Green
$serverJob = Start-Process -FilePath $serverBin -PassThru

Start-Sleep -Milliseconds 600

if (-not $NoBrowser) {
    # Check for modern browser to launch in App Mode
    $browserApp = $null
    $candidates = @(
        "C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        "C:\Program Files\Google\Chrome\Application\chrome.exe"
    )
    foreach ($cand in $candidates) {
        if (Test-Path $cand) {
            $browserApp = $cand
            break
        }
    }

    if ($browserApp) {
        Write-Host "Launching ZOET Desktop Window via $browserApp --app..." -ForegroundColor Cyan
        Start-Process -FilePath $browserApp -ArgumentList "--app=http://127.0.0.1:$Port", "--window-size=1320,840"
    } else {
        Start-Process "http://127.0.0.1:$Port"
    }
}

Write-Host "ZOET Desktop running. Press Ctrl+C or close this window to exit." -ForegroundColor Yellow
try {
    $serverJob.WaitForExit()
} finally {
    if (-not $serverJob.HasExited) {
        $serverJob.Kill()
    }
}
