[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Package,
    [switch]$NativeCapture
)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$packagePath = (Resolve-Path -LiteralPath $Package).Path
$verificationRoot = Join-Path $workspace ('target/playtest-verification/' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $verificationRoot | Out-Null
if ([IO.Path]::GetExtension($packagePath) -eq '.zip') {
    $bundle = Join-Path $verificationRoot 'unpacked'
    New-Item -ItemType Directory -Path $bundle | Out-Null
    $archive = [IO.Compression.ZipFile]::OpenRead($packagePath)
    try {
        foreach ($entry in $archive.Entries) {
            $entryPath = [IO.Path]::GetFullPath((Join-Path $bundle $entry.FullName))
            if (-not $entryPath.StartsWith($bundle + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe archive path: $($entry.FullName)" }
        }
    } finally { $archive.Dispose() }
    Expand-Archive -LiteralPath $packagePath -DestinationPath $bundle
} else { $bundle = $packagePath }
$manifest = Get-Content -LiteralPath (Join-Path $bundle 'manifest.json') -Raw | ConvertFrom-Json
foreach ($entry in $manifest.files) {
    $file = [IO.Path]::GetFullPath((Join-Path $bundle $entry.path))
    if (-not $file.StartsWith($bundle + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Manifest path escaped package: $($entry.path)" }
    if ((Get-Item -LiteralPath $file).Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw "Package integrity failed: $($entry.path)" }
}
if (@(Get-ChildItem -LiteralPath $bundle -File -Recurse).Count -ne @($manifest.files).Count + 1) { throw 'Package contains unexpected files' }
$variables = @('LASTLIGHT_ASSET_DIR','LASTLIGHT_WORLD','LASTLIGHT_CONTENT','LASTLIGHT_SKILLS','LASTLIGHT_ENEMIES','LASTLIGHT_ORDNANCE','LASTLIGHT_PROGRESSION','LASTLIGHT_QUALITY','LASTLIGHT_BALANCE','LASTLIGHT_SAVE_DIR','LASTLIGHT_REVIEW_SCREEN','LASTLIGHT_REVIEW_CAPTURE','LASTLIGHT_REVIEW_EXIT','LASTLIGHT_PRACTICE','LASTLIGHT_PERF_REPORT','LASTLIGHT_CAPTURE_SEQUENCE','LASTLIGHT_REVIEW_MOTION','LASTLIGHT_REVIEW_REDUCED')
$previous = @{}
foreach ($name in $variables) { $previous[$name] = [Environment]::GetEnvironmentVariable($name,'Process') }
try {
    $env:LASTLIGHT_ASSET_DIR = Join-Path $bundle 'assets'
    $env:LASTLIGHT_WORLD = Join-Path $bundle 'content/world.json'
    $env:LASTLIGHT_CONTENT = Join-Path $bundle 'content/encounters.json'
    $env:LASTLIGHT_SKILLS = Join-Path $bundle 'content/skills.json'
    $env:LASTLIGHT_ENEMIES = Join-Path $bundle 'content/enemy_catalog.json'
    $env:LASTLIGHT_ORDNANCE = Join-Path $bundle 'content/ordnance.json'
    $env:LASTLIGHT_PROGRESSION = Join-Path $bundle 'content/progression.json'
    $env:LASTLIGHT_QUALITY = Join-Path $bundle 'content/quality.json'
    $env:LASTLIGHT_BALANCE = Join-Path $bundle 'content/balance.json'
    $env:LASTLIGHT_SAVE_DIR = Join-Path $verificationRoot 'saves'
    foreach ($name in @('LASTLIGHT_REVIEW_SCREEN','LASTLIGHT_REVIEW_CAPTURE','LASTLIGHT_REVIEW_EXIT','LASTLIGHT_PRACTICE','LASTLIGHT_PERF_REPORT','LASTLIGHT_CAPTURE_SEQUENCE','LASTLIGHT_REVIEW_MOTION','LASTLIGHT_REVIEW_REDUCED')) { [Environment]::SetEnvironmentVariable($name,$null,'Process') }
    & (Join-Path $bundle 'Lastlight.exe') --smoke
    if ($LASTEXITCODE -ne 0) { throw 'Copied executable campaign smoke failed' }
    if ($NativeCapture) {
        $env:LASTLIGHT_REVIEW_SCREEN = 'peak'
        $env:LASTLIGHT_REVIEW_EXIT = '1'
        $env:LASTLIGHT_REVIEW_CAPTURE = Join-Path $verificationRoot 'portable-peak.png'
        & (Join-Path $bundle 'Lastlight.exe')
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $env:LASTLIGHT_REVIEW_CAPTURE)) { throw 'Copied executable native capture failed' }
    }
    Write-Output "PASS: $(@($manifest.files).Count) manifest hashes; copied executable smoke; native capture=$NativeCapture"
    Write-Output $verificationRoot
} finally {
    foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process') }
}
