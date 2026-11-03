# Native real-time evidence. Deliberately separate from deterministic motion export.
[CmdletBinding()]
param([ValidateSet('peak','progressed-peak')][string]$Scene='progressed-peak',[string]$OutputDirectory='target/increment-review/performance',[switch]$NoBuild)
$ErrorActionPreference='Stop'
$workspace=Split-Path -Parent $PSScriptRoot
Push-Location $workspace
$names=@('LASTLIGHT_REVIEW_SCREEN','LASTLIGHT_REVIEW_CAPTURE','LASTLIGHT_REVIEW_EXIT','LASTLIGHT_REVIEW_SIZE','LASTLIGHT_REVIEW_SCALE','LASTLIGHT_PRACTICE','LASTLIGHT_PERF_REPORT','LASTLIGHT_PERF_CPU','LASTLIGHT_REVIEW_MOTION','LASTLIGHT_CAPTURE_SEQUENCE','LASTLIGHT_REVIEW_REDUCED','LASTLIGHT_SAVE_DIR')
$previous=@{}
foreach($name in $names){$previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
    if(-not $NoBuild){& cargo build --locked -p horde-defense;if($LASTEXITCODE -ne 0){throw 'Native build failed'}}
    $destination=[IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory))
    if(Test-Path -LiteralPath $destination){throw "Use a fresh evidence directory: $destination"}
    New-Item -ItemType Directory -Path $destination | Out-Null
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$null,'Process')}
    $env:LASTLIGHT_REVIEW_SCREEN=$Scene
    # This only unpauses the isolated scenario; no CAPTURE_SEQUENCE means ordinary real time.
    $env:LASTLIGHT_REVIEW_MOTION='1'
    $env:LASTLIGHT_PERF_CPU=(Get-CimInstance Win32_Processor | Select-Object -First 1).Name
    foreach($case in @(@{Name='peak';Size='1440x900';Scale='1.0'},@{Name='peak-small';Size='1050x700';Scale='1.35'})) {
        $env:LASTLIGHT_REVIEW_SIZE=$case.Size
        $env:LASTLIGHT_REVIEW_SCALE=$case.Scale
        $env:LASTLIGHT_PERF_REPORT=Join-Path $destination ($case.Name+'.json')
        $env:LASTLIGHT_SAVE_DIR=Join-Path $destination ($case.Name+'-isolated-saves')
        & './target/debug/horde-defense.exe' *> (Join-Path $destination ($case.Name+'.log'))
        if($LASTEXITCODE -ne 0){throw "Native performance run failed: $($case.Name)"}
        $report=Get-Content -Raw -LiteralPath $env:LASTLIGHT_PERF_REPORT|ConvertFrom-Json
        if($report.sample_frames -ne 300 -or $report.paused){throw "Incomplete/inactive performance run: $($case.Name)"}
        Write-Output ($report|Select-Object scene,width_physical,height_physical,ui_scale,mean_observed_fps,p95_frame_ms,p99_frame_ms,visible_enemies|ConvertTo-Json -Compress)
    }
} finally {
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')}
    Pop-Location
}
