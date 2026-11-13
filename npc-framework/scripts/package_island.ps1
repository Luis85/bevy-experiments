<#
.SYNOPSIS
  Copies a built Island Village into a self-contained folder.

.DESCRIPTION
  Takes a Cargo profile directory that already holds the game executable
  (for example target\release) and writes:

    <OutDir>\game_menu.exe     the game (title, play, pause: one process)
    <OutDir>\assets\           models, textures, audio, fonts
    <OutDir>\LICENSE.txt       the repository licence
    <OutDir>\CREDITS.md        asset sources and licences
    <OutDir>\licenses\         the asset packs' own licence texts

  The executable finds `assets` beside itself at run time, so the folder can
  be moved or zipped. It does not build anything: run
  `cargo build --locked --release -p game_menu` first.

.EXAMPLE
  .\scripts\package_island.ps1 -ProfileDir target\release -OutDir dist\IslandVillage
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ProfileDir,
    [Parameter(Mandatory = $true)][string]$OutDir,
    # The executable to ship, as named in the profile directory.
    [string]$Executable = "game_menu.exe",
    # Replace an existing package folder.
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$repository = Split-Path -Parent $workspace

$game = Join-Path $ProfileDir $Executable
if (-not (Test-Path -LiteralPath $game -PathType Leaf)) {
    throw "No $Executable in $ProfileDir. Build it first: cargo build --locked --release -p game_menu"
}
$assets = Join-Path $workspace "assets"
if (-not (Test-Path -LiteralPath $assets -PathType Container)) {
    throw "No assets folder at $assets"
}
$credits = Join-Path $assets "CREDITS.md"
$packLicences = Join-Path $assets "licenses"
foreach ($required in @($credits, $packLicences)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "Missing ${required}: a package must carry its credits and licences"
    }
}
$licence = @((Join-Path $workspace "LICENSE"), (Join-Path $repository "LICENSE")) |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    Select-Object -First 1
if (-not $licence) {
    throw "No LICENSE file in $workspace or $repository"
}

if (Test-Path -LiteralPath $OutDir) {
    if (-not $Force) {
        throw "$OutDir already exists. Pass -Force to replace it."
    }
    # Only ever remove a folder this script made: it must look like a package.
    $entries = @(Get-ChildItem -LiteralPath $OutDir -Force)
    $isPackage = (Test-Path -LiteralPath (Join-Path $OutDir "assets")) -and
        (Test-Path -LiteralPath (Join-Path $OutDir $Executable))
    if ($entries.Count -gt 0 -and -not $isPackage) {
        throw "$OutDir exists and is not an Island Village package; refusing to replace it."
    }
    Remove-Item -LiteralPath $OutDir -Recurse -Force -Confirm:$false
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path -LiteralPath $OutDir).Path

Copy-Item -LiteralPath $game -Destination (Join-Path $OutDir $Executable)
Copy-Item -LiteralPath $assets -Destination (Join-Path $OutDir "assets") -Recurse
Copy-Item -LiteralPath $licence -Destination (Join-Path $OutDir "LICENSE.txt")
Copy-Item -LiteralPath $credits -Destination (Join-Path $OutDir "CREDITS.md")
Copy-Item -LiteralPath $packLicences -Destination (Join-Path $OutDir "licenses") -Recurse

$files = @(Get-ChildItem -LiteralPath $OutDir -Recurse -File)
$bytes = ($files | Measure-Object -Property Length -Sum).Sum
$assetBytes = (Get-ChildItem -LiteralPath (Join-Path $OutDir "assets") -Recurse -File |
    Measure-Object -Property Length -Sum).Sum
$exeBytes = (Get-Item -LiteralPath (Join-Path $OutDir $Executable)).Length

"Packaged $Executable from $ProfileDir"
"  folder      $OutDir"
"  files       $($files.Count)"
"  executable  {0:N1} MB" -f ($exeBytes / 1MB)
"  assets      {0:N1} MB" -f ($assetBytes / 1MB)
"  total       {0:N1} MB" -f ($bytes / 1MB)
