#requires -Version 5.1
# Machine load for perf evidence (R30, plan 2.4). A perf pass starts only in a quiet window: no foreign build or test
# process runs (the pass's own process tree is ignored) and the mean CPU load (Win32_Processor.LoadPercentage, sampled
# every 5 s) over the last 60 s is under 35%. A 5 s sampler runs beside every run, and a run with any load sample of
# 90% or more is invalid evidence.
# Dot-source it: . (Join-Path $PSScriptRoot 'lib\MachineLoad.ps1')
# The two system queries are Get-CimInstance calls in Get-ProcessTable and Get-CpuLoadPercent, the waits are
# Start-Sleep and the clock is Get-MachineLoadClock. Self-tests: scripts/tests/run_script_tests.ps1 shims
# Get-CimInstance, Start-Sleep and the clock, or the functions Wait-QuietMachine calls.

function Get-MachineLoadRule {
    # The R30 numbers. A quiet window is WindowSeconds / IntervalSeconds + 1 samples (13), so it spans 60 s.
    return [pscustomobject]@{ IntervalSeconds = 5; WindowSeconds = 60; QuietBelowPercent = 35; InvalidAtPercent = 90; TimeoutMinutes = 240 }
}

function Get-BuildProcessName {
    # The image names, without their extension, that count as a build or test process: cargo and its tools, the
    # compilers and the linkers. (Without the extension so the 5.1 lint does not read the list as native starts.)
    return @('cargo', 'rustc', 'cargo-nextest', 'cargo-clippy', 'clippy-driver', 'rustdoc', 'rustfmt', 'link', 'lld-link',
        'rust-lld', 'cl', 'msbuild')
}

function Get-ProcessTable {
    # Every process as ProcessId, ParentProcessId and Name (Win32_Process). Throws when the table cannot be read.
    @(Get-CimInstance -ClassName Win32_Process -Property ProcessId, ParentProcessId, Name -ErrorAction Stop) | ForEach-Object {
        [pscustomobject]@{ ProcessId = [int]$_.ProcessId; ParentProcessId = [int]$_.ParentProcessId; Name = [string]$_.Name }
    }
}

function Get-ForeignBuildProcess([int]$RootId = $PID) {
    # The build and test processes outside RootId's process tree, by ProcessId. RootId is the pass itself, so its own
    # cargo build and the game it starts never count. Throws when the process table cannot be read.
    $table = @(Get-ProcessTable)
    $own = New-Object 'System.Collections.Generic.HashSet[int]'
    [void]$own.Add($RootId)
    do {
        $grew = $false
        foreach ($entry in $table) {
            if ($own.Contains($entry.ParentProcessId) -and $entry.ProcessId -ne $entry.ParentProcessId -and $own.Add($entry.ProcessId)) { $grew = $true }
        }
    } while ($grew)
    $names = Get-BuildProcessName
    $table | Where-Object { -not $own.Contains($_.ProcessId) -and $names -contains [IO.Path]::GetFileNameWithoutExtension($_.Name) } |
        Sort-Object -Property ProcessId
}

function Format-BuildProcess($Processes) {
    # "cargo.exe (4242), rustc.exe (4243)".
    return (@($Processes) | ForEach-Object { "$($_.Name) ($($_.ProcessId))" }) -join ', '
}

function Get-CpuLoadPercent {
    # The mean Win32_Processor.LoadPercentage over every processor (socket), or $null when it cannot be read.
    try { $found = @(Get-CimInstance -ClassName Win32_Processor -Property LoadPercentage -ErrorAction Stop) }
    catch { return $null }
    $values = @($found | ForEach-Object { $_.LoadPercentage } | Where-Object { $null -ne $_ })
    if ($values.Count -eq 0) { return $null }
    return [double](($values | Measure-Object -Average).Average)
}

function Get-MachineLoadClock {
    # The wall clock (UTC) that Wait-QuietMachine measures its timeout and its elapsed seconds by.
    return [DateTime]::UtcNow
}

function Format-Percent($Value) {
    if ($null -eq $Value) { return '-' }
    return ([double]$Value).ToString('0.####', [Globalization.CultureInfo]::InvariantCulture)
}

