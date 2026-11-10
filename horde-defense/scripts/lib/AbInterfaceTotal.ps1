#requires -Version 5.1
# The interface_total A/B decision (R31, plan 2.4). The re-gate alternates A (the wave-1-close executable, sources at
# ab03da9) and B (HEAD) in one sitting, 5 pairs per size, on the scene, profile and flags of the interface_total gate
# (progressed-peak, dev, VSync on, the default pipelined state). Per size, B's median interface_total must be at most
# 1.05 x A's median. Every run of every pair must be valid evidence (measure_performance.ps1's run validity, including
# the R27 display and R30 load checks) and echo the gate's present mode and pipelined state; a pair with an invalid or
# missing run makes its size invalid evidence.
# Dot-source it: . (Join-Path $PSScriptRoot 'lib\AbInterfaceTotal.ps1')
# Self-tests: scripts/tests/run_script_tests.ps1 (fixtures for pass, fail and an invalid pair).

function Get-AbRule {
    return [pscustomobject]@{ Commit = 'ab03da9'; Pairs = 5; Tolerance = 1.05; Scene = 'progressed-peak'; Profile = 'dev' }
}

function Invoke-AbArmBuild([scriptblock]$Cargo, [string]$LogStem, [string]$WorkingDirectory) {
    # Builds one arm into the shared CARGO_TARGET_DIR. Both arms are the same workspace packages (horde-defense,
    # horde_core) in the same dev profile, and cargo keeps one fingerprint and one uplifted dev executable for
    # them, so the second arm's build read the first arm's sources as fresh and left the first arm's executable in place
    # (W2-47: both arms of the first re-gate ran ab03da9). Cleaning the two packages first makes every arm a real build.
    # $Cargo takes (Arguments, LogStem, WorkingDirectory); the A/B script passes its cargo runner.
    $Cargo.Invoke(@(@('clean', '-p', 'horde-defense', '-p', 'horde_core'), "$LogStem-clean", $WorkingDirectory)) | Out-Null
    $Cargo.Invoke(@(@('build', '--locked', '-p', 'horde-defense'), $LogStem, $WorkingDirectory)) | Out-Null
}

function Restore-AbWorkspaceBuild([scriptblock]$Cargo, [bool]$ABuilt, [bool]$BBuilt, [string]$LogStem, [string]$Workspace) {
    # A's build (Invoke-AbArmBuild in the ab03da9 tree) leaves its fingerprints and its executable in the shared
    # CARGO_TARGET_DIR under the workspace's own package names, so the next plain workspace build (measure_performance,
    # cargo run --smoke) would be fresh and keep A (W2-47 review). Unless B was built after it, clean the two packages
    # in the workspace so that build is HEAD's. Returns $true when it cleaned.
    if (-not $ABuilt -or $BBuilt) { return $false }
    $Cargo.Invoke(@(@('clean', '-p', 'horde-defense', '-p', 'horde_core'), $LogStem, $Workspace)) | Out-Null
    return $true
}

function Assert-AbArmsDiffer([string]$ShaA, [string]$ShaB) {
    # An A/B of one executable against itself is no evidence: throw before any pair runs.
    if ($ShaA.ToLowerInvariant() -eq $ShaB.ToLowerInvariant()) {
        throw "A and B are the same executable (sha256 $($ShaA.ToLowerInvariant())); an arm's build did not replace the other's"
    }
}

function Get-AbMedian([double[]]$Values) {
    if ($null -eq $Values -or $Values.Count -eq 0) { return $null }
    $sorted = [double[]]$Values.Clone()
    [Array]::Sort($sorted)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) { return $sorted[$middle] }
    return ($sorted[$middle - 1] + $sorted[$middle]) / 2
}

function Test-AbNumber($Value) { return ($null -ne $Value -and $Value -is [ValueType] -and $Value -isnot [bool]) }

