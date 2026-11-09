$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$manifestPath = Join-Path $projectRoot 'rust/otium-ui/Cargo.toml'

$localToolRoot = "E:\PROJECTS\ARGON\release\update-tools"
if (Test-Path "$localToolRoot\cargo\bin\cargo.exe") {
    $env:RUSTUP_HOME = "$localToolRoot\rustup"
    $env:CARGO_HOME = "$localToolRoot\cargo"
    $env:PATH = "$localToolRoot\mingw64\bin;$localToolRoot\cargo\bin;$env:PATH"
}
Push-Location $projectRoot
try {
    cargo build --manifest-path $manifestPath --target wasm32-unknown-unknown --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust UI compilation failed' }
    $wasmPath = Join-Path $projectRoot 'rust/otium-ui/target/wasm32-unknown-unknown/release/otium_ui.wasm'
    $outputPath = Join-Path $projectRoot 'public/wasm'
    wasm-bindgen $wasmPath --out-dir $outputPath --target web --no-typescript
    if ($LASTEXITCODE -ne 0) { throw 'WebAssembly binding generation failed' }
    $assetPath = Join-Path $projectRoot 'android/app/src/main/assets/public'
    if (-not (Test-Path $assetPath)) { New-Item -ItemType Directory -Path $assetPath | Out-Null }
    Copy-Item -Path (Join-Path $projectRoot 'public/*') -Destination $assetPath -Recurse -Force
    Write-Output 'Rust UI built and Android assets synchronized.'
} finally { Pop-Location }
