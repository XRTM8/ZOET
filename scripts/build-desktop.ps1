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

Write-Host "==> Step 2: Compiling Native otium-server Host Binary..." -ForegroundColor Cyan
cargo build --bin otium-server --release --manifest-path (Join-Path $repoRoot "rust\otium-core\Cargo.toml")

$distDir = Join-Path $repoRoot "dist\desktop"
if (-not (Test-Path $distDir)) {
    New-Item -ItemType Directory -Path $distDir -Force | Out-Null
}

$serverExe = Join-Path $repoRoot "rust\otium-core\target\release\otium-server.exe"
Copy-Item $serverExe -Destination (Join-Path $distDir "zoet-server.exe") -Force
Copy-Item (Join-Path $scriptDir "start-desktop.ps1") -Destination (Join-Path $distDir "start-zoet.ps1") -Force
Copy-Item -Recurse -Path (Join-Path $repoRoot "public") -Destination (Join-Path $distDir "public") -Force

Write-Host "==> Standalone ZOET Desktop bundle prepared at: $distDir" -ForegroundColor Green

if ($TauriBundle) {
    Write-Host "==> Step 3: Packaging standalone Tauri v2 installer..." -ForegroundColor Cyan
    Set-Location (Join-Path $repoRoot "src-tauri")
    cargo tauri build
    Set-Location $repoRoot
}

Write-Host "Desktop build complete!" -ForegroundColor Green
