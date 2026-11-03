# Reproduce an NPC checkpoint in an isolated profile and record native hitch evidence.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$InputSave,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateSet('dx12','vulkan')][string]$Backend='dx12',
    [ValidateSet(1,2,4)][int]$Speed=1,
    [ValidateRange(300,3600)][int]$SampleFrames=1800,
    [switch]$Audio,
    [switch]$NoBuild
)
$ErrorActionPreference='Stop'
$workspace=Split-Path -Parent $PSScriptRoot
Push-Location $workspace
$names=@('WGPU_BACKEND','LASTLIGHT_SAVE_DIR','LASTLIGHT_ASSET_DIR','LASTLIGHT_REVIEW_SCREEN','LASTLIGHT_REVIEW_INPUT_SAVE','LASTLIGHT_REVIEW_CAPTURE','LASTLIGHT_REVIEW_EXIT','LASTLIGHT_REVIEW_SIZE','LASTLIGHT_REVIEW_SCALE','LASTLIGHT_REVIEW_MOTION','LASTLIGHT_REVIEW_REDUCED','LASTLIGHT_CAPTURE_SEQUENCE','LASTLIGHT_PRACTICE','LASTLIGHT_PERF_REPORT','LASTLIGHT_PERF_CPU','LASTLIGHT_PERF_FRAMES','LASTLIGHT_PERF_WARMUP_FRAMES','LASTLIGHT_PERF_ACTIVE_ONLY','LASTLIGHT_PERF_AUDIO','LASTLIGHT_PERF_NPC_SPEED')
$previous=@{}
foreach($name in $names){$previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
    $source=(Resolve-Path -LiteralPath $InputSave).Path
    $destination=[IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory))
    if(Test-Path -LiteralPath $destination){throw 'Choose a new evidence directory; existing traces are preserved.'}
    if(-not $NoBuild){& cargo build --locked -p horde-defense;if($LASTEXITCODE -ne 0){throw 'Native build failed'}}
    New-Item -ItemType Directory -Path $destination | Out-Null
    $snapshot=Join-Path $destination 'input-checkpoint.json'
    Copy-Item -LiteralPath $source -Destination $snapshot
    $profile=Join-Path $destination 'isolated-profile'
    New-Item -ItemType Directory -Path $profile | Out-Null
    $sourceSettings=Join-Path (Split-Path -Parent $source) 'settings.json'
    if(Test-Path -LiteralPath $sourceSettings){
        Copy-Item -LiteralPath $sourceSettings -Destination (Join-Path $destination 'settings.json')
        Copy-Item -LiteralPath $sourceSettings -Destination (Join-Path $profile 'settings.json')
    }
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$null,'Process')}
    $env:WGPU_BACKEND=$Backend
    $env:LASTLIGHT_SAVE_DIR=$profile
    $env:LASTLIGHT_ASSET_DIR=Join-Path $workspace 'assets'
    $env:LASTLIGHT_REVIEW_SCREEN='npc-stutter'
    $env:LASTLIGHT_REVIEW_INPUT_SAVE=$snapshot
    $env:LASTLIGHT_REVIEW_MOTION='1'
    $env:LASTLIGHT_PERF_REPORT=Join-Path $destination 'frames.json'
    $env:LASTLIGHT_PERF_CPU=(Get-CimInstance Win32_Processor | Select-Object -First 1).Name
    $env:LASTLIGHT_PERF_FRAMES=[string]$SampleFrames
    $env:LASTLIGHT_PERF_WARMUP_FRAMES='90'
    $env:LASTLIGHT_PERF_ACTIVE_ONLY='1'
    $env:LASTLIGHT_PERF_NPC_SPEED=[string]$Speed
    if($Audio){$env:LASTLIGHT_PERF_AUDIO='1'}
    $process=Start-Process -FilePath (Join-Path $workspace 'target/debug/horde-defense.exe') -WorkingDirectory $workspace -WindowStyle Normal -RedirectStandardOutput (Join-Path $destination 'stdout.log') -RedirectStandardError (Join-Path $destination 'stderr.log') -PassThru
    Write-Output "NPC trace PID $($process.Id), input SHA256 $((Get-FileHash -LiteralPath $snapshot).Hash.ToLowerInvariant())"
    $deadline=[DateTime]::UtcNow.AddMinutes(5)
    while(-not $process.HasExited -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 250;$process.Refresh()}
    if(-not $process.HasExited){Stop-Process -Id $process.Id;throw 'NPC trace exceeded five-minute bound; only the isolated trace process was stopped.'}
    $process.WaitForExit()
    if($process.ExitCode -ne 0 -or -not(Test-Path -LiteralPath $env:LASTLIGHT_PERF_REPORT)){throw 'Trace did not complete; inspect retained stdout/stderr.'}
    # Automated review always selects its own PID-specific temporary profile.
    # Retain the committed generations beside the trace for deterministic comparison.
    $nativeProfile=Join-Path ([IO.Path]::GetTempPath()) "lastlight-review-$($process.Id)"
    foreach($file in @('campaign.json','campaign.bak','settings.json')) {
        $generation=Join-Path $nativeProfile $file
        if(Test-Path -LiteralPath $generation){Copy-Item -LiteralPath $generation -Destination (Join-Path $profile $file)}
    }
    if(-not(Test-Path -LiteralPath (Join-Path $profile 'campaign.json'))){throw 'Trace exited without a committed campaign generation.'}
    Get-Content -Raw -LiteralPath $env:LASTLIGHT_PERF_REPORT
} finally {
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')}
    Pop-Location
}
