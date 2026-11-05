#requires -Version 5.1
# Native framebuffer evidence under Windows PowerShell 5.1. Every fixture uses an isolated temporary campaign.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture_review.ps1 -Scenes menu -Modes normal,reduced -OutputDirectory target/production-after/wave1
# Layout under -OutputDirectory (which must be missing or empty):
#   <mode>/<scene>[-small].png, .out.txt, .err.txt and .json (the sidecar, when the build writes one)
#   saves/<mode>-<scene>[-small]/  per-capture LASTLIGHT_SAVE_DIR, so logs never reach the player profile
#   scenes.tsv                     the binary's --list-scenes output
#   tools/                         cargo, --list-scenes and --version logs
#   captures.json                  version 1, UTF-8 without BOM
# -Scenes defaults to every Still the binary lists; -SmallScenes defaults to -Scenes. List parameters accept
# 'a,b' as one argument, because -File passes commas through verbatim.
[CmdletBinding()]
param(
    [string]$OutputDirectory = 'target/playtest-review/screenshots',
    [string[]]$Scenes,
    [string[]]$SmallScenes,
    [string[]]$Modes = @('normal'),
    [string]$Executable,
    [switch]$NoBuild,
    [switch]$ContinueOnError,
    [int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
$workspace = Split-Path -Parent $PSScriptRoot

function Split-ListArgument([string[]]$Values) {
    $items = New-Object System.Collections.Generic.List[string]
    foreach ($value in @($Values)) {
        if ($null -eq $value) { continue }
        foreach ($part in ($value -split ',')) {
            $trimmed = $part.Trim()
            if ($trimmed.Length -gt 0 -and -not $items.Contains($trimmed)) { $items.Add($trimmed) }
        }
    }
    # Callers wrap the result in @() so zero or one item still reads as an array.
    return $items.ToArray()
}

function Get-PngWidth([string]$Path) {
    # Width from the IHDR chunk: bytes 16-19, big-endian.
    $stream = [IO.File]::OpenRead($Path)
    try {
        $header = New-Object byte[] 24
        if ($stream.Read($header, 0, 24) -ne 24) { return $null }
        return ([int]$header[16] -shl 24) -bor ([int]$header[17] -shl 16) -bor ([int]$header[18] -shl 8) -bor [int]$header[19]
    }
    finally { $stream.Dispose() }
}

function Get-LogListing([string]$Directory) {
    if (-not (Test-Path -LiteralPath $Directory)) { return '' }
    $rows = @(Get-ChildItem -LiteralPath $Directory -Force -Recurse -File | Sort-Object FullName | ForEach-Object { '{0}|{1}|{2}' -f $_.FullName, $_.Length, $_.LastWriteTimeUtc.Ticks })
    return ($rows -join "`n")
}

$modeList = @(Split-ListArgument $Modes)
foreach ($mode in $modeList) {
    if ($mode -notin @('normal', 'reduced')) { throw "Unknown mode '$mode'; use normal, reduced or normal,reduced" }
}
if ($modeList.Count -eq 0) { throw 'No -Modes given' }

if ([IO.Path]::IsPathRooted($OutputDirectory)) { $destination = [IO.Path]::GetFullPath($OutputDirectory) }
else { $destination = [IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory)) }
if (Test-Path -LiteralPath $destination) {
    if (-not (Test-Path -LiteralPath $destination -PathType Container)) { throw "Output path is a file: $destination" }
    if (@(Get-ChildItem -LiteralPath $destination -Force).Count -gt 0) {
        throw "Refusing non-empty evidence directory $destination; choose a fresh -OutputDirectory"
    }
}
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$tools = Join-Path $destination 'tools'

$playerLogs = $null
if ($env:LOCALAPPDATA) { $playerLogs = Join-Path $env:LOCALAPPDATA 'LastlightHordeDefense\logs' }
$playerLogsBefore = Get-LogListing $playerLogs
$startedUtc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
$entries = New-Object System.Collections.Generic.List[object]
$failures = New-Object System.Collections.Generic.List[string]
$sceneListFile = $null
$saved = Reset-LastlightEnv
try {
    if (-not $NoBuild) {
        Invoke-Cargo -Arguments @('build', '--locked', '-p', 'horde-defense') -LogStem (Join-Path $tools 'cargo-build') | Out-Null
    }
    if ($Executable) {
        $exe = [IO.Path]::GetFullPath($Executable)
        if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "No executable at $exe" }
    }
    else { $exe = Get-HordeExecutable -Profile dev }

    $stamp = $null
    $version = Invoke-Native -FilePath $exe -Arguments @('--version') -LogStem (Join-Path $tools 'version') -TimeoutSeconds 30 -WorkingDirectory $workspace
    if ($version.ExitCode -eq 0) {
        $line = @(Get-Content -LiteralPath $version.Out | Where-Object { $_.Trim() } | Select-Object -First 1)
        if ($line.Count -gt 0) { $stamp = $line[0].Trim() }
    }

    $registry = $null
    $listing = Invoke-Native -FilePath $exe -Arguments @('--list-scenes') -LogStem (Join-Path $tools 'list-scenes') -TimeoutSeconds 30 -WorkingDirectory $workspace
    if ($listing.ExitCode -eq 0) {
        $sceneListFile = 'scenes.tsv'
        Copy-Item -LiteralPath $listing.Out -Destination (Join-Path $destination $sceneListFile)
        $registry = @(Get-Content -LiteralPath $listing.Out | Where-Object { $_.Trim() -and -not $_.StartsWith('#') } | ForEach-Object {
                $columns = $_ -split "`t"
                if ($columns[0] -ne 'name') { [pscustomobject]@{ Name = $columns[0]; Kind = $columns[1] } }
            })
    }
    elseif (-not $Scenes) {
        throw "This build has no --list-scenes (exit $($listing.ExitCode), see $($listing.Err)); pass -Scenes explicitly"
    }
    else { Write-Warning "This build has no --list-scenes; capturing the given -Scenes without registry validation or scenes.tsv" }

    if ($Scenes) { $sceneList = @(Split-ListArgument $Scenes) }
    else { $sceneList = @($registry | Where-Object { $_.Kind -eq 'Still' } | ForEach-Object { $_.Name }) }
    if ($PSBoundParameters.ContainsKey('SmallScenes')) { $smallList = @(Split-ListArgument $SmallScenes) }
    else { $smallList = $sceneList }
    if ($null -ne $registry) {
        foreach ($scene in @($sceneList + $smallList | Sort-Object -Unique)) {
            $known = @($registry | Where-Object { $_.Name -eq $scene -or ($_.Kind -eq 'Workshop' -and $scene.Length -gt $_.Name.Length -and $scene.StartsWith($_.Name)) })
            if ($known.Count -eq 0) { throw "Unknown review scene `"$scene`"; run --list-scenes" }
        }
    }
    if ($sceneList.Count + $smallList.Count -eq 0) { throw 'No scenes to capture' }

    $sizes = @(
        @{ Name = 'large'; Size = '1440x900'; Width = 1440; Scale = 1.0; Suffix = ''; Scenes = $sceneList },
        @{ Name = 'small'; Size = '1050x700'; Width = 1050; Scale = 1.35; Suffix = '-small'; Scenes = $smallList }
    )
    $windowsScale = $null
    foreach ($mode in $modeList) {
        $modeDirectory = Join-Path $destination $mode
        foreach ($size in $sizes) {
            foreach ($scene in $size.Scenes) {
                $stem = $scene + $size.Suffix
                $png = Join-Path $modeDirectory "$stem.png"
                $sidecar = Join-Path $modeDirectory "$stem.json"
                $env:LASTLIGHT_REVIEW_SCREEN = $scene
                $env:LASTLIGHT_REVIEW_CAPTURE = $png
                $env:LASTLIGHT_REVIEW_EXIT = '1'
                $env:LASTLIGHT_REVIEW_SIZE = $size.Size
                $env:LASTLIGHT_REVIEW_SCALE = $size.Scale.ToString([Globalization.CultureInfo]::InvariantCulture)
                $env:LASTLIGHT_SAVE_DIR = Join-Path $destination "saves\$mode-$stem"
                if ($mode -eq 'reduced') { $env:LASTLIGHT_REVIEW_REDUCED = '1' }
                else { [Environment]::SetEnvironmentVariable('LASTLIGHT_REVIEW_REDUCED', $null, 'Process') }
                $result = $null
                $problem = $null
                try {
                    $result = Invoke-Native -FilePath $exe -LogStem (Join-Path $modeDirectory $stem) -TimeoutSeconds $TimeoutSeconds -WorkingDirectory $workspace
                    if ($result.ExitCode -ne 0) { $problem = "exit code $($result.ExitCode)" }
                    elseif (-not (Test-Path -LiteralPath $png -PathType Leaf)) { $problem = 'no PNG written' }
                }
                catch { $problem = $_.Exception.Message }
                finally {
                    if ($null -ne $result) {
                        # profile.rs never deletes the per-PID review profile.
                        $reviewProfile = Join-Path ([IO.Path]::GetTempPath()) ('lastlight-review-' + $result.Id)
                        if (Test-Path -LiteralPath $reviewProfile) { Remove-Item -LiteralPath $reviewProfile -Recurse -Force }
                    }
                }
                $hasPng = Test-Path -LiteralPath $png -PathType Leaf
                if ($hasPng -and $null -eq $windowsScale -and $null -eq $problem) {
                    $width = Get-PngWidth $png
                    if ($width) { $windowsScale = [Math]::Round($width / $size.Width, 3) }
                }
                $entry = [ordered]@{
                    scene               = $scene
                    mode                = $mode
                    size                = $size.Name
                    scale               = $size.Scale
                    file                = "$mode/$stem.png"
                    exit_code           = $(if ($null -ne $result) { $result.ExitCode } else { $null })
                    seconds             = $(if ($null -ne $result) { $result.Seconds } else { $null })
                    bytes               = $(if ($hasPng) { (Get-Item -LiteralPath $png).Length } else { $null })
                    sha256              = $(if ($hasPng) { Get-FileSha256 -Path $png } else { $null })
                    out_log             = "$mode/$stem.out.txt"
                    err_log             = "$mode/$stem.err.txt"
                    sidecar             = $(if (Test-Path -LiteralPath $sidecar -PathType Leaf) { "$mode/$stem.json" } else { $null })
                    error               = $problem
                    identical_to_normal = $null
                }
                $entries.Add($entry)
                if ($null -ne $problem) {
                    $message = "Capture failed: $mode $stem - $problem (see $(Join-Path $modeDirectory "$stem.err.txt"))"
                    $failures.Add($message)
                    if (-not $ContinueOnError) { throw $message }
                    Write-Warning $message
                }
                else { Write-Output "ok   $mode/$stem.png" }
            }
        }
    }
}
finally {
    Restore-LastlightEnv -Saved $saved
    foreach ($entry in $entries) {
        if ($entry.mode -ne 'reduced' -or $null -eq $entry.sha256) { continue }
        $pair = @($entries | Where-Object { $_.mode -eq 'normal' -and $_.scene -eq $entry.scene -and $_.size -eq $entry.size -and $null -ne $_.sha256 })
        if ($pair.Count -gt 0) { $entry.identical_to_normal = ($pair[0].sha256 -eq $entry.sha256) }
    }
    $playerLogsUnchanged = ((Get-LogListing $playerLogs) -eq $playerLogsBefore)
    if (-not $playerLogsUnchanged) { Write-Warning "The player log folder changed during the run: $playerLogs" }
    $exePath = $null
    $exeHash = $null
    if ((Get-Variable -Name exe -ErrorAction SilentlyContinue) -and $exe -and (Test-Path -LiteralPath $exe -PathType Leaf)) {
        $exePath = $exe
        $exeHash = Get-FileSha256 -Path $exe
    }
    $report = [ordered]@{
        version               = 1
        executable            = $exePath
        executable_sha256     = $exeHash
        stamp                 = $(if (Get-Variable -Name stamp -ErrorAction SilentlyContinue) { $stamp } else { $null })
        wgpu_backend          = $(if ($env:WGPU_BACKEND) { $env:WGPU_BACKEND } else { $null })
        windows_scale         = $(if (Get-Variable -Name windowsScale -ErrorAction SilentlyContinue) { $windowsScale } else { $null })
        started_utc           = $startedUtc
        scene_list            = $sceneListFile
        player_logs_unchanged = $playerLogsUnchanged
        captures              = $entries.ToArray()
    }
    Write-Utf8NoBom -Path (Join-Path $destination 'captures.json') -Text ($report | ConvertTo-Json -Depth 5)
}
Write-Output "Evidence: $destination ($($entries.Count) captures, $($failures.Count) failed)"
if ($failures.Count -gt 0) {
    foreach ($failure in $failures) { Write-Output "FAIL $failure" }
    exit 1
}
