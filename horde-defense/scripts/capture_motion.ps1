#requires -Version 5.1
# Deterministic native animation evidence under Windows PowerShell 5.1. This is not a performance benchmark.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture_motion.ps1 -Scene peak -OutputDirectory target/motion/peak
# The game creates -OutputDirectory itself (frames and manifest.json) and refuses an existing one, so the
# process logs, cargo log and the isolated LASTLIGHT_SAVE_DIR go to the sibling folder <OutputDirectory>.logs.
[CmdletBinding()]
param(
    # Any scene the binary's --list-scenes marks motion=true.
    [string]$Scene = 'peak',
    [string]$OutputDirectory = 'target/animation-playtest-review/motion',
    [switch]$ReducedEffects,
    [switch]$NoBuild,
    [string]$Executable,
    [int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
$workspace = Split-Path -Parent $PSScriptRoot
if ([IO.Path]::IsPathRooted($OutputDirectory)) { $directory = [IO.Path]::GetFullPath($OutputDirectory) }
else { $directory = [IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory)) }
$logs = $directory.TrimEnd('\', '/') + '.logs'
if (Test-Path -LiteralPath $directory) { throw "Use a fresh evidence directory: $directory" }
if (Test-Path -LiteralPath $logs) { throw "Use a fresh evidence directory: $logs already exists" }
$saved = Reset-LastlightEnv
try {
    if (-not $NoBuild) {
        Invoke-Cargo -Arguments @('build', '--locked', '-p', 'horde-defense') -LogStem (Join-Path $logs 'cargo-build') | Out-Null
    }
    if ($Executable) {
        $exe = [IO.Path]::GetFullPath($Executable)
        if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "No executable at $exe" }
    }
    else { $exe = Get-HordeExecutable -Profile dev }
    $listing = Invoke-Native -FilePath $exe -Arguments @('--list-scenes') -LogStem (Join-Path $logs 'list-scenes') -TimeoutSeconds 30 -WorkingDirectory $workspace
    if ($listing.ExitCode -ne 0) { throw "This build has no --list-scenes (exit $($listing.ExitCode)); see $($listing.Err)" }
    $motionScenes = @(Get-Content -LiteralPath $listing.Out | Where-Object { $_.Trim() -and -not $_.StartsWith('#') } | ForEach-Object {
            $columns = $_ -split "`t"
            if ($columns.Count -ge 3 -and $columns[0] -ne 'name' -and $columns[2] -eq 'true') { $columns[0] }
        })
    if ($motionScenes -cnotcontains $Scene) { throw "Unknown motion scene `"$Scene`"; run --list-scenes (motion scenes: $($motionScenes -join ', '))" }
    $env:LASTLIGHT_REVIEW_SCREEN = $Scene
    $env:LASTLIGHT_REVIEW_SIZE = '1440x900'
    $env:LASTLIGHT_REVIEW_SCALE = '1.0'
    $env:LASTLIGHT_REVIEW_MOTION = '1'
    $env:LASTLIGHT_CAPTURE_SEQUENCE = $directory
    $env:LASTLIGHT_SAVE_DIR = Join-Path $logs 'saves'
    if ($ReducedEffects) { $env:LASTLIGHT_REVIEW_REDUCED = '1' }
    $result = Invoke-Native -FilePath $exe -LogStem (Join-Path $logs $Scene) -TimeoutSeconds $TimeoutSeconds -WorkingDirectory $workspace
    # profile.rs never deletes the per-PID review profile.
    $reviewProfile = Join-Path ([IO.Path]::GetTempPath()) ('lastlight-review-' + $result.Id)
    if (Test-Path -LiteralPath $reviewProfile) { Remove-Item -LiteralPath $reviewProfile -Recurse -Force }
    if ($result.ExitCode -ne 0) { throw "Native animation export failed with exit code $($result.ExitCode); see $($result.Err)" }
    $manifest = Get-Content -Raw -LiteralPath (Join-Path $directory 'manifest.json') | ConvertFrom-Json
    if (@($manifest.frames).Count -ne 24) { throw 'Expected all 24 frames' }
    foreach ($frame in $manifest.frames) { if (-not (Test-Path -LiteralPath (Join-Path $directory $frame.file))) { throw "Missing $($frame.file)" } }
    for ($i = 1; $i -lt $manifest.frames.Count; $i++) {
        if ($manifest.frames[$i].native_frame -le $manifest.frames[$i - 1].native_frame) { throw 'Native animation frames did not increase' }
    }
    $menuScene = $Scene -in @('menu', 'menu-female', 'survivor-select')
    $hashes = @($manifest.frames | ForEach-Object { Get-FileSha256 -Path (Join-Path $directory $_.file) } | Sort-Object -Unique)
    if ($menuScene) {
        if (@($manifest.frames | Where-Object { $null -ne $_.simulation_tick }).Count -ne 0) { throw 'Menu animation must not simulate an encounter' }
        if (-not $ReducedEffects -and $hashes.Count -lt 2) { throw 'Menu presentation did not animate across native frames' }
        if ($ReducedEffects -and $hashes.Count -ne 1) { throw 'Reduced-effects menu should remain visually static' }
    }
    else {
        for ($i = 1; $i -lt $manifest.frames.Count; $i++) {
            if ($null -eq $manifest.frames[$i].simulation_tick -or $null -eq $manifest.frames[$i - 1].simulation_tick -or $manifest.frames[$i].simulation_tick -le $manifest.frames[$i - 1].simulation_tick) { throw 'Combat animation fixture did not advance authoritative ticks' }
        }
    }
    if ($Scene -eq 'gesture-throws') {
        $firstTick = $manifest.frames[0].simulation_tick
        $throwTicks = @($manifest.frames | Where-Object { $null -ne $_.latest_throw_tick -and $_.latest_throw_tick -gt $firstTick } | Select-Object -ExpandProperty latest_throw_tick -Unique)
        if ($throwTicks.Count -ne 2) { throw 'Expected two real post-warmup throw releases in the captured window' }
        $kinds = @($manifest.frames | ForEach-Object { $_.active_projectile_kinds } | Sort-Object -Unique)
        if ($kinds -notcontains 'Grenade' -or $kinds -notcontains 'Molotov') { throw 'Both real grenade and Molotov projectiles must be visible during export' }
        foreach ($tick in $throwTicks) {
            if (@($manifest.frames | Where-Object { $_.simulation_tick -ge $tick -and $_.simulation_tick -le $tick + 8 }).Count -eq 0) { throw 'Throw release pose was not captured' }
            if (@($manifest.frames | Where-Object { $_.simulation_tick -ge $tick + 24 }).Count -eq 0) { throw 'Throw recovery was not captured' }
        }
    }
    if ($Scene -eq 'gesture-melee') {
        $firstTick = $manifest.frames[0].simulation_tick
        $meleeTicks = @($manifest.frames | Where-Object { $null -ne $_.latest_melee_tick -and $_.latest_melee_tick -gt $firstTick } | Select-Object -ExpandProperty latest_melee_tick -Unique)
        if ($meleeTicks.Count -ne 1) { throw 'Expected one real post-warmup contact melee event' }
        if (@($manifest.frames | Where-Object { $null -ne $_.latest_melee_target_id -and $_.melee_commit -gt 0 }).Count -eq 0) { throw 'Actual targeted melee and commitment were not captured' }
        $last = $manifest.frames[-1]
        if ($last.melee_commit -ne 0 -or $last.simulation_tick -lt $meleeTicks[0] + 30) { throw 'Melee recovery did not finish inside the export' }
    }
    $encoder = Get-Command ffmpeg -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($encoder) {
        $video = Join-Path $directory 'preview.mp4'
        $arguments = @('-loglevel', 'error', '-y', '-framerate', [string]$manifest.nominal_video_fps, '-i', (Join-Path $directory 'frame-%04d.png'), '-frames:v', '24', '-vf', 'pad=ceil(iw/2)*2:ceil(ih/2)*2', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', $video)
        $encoded = Invoke-Native -FilePath $encoder.Source -Arguments $arguments -LogStem (Join-Path $logs 'ffmpeg') -TimeoutSeconds $TimeoutSeconds
        if ($encoded.ExitCode -ne 0) { throw "Animation MP4 encoding failed; see $($encoded.Err)" }
        Write-Output $video
    }
    else { Write-Warning 'ffmpeg is unavailable; validated PNG frames and manifest are retained without an MP4.' }
    Write-Output $directory
}
finally {
    Restore-LastlightEnv -Saved $saved
}
