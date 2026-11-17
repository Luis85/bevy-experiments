#requires -Version 5.1
# Shared Windows PowerShell 5.1 harness for scripts/. Dot-source it: . (Join-Path $PSScriptRoot 'lib\Native.ps1')
# Native processes always run through Invoke-Native (a Diagnostics.Process with redirected streams), because
# '&' on a native exe that writes stderr raises NativeCommandError under 5.1 when streams are redirected.
# Suggested timeouts: cargo 1800 s, capture 300 s, smoke 900 s, --version 30 s, git and rustc 30 s.
# Self-tests: scripts/tests/run_script_tests.ps1.

$script:NativeHordeWorkspace = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

function Resolve-HordeTarget {
    # A rooted CARGO_TARGET_DIR is used as-is, a relative one is joined to the workspace, otherwise <workspace>\target.
    [CmdletBinding()]
    param([string]$Workspace = $script:NativeHordeWorkspace)
    $configured = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
    if ([string]::IsNullOrEmpty($configured)) { return [IO.Path]::GetFullPath((Join-Path $Workspace 'target')) }
    if ([IO.Path]::IsPathRooted($configured)) { return $configured }
    return [IO.Path]::GetFullPath((Join-Path $Workspace $configured))
}

function Resolve-WorkspacePath {
    # A rooted path is normalised as-is, a relative one is joined to the workspace. Never the process directory,
    # which a caller's Set-Location does not move.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [string]$Workspace = $script:NativeHordeWorkspace
    )
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $Workspace $Path))
}

function Get-HordeExecutable {
    [CmdletBinding()]
    param(
        [ValidateSet('dev', 'debug', 'release', 'playtest')][string]$Profile = 'dev',
        [string]$Workspace = $script:NativeHordeWorkspace
    )
    $directory = $Profile
    if ($Profile -eq 'dev') { $directory = 'debug' }
    $path = Join-Path (Join-Path (Resolve-HordeTarget -Workspace $Workspace) $directory) 'horde-defense.exe'
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "No $Profile build at $path" }
    return $path
}

