#requires -Version 5.1
# The interface_total re-gate (R31, plan 2.4): an interleaved A/B against the wave-1-close executable, under Windows
# PowerShell 5.1. A is built from the ab03da9 sources, B is HEAD. In one sitting the script alternates A and B, each
# arm one measure_performance.ps1 pass of progressed-peak at both sizes (dev, VSync on, the default pipelined state:
# the interface_total gate's scene, profile and flags), for -Pairs pairs (5 for the gate). Per size, B's median
# interface_total mean must be at most 1.05 x A's median (scripts/lib/AbInterfaceTotal.ps1).
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ab_interface_total.ps1
# Parameters:
#   -Pairs N                     1..20, default 5 (the R31 gate)
#   -OutputDirectory <dir>       default <target>/perf/<yyyyMMdd-HHmmss>-ab-interface-total; must be missing or empty
#   -NoBuild                     use -ExecutableA (required) and -ExecutableB (default <target>/debug/horde-defense.exe)
#   -ExecutableA, -ExecutableB   override an arm's executable (copied into <out>/exe-a|exe-b/debug/)
#   -WaitQuiet | -NoWaitQuiet    the R30 quiet window before the first pair (default on)
#   -QuietTimeoutMinutes N       1..1440, default 240; a quiet window that times out exits 2
#   -TimeoutSeconds N            per run, default 900
# Build: A is extracted with `git archive --format=zip ab03da9 horde-defense` into <target>/ab-w1/src (reused while
# <target>/ab-w1/src/.ab-commit names the same commit) and built there with the same CARGO_TARGET_DIR; its exe is copied
# out before B (the workspace) is built, because both builds write <target>/debug/horde-defense.exe. A copied exe has no
# assets folder beside it, so each arm reads the assets of the tree it was built from (main.rs asset_directory).
# Each arm is measure_performance.ps1 -Scene progressed-peak -Profile dev -NoBuild -NoBudget -NoWaitQuiet with
# CARGO_TARGET_DIR at that arm's exe folder, so every run keeps that script's validity (report, window size, display R27,
# CPU load R30) and its one rerun. A pair with an invalid or missing run makes its size invalid evidence. The display is
# held on for the whole sitting. R29 still holds: baseline.v3 is re-measured and recorded, but this A/B decides the gate.
# Exit codes: 0 pass, 1 B over 1.05 x A, 2 invalid evidence (an invalid pair, or no quiet window), 3 build or run failure.
# ab.json and ab.md are written to the output directory once it exists.
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$Pairs = '5',
    [string]$OutputDirectory,
    [switch]$NoBuild,
    [string]$ExecutableA,
    [string]$ExecutableB,
    [switch]$WaitQuiet,
    [switch]$NoWaitQuiet,
    [string]$QuietTimeoutMinutes = '240',
    [string]$TimeoutSeconds = '900',
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$Unexpected
)
$ErrorActionPreference = 'Stop'
trap { Write-Output "FAIL $($_.Exception.Message)"; exit 3 }

function ConvertTo-BoundedInt([string]$Name, [string]$Text, [int]$Minimum, [int]$Maximum) {
    $value = 0
    $styles = [Globalization.NumberStyles]::AllowLeadingSign
    if (-not [int]::TryParse($Text, $styles, [Globalization.CultureInfo]::InvariantCulture, [ref]$value) -or $value -lt $Minimum -or $value -gt $Maximum) {
        throw "-$Name '$Text' is not a whole number in $Minimum..$Maximum"
    }
    return $value
}

if ($null -ne $Unexpected -and $Unexpected.Count -gt 0) { throw "Unexpected argument(s): $($Unexpected -join ' ')" }
[int]$Pairs = ConvertTo-BoundedInt 'Pairs' $Pairs 1 20
[int]$QuietTimeoutMinutes = ConvertTo-BoundedInt 'QuietTimeoutMinutes' $QuietTimeoutMinutes 1 1440
[int]$TimeoutSeconds = ConvertTo-BoundedInt 'TimeoutSeconds' $TimeoutSeconds 1 86400
if ($WaitQuiet -and $NoWaitQuiet) { throw '-WaitQuiet and -NoWaitQuiet exclude each other' }
if ($NoBuild -and -not $ExecutableA) { throw '-NoBuild needs -ExecutableA (the wave-1-close executable)' }
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
. (Join-Path $PSScriptRoot 'lib\DisplayPower.ps1')
. (Join-Path $PSScriptRoot 'lib\MachineLoad.ps1')
. (Join-Path $PSScriptRoot 'lib\AbInterfaceTotal.ps1')
Add-Type -AssemblyName System.IO.Compression.FileSystem
$workspace = Split-Path -Parent $PSScriptRoot
$repository = Split-Path -Parent $workspace
$measurePath = Join-Path $PSScriptRoot 'measure_performance.ps1'
$budgetPath = Join-Path $workspace 'docs\PERF_BUDGET.json'
$budget = [IO.File]::ReadAllText($budgetPath) | ConvertFrom-Json
$rule = Get-AbRule
$sizeNames = @($budget.sizes.PSObject.Properties | ForEach-Object { $_.Name })
# The gate's present mode and pipelined state are baseline.v3's (measure_performance gates interface_total only there).
$gatePresent = [string]$budget.baseline.v3.present_mode
$gatePipelined = $budget.baseline.v3.pipelined
if (-not $gatePresent -or $gatePipelined -isnot [bool]) { throw 'PERF_BUDGET.json baseline.v3 has no present_mode and pipelined state' }
$invariant = [Globalization.CultureInfo]::InvariantCulture

