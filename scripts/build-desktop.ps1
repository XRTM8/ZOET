param(
    [switch]$TauriBundle
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir

# Configure Toolchains
$localToolRoot = "E:\PROJECTS\ARGON\release\update-tools"
if (Test-Path $localToolRoot) {
    $env:RUSTUP_HOME = Join-Path $localToolRoot "rustup"
    $env:CARGO_HOME = Join-Path $localToolRoot "cargo"
    $env:PATH = "$(Join-Path $localToolRoot 'mingw64\bin');$(Join-Path $localToolRoot 'cargo\bin');$env:PATH"
}

Write-Host "==> Step 1: Building Rust WASM UI & Syncing Assets..." -ForegroundColor Cyan
& powershell -ExecutionPolicy Bypass -File (Join-Path $scriptDir "build-rust-ui.ps1")

Write-Host "==> Step 2: Compiling Standalone Native Desktop EXE (Tauri v2 + Embedded Assets)..." -ForegroundColor Cyan
cargo build --manifest-path (Join-Path $repoRoot "src-tauri\Cargo.toml") --release

$releaseOutDir = Join-Path $repoRoot "release-output"
if (-not (Test-Path $releaseOutDir)) {
    New-Item -ItemType Directory -Path $releaseOutDir -Force | Out-Null
}

$tauriExe = Join-Path $repoRoot "src-tauri\target\release\zoet-desktop.exe"
if (Test-Path $tauriExe) {
    Copy-Item $tauriExe -Destination (Join-Path $releaseOutDir "ZOET.exe") -Force
    Copy-Item $tauriExe -Destination (Join-Path $releaseOutDir "zoet-desktop.exe") -Force
    Write-Host "==> Standalone ZOET.exe ready at: $(Join-Path $releaseOutDir 'ZOET.exe')" -ForegroundColor Green
}

$distDir = Join-Path $repoRoot "dist\desktop"
if (-not (Test-Path $distDir)) {
    New-Item -ItemType Directory -Path $distDir -Force | Out-Null
}

if (Test-Path $tauriExe) {
    Copy-Item $tauriExe -Destination (Join-Path $distDir "ZOET.exe") -Force
}

# Also compile otium-server as fallback headless server
cargo build --bin otium-server --release --manifest-path (Join-Path $repoRoot "rust\otium-core\Cargo.toml")
$serverExe = Join-Path $repoRoot "rust\otium-core\target\release\otium-server.exe"
if (Test-Path $serverExe) {
    Copy-Item $serverExe -Destination (Join-Path $distDir "zoet-server.exe") -Force
    Copy-Item (Join-Path $scriptDir "start-desktop.ps1") -Destination (Join-Path $distDir "start-zoet.ps1") -Force
    Copy-Item -Recurse -Path (Join-Path $repoRoot "public") -Destination (Join-Path $distDir "public") -Force
}

Write-Host "==> Desktop bundle prepared at: $distDir" -ForegroundColor Green
Write-Host "Desktop build complete!" -ForegroundColor Green
