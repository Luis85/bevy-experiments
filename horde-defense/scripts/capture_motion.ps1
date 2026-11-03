# Deterministic native animation evidence. This is not a performance benchmark.
[CmdletBinding()]
param([ValidateSet('peak','npc','battle','battle-female','intro','boss','arid','tropical','ordnance','menu','menu-female','survivor-select','progressed-peak','max-lanes','quality-loot','reticle-reload','damage','damage-4x','critical-npc','gesture-throws','gesture-melee')][string]$Scene='peak', [string]$OutputDirectory='target/animation-playtest-review/motion', [switch]$ReducedEffects, [switch]$NoBuild)
$ErrorActionPreference='Stop'
$workspace=Split-Path -Parent $PSScriptRoot
Push-Location $workspace
$names=@('LASTLIGHT_REVIEW_SCREEN','LASTLIGHT_REVIEW_CAPTURE','LASTLIGHT_REVIEW_EXIT','LASTLIGHT_REVIEW_SIZE','LASTLIGHT_REVIEW_SCALE','LASTLIGHT_PRACTICE','LASTLIGHT_PERF_REPORT','LASTLIGHT_REVIEW_MOTION','LASTLIGHT_CAPTURE_SEQUENCE','LASTLIGHT_REVIEW_REDUCED')
$previous=@{}
foreach($name in $names){$previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
    if(-not $NoBuild){& cargo build --locked -p horde-defense;if($LASTEXITCODE -ne 0){throw 'Native build failed'}}
    $directory=[IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory))
    if(Test-Path -LiteralPath $directory){throw "Use a fresh evidence directory: $directory"}
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$null,'Process')}
    $env:LASTLIGHT_REVIEW_SCREEN=$Scene
    $env:LASTLIGHT_REVIEW_SIZE='1440x900'
    $env:LASTLIGHT_REVIEW_SCALE='1.0'
    $env:LASTLIGHT_REVIEW_MOTION='1'
    $env:LASTLIGHT_CAPTURE_SEQUENCE=$directory
    if($ReducedEffects){$env:LASTLIGHT_REVIEW_REDUCED='1'}
    & './target/debug/horde-defense.exe'
    if($LASTEXITCODE -ne 0){throw 'Native animation export failed'}
    $manifest=Get-Content -Raw -LiteralPath (Join-Path $directory 'manifest.json')|ConvertFrom-Json
    if(@($manifest.frames).Count -ne 24){throw 'Expected all 24 frames'}
    foreach($frame in $manifest.frames){if(-not(Test-Path -LiteralPath (Join-Path $directory $frame.file))){throw "Missing $($frame.file)"}}
    for($i=1;$i -lt $manifest.frames.Count;$i++) {
        if($manifest.frames[$i].native_frame -le $manifest.frames[$i-1].native_frame){throw 'Native animation frames did not increase'}
    }
    $menuScene=$Scene -in @('menu','menu-female','survivor-select')
    $hashes=@($manifest.frames | ForEach-Object {(Get-FileHash -LiteralPath (Join-Path $directory $_.file) -Algorithm SHA256).Hash} | Sort-Object -Unique)
    if($menuScene) {
        if(@($manifest.frames | Where-Object {$null -ne $_.simulation_tick}).Count -ne 0){throw 'Menu animation must not simulate an encounter'}
        if(-not $ReducedEffects -and $hashes.Count -lt 2){throw 'Menu presentation did not animate across native frames'}
        if($ReducedEffects -and $hashes.Count -ne 1){throw 'Reduced-effects menu should remain visually static'}
    } else {
        for($i=1;$i -lt $manifest.frames.Count;$i++) {
            if($null -eq $manifest.frames[$i].simulation_tick -or $null -eq $manifest.frames[$i-1].simulation_tick -or $manifest.frames[$i].simulation_tick -le $manifest.frames[$i-1].simulation_tick){throw 'Combat animation fixture did not advance authoritative ticks'}
        }
    }
    if($Scene -eq 'gesture-throws') {
        $firstTick=$manifest.frames[0].simulation_tick
        $throwTicks=@($manifest.frames | Where-Object {$null -ne $_.latest_throw_tick -and $_.latest_throw_tick -gt $firstTick} | Select-Object -ExpandProperty latest_throw_tick -Unique)
        if($throwTicks.Count -ne 2){throw 'Expected two real post-warmup throw releases in the captured window'}
        $kinds=@($manifest.frames | ForEach-Object {$_.active_projectile_kinds} | Sort-Object -Unique)
        if($kinds -notcontains 'Grenade' -or $kinds -notcontains 'Molotov'){throw 'Both real grenade and Molotov projectiles must be visible during export'}
        foreach($tick in $throwTicks){
            if(@($manifest.frames | Where-Object {$_.simulation_tick -ge $tick -and $_.simulation_tick -le $tick+8}).Count -eq 0){throw 'Throw release pose was not captured'}
            if(@($manifest.frames | Where-Object {$_.simulation_tick -ge $tick+24}).Count -eq 0){throw 'Throw recovery was not captured'}
        }
    }
    if($Scene -eq 'gesture-melee') {
        $firstTick=$manifest.frames[0].simulation_tick
        $meleeTicks=@($manifest.frames | Where-Object {$null -ne $_.latest_melee_tick -and $_.latest_melee_tick -gt $firstTick} | Select-Object -ExpandProperty latest_melee_tick -Unique)
        if($meleeTicks.Count -ne 1){throw 'Expected one real post-warmup contact melee event'}
        if(@($manifest.frames | Where-Object {$null -ne $_.latest_melee_target_id -and $_.melee_commit -gt 0}).Count -eq 0){throw 'Actual targeted melee and commitment were not captured'}
        $last=$manifest.frames[-1]
        if($last.melee_commit -ne 0 -or $last.simulation_tick -lt $meleeTicks[0]+30){throw 'Melee recovery did not finish inside the export'}
    }
    $encoder=Get-Command ffmpeg -ErrorAction SilentlyContinue
    if($encoder) {
        $video=Join-Path $directory 'preview.mp4'
        & $encoder.Source -loglevel error -y -framerate $manifest.nominal_video_fps -i (Join-Path $directory 'frame-%04d.png') -frames:v 24 -vf 'pad=ceil(iw/2)*2:ceil(ih/2)*2' -c:v libx264 -pix_fmt yuv420p -movflags +faststart $video
        if($LASTEXITCODE -ne 0){throw 'Animation MP4 encoding failed'}
        Write-Output $video
    } else { Write-Warning 'ffmpeg is unavailable; validated PNG frames and manifest are retained without an MP4.' }
    Write-Output $directory
} finally {
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')}
    Pop-Location
}