function Wait-QuietMachine {
    # Samples the process table and the CPU load every 5 s until the last 13 samples (60 s) saw no foreign build
    # process and their mean load is under 35%, or until TimeoutMinutes of wall time have passed (Get-MachineLoadClock;
    # no new sample starts after the deadline, and never more than TimeoutMinutes x 12 + 1 samples, since the queries
    # take time beside each 5 s sleep). A sample with a foreign build, an unreadable process table or an unreadable
    # load restarts the window. OnLog gets one line whenever what it waits for changes. Returns Quiet, TimedOut,
    # Samples, Seconds (the elapsed wall time, whole seconds), MeanPercent and MaxPercent (of the last window),
    # WaitedFor (what held the start, in order) and Log.
    [CmdletBinding()]
    param([int]$TimeoutMinutes = 240, [int]$RootId = $PID, [scriptblock]$OnLog)
    $rule = Get-MachineLoadRule
    $needed = [int]($rule.WindowSeconds / $rule.IntervalSeconds) + 1
    $limit = [int]($TimeoutMinutes * 60 / $rule.IntervalSeconds) + 1
    $started = Get-MachineLoadClock
    $deadline = $started.AddMinutes($TimeoutMinutes)
    $windowLoads = New-Object System.Collections.Generic.List[double]
    $quietLines = New-Object System.Collections.Generic.List[string]
    $heldBy = New-Object System.Collections.Generic.List[string]
    $state = $null
    $waitingFor = $null
    $sample = 0
    $quiet = $false
    $mean = $null
    $max = $null
    $note = {
        param([string]$Text)
        $quietLines.Add("quiet window: $Text")
        if ($OnLog) { & $OnLog "quiet window: $Text" }
    }
    while ($sample -lt $limit) {
        if ($sample -gt 0) {
            if ((Get-MachineLoadClock) -ge $deadline) { break }
            Start-Sleep -Seconds $rule.IntervalSeconds
        }
        $sample += 1
        $foreign = @()
        $tableProblem = $null
        try { $foreign = @(Get-ForeignBuildProcess -RootId $RootId) }
        catch { $tableProblem = $_.Exception.Message }
        $load = Get-CpuLoadPercent
        $key = $null
        $text = $null
        $label = $null
        if ($null -ne $tableProblem) {
            $key = 'table'; $label = 'readable process table'; $text = "waiting for a readable process table ($tableProblem)"
        }
        elseif ($foreign.Count -gt 0) {
            $names = @($foreign | ForEach-Object { $_.Name.ToLowerInvariant() } | Select-Object -Unique) -join ', '
            $key = "build:$names"; $label = "build processes: $names"; $text = "waiting for build processes: $(Format-BuildProcess $foreign)"
        }
        elseif ($null -eq $load) {
            $key = 'load-unknown'; $label = 'readable CPU load'; $text = 'waiting for a readable CPU load (Win32_Processor.LoadPercentage)'
        }
        if ($null -ne $key) { $windowLoads.Clear() }
        else {
            $windowLoads.Add([double]$load)
            if ($windowLoads.Count -gt $needed) { $windowLoads.RemoveAt(0) }
            $measured = $windowLoads | Measure-Object -Average -Maximum
            $mean = [double]$measured.Average
            $max = [double]$measured.Maximum
            if ($windowLoads.Count -lt $needed) { $key = 'settling'; $text = "sampling the CPU load for $($rule.WindowSeconds) s" }
            elseif ($mean -lt $rule.QuietBelowPercent) { $quiet = $true }
            else {
                $key = 'load'; $label = 'mean CPU load'
                $text = "waiting for the mean CPU load to fall under $($rule.QuietBelowPercent)% ($(Format-Percent $mean)% over $($rule.WindowSeconds) s)"
            }
        }
        if ($quiet) { break }
        if ($key -ne $state) {
            $null = & $note $text
            $state = $key
            if ($label -and -not $heldBy.Contains($label)) { $heldBy.Add($label) }
        }
        if ($key -eq 'settling') { $waitingFor = "waiting for $needed load samples ($($windowLoads.Count) so far)" }
        else { $waitingFor = $text }
    }
    $seconds = [int][Math]::Round(((Get-MachineLoadClock) - $started).TotalSeconds)
    if ($quiet) {
        $null = & $note "quiet after $sample samples ($seconds s): mean CPU load $(Format-Percent $mean)% over $($rule.WindowSeconds) s (under $($rule.QuietBelowPercent)%), max $(Format-Percent $max)%, no foreign build process"
    }
    else {
        if (-not $waitingFor) { $waitingFor = "waiting for $needed load samples" }
        $null = & $note "timed out after $TimeoutMinutes min ($sample samples, $seconds s): still $waitingFor"
    }
    if ($windowLoads.Count -eq 0) { $mean = $null; $max = $null }
    return [pscustomobject]@{
        Quiet = $quiet; TimedOut = (-not $quiet); Samples = $sample; Seconds = $seconds; TimeoutMinutes = $TimeoutMinutes
        MeanPercent = $mean; MaxPercent = $max; WaitedFor = $heldBy.ToArray(); Log = $quietLines.ToArray()
    }
}

function Test-RunLoad {
    # The load check of one run from its samples (Get-CpuLoadPercent, $null when unreadable): Samples, Mean and Max of
    # the readable ones, and the problems that make the run invalid evidence: any sample at 90% or more, or a sample that
    # could not be read (an unverified run is not evidence either).
    [CmdletBinding()]
    param([AllowEmptyCollection()][AllowNull()][object[]]$Samples = @())
    $rule = Get-MachineLoadRule
    $all = @($Samples)
    $values = @($all | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
    $mean = $null
    $max = $null
    if ($values.Count -gt 0) {
        $measured = $values | Measure-Object -Average -Maximum
        $mean = [double]$measured.Average
        $max = [double]$measured.Maximum
    }
    $problems = @()
    if ($all.Count -eq 0) { $problems += 'CPU load unknown (no sample)' }
    elseif ($values.Count -lt $all.Count) { $problems += "CPU load unknown in $($all.Count - $values.Count) of $($all.Count) samples" }
    if ($null -ne $max -and $max -ge $rule.InvalidAtPercent) {
        $list = @($values | ForEach-Object { Format-Percent $_ }) -join ', '
        $problems += "CPU load $(Format-Percent $max)% during the run (a sample at $($rule.InvalidAtPercent)% or more is invalid evidence; samples $list)"
    }
    return [pscustomobject]@{ Samples = $all.Count; Mean = $mean; Max = $max; Problems = $problems }
}
