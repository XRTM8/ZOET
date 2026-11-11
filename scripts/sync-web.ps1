param(
    [string]$WebRoot = 'E:\PROJECTS\ZOET-Web',
    [string]$CacheSuffix = 'web2'
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$publicDir = Join-Path $projectRoot 'public'
$assetDir = Join-Path $projectRoot 'android\app\src\main\assets\public'

if (-not (Test-Path $WebRoot)) {
    throw "ZOET-Web target directory not found: $WebRoot"
}

# Keep Android assets in sync with public/
if (Test-Path $assetDir) {
    Copy-Item -Path (Join-Path $publicDir '*') -Destination $assetDir -Recurse -Force
}

# Detect shipped asset version from public/index.html (falls back to 2.9.6)
$publicIndex = Get-Content -Path (Join-Path $publicDir 'index.html') -Raw -Encoding UTF8
$verMatch = [regex]::Match($publicIndex, 'styles\.css\?v=([0-9]+\.[0-9]+\.[0-9]+)')
$version = if ($verMatch.Success) { $verMatch.Groups[1].Value } else { '2.9.6' }

# Mirror static web files while leaving ZOET-Web-specific files (.git, .github, .nojekyll,
# web-gateway.js, worker/, README.md, LICENSE) untouched.
foreach ($entry in Get-ChildItem -Path $publicDir) {
    $dest = Join-Path $WebRoot $entry.Name
    if ($entry.PSIsContainer) {
        if (Test-Path $dest) { Remove-Item -Path $dest -Recurse -Force }
        Copy-Item -Path $entry.FullName -Destination $dest -Recurse -Force
    } else {
        Copy-Item -Path $entry.FullName -Destination $dest -Force
    }
}

# Inject web-gateway.js before WASM initialization in ZOET-Web/index.html
$webIndex = $publicIndex
$gatewayTag = "<script src=`"web-gateway.js?v=$version`"></script>"
if ($webIndex -notmatch 'web-gateway\.js') {
    $cssTag = "<link rel=`"stylesheet`" href=`"styles.css?v=$version`">"
    if ($webIndex.Contains($cssTag)) {
        $webIndex = $webIndex.Replace($cssTag, "$cssTag`r`n  $gatewayTag")
    } else {
        $webIndex = $webIndex.Replace('</head>', "  $gatewayTag`r`n</head>")
    }
}
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText((Join-Path $WebRoot 'index.html'), $webIndex, $utf8NoBom)

# Include web-gateway.js in Service Worker shell cache and stamp cache revision
$swPath = Join-Path $WebRoot 'sw.js'
$swContent = Get-Content -Path $swPath -Raw -Encoding UTF8
$gatewayShellEntry = "'./web-gateway.js?v=$version'"
if ($swContent -notmatch 'web-gateway\.js') {
    $cssShellEntry = "'./styles.css?v=$version',"
    if ($swContent.Contains($cssShellEntry)) {
        $swContent = $swContent.Replace($cssShellEntry, "$cssShellEntry$gatewayShellEntry,")
    }
}
if ($CacheSuffix) {
    $swContent = [regex]::Replace($swContent, "const CACHE='zoet-shell-([^']+)';", "const CACHE='zoet-shell-$version-$CacheSuffix';")
}
[System.IO.File]::WriteAllText($swPath, $swContent, $utf8NoBom)

Write-Output "Synced public/ -> $WebRoot (version=$version, gateway injected, sw cache updated)."