function ConvertTo-NativeArgument {
    # One command-line string with MSVC (CommandLineToArgvW) quoting rules.
    [CmdletBinding()]
    param([AllowEmptyString()][AllowEmptyCollection()][string[]]$Arguments = @())
    $parts = New-Object System.Collections.Generic.List[string]
    foreach ($argument in $Arguments) {
        if ($null -eq $argument -or $argument.Length -eq 0) { $parts.Add('""'); continue }
        if ($argument -notmatch '[\s"]') { $parts.Add($argument); continue }
        $builder = New-Object System.Text.StringBuilder
        [void]$builder.Append('"')
        $backslashes = 0
        foreach ($character in $argument.ToCharArray()) {
            if ($character -eq '\') { $backslashes += 1; continue }
            if ($character -eq '"') {
                [void]$builder.Append([char]'\', 2 * $backslashes + 1)
                [void]$builder.Append('"')
            }
            else {
                if ($backslashes -gt 0) { [void]$builder.Append([char]'\', $backslashes) }
                [void]$builder.Append($character)
            }
            $backslashes = 0
        }
        if ($backslashes -gt 0) { [void]$builder.Append([char]'\', 2 * $backslashes) }
        [void]$builder.Append('"')
        $parts.Add($builder.ToString())
    }
    return ($parts -join ' ')
}

function Invoke-Native {
    # Runs a native exe with stdout/stderr in <LogStem>.out.txt and <LogStem>.err.txt.
    # On timeout the whole process tree is killed (taskkill /T, which also stops cargo's rustc children) and it
    # throws a message naming the PID. -RemoveReviewProfile deletes the review build's per-PID profile once the
    # process has exited or been killed, so a hung capture does not leak one.
    # -Sample runs a scriptblock beside the process (R30, the per-run load sampler): once right after the start, then
    # every -SampleMilliseconds until the exit or the timeout. It runs in this function's scope chain, so it reaches the
    # caller's state through $script: or a variable name this function does not itself use (not $process, $watch, $left
    # or its other locals).
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [AllowEmptyString()][AllowEmptyCollection()][string[]]$Arguments = @(),
        [Parameter(Mandatory = $true)][string]$LogStem,
        [int]$TimeoutSeconds = 300,
        [string]$WorkingDirectory,
        [switch]$RemoveReviewProfile,
        [scriptblock]$Sample,
        [ValidateRange(100, 600000)][int]$SampleMilliseconds = 5000
    )
    $LogStem = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($LogStem)
    $logDirectory = Split-Path -Parent $LogStem
    if (-not (Test-Path -LiteralPath $logDirectory)) { New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null }
    $outLog = "$LogStem.out.txt"
    $errLog = "$LogStem.err.txt"
    $argumentLine = ConvertTo-NativeArgument -Arguments $Arguments
    # The Process object that creates the child owns its creation handle, so ExitCode is always readable. Start-Process
    # -PassThru returns a process looked up by id instead: when the child exits and Start-Process closes its handle
    # before the caller opens one, ExitCode reads as $null.
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $FilePath
    if ($FilePath.IndexOfAny([char[]]@('\', '/')) -ge 0) {
        $info.FileName = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($FilePath)
    }
    $info.Arguments = $argumentLine
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    # As Start-Process did, the default working directory is the caller's location, not the process directory.
    $info.WorkingDirectory = (Get-Location -PSProvider FileSystem).ProviderPath
    if ($WorkingDirectory) {
        $info.WorkingDirectory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($WorkingDirectory)
    }
    $outFile = $null
    $errFile = $null
    $copies = @()
    $process = $null
    $watch = [Diagnostics.Stopwatch]::StartNew()
    try {
        $outFile = New-Object IO.FileStream($outLog, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
        $errFile = New-Object IO.FileStream($errLog, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
        $process = [Diagnostics.Process]::Start($info)
        # Raw bytes, as a file redirect writes them.
        $copies = @(
            $process.StandardOutput.BaseStream.CopyToAsync($outFile),
            $process.StandardError.BaseStream.CopyToAsync($errFile)
        )
        $exited = $false
        if ($null -eq $Sample) { $exited = $process.WaitForExit($TimeoutSeconds * 1000) }
        else {
            while ($true) {
                & $Sample | Out-Null
                $left = [long]$TimeoutSeconds * 1000 - $watch.ElapsedMilliseconds
                if ($left -le 0) { break }
                if ($process.WaitForExit([int][Math]::Min($left, [long]$SampleMilliseconds))) { $exited = $true; break }
            }
        }
        if (-not $exited) {
            $kill = @{
                FilePath               = (Resolve-Tool 'taskkill')
                ArgumentList           = "/PID $($process.Id) /T /F"
                NoNewWindow            = $true
                PassThru               = $true
                Wait                   = $true
                RedirectStandardOutput = "$LogStem.kill.out.txt"
                RedirectStandardError  = "$LogStem.kill.err.txt"
            }
            $null = Start-Process @kill
            $null = $process.WaitForExit(5000)
            throw "Timed out after $TimeoutSeconds s (PID $($process.Id)): $FilePath $argumentLine"
        }
        $process.WaitForExit()
        $watch.Stop()
        $exitCode = $process.ExitCode
        $processId = $process.Id
        # Never report a run whose exit code could not be read as an ordinary failure with a blank code.
        if ($null -eq $exitCode) { throw "Exit code unavailable (PID $processId): $FilePath $argumentLine" }
    }
    finally {
        # The pipes close once every process holding them has exited. A straggler (a grandchild that inherited them)
        # still holding one after 10 s only cuts the logs off; the call does not wait for it.
        if ($copies.Count -gt 0) {
            $drained = $false
            try { $drained = [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]$copies, 10000) } catch { $drained = $true }
            if (-not $drained) { Write-Warning "A process still holds the output pipes of PID $($process.Id); the $LogStem logs are cut off" }
        }
        if ($null -ne $outFile) { $outFile.Dispose() }
        if ($null -ne $errFile) { $errFile.Dispose() }
        if ($null -ne $process) {
            if ($RemoveReviewProfile) { Remove-ReviewProfile -ProcessId $process.Id }
            $process.Dispose()
        }
    }
    return [pscustomobject]@{
        ExitCode = $exitCode
        Id       = $processId
        Seconds  = [Math]::Round($watch.Elapsed.TotalSeconds, 3)
        Out      = $outLog
        Err      = $errLog
    }
}

function Invoke-Cargo {
    # Invoke-Native for cargo in the workspace; a non-zero exit prints the last 40 err lines and throws.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$LogStem,
        [int]$TimeoutSeconds = 1800,
        [string]$WorkingDirectory = $script:NativeHordeWorkspace
    )
    $result = Invoke-Native -FilePath (Resolve-Tool 'cargo') -Arguments $Arguments -LogStem $LogStem -TimeoutSeconds $TimeoutSeconds -WorkingDirectory $WorkingDirectory
    if ($result.ExitCode -ne 0) {
        if (Test-Path -LiteralPath $result.Err) {
            foreach ($line in @(Get-Content -LiteralPath $result.Err -Tail 40)) { Write-Host $line }
        }
        throw "cargo $($Arguments -join ' ') failed with exit code $($result.ExitCode); see $($result.Err)"
    }
    return $result
}

function Test-PathUnder([string]$Path, [string]$Root) {
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + '\'
    return $Path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function Get-ForeignBuildSource {
    # The sources a build's uplifted dep-info (<exe>.d beside the exe cargo copies out) names outside -Workspace or
    # under -Target. Cargo keeps a source under the target dir relative to the target dir, so a tree extracted under a
    # shared CARGO_TARGET_DIR (git archive into <target>\<name>\src) and built there writes its own sources into this
    # workspace's fingerprint (one unit hash for both): the workspace build then stays fresh and `cargo run` starts that
    # tree's code (closing, plan 9: 9101dbe3 ran for three hours; the fix-ui*-stale-exe folders). OUT_DIR files
    # (<target>\...\build\<unit>\out\) belong to the build. When -Workspace is itself under -Target (the A/B script's
    # A arm), only its own folder counts. Returns the foreign paths in dep-info order (wrap the call in @()); throws on a missing dep-info.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$DepInfo,
        [string]$Workspace = $script:NativeHordeWorkspace,
        [string]$Target
    )
    if (-not $Target) { $Target = Resolve-HordeTarget -Workspace $Workspace }
    $DepInfo = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($DepInfo)
    if (-not (Test-Path -LiteralPath $DepInfo -PathType Leaf)) { throw "No dep-info at $DepInfo; the build wrote none" }
    $Workspace = [IO.Path]::GetFullPath($Workspace)
    $Target = [IO.Path]::GetFullPath($Target)
    $targetShadows = -not (Test-PathUnder $Workspace $Target)
    $foreign = New-Object System.Collections.Generic.List[string]
    $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($line in [IO.File]::ReadAllLines($DepInfo)) {
        # Make syntax: "<output>: <dep> <dep> ...", a space inside a path escaped as "\ ". A drive colon is never
        # followed by a space; later "<dep>:" lines list no dependencies.
        $colon = $line.IndexOf(': ')
        if ($line.StartsWith('#') -or $colon -lt 0) { continue }
        foreach ($token in [regex]::Split($line.Substring($colon + 2).Trim(), '(?<!\\) +')) {
            if (-not $token) { continue }
            # rustc writes an example's dep-info relative to the workspace it ran in; cargo's bin dep-info is absolute.
            $path = Resolve-WorkspacePath -Path ($token -replace '\\ ', ' ') -Workspace $Workspace
            if (-not $seen.Add($path)) { continue }
            $isForeign = -not (Test-PathUnder $path $Workspace)
            if (-not $isForeign -and $targetShadows -and (Test-PathUnder $path $Target)) {
                $isForeign = $path.Substring($Target.Length) -notmatch '\\build\\[^\\]+\\out\\'
            }
            if ($isForeign) { $foreign.Add($path) }
        }
    }
    return $foreign.ToArray()
}

function Invoke-HordeBuild {
    # Invoke-Cargo build, then Get-ForeignBuildSource on the built exe's dep-info. A build that took a foreign tree's
    # sources cleans the workspace packages in that profile and builds once more; one still foreign throws. Returns
    # Cleaned (whether it cleaned) and Foreign (the first build's foreign sources). -Cargo takes (Arguments, LogStem,
    # WorkingDirectory) for the self-tests; the default runs Invoke-Cargo.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$LogStem,
        [Parameter(Mandatory = $true)][string]$DepInfo,
        [string]$Workspace = $script:NativeHordeWorkspace,
        [string]$Target,
        [scriptblock]$Cargo = { param($CargoArguments, $CargoLog, $CargoDirectory) Invoke-Cargo -Arguments $CargoArguments -LogStem $CargoLog -WorkingDirectory $CargoDirectory | Out-Null }
    )
    if (-not $Target) { $Target = Resolve-HordeTarget -Workspace $Workspace }
    $profileName = 'dev'
    for ($i = 0; $i -lt $Arguments.Count; $i++) {
        if ($Arguments[$i] -eq '--release') { $profileName = 'release' }
        elseif ($Arguments[$i] -eq '--profile' -and $i + 1 -lt $Arguments.Count) { $profileName = $Arguments[$i + 1] }
    }
    & $Cargo $Arguments $LogStem $Workspace | Out-Null
    $foreign = @(Get-ForeignBuildSource -DepInfo $DepInfo -Workspace $Workspace -Target $Target)
    if ($foreign.Count -eq 0) { return [pscustomobject]@{ Cleaned = $false; Foreign = @() } }
    Write-Host "The $profileName build of this workspace took $($foreign.Count) source(s) from another tree (first: $($foreign[0])); cleaning the workspace packages and building again"
    & $Cargo @('clean', '--locked', '-p', 'horde-defense', '-p', 'horde_core', '-p', 'horde_audio', '--profile', $profileName) "$LogStem-clean" $Workspace | Out-Null
    & $Cargo $Arguments "$LogStem-rebuild" $Workspace | Out-Null
    $still = @(Get-ForeignBuildSource -DepInfo $DepInfo -Workspace $Workspace -Target $Target)
    if ($still.Count -gt 0) { throw "The $profileName build still takes sources from another tree after cargo clean: $($still -join ', ')" }
    return [pscustomobject]@{ Cleaned = $true; Foreign = $foreign }
}

function Get-ReviewSceneList {
    # Runs <Executable> --list-scenes through Invoke-Native and parses its TSV (name kind motion perf practice primary
    # before_proxy since_wave; the header and '#' lines are skipped). Returns the Invoke-Native result plus Scenes: one
    # object per row (Name, Kind, Motion, Perf, Practice, Primary, BeforeProxy, SinceWave; an empty column is $null),
    # or $null when the exit code is not 0, as for a build without --list-scenes. The raw TSV stays in Out.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$LogStem,
        [int]$TimeoutSeconds = 30,
        [string]$WorkingDirectory
    )
    $invoke = @{ FilePath = $Executable; Arguments = @('--list-scenes'); LogStem = $LogStem; TimeoutSeconds = $TimeoutSeconds }
    if ($WorkingDirectory) { $invoke.WorkingDirectory = $WorkingDirectory }
    $result = Invoke-Native @invoke
    $scenes = $null
    if ($result.ExitCode -eq 0) {
        $rows = New-Object System.Collections.Generic.List[object]
        foreach ($line in [IO.File]::ReadAllLines($result.Out)) {
            if (-not $line.Trim() -or $line.StartsWith('#')) { continue }
            $columns = @($line -split "`t")
            if ($columns[0] -eq 'name') { continue }
            $cells = New-Object object[] 8
            for ($i = 0; $i -lt 8 -and $i -lt $columns.Count; $i++) {
                if ($columns[$i].Length -gt 0) { $cells[$i] = $columns[$i] }
            }
            $rows.Add([pscustomobject]@{
                    Name        = $cells[0]
                    Kind        = $cells[1]
                    Motion      = ($cells[2] -eq 'true')
                    Perf        = ($cells[3] -eq 'true')
                    Practice    = ($cells[4] -eq 'true')
                    Primary     = $cells[5]
                    BeforeProxy = $cells[6]
                    SinceWave   = $(if ($cells[7] -match '^\d+$') { [int]$cells[7] } else { $null })
                })
        }
        $scenes = $rows.ToArray()
    }
    return [pscustomobject]@{
        ExitCode = $result.ExitCode
        Id       = $result.Id
        Seconds  = $result.Seconds
        Out      = $result.Out
        Err      = $result.Err
        Scenes   = $scenes
    }
}

function Get-MotionFlashProblem {
    # The flash gate of a motion manifest's frames (design-qa-captures 1.9, W5-10). Every frame must carry the numeric
    # fields flash_alpha_max, sky_flash and muzzle_flash_count. With -ReducedEffects all three are 0 in every frame.
    # With -Storm (the storm scene, normal effects) at least one frame has sky_flash > 0 and the pulse is lit in two
    # neighbouring frames, so it rises and decays rather than popping. The game writes sky_flash as lighting::sky_alpha
    # of a clamped pulse, so no manifest it writes can exceed the 0.14 peak alpha and the peak is not checked here; that
    # each frame's sky_flash follows the lightning schedule at its own tick is the Rust test
    # motion_exports_flash_in_normal_mode_and_never_in_reduced. sky_flash is an over-rect alpha, not a luminance: the
    # storm's frame-to-frame luminance step (G7.3, delta L < 0.2) is gated by measure_field_luminance.py storm on the
    # PNGs.
    # Returns the first problem as a string, or $null.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$Frames,
        [switch]$ReducedEffects,
        [switch]$Storm
    )
    $fields = @('flash_alpha_max', 'sky_flash', 'muzzle_flash_count')
    for ($i = 0; $i -lt $Frames.Count; $i++) {
        foreach ($field in $fields) {
            $property = $Frames[$i].PSObject.Properties[$field]
            if ($null -eq $property -or $null -eq $property.Value -or $property.Value -is [string] -or $property.Value -is [bool]) { return "Frame $i has no numeric $field" }
            if ([double]$property.Value -lt 0) { return "Frame $i has a negative $field" }
            if ($ReducedEffects -and [double]$property.Value -ne 0) { return "Reduced effects must not flash: frame $i has $field $($property.Value)" }
        }
    }
    if ($Storm -and -not $ReducedEffects) {
        if (@($Frames | Where-Object { [double]$_.sky_flash -gt 0 }).Count -eq 0) { return 'The storm export captured no lightning pulse (sky_flash is 0 in every frame)' }
        $held = $false
        for ($i = 1; $i -lt $Frames.Count; $i++) {
            if ([double]$Frames[$i].sky_flash -gt 0 -and [double]$Frames[$i - 1].sky_flash -gt 0) { $held = $true; break }
        }
        if (-not $held) { return 'The storm pulse pops: no two neighbouring frames have sky_flash > 0' }
    }
    return $null
}

function Remove-ReviewProfile {
    # profile.rs keeps a review build's settings and logs in <temp>\lastlight-review-<pid> and never deletes them.
    # A failed removal only warns, so cleanup never hides the run's own outcome.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][int]$ProcessId)
    $path = Join-Path ([IO.Path]::GetTempPath()) ('lastlight-review-' + $ProcessId)
    if (-not (Test-Path -LiteralPath $path)) { return }
    try { Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop }
    catch { Write-Warning "Could not remove the review profile ${path}: $($_.Exception.Message)" }
}

function Resolve-Tool {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$Name)
    return (Get-Command $Name -CommandType Application -ErrorAction Stop)[0].Source
}

function Reset-LastlightEnv {
    # Saves and clears every LASTLIGHT_* process variable; pass the result to Restore-LastlightEnv in finally.
    # WGPU_BACKEND is deliberately left alone.
    [CmdletBinding()]
    param()
    $saved = @{}
    foreach ($entry in @(Get-ChildItem Env: | Where-Object { $_.Name -like 'LASTLIGHT_*' })) {
        $saved[$entry.Name] = $entry.Value
        [Environment]::SetEnvironmentVariable($entry.Name, $null, 'Process')
    }
    return $saved
}

function Restore-LastlightEnv {
    # Clears any LASTLIGHT_* variable set since Reset-LastlightEnv, then restores the saved values.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][AllowEmptyCollection()][hashtable]$Saved)
    foreach ($entry in @(Get-ChildItem Env: | Where-Object { $_.Name -like 'LASTLIGHT_*' })) {
        [Environment]::SetEnvironmentVariable($entry.Name, $null, 'Process')
    }
    foreach ($name in $Saved.Keys) { [Environment]::SetEnvironmentVariable($name, $Saved[$name], 'Process') }
}

function Write-Utf8NoBom {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text
    )
    $full = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
    $directory = Split-Path -Parent $full
    if (-not (Test-Path -LiteralPath $directory)) { New-Item -ItemType Directory -Force -Path $directory | Out-Null }
    [IO.File]::WriteAllText($full, $Text, (New-Object Text.UTF8Encoding $false))
}

function Get-FileSha256 {
    # Lower-case hex SHA-256 of a file.
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}
