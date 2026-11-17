#requires -Version 5.1
# Deterministic native animation evidence under Windows PowerShell 5.1. This is not a performance benchmark.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture_motion.ps1 -Scene peak -OutputDirectory target/motion/peak
# The game creates -OutputDirectory itself (frames and manifest.json) and refuses an existing one, so the
# process logs, cargo log, the isolated LASTLIGHT_SAVE_DIR and the session's LASTLIGHT_REVIEW_CUE_LOG (cues.jsonl,
# design-audio 6.18, for replay_cue_log) go to the sibling folder <OutputDirectory>.logs.
# -Viewport normal is 1440x900 at UI scale 1.0, small is 1050x700 at 1.35 (capture_review's two sizes). The
# version-2 manifest must report the requested reduced_effects, width, height and scale, plus windows_scale.
# Every frame carries flash_alpha_max, sky_flash and muzzle_flash_count: with -ReducedEffects all three must be 0 in
# every frame; a normal storm export must show a pulse (sky_flash > 0) lit in two neighbouring frames. sky_flash is an
# alpha that cannot exceed the 0.14 peak by construction, so its peak is not checked; the storm's luminance step
# (delta L < 0.2) is measured on the PNGs by measure_field_luminance.py storm.
# A relative -OutputDirectory or -Executable resolves against the workspace, never the caller's directory.
[CmdletBinding()]
param(
    # Any scene the binary's --list-scenes marks motion=true.
    [string]$Scene = 'peak',
    [string]$OutputDirectory = 'target/animation-playtest-review/motion',
    [switch]$ReducedEffects,
    [ValidateSet('normal', 'small')]
    [string]$Viewport = 'normal',
    [switch]$NoBuild,
    [string]$Executable,
    [int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
$workspace = Split-Path -Parent $PSScriptRoot
$directory = Resolve-WorkspacePath $OutputDirectory -Workspace $workspace
$logs = $directory.TrimEnd('\', '/') + '.logs'
if (Test-Path -LiteralPath $directory) { throw "Use a fresh evidence directory: $directory" }
if (Test-Path -LiteralPath $logs) { throw "Use a fresh evidence directory: $logs already exists" }
if ($Viewport -eq 'small') { $view = @{ Width = 1050; Height = 700; Scale = 1.35 } }
else { $view = @{ Width = 1440; Height = 900; Scale = 1.0 } }
$saved = Reset-LastlightEnv
try {
    if (-not $NoBuild) {
        Invoke-HordeBuild -Arguments @('build', '--locked', '-p', 'horde-defense') -LogStem (Join-Path $logs 'cargo-build') -DepInfo (Join-Path (Resolve-HordeTarget -Workspace $workspace) 'debug\horde-defense.d') -Workspace $workspace | Out-Null
    }
    if ($Executable) {
        $exe = Resolve-WorkspacePath $Executable -Workspace $workspace
        if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "No executable at $exe" }
    }
    else { $exe = Get-HordeExecutable -Profile dev }
    $listing = Get-ReviewSceneList -Executable $exe -LogStem (Join-Path $logs 'list-scenes') -WorkingDirectory $workspace
    if ($listing.ExitCode -ne 0) { throw "This build has no --list-scenes (exit $($listing.ExitCode)); see $($listing.Err)" }
    $motionScenes = @($listing.Scenes | Where-Object { $_.Motion } | ForEach-Object { $_.Name })
    if ($motionScenes -cnotcontains $Scene) { throw "Unknown motion scene `"$Scene`"; run --list-scenes (motion scenes: $($motionScenes -join ', '))" }
    $env:LASTLIGHT_REVIEW_SCREEN = $Scene
    $env:LASTLIGHT_REVIEW_SIZE = '{0}x{1}' -f $view.Width, $view.Height
    $env:LASTLIGHT_REVIEW_SCALE = $view.Scale.ToString('0.0##', [Globalization.CultureInfo]::InvariantCulture)
    $env:LASTLIGHT_REVIEW_MOTION = '1'
    $env:LASTLIGHT_CAPTURE_SEQUENCE = $directory
    $env:LASTLIGHT_SAVE_DIR = Join-Path $logs 'saves'
    $env:LASTLIGHT_REVIEW_CUE_LOG = Join-Path $logs 'cues.jsonl'
    if ($ReducedEffects) { $env:LASTLIGHT_REVIEW_REDUCED = '1' }
    $result = Invoke-Native -FilePath $exe -LogStem (Join-Path $logs $Scene) -TimeoutSeconds $TimeoutSeconds -WorkingDirectory $workspace -RemoveReviewProfile
    if ($result.ExitCode -ne 0) { throw "Native animation export failed with exit code $($result.ExitCode); see $($result.Err)" }
    $manifest = Get-Content -Raw -LiteralPath (Join-Path $directory 'manifest.json') | ConvertFrom-Json
    if ($manifest.version -ne 2) { throw "Expected motion manifest version 2, got $($manifest.version)" }
    if ($manifest.reduced_effects -isnot [bool] -or $manifest.reduced_effects -ne [bool]$ReducedEffects) { throw "Manifest reduced_effects is $($manifest.reduced_effects); expected $([bool]$ReducedEffects)" }
    if ($manifest.width -ne $view.Width -or $manifest.height -ne $view.Height) { throw "Manifest viewport is $($manifest.width)x$($manifest.height); expected $($view.Width)x$($view.Height)" }
    if ($null -eq $manifest.scale -or [Math]::Abs([double]$manifest.scale - $view.Scale) -gt 0.001) { throw "Manifest scale is $($manifest.scale); expected $($view.Scale)" }
    if ($null -eq $manifest.windows_scale -or [double]$manifest.windows_scale -le 0) { throw "Manifest windows_scale is missing or not positive: $($manifest.windows_scale)" }
    if (@($manifest.frames).Count -ne 24) { throw 'Expected all 24 frames' }
    foreach ($frame in $manifest.frames) { if (-not (Test-Path -LiteralPath (Join-Path $directory $frame.file))) { throw "Missing $($frame.file)" } }
    for ($i = 1; $i -lt $manifest.frames.Count; $i++) {
        if ($manifest.frames[$i].native_frame -le $manifest.frames[$i - 1].native_frame) { throw 'Native animation frames did not increase' }
    }
    $flashProblem = Get-MotionFlashProblem -Frames @($manifest.frames) -ReducedEffects:$ReducedEffects -Storm:($Scene -eq 'storm')
    if ($flashProblem) { throw $flashProblem }
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
