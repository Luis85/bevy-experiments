#requires -Version 5.1
# Checks a playtest package (the zip or its unpacked folder) from scripts/package_playtest.ps1 under Windows
# PowerShell 5.1. Run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify_playtest.ps1 -Package <zip>
# In order: zip entry safety, unpack, manifest file integrity, the manifest v2 rules (commit, game version, curated
# docs), the packed executable's --version against the manifest stamp, then its --smoke. -NativeCapture also takes
# one review still; it needs a package built with -Review and is refused otherwise, from the manifest before the
# package is unpacked or anything is launched.
# Prints 'PASS: ...' and the verification folder; any failure throws (exit code 1).
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Package,
    [switch]$NativeCapture,
    [int]$SmokeTimeoutSeconds = 900,
    [int]$CaptureTimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
. (Join-Path $PSScriptRoot 'lib\Playtest.ps1')

$workspace = Split-Path -Parent $PSScriptRoot
$packagePath = (Resolve-Path -LiteralPath $Package).ProviderPath
$verificationRoot = Join-Path (Resolve-HordeTarget -Workspace $workspace) ('playtest-verification\' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $verificationRoot | Out-Null
$logs = Join-Path $verificationRoot 'logs'
# A build without the review feature ignores the capture variables and opens the ordinary game, which never exits.
$noReview = 'This package has no review feature, so -NativeCapture cannot run (build it with package_playtest.ps1 -Review)'
# Refused from the manifest alone, before a 50 MB zip is unpacked and hashed; the check after the integrity pass stays.
if ($NativeCapture) {
    $declared = Get-PlaytestPackageFeature -Package $packagePath
    if ($null -ne $declared -and @($declared) -notcontains 'review') { throw $noReview }
}

if ([IO.Path]::GetExtension($packagePath) -eq '.zip') {
    $bundle = Join-Path $verificationRoot 'unpacked'
    # Throws on a backslash, rooted or parent entry before anything is written.
    Expand-PlaytestZip -Path $packagePath -Destination $bundle
}
else { $bundle = $packagePath.TrimEnd('\', '/') }

$manifestPath = Join-Path $bundle 'manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "No manifest.json in $bundle" }
$manifest = [IO.File]::ReadAllText($manifestPath) | ConvertFrom-Json
$integrity = @(Test-PackageIntegrity -Manifest $manifest -PackageRoot $bundle)
if ($integrity.Count -gt 0) { throw "Package integrity failed:`n$($integrity -join "`n")" }
$rules = @(Test-PlaytestManifest -Manifest $manifest -PackageRoot $bundle -GameVersion (Get-WorkspaceVersion -Workspace $workspace))
if ($rules.Count -gt 0) { throw "Package manifest failed:`n$($rules -join "`n")" }
$features = @($manifest.features | ForEach-Object { [string]$_ })
if ($NativeCapture -and $features -notcontains 'review') { throw $noReview }

$game = Join-Path $bundle 'Lastlight.exe'
$saved = Reset-LastlightEnv
try {
    $env:LASTLIGHT_ASSET_DIR = Join-Path $bundle 'assets'
    $env:LASTLIGHT_WORLD = Join-Path $bundle 'content\world.json'
    $env:LASTLIGHT_CONTENT = Join-Path $bundle 'content\encounters.json'
    $env:LASTLIGHT_SKILLS = Join-Path $bundle 'content\skills.json'
    $env:LASTLIGHT_ENEMIES = Join-Path $bundle 'content\enemy_catalog.json'
    $env:LASTLIGHT_ORDNANCE = Join-Path $bundle 'content\ordnance.json'
    $env:LASTLIGHT_PROGRESSION = Join-Path $bundle 'content\progression.json'
    $env:LASTLIGHT_QUALITY = Join-Path $bundle 'content\quality.json'
    $env:LASTLIGHT_BALANCE = Join-Path $bundle 'content\balance.json'
    $env:LASTLIGHT_SAVE_DIR = Join-Path $verificationRoot 'saves'

    $version = Invoke-Native -FilePath $game -Arguments @('--version') -LogStem (Join-Path $logs 'version') -TimeoutSeconds 30 -WorkingDirectory $bundle
    $versionLine = [IO.File]::ReadAllText($version.Out).Trim()
    if ($version.ExitCode -ne 0 -or -not $versionLine.Contains([string]$manifest.stamp)) {
        throw "Copied executable reports '$versionLine' (exit $($version.ExitCode)); the manifest stamp is '$($manifest.stamp)'"
    }

    $smoke = Invoke-Native -FilePath $game -Arguments @('--smoke') -LogStem (Join-Path $logs 'smoke') -TimeoutSeconds $SmokeTimeoutSeconds -WorkingDirectory $bundle
    $smokeLine = @([IO.File]::ReadAllLines($smoke.Out) | Select-Object -First 1)
    if ($smoke.ExitCode -ne 0 -or $smokeLine.Count -eq 0 -or $smokeLine[0] -cnotmatch '^PASS: ') {
        throw "Copied executable campaign smoke failed (exit $($smoke.ExitCode)); see $($smoke.Out) and $($smoke.Err)"
    }

    if ($NativeCapture) {
        $capture = Join-Path $verificationRoot 'portable-peak.png'
        $env:LASTLIGHT_REVIEW_SCREEN = 'peak'
        $env:LASTLIGHT_REVIEW_EXIT = '1'
        $env:LASTLIGHT_REVIEW_SIZE = '1440x900'
        $env:LASTLIGHT_REVIEW_CAPTURE = $capture
        $still = Invoke-Native -FilePath $game -LogStem (Join-Path $logs 'capture') -TimeoutSeconds $CaptureTimeoutSeconds -WorkingDirectory $bundle -RemoveReviewProfile
        if ($still.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $capture -PathType Leaf)) {
            throw "Copied executable native capture failed (exit $($still.ExitCode)); see $($still.Err)"
        }
    }
    Write-Output "PASS: $(@($manifest.files).Count) manifest hashes; $($manifest.stamp); copied executable smoke; native capture=$NativeCapture"
    Write-Output $verificationRoot
}
finally { Restore-LastlightEnv -Saved $saved }