function Format-AbNumber($Value) {
    if ($null -eq $Value) { return '-' }
    return ([double]$Value).ToString('0.####', $invariant)
}

function Get-GitLine([string[]]$Arguments, [string]$Stem) {
    # The first output line of a git command in the repository, or $null when it fails.
    try {
        $r = Invoke-Native -FilePath (Resolve-Tool 'git') -Arguments $Arguments -LogStem $Stem -TimeoutSeconds 60 -WorkingDirectory $repository
        if ($r.ExitCode -ne 0) { return $null }
        $line = @(Get-Content -LiteralPath $r.Out | Where-Object { $_.Trim() } | Select-Object -First 1)
        if ($line.Count -gt 0) { return $line[0].Trim() }
    }
    catch { return $null }
    return $null
}

$savedTarget = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
$target = Resolve-HordeTarget -Workspace $workspace
$startedUtc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
if (-not $OutputDirectory) { $destination = Join-Path $target ('perf\' + (Get-Date).ToString('yyyyMMdd-HHmmss') + '-ab-interface-total') }
else { $destination = Resolve-WorkspacePath $OutputDirectory -Workspace $workspace }
if (Test-Path -LiteralPath $destination) {
    if (-not (Test-Path -LiteralPath $destination -PathType Container)) { throw "Output path is a file: $destination" }
    if (@(Get-ChildItem -LiteralPath $destination -Force).Count -gt 0) {
        throw "Refusing non-empty evidence directory $destination; choose a fresh -OutputDirectory"
    }
}
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$tools = Join-Path $destination 'tools'

$abRuns = New-Object System.Collections.Generic.List[object]
$order = New-Object System.Collections.Generic.List[object]
$arms = [ordered]@{}
$quietRecord = $null
$abQuietFailure = $null
$verdict = $null
$failure = $null
$exitCode = 3
$displayHold = $null
$commitA = $null
$commitB = $null
$dirtyB = $null
try {
    # The sitting is held on as a whole: measure_performance's own hold restores this one when it releases (R27).
    $displayHold = Enter-DisplayHold
    $commitA = Get-GitLine @('rev-parse', '--verify', "$($rule.Commit)^{commit}") (Join-Path $tools 'git-commit-a')
    $commitB = Get-GitLine @('rev-parse', '--verify', 'HEAD') (Join-Path $tools 'git-commit-b')
    $status = Get-GitLine @('status', '--porcelain', '--', 'horde-defense') (Join-Path $tools 'git-status')
    $dirtyB = [bool]$status
    $sourceA = $null
    if ($ExecutableA) { $builtA = Resolve-WorkspacePath $ExecutableA -Workspace $workspace }
    else {
        if (-not $commitA) { throw "git cannot resolve $($rule.Commit)" }
        $abRoot = Join-Path $target 'ab-w1'
        $sourceRoot = Join-Path $abRoot 'src'
        $marker = Join-Path $sourceRoot '.ab-commit'
        $current = $null
        if (Test-Path -LiteralPath $marker) { $current = ([IO.File]::ReadAllText($marker)).Trim() }
        if ($current -ne $commitA) {
            if (Test-Path -LiteralPath $sourceRoot) { Remove-Item -LiteralPath $sourceRoot -Recurse -Force }
            New-Item -ItemType Directory -Force -Path $sourceRoot | Out-Null
            $zip = Join-Path $abRoot "$($rule.Commit).zip"
            $archive = Invoke-Native -FilePath (Resolve-Tool 'git') -Arguments @('archive', '--format=zip', "--output=$zip", $commitA, 'horde-defense') -LogStem (Join-Path $tools 'git-archive') -TimeoutSeconds 300 -WorkingDirectory $repository
            if ($archive.ExitCode -ne 0) { throw "git archive $($rule.Commit) failed with exit code $($archive.ExitCode); see $($archive.Err)" }
            [IO.Compression.ZipFile]::ExtractToDirectory($zip, $sourceRoot)
            Remove-Item -LiteralPath $zip -Force
            Write-Utf8NoBom -Path $marker -Text $commitA
        }
        $sourceA = Join-Path $sourceRoot 'horde-defense'
        Invoke-Cargo -Arguments @('build', '--locked', '-p', 'horde-defense') -LogStem (Join-Path $tools 'cargo-a') -WorkingDirectory $sourceA | Out-Null
        $builtA = Get-HordeExecutable -Profile dev -Workspace $workspace
    }
    $exeA = Join-Path $destination 'exe-a\debug\horde-defense.exe'
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $exeA) | Out-Null
    Copy-Item -LiteralPath $builtA -Destination $exeA
    if ($ExecutableB) { $builtB = Resolve-WorkspacePath $ExecutableB -Workspace $workspace }
    else {
        if (-not $NoBuild) { Invoke-Cargo -Arguments @('build', '--locked', '-p', 'horde-defense') -LogStem (Join-Path $tools 'cargo-b') | Out-Null }
        $builtB = Get-HordeExecutable -Profile dev -Workspace $workspace
    }
    $exeB = Join-Path $destination 'exe-b\debug\horde-defense.exe'
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $exeB) | Out-Null
    Copy-Item -LiteralPath $builtB -Destination $exeB
    $arms['A'] = [ordered]@{ label = "wave-1 close ($($rule.Commit))"; commit = $commitA; source = $sourceA; built_from = $builtA; executable = 'exe-a/debug/horde-defense.exe'; executable_sha256 = (Get-FileSha256 -Path $exeA) }
    $arms['B'] = [ordered]@{ label = 'HEAD'; commit = $commitB; dirty = $dirtyB; built_from = $builtB; executable = 'exe-b/debug/horde-defense.exe'; executable_sha256 = (Get-FileSha256 -Path $exeB) }

    # R30: one quiet window before the sitting; every run still samples the load (measure_performance).
    $quietRecord = [ordered]@{ enabled = (-not $NoWaitQuiet); timeout_minutes = $QuietTimeoutMinutes; wait = $null }
    if (-not $NoWaitQuiet) {
        Write-Output "quiet window before the A/B (timeout $QuietTimeoutMinutes min)"
        $wait = Wait-QuietMachine -TimeoutMinutes $QuietTimeoutMinutes -OnLog { param($Line) Write-Host $Line }
        $quietRecord.wait = [ordered]@{
            quiet = $wait.Quiet; timed_out = $wait.TimedOut; samples = $wait.Samples; seconds = $wait.Seconds
            mean_percent = $wait.MeanPercent; max_percent = $wait.MaxPercent; waited_for = @($wait.WaitedFor); log = @($wait.Log)
        }
        if ($wait.TimedOut) {
            $abQuietFailure = "quiet window timed out after $QuietTimeoutMinutes min before the A/B " + (@($wait.Log)[-1] -replace '^quiet window: timed out after \d+ min ', '')
            throw $abQuietFailure
        }
    }

    for ($pair = 1; $pair -le $Pairs; $pair++) {
        foreach ($arm in @('A', 'B')) {
            $name = "pair-$pair-$($arm.ToLowerInvariant())"
            $armTarget = Join-Path $destination "exe-$($arm.ToLowerInvariant())"
            [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $armTarget, 'Process')
            $named = @{
                Scene = $rule.Scene; Profile = $rule.Profile; NoBuild = $true; NoBudget = $true; NoWaitQuiet = $true
                OutputDirectory = (Join-Path $destination $name); TimeoutSeconds = [string]$TimeoutSeconds
            }
            Write-Output "== $name"
            $global:LASTEXITCODE = $null
            # measure_performance is dot-sourced into a scriptblock scope of this process, so its exit ends only that
            # block and the shims and functions of this session reach it. Its $script: variables (waitQuiet, quietWaits,
            # quietFailure) land in this script's scope, so this script keeps its own state under other names.
            $lines = @(& { param($Arguments) . $measurePath @Arguments } $named)
            $code = $LASTEXITCODE
            foreach ($line in $lines) { Write-Output "   $line" }
            $summaryPath = Join-Path $destination "$name\summary.json"
            if ($code -eq 3 -or -not (Test-Path -LiteralPath $summaryPath)) { throw "measure_performance for $name failed (exit $code); see $name\summary.md" }
            $summary = [IO.File]::ReadAllText($summaryPath) | ConvertFrom-Json
            foreach ($run in @(ConvertTo-AbRun $summary $pair $arm $gatePresent $gatePipelined)) { $abRuns.Add($run) }
            $order.Add([ordered]@{ pair = $pair; arm = $arm; directory = $name; measure_exit = $code; started_utc = $summary.started_utc; finished_utc = $summary.finished_utc })
        }
    }
    $verdict = Measure-AbInterfaceTotal $abRuns.ToArray() $sizeNames $Pairs $rule.Tolerance
    $exitCode = $verdict.ExitCode
}
catch {
    $failure = $_.Exception.Message
    if ($abQuietFailure) { $exitCode = 2; Write-Output "INVALID $failure" }
    else { $exitCode = 3; Write-Output "FAIL $failure" }
}
finally {
    Exit-DisplayHold $displayHold
    [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $savedTarget, 'Process')
    $labels = @{ 0 = 'pass'; 1 = 'B over 1.05 x A'; 2 = 'invalid evidence'; 3 = 'build or run failure' }
    $checks = @()
    if ($null -ne $verdict) { $checks = @($verdict.Checks) }
    $runRows = @($abRuns | ForEach-Object {
            [ordered]@{ pair = $_.Pair; arm = $_.Arm; size = $_.Size; interface_total_mean_ms = $_.Value; valid = $_.Valid; problems = @($_.Problems); file = $_.File }
        })
    $record = [ordered]@{
        version = 1; ruling = 'R31'; started_utc = $startedUtc; finished_utc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
        exit_code = $exitCode; verdict = $labels[$exitCode]; failure = $failure
        scene = $rule.Scene; profile = $rule.Profile; present_mode = $gatePresent; pipelined = $gatePipelined
        pairs = $Pairs; tolerance = $rule.Tolerance; sizes = $sizeNames; cargo_target_dir = $target
        arms = $arms; quiet_window = $quietRecord
        display_hold = $(if ($null -ne $displayHold) { [ordered]@{ held = $displayHold.Held; released = $displayHold.Released; released_from = $displayHold.ReleasedFrom } } else { $null })
        order = @($order.ToArray()); runs = $runRows; checks = $checks
    }
    Write-Utf8NoBom -Path (Join-Path $destination 'ab.json') -Text ($record | ConvertTo-Json -Depth 8)
    $md = New-Object System.Collections.Generic.List[string]
    $md.Add('# interface_total A/B (R31)')
    $md.Add('')
    $md.Add("- Verdict: exit $exitCode ($($labels[$exitCode]))")
    if ($failure) { $md.Add("- Failure: $failure") }
    $md.Add("- $($rule.Scene), $($rule.Profile), present mode $gatePresent, pipelined $gatePipelined; $Pairs pairs per size, A then B in each pair; B median <= $(Format-AbNumber $rule.Tolerance) x A median per size")
    foreach ($key in $arms.Keys) { $md.Add("- $key $($arms[$key].label): commit $($arms[$key].commit), $($arms[$key].executable) ($($arms[$key].executable_sha256))") }
    if ($null -ne $quietRecord -and $null -ne $quietRecord.wait) {
        $md.Add("- Quiet window (R30): $(if ($quietRecord.wait.quiet) { 'quiet' } else { 'timed out' }) after $($quietRecord.wait.samples) samples, mean CPU load $(Format-AbNumber $quietRecord.wait.mean_percent)%")
    }
    elseif ($NoWaitQuiet) { $md.Add('- Quiet window off (-NoWaitQuiet)') }
    $md.Add('')
    $md.Add('| Size | A values | B values | A median | B median | B / A | Limit | Status | Note |')
    $md.Add('|---|---|---|---|---|---|---|---|---|')
    foreach ($c in $checks) {
        $md.Add("| $($c.size) | $((@($c.a_values) | ForEach-Object { Format-AbNumber $_ }) -join ', ') | $((@($c.b_values) | ForEach-Object { Format-AbNumber $_ }) -join ', ') | $(Format-AbNumber $c.a_median) | $(Format-AbNumber $c.b_median) | $(Format-AbNumber $c.ratio) | $(Format-AbNumber $c.limit) | $($c.status) | $($c.note) |")
    }
    $md.Add('')
    $md.Add('| Pair | Arm | Size | interface_total mean ms | Valid | Problems |')
    $md.Add('|---|---|---|---|---|---|')
    foreach ($row in $runRows) { $md.Add("| $($row.pair) | $($row.arm) | $($row.size) | $(Format-AbNumber $row.interface_total_mean_ms) | $($row.valid) | $(@($row.problems) -join '; ') |") }
    Write-Utf8NoBom -Path (Join-Path $destination 'ab.md') -Text (($md -join "`n") + "`n")
}
Write-Output "A/B evidence: $destination (exit $exitCode, $($labels[$exitCode]))"
exit $exitCode
