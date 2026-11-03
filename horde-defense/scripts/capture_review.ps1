# Native framebuffer evidence; every fixture uses an isolated temporary campaign.
[CmdletBinding()]
param(
    [string]$OutputDirectory = "target/playtest-review/screenshots",
    [switch]$NoBuild,
    [string[]]$Scenes = @('menu', 'menu-female', 'survivor-select', 'battle-female', 'progressed-battle', 'leveled-armory', 'quality-armory', 'quality-loot', 'reticle-reload', 'damage', 'damage-4x', 'critical-npc', 'progressed-peak', 'world', 'campaign', 'briefing', 'briefing-npc', 'intro', 'battle', 'npc', 'npc-expanded', 'boss', 'construction', 'character', 'character-max', 'survivor', 'armory', 'skills', 'skills-survival', 'skills-ordnance', 'ordnance', 'engineering', 'results', 'operations', 'settings', 'history', 'history-empty', 'arid', 'tropical', 'peak', 'active-camp'),
    [string[]]$SmallScenes = @('menu', 'menu-female', 'survivor-select', 'battle-female', 'progressed-battle', 'leveled-armory', 'quality-armory', 'quality-loot', 'reticle-reload', 'damage', 'damage-4x', 'critical-npc', 'progressed-peak', 'world', 'campaign', 'briefing', 'briefing-npc', 'intro', 'battle', 'npc', 'npc-expanded', 'boss', 'construction', 'character', 'character-max', 'survivor', 'armory', 'skills', 'skills-survival', 'skills-ordnance', 'ordnance', 'engineering', 'results', 'operations', 'settings', 'history', 'history-empty', 'arid', 'tropical', 'peak', 'active-camp')
)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
$names = @('LASTLIGHT_REVIEW_SCREEN', 'LASTLIGHT_REVIEW_CAPTURE', 'LASTLIGHT_REVIEW_EXIT', 'LASTLIGHT_REVIEW_SIZE', 'LASTLIGHT_REVIEW_SCALE', 'LASTLIGHT_PRACTICE', 'LASTLIGHT_PERF_REPORT', 'LASTLIGHT_REVIEW_MOTION', 'LASTLIGHT_CAPTURE_SEQUENCE', 'LASTLIGHT_REVIEW_REDUCED')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
try {
    if (-not $NoBuild) {
        & cargo build --locked -p horde-defense
        if ($LASTEXITCODE -ne 0) { throw 'Native build failed' }
    }
    $destination = Join-Path $workspace $OutputDirectory
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    [Environment]::SetEnvironmentVariable('LASTLIGHT_PRACTICE', $null, 'Process')
    [Environment]::SetEnvironmentVariable('LASTLIGHT_PERF_REPORT', $null, 'Process')
    foreach ($optional in @('LASTLIGHT_REVIEW_MOTION', 'LASTLIGHT_CAPTURE_SEQUENCE', 'LASTLIGHT_REVIEW_REDUCED')) { [Environment]::SetEnvironmentVariable($optional, $null, 'Process') }
    $env:LASTLIGHT_REVIEW_EXIT = '1'
    $env:LASTLIGHT_REVIEW_SIZE = '1440x900'
    $env:LASTLIGHT_REVIEW_SCALE = '1.0'
    foreach ($scene in $Scenes) {
        $env:LASTLIGHT_REVIEW_SCREEN = $scene
        $env:LASTLIGHT_REVIEW_CAPTURE = Join-Path $destination "$scene.png"
        & './target/debug/horde-defense.exe'
        if ($LASTEXITCODE -ne 0) { throw "Native capture failed: $scene" }
    }
    foreach ($smallScene in $SmallScenes) {
        $env:LASTLIGHT_REVIEW_SCREEN = $smallScene
        $env:LASTLIGHT_REVIEW_SIZE = '1050x700'
        $env:LASTLIGHT_REVIEW_SCALE = '1.35'
        $env:LASTLIGHT_REVIEW_CAPTURE = Join-Path $destination "$smallScene-small.png"
        & './target/debug/horde-defense.exe'
        if ($LASTEXITCODE -ne 0) { throw "Minimum viewport capture failed: $smallScene" }
    }
}
finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    Pop-Location
}
