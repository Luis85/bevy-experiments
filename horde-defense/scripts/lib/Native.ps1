#requires -Version 5.1
# Shared Windows PowerShell 5.1 harness for scripts/. Dot-source it: . (Join-Path $PSScriptRoot 'lib\Native.ps1')
# Native processes always run through Invoke-Native (Start-Process with redirected streams), because
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
    $start = @{
        FilePath               = $FilePath
        NoNewWindow            = $true
        PassThru               = $true
        RedirectStandardOutput = $outLog
        RedirectStandardError  = $errLog
    }
    $argumentLine = ConvertTo-NativeArgument -Arguments $Arguments
    # An empty -ArgumentList throws in 5.1, so it is passed only when there is something to pass.
    if ($argumentLine.Length -gt 0) { $start.ArgumentList = $argumentLine }
    if ($WorkingDirectory) { $start.WorkingDirectory = $WorkingDirectory }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process @start
    # Touching Handle keeps ExitCode readable after the process exits (5.1 Start-Process quirk).
    $null = $process.Handle
    try {
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
    }
    finally {
        if ($RemoveReviewProfile) { Remove-ReviewProfile -ProcessId $process.Id }
    }
    return [pscustomobject]@{
        ExitCode = $process.ExitCode
        Id       = $process.Id
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