function ConvertTo-AbRun($Summary, [int]$Pair, [string]$Arm, [string]$PresentMode, $Pipelined) {
    # One A/B run per size from a measure_performance.ps1 summary.json: Pair, Arm, Size, Value (interface_total mean ms),
    # Valid and Problems. A run is valid when measure_performance found it valid and it echoes the gate's present mode
    # and pipelined state and carries an interface_total mean.
    foreach ($run in @($Summary.runs)) {
        $problems = New-Object System.Collections.Generic.List[string]
        if ($run.valid -ne $true) {
            $why = @($run.problems) -join '; '
            if (-not $why) { $why = 'invalid' }
            $problems.Add($why)
        }
        $value = $run.interface_total_mean_ms
        if (-not (Test-AbNumber $value)) { $problems.Add('no interface_total_mean_ms') }
        if ($run.present_mode -ne $PresentMode) { $problems.Add("present mode '$($run.present_mode)', the gate's is '$PresentMode'") }
        if ($run.pipelined_rendering -isnot [bool] -or $run.pipelined_rendering -ne $Pipelined) {
            $problems.Add("pipelined_rendering '$($run.pipelined_rendering)', the gate's is '$Pipelined'")
        }
        $number = $null
        if (Test-AbNumber $value) { $number = [double]$value }
        [pscustomobject]@{
            Pair = $Pair; Arm = $Arm; Size = [string]$run.size; Value = $number; Valid = ($problems.Count -eq 0)
            Problems = $problems.ToArray(); File = $run.file
        }
    }
}

function Measure-AbInterfaceTotal($Runs, [string[]]$Sizes, [int]$Pairs = 5, [double]$Tolerance = 1.05) {
    # One check per size: pass when every pair 1..Pairs has a valid A and B run and B's median <= Tolerance x A's
    # median, over when B's median is above it, invalid when a pair has an invalid or missing run. Exit code 2 when any
    # size is invalid, else 1 when any is over, else 0.
    $checks = New-Object System.Collections.Generic.List[object]
    foreach ($size in $Sizes) {
        $sized = @($Runs | Where-Object { $_.Size -eq $size })
        $bad = New-Object System.Collections.Generic.List[string]
        $a = New-Object System.Collections.Generic.List[double]
        $b = New-Object System.Collections.Generic.List[double]
        for ($pair = 1; $pair -le $Pairs; $pair++) {
            $faults = @()
            foreach ($arm in @('A', 'B')) {
                $run = @($sized | Where-Object { $_.Pair -eq $pair -and $_.Arm -eq $arm })
                if ($run.Count -eq 0) { $faults += "$arm missing"; continue }
                if ($run.Count -gt 1) { $faults += "$arm measured $($run.Count) times"; continue }
                if (-not $run[0].Valid) { $faults += "$arm invalid ($(@($run[0].Problems) -join '; '))"; continue }
                if ($arm -eq 'A') { $a.Add($run[0].Value) } else { $b.Add($run[0].Value) }
            }
            if ($faults.Count -gt 0) { $bad.Add("pair ${pair}: $($faults -join ', ')") }
        }
        $check = [ordered]@{
            size = $size; pairs = $Pairs; tolerance = $Tolerance; a_values = $a.ToArray(); b_values = $b.ToArray()
            a_median = $null; b_median = $null; ratio = $null; limit = $null; status = 'invalid'; note = ''
        }
        if ($bad.Count -gt 0) { $check.note = ($bad -join '; ') }
        else {
            $check.a_median = Get-AbMedian $a.ToArray()
            $check.b_median = Get-AbMedian $b.ToArray()
            $check.limit = $Tolerance * $check.a_median
            if ($check.a_median -gt 0) { $check.ratio = $check.b_median / $check.a_median }
            if ($check.b_median -le $check.limit) { $check.status = 'pass' } else { $check.status = 'over' }
        }
        $checks.Add([pscustomobject]$check)
    }
    $exitCode = 0
    if (@($checks | Where-Object { $_.status -eq 'invalid' }).Count -gt 0) { $exitCode = 2 }
    elseif (@($checks | Where-Object { $_.status -eq 'over' }).Count -gt 0) { $exitCode = 1 }
    return [pscustomobject]@{ ExitCode = $exitCode; Checks = $checks.ToArray() }
}
