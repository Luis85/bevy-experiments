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
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [AllowEmptyString()][AllowEmptyCollection()][string[]]$Arguments = @(),
        [Parameter(Mandatory = $true)][string]$LogStem,
        [int]$TimeoutSeconds = 300,
        [string]$WorkingDirectory,
        [switch]$RemoveReviewProfile
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
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
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
