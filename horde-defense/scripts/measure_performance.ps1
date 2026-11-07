#requires -Version 5.1
# Native real-time frame evidence (frame report v3) compared against docs/PERF_BUDGET.json, under Windows
# PowerShell 5.1. Deliberately separate from the deterministic motion export.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_performance.ps1 -Scene all -Profile dev -VsyncOff -Repeat 3
#      powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_performance.ps1 -SelfTest
# Parameters:
#   -Scene peak|progressed-peak|stress|all   all = every scene of PERF_BUDGET.json run.frames, in that order
#   -Profile dev|playtest                    dev builds into <target>/debug, playtest adds --profile playtest
#                                            (both with default features, so the review fixtures exist)
#   -VsyncOff                                LASTLIGHT_PERF_PRESENT_MODE=autonovsync; only these runs carry the
#                                            cpu_frame p99 gate
#   -Pipelined default|on|off                LASTLIGHT_PERF_PIPELINED (default leaves the plan alone)
#   -Frames N                                0 = the budget's per-scene run.frames; otherwise 300..3600
#   -Warmup N                                default the budget's run.warmup_frames (180)
#   -Repeat N                                runs per scene and size (3 for acceptance)
#   -Adapter <name>                          WGPU_ADAPTER_NAME; an informational run that never fails the budget. Bevy
#                                            falls back to the default GPU when no adapter matches, so a run is valid
#                                            only when its report's adapter contains the name (case-insensitive)
#   -OutputDirectory <dir>                   default <target>/perf/<yyyyMMdd-HHmmss>-<profile>[-novsync]; must
#                                            be missing or empty
#   -NoBuild, -NoBudget                      skip the cargo build / the budget compare (validity still counts)
#   -SaveClone                               also runs the horde_core save_clone example in the same profile and
#                                            gates its save_snapshot p99 against PERF_BUDGET.json save_clone (W1-50)
#   -ShippedSmoke                            first builds --profile playtest --no-default-features, copies that exe
#                                            to <out>/shipped/, times --smoke three times (median wall seconds),
#                                            and only then builds the review variant
#   -SelfTest                                evaluates scripts/fixtures/perf/*.json and the rerun rule; no build
# <target> is CARGO_TARGET_DIR (relative values are joined to the workspace), else <workspace>/target.
# Every run sets LASTLIGHT_PERF_ACTIVE_ONLY=1, LASTLIGHT_PERF_NPC_SPEED=1, LASTLIGHT_PERF_PROFILE=<profile> and a fresh
# LASTLIGHT_SAVE_DIR, and writes <scene>-<size>[-rN].json. A run is valid only when the report is version 3, met its
# sample target, is not paused, had no unfocused sample frame, and echoes the requested present mode, pipelined state,
# scene, profile and (with -Adapter) adapter. An invalid run is rerun once (the rejected report is kept as
# <stem>.invalid.json).
# Budget compare: repeats by median, frames_over_50ms and egui texture uploads by the worst repeat. frames_over_50ms and
# egui.texture_uploads.count (against egui.texture_uploads_after_warmup_max, every scene, W1-64) always; frames_over_50ms
# counts the frames outside driver stalls (driver_stalls.outside.frames_over_50ms: W1-61 user ruling 'Exclude driver
# stalls', PERF_BUDGET.json driver_stall; the report and summary still count and list every stall); cpu_frame p99 only
# with -VsyncOff; interface_draw (v2 work_mean_ms[2], ui::draw) only when profile and VSync match baseline.wave0;
# interface_total only against baseline.v3.interface_total_mean_ms.<scene>.<size> when baseline.v3 is recorded and its
# profile and vsync match the run and every repeat echoes its present_mode and pipelined state; a run under another
# power plan than baseline.v3.power_plan (powercfg /getactivescheme) is still gated, with a 'noted' baseline_v3_conditions
# check naming both plans. A gated metric missing from any repeat, or a recorded baseline.v3 without that layout
# (perf_budget.rs pins it), is invalid evidence (exit 2), never a pass or a skip.
# Exit codes: 0 pass, 1 over budget, 2 invalid evidence, 3 build or run failure. summary.json and summary.md are
# always written to the output directory once it exists.
# Arguments are plain strings validated in the body: a binding failure (ValidateSet, ValidateRange, an [int] that does not
# parse, an unknown or positional argument) would exit 1 under -File before the trap exists, and 1 means over budget.
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$Scene = 'all',
    [string]$Profile = 'dev',
    [switch]$VsyncOff,
    [string]$Pipelined = 'default',
    [string]$Frames = '0',
    [string]$Warmup = '-1',
    [string]$Repeat = '1',
    [string]$Adapter,
    [string]$OutputDirectory,
    [switch]$NoBuild,
    [switch]$NoBudget,
    [switch]$SaveClone,
    [switch]$ShippedSmoke,
    [switch]$SelfTest,
    [string]$TimeoutSeconds = '900',
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$Unexpected
)
$ErrorActionPreference = 'Stop'
# Any error outside the measured runs (arguments, output directory, summary) is a run failure, never exit 1.
trap { Write-Output "FAIL $($_.Exception.Message)"; exit 3 }
# perf_budget.rs reads this list: the -Scene values minus all are the PERF_BUDGET.json scenes.
$sceneChoices = @('peak', 'progressed-peak', 'stress', 'all')
$profileChoices = @('dev', 'playtest')
$pipelinedChoices = @('default', 'on', 'off')

function Assert-Choice([string]$Name, [string]$Value, [string[]]$Choices) {
    if ($Choices -notcontains $Value) { throw "-$Name '$Value' is not one of $($Choices -join ', ')" }
}

function ConvertTo-BoundedInt([string]$Name, [string]$Text, [int]$Minimum, [int]$Maximum) {
    $value = 0
    $styles = [Globalization.NumberStyles]::AllowLeadingSign
    if (-not [int]::TryParse($Text, $styles, [Globalization.CultureInfo]::InvariantCulture, [ref]$value) -or $value -lt $Minimum -or $value -gt $Maximum) {
        throw "-$Name '$Text' is not a whole number in $Minimum..$Maximum"
    }
    return $value
}

if ($null -ne $Unexpected -and $Unexpected.Count -gt 0) { throw "Unexpected argument(s): $($Unexpected -join ' ')" }
Assert-Choice 'Scene' $Scene $sceneChoices
Assert-Choice 'Profile' $Profile $profileChoices
Assert-Choice 'Pipelined' $Pipelined $pipelinedChoices
[int]$Frames = ConvertTo-BoundedInt 'Frames' $Frames 0 3600
if ($Frames -ne 0 -and $Frames -lt 300) { throw '-Frames is 0 (budget default) or 300..3600' }
[int]$Warmup = ConvertTo-BoundedInt 'Warmup' $Warmup -1 600
[int]$Repeat = ConvertTo-BoundedInt 'Repeat' $Repeat 1 20
[int]$TimeoutSeconds = ConvertTo-BoundedInt 'TimeoutSeconds' $TimeoutSeconds 1 86400
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
$workspace = Split-Path -Parent $PSScriptRoot
$budgetPath = Join-Path $workspace 'docs\PERF_BUDGET.json'
$invariant = [Globalization.CultureInfo]::InvariantCulture

function Read-JsonFile([string]$Path) {
    # ReadAllText detects a BOM and otherwise reads UTF-8, unlike 5.1's Get-Content.
    return ([IO.File]::ReadAllText($Path) | ConvertFrom-Json)
}

function Get-Field($Object, [string[]]$Names) {
    # Nested property lookup that tolerates missing members (no StrictMode errors, hyphenated names).
    $value = $Object
    foreach ($name in $Names) {
        if ($null -eq $value) { return $null }
        $property = $value.PSObject.Properties[$name]
        if ($null -eq $property) { return $null }
        $value = $property.Value
    }
    return $value
}

function Get-Median([double[]]$Values) {
    if ($null -eq $Values -or $Values.Count -eq 0) { return $null }
    $sorted = [double[]]$Values.Clone()
    [Array]::Sort($sorted)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) { return $sorted[$middle] }
    return ($sorted[$middle - 1] + $sorted[$middle]) / 2
}

function Get-PowerPlan {
    # The active Windows power plan, or $null when it cannot be read. powercfg localizes the plan name, so the built-in
    # plans are named by their fixed GUIDs; any other plan keeps the name powercfg prints. It runs before the evidence
    # directory exists, so its logs go to a per-PID temp stem that is removed again.
    $stem = Join-Path ([IO.Path]::GetTempPath()) ('lastlight-powercfg-' + $PID)
    try {
        $r = Invoke-Native -FilePath (Resolve-Tool 'powercfg') -Arguments @('/getactivescheme') -LogStem $stem -TimeoutSeconds 30
        if ($r.ExitCode -ne 0) { return $null }
        $oem = [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
        $text = ([IO.File]::ReadAllText($r.Out, $oem) -split "`r?`n") -join ' '
    }
    catch { return $null }
    finally {
        foreach ($log in @("$stem.out.txt", "$stem.err.txt")) {
            if (Test-Path -LiteralPath $log) { Remove-Item -LiteralPath $log -Force -ErrorAction SilentlyContinue }
        }
    }
    $builtIn = @{
        '381b4222-f694-41f0-9685-ff5bb260df2e' = 'Balanced'; '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c' = 'High performance'
        'a1841308-3bce-3e2b-91e2-8e6c1b6f9f3c' = 'Power saver'; 'e9a42b02-d5df-448d-aa00-03f14749eb61' = 'Ultimate Performance'
    }
    if ($text -match '([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})') {
        $guid = $Matches[1].ToLowerInvariant()
        if ($builtIn.ContainsKey($guid)) { return $builtIn[$guid] }
    }
    if ($text -match '\(([^)]+)\)\s*$') { return $Matches[1].Trim() }
    return $null
}

function Format-Number($Value) {
    if ($null -eq $Value) { return '-' }
    return ([double]$Value).ToString('0.####', $invariant)
}

function Get-PerfExpectation([string]$RunProfile, [bool]$NoVsync, [string]$PipelinedRequest, [string]$AdapterRequest = '') {
    $pipelinedState = $null
    if ($PipelinedRequest -eq 'on') { $pipelinedState = $true }
    elseif ($PipelinedRequest -eq 'off') { $pipelinedState = $false }
    $mode = 'Fifo'
    if ($NoVsync) { $mode = 'AutoNoVsync' }
    $adapterName = $null
    if ($AdapterRequest) { $adapterName = $AdapterRequest }
    return [pscustomobject]@{ PresentMode = $mode; Pipelined = $pipelinedState; Profile = $RunProfile; Adapter = $adapterName }
}

function Test-PerfReport($Report, [string]$SceneName, $Expect) {
    # Every reason the report is not valid evidence; empty when it is.
    $problems = New-Object System.Collections.Generic.List[string]
    if ($null -eq $Report) { $problems.Add('no report'); return , $problems.ToArray() }
    $version = Get-Field $Report @('version')
    if ($version -ne 3) { $problems.Add("report version $version, expected 3") }
    $reportScene = Get-Field $Report @('scene')
    if ($reportScene -ne $SceneName) { $problems.Add("scene '$reportScene', expected '$SceneName'") }
    if ((Get-Field $Report @('completed_sample_target')) -ne $true) { $problems.Add('sample target not completed') }
    if ((Get-Field $Report @('paused')) -ne $false) { $problems.Add('paused (or no paused field)') }
    $unfocused = Get-Field $Report @('unfocused_sample_frames')
    if ($null -eq $unfocused -or [int]$unfocused -ne 0) { $problems.Add("unfocused_sample_frames $unfocused, expected 0") }
    $mode = Get-Field $Report @('present', 'requested_mode')
    if ($mode -ne $Expect.PresentMode) { $problems.Add("present mode '$mode', requested '$($Expect.PresentMode)'") }
    if ($null -ne $Expect.Pipelined) {
        $state = Get-Field $Report @('present', 'pipelined_rendering')
        if ($state -ne $Expect.Pipelined) { $problems.Add("pipelined_rendering '$state', requested '$($Expect.Pipelined)'") }
    }
    $label = Get-Field $Report @('build', 'profile_label')
    if ($label -ne $Expect.Profile) { $problems.Add("profile label '$label', expected '$($Expect.Profile)'") }
    if ($Expect.Adapter) {
        # Bevy 0.20 matches a lowercased substring of the adapter name and otherwise silently uses the default GPU.
        $reportAdapter = Get-Field $Report @('adapter')
        if ($reportAdapter -isnot [string] -or -not $reportAdapter.ToLowerInvariant().Contains($Expect.Adapter.ToLowerInvariant())) {
            $problems.Add("adapter '$reportAdapter', requested '$($Expect.Adapter)'")
        }
    }
    return , $problems.ToArray()
}

function Invoke-WithRerun([scriptblock]$Attempt) {
    # $Attempt takes the attempt number and returns an object with a Problems array. One rerun, never more.
    $first = & $Attempt 1
    if (@($first.Problems).Count -eq 0) { return [pscustomobject]@{ Result = $first; Attempts = 1; Rejected = @() } }
    $second = & $Attempt 2
    return [pscustomobject]@{ Result = $second; Attempts = 2; Rejected = @($first) }
}

function Get-GatedValues($Reports, [scriptblock]$Read) {
    # The metric of every repeat as numbers, or $null when any repeat lacks it. A missing gated metric is invalid
    # evidence, never a zero that passes ([int]$null and [double[]]@($null) are both 0).
    $values = New-Object System.Collections.Generic.List[double]
    foreach ($report in @($Reports)) {
        $raw = & $Read $report
        if ($null -eq $raw -or $raw -is [bool] -or $raw -isnot [ValueType]) { return $null }
        $values.Add([double]$raw)
    }
    if ($values.Count -eq 0) { return $null }
    return , $values.ToArray()
}

function Get-SaveCloneCheck($Report, [string]$RunProfile, $Budget) {
    # The save_clone example's gate (W1-50) as one budget check: save_snapshot p99 at the budget's fixture, gated only in
    # the budget's profiles. A report for another fixture, or without the p99, is invalid evidence.
    $gate = Get-Field $Budget @('save_clone')
    $limit = Get-Field $gate @('p99_ms_max')
    $items = Get-Field $Report @('gate', 'items')
    $check = [pscustomobject][ordered]@{
        scene = 'save_clone'; size = "$items items"; metric = 'save_snapshot_p99_ms'; aggregate = 'p99'
        value = (Get-Field $Report @('gate', 'p99_ms')); limit = $limit; status = 'pass'; note = ''
    }
    if ($null -eq $gate -or $null -eq $limit) { $check.status = 'invalid'; $check.note = 'PERF_BUDGET.json has no save_clone.p99_ms_max' }
    elseif ($items -ne (Get-Field $gate @('items_min')) -or (Get-Field $Report @('gate', 'workers')) -ne (Get-Field $gate @('workers'))) {
        $check.status = 'invalid'; $check.note = 'the report measured another fixture than save_clone.items_min and workers'
    }
    elseif ($null -eq $check.value -or $check.value -is [bool] -or $check.value -isnot [ValueType]) {
        $check.status = 'invalid'; $check.note = 'the report has no gate.p99_ms'
    }
    elseif (@(Get-Field $gate @('profiles')) -notcontains $RunProfile) { $check.status = 'skipped'; $check.note = 'informational outside the budget profiles' }
    elseif ([double]$check.value -gt [double]$limit) { $check.status = 'over' }
    return $check
}

function Measure-PerfBudget($Runs, $Context, $Budget, $ExtraChecks = @()) {
    # Runs: objects with Scene, Size, Repeat, Report, Problems. Context: Profile, VsyncOff, NoBudget, Informational,
    # PowerPlan (Get-PowerPlan; $null when unknown).
    # ExtraChecks: finished check rows (the save clone) that count like the scene checks.
    $checks = New-Object System.Collections.Generic.List[object]
    foreach ($extra in @($ExtraChecks)) { if ($null -ne $extra) { $checks.Add($extra) } }
    $invalid = @($Runs | Where-Object { @($_.Problems).Count -gt 0 })
    $wave0 = Get-Field $Budget @('baseline', 'wave0')
    $v3 = Get-Field $Budget @('baseline', 'v3')
    $vsyncOn = -not $Context.VsyncOff
    $groups = @($Runs | Group-Object -Property Scene, Size)
    foreach ($group in $groups) {
        $first = $group.Group[0]
        $sceneBudget = Get-Field $Budget @('scenes', $first.Scene)
        $check = {
            param([string]$Metric, [string]$Aggregate, $Value, $Limit, [string]$Status, [string]$Note)
            $checks.Add([pscustomobject][ordered]@{
                    scene = $first.Scene; size = $first.Size; metric = $Metric; aggregate = $Aggregate
                    value = $Value; limit = $Limit; status = $Status; note = $Note
                })
        }
        if ($null -eq $sceneBudget) { & $check 'budget' '' $null $null 'invalid' 'the scene has no PERF_BUDGET.json entry'; continue }
        if (@($group.Group | Where-Object { @($_.Problems).Count -gt 0 }).Count -gt 0) {
            & $check 'all' '' $null $null 'skipped' 'invalid evidence in this scene and size'
            continue
        }
        $reports = @($group.Group | ForEach-Object { $_.Report })
        $measure = {
            # worst = the highest repeat, median = the median of the repeats.
            param([string]$Metric, [string]$Aggregate, $Limit, [scriptblock]$Read, [string]$Field)
            $values = Get-GatedValues $reports $Read
            if ($null -eq $values) { & $check $Metric $Aggregate $null $Limit 'invalid' "a repeat has no $Field"; return }
            if ($Aggregate -eq 'worst') { $value = ($values | Measure-Object -Maximum).Maximum }
            else { $value = Get-Median $values }
            $status = 'pass'
            if ([double]$value -gt [double]$Limit) { $status = 'over' }
            & $check $Metric $Aggregate $value $Limit $status ''
        }

        $limit = Get-Field $sceneBudget @('frames_over_50ms_max')
        if ($null -ne $limit) {
            # W1-61 user ruling: the driver stalls the report classified are counted and listed, never gated.
            & $measure 'frames_over_50ms' 'worst' $limit { param($r) Get-Field $r @('driver_stalls', 'outside', 'frames_over_50ms') } 'driver_stalls.outside.frames_over_50ms'
        }

        # Every gated scene is a battle, so no repeat may upload an egui texture after warmup (W1-64), worst repeat.
        $limit = Get-Field $Budget @('egui', 'texture_uploads_after_warmup_max')
        if ($null -ne $limit) {
            & $measure 'egui_texture_uploads' 'worst' $limit { param($r) Get-Field $r @('egui', 'texture_uploads', 'count') } 'egui.texture_uploads.count'
        }

        $limit = Get-Field $sceneBudget @('cpu_frame_p99_ms_max')
        if ($null -ne $limit) {
            if ($Context.VsyncOff) {
                & $measure 'cpu_frame_p99_ms' 'median' $limit { param($r) Get-Field $r @('phases', 'cpu_frame_ms', 'p99') } 'phases.cpu_frame_ms.p99'
            }
            else { & $check 'cpu_frame_p99_ms' 'median' $null $limit 'skipped' 'gated only with -VsyncOff' }
        }

        $limit = Get-Field $sceneBudget @('interface_draw_mean_ms_max', $first.Size)
        if ($null -ne $limit) {
            if ($Context.Profile -eq (Get-Field $wave0 @('profile')) -and $vsyncOn -eq (Get-Field $wave0 @('vsync'))) {
                $readDraw = {
                    param($r)
                    $work = @(Get-Field $r @('work_mean_ms'))
                    if ($work.Count -gt 2) { $work[2] }
                }
                & $measure 'interface_draw_mean_ms' 'median' $limit $readDraw 'work_mean_ms[2] (ui::draw)'
            }
            else { & $check 'interface_draw_mean_ms' 'median' $null $limit 'skipped' 'gated only in the baseline profile with VSync on' }
        }

        if ((Get-Field $sceneBudget @('interface_total_not_above_baseline_v3')) -eq $true) {
            # Once baseline.v3 is recorded, any other layout than perf_budget.rs pins is invalid evidence, never a skip.
            $v3Profile = Get-Field $v3 @('profile')
            $v3Vsync = Get-Field $v3 @('vsync')
            $v3Mode = Get-Field $v3 @('present_mode')
            $v3Pipelined = Get-Field $v3 @('pipelined')
            $v3Plan = Get-Field $v3 @('power_plan')
            $limit = Get-Field $v3 @('interface_total_mean_ms', $first.Scene, $first.Size)
            # W1-37 review: the baseline was measured in one present mode and pipelined state; a repeat in another is
            # not gated against it, and a repeat that does not echo its state is invalid evidence.
            $noState = $false
            $unlike = $false
            foreach ($report in $reports) {
                $state = Get-Field $report @('present', 'pipelined_rendering')
                if ($state -isnot [bool]) { $noState = $true }
                elseif ($state -ne $v3Pipelined -or (Get-Field $report @('present', 'requested_mode')) -ne $v3Mode) { $unlike = $true }
            }
            if ($null -eq $v3) { & $check 'interface_total_mean_ms' 'median' $null $null 'skipped' 'baseline.v3 is not recorded yet' }
            elseif ($v3Profile -isnot [string] -or $v3Vsync -isnot [bool]) {
                & $check 'interface_total_mean_ms' 'median' $null $null 'invalid' 'baseline.v3 has no profile and vsync'
            }
            elseif ($v3Mode -isnot [string] -or $v3Pipelined -isnot [bool] -or $v3Plan -isnot [string]) {
                & $check 'interface_total_mean_ms' 'median' $null $null 'invalid' 'baseline.v3 has no present_mode, pipelined state and power_plan'
            }
            elseif ($Context.Profile -ne $v3Profile -or $vsyncOn -ne $v3Vsync) {
                & $check 'interface_total_mean_ms' 'median' $null $limit 'skipped' 'gated only in the baseline.v3 profile and VSync state'
            }
            elseif ($noState) {
                & $check 'interface_total_mean_ms' 'median' $null $limit 'invalid' 'a repeat has no present.pipelined_rendering'
            }
            elseif ($unlike) {
                & $check 'interface_total_mean_ms' 'median' $null $limit 'skipped' "gated only in the baseline.v3 present mode $v3Mode with pipelined $v3Pipelined"
            }
            elseif ($null -eq $limit -or $limit -is [bool] -or $limit -isnot [ValueType]) {
                & $check 'interface_total_mean_ms' 'median' $null $null 'invalid' 'baseline.v3 has no interface_total_mean_ms for this scene and size'
            }
            else {
                & $measure 'interface_total_mean_ms' 'median' $limit { param($r) Get-Field $r @('phases', 'interface_total_ms', 'mean') } 'phases.interface_total_ms.mean'
                if ($Context.PowerPlan -ne $v3Plan) {
                    $plan = 'an unknown'
                    if ($Context.PowerPlan) { $plan = "the '$($Context.PowerPlan)'" }
                    & $check 'baseline_v3_conditions' '' $null $null 'noted' "measured under $plan power plan, baseline.v3 under '$v3Plan'"
                }
            }
        }
    }
    $over = @($checks | Where-Object { $_.status -eq 'over' })
    $missing = @($checks | Where-Object { $_.status -eq 'invalid' })
    $exitCode = 0
    if ($invalid.Count -gt 0 -or $missing.Count -gt 0) { $exitCode = 2 }
    elseif ($over.Count -gt 0 -and -not $Context.NoBudget -and -not $Context.Informational) { $exitCode = 1 }
    return [pscustomobject]@{ ExitCode = $exitCode; Checks = $checks.ToArray(); Invalid = $invalid.Count; Over = $over.Count }
}

function Get-ExitLabel([int]$Code) {
    switch ($Code) {
        0 { return 'pass' }
        1 { return 'over budget' }
        2 { return 'invalid evidence' }
        default { return 'build or run failure' }
    }
}

function Get-FixtureRuns($Fixture) {
    $run = $Fixture.run
    $expect = Get-PerfExpectation $run.profile ([bool]$run.vsync_off) $run.pipelined
    return , @($Fixture.reports | ForEach-Object {
            [pscustomobject]@{
                Scene = $_.scene; Size = $_.size; Repeat = $_.repeat; Report = $_.report
                Problems = (Test-PerfReport $_.report $_.scene $expect)
            }
        })
}

function Test-BudgetCase([string]$Name, $Runs, $Context, $Budget, [int]$Expected, [string[]]$Checks, $ExtraChecks = @()) {
    # One self-test case: the exit code and every named "<scene>/<size> <metric> <status>" check line.
    $result = Measure-PerfBudget $Runs $Context $Budget $ExtraChecks
    $lines = @($result.Checks | ForEach-Object { "$($_.scene)/$($_.size) $($_.metric) $($_.status)" })
    $detail = @($result.Checks | Where-Object { $_.status -ne 'pass' } | ForEach-Object { "$($_.scene)/$($_.size) $($_.metric) $($_.status)" }) -join '; '
    $absent = @($Checks | Where-Object { $_ -and $lines -notcontains $_ })
    if ($result.ExitCode -eq $Expected -and $absent.Count -eq 0) { Write-Output "ok   $Name -> $($result.ExitCode) ($detail)"; return }
    $script:selfTestFailed += 1
    Write-Output "FAIL $Name -> $($result.ExitCode), expected $Expected ($detail)$(if ($absent.Count -gt 0) { '; missing check ' + ($absent -join ', ') })"
}

function New-V3Baseline([string]$RunProfile, [bool]$Vsync, $Normal, $Small, $Pipelined = $true) {
    $means = [ordered]@{}
    if ($null -ne $Normal) { $means['normal'] = $Normal }
    if ($null -ne $Small) { $means['small'] = $Small }
    $mode = 'Fifo'
    if (-not $Vsync) { $mode = 'AutoNoVsync' }
    return [pscustomobject]@{
        profile = $RunProfile; vsync = $Vsync; present_mode = $mode; pipelined = $Pipelined; power_plan = 'Balanced'
        interface_total_mean_ms = [pscustomobject]@{ 'progressed-peak' = [pscustomobject]$means }
    }
}

function Invoke-SelfTest {
    $budget = Read-JsonFile $budgetPath
    # The fixtures' interface_total values are synthetic, so they run against a copy without the measured baseline.v3;
    # the v3 cases below gate interface_total against copies that record one.
    $fixtureBudget = $budget | ConvertTo-Json -Depth 20 | ConvertFrom-Json
    $fixtureBudget.baseline.v3 = $null
    $fixtureDirectory = Join-Path $PSScriptRoot 'fixtures\perf'
    $expected = [ordered]@{
        'within-budget' = 0; 'over-budget-cpu' = 1; 'over-50ms' = 1; 'unfocused' = 2; 'driver-stall' = 0; 'missing-driver-stalls' = 2
        'vsync-on-within-budget' = 0; 'over-interface-draw' = 1; 'playtest-vsync-on' = 0
        'missing-metric' = 2; 'missing-cpu-frame' = 2; 'over-texture-uploads' = 1; 'missing-texture-uploads' = 2
    }
    $script:selfTestFailed = 0
    foreach ($file in @(Get-ChildItem -LiteralPath $fixtureDirectory -Filter '*.json' -File)) {
        $stem = [IO.Path]::GetFileNameWithoutExtension($file.Name)
        if (-not $expected.Contains($stem)) { $script:selfTestFailed += 1; Write-Output "FAIL fixture $($file.Name) has no expected exit code" }
    }
    foreach ($name in $expected.Keys) {
        $fixture = Read-JsonFile (Join-Path $fixtureDirectory "$name.json")
        $run = $fixture.run
        $context = [pscustomobject]@{ Profile = $run.profile; VsyncOff = [bool]$run.vsync_off; NoBudget = $false; Informational = $false; PowerPlan = 'Balanced' }
        Test-BudgetCase $name (Get-FixtureRuns $fixture) $context $fixtureBudget $expected[$name] @($fixture.expected_checks)
    }

    # The interface_total gate runs against copies of the budget that record their own baseline.v3.
    $vsyncOn = Read-JsonFile (Join-Path $fixtureDirectory 'vsync-on-within-budget.json')
    $onContext = [pscustomobject]@{ Profile = 'dev'; VsyncOff = $false; NoBudget = $false; Informational = $false; PowerPlan = 'Balanced' }
    $highContext = [pscustomobject]@{ Profile = 'dev'; VsyncOff = $false; NoBudget = $false; Informational = $false; PowerPlan = 'High performance' }
    $total = 'progressed-peak/normal interface_total_mean_ms'
    $noPlan = New-V3Baseline 'dev' $true 2.5 2.5
    $noPlan.PSObject.Properties.Remove('power_plan')
    $v3Cases = @(
        @{ Name = 'v3 interface_total within the baseline'; V3 = (New-V3Baseline 'dev' $true 2.5 2.5); Expected = 0; Checks = @("$total pass", 'progressed-peak/small interface_total_mean_ms pass') },
        @{ Name = 'v3 interface_total above the baseline'; V3 = (New-V3Baseline 'dev' $true 1.9 2.5); Expected = 1; Checks = @("$total over") },
        @{ Name = 'v3 recorded in another profile'; V3 = (New-V3Baseline 'playtest' $true 1.9 1.9); Expected = 0; Checks = @("$total skipped") },
        @{ Name = 'v3 recorded with VSync off'; V3 = (New-V3Baseline 'dev' $false 1.9 1.9); Expected = 0; Checks = @("$total skipped") },
        @{ Name = 'v3 without a small value'; V3 = (New-V3Baseline 'dev' $true 2.5 $null); Expected = 2; Checks = @("$total pass", 'progressed-peak/small interface_total_mean_ms invalid') },
        @{ Name = 'v3 in a per-profile layout'; V3 = [pscustomobject]@{ dev = (New-V3Baseline 'dev' $true 2.5 2.5) }; Expected = 2; Checks = @("$total invalid") },
        @{ Name = 'v3 recorded, a report without interface_total'; V3 = (New-V3Baseline 'dev' $true 2.5 2.5); Strip = $true; Expected = 2; Checks = @("$total invalid") },
        @{ Name = 'v3 recorded pipelined, a run with pipelining off'; V3 = (New-V3Baseline 'dev' $true 1.9 1.9); Pipelined = $false; Expected = 0; Checks = @("$total skipped", 'progressed-peak/small interface_total_mean_ms skipped') },
        @{ Name = 'v3 recorded unpipelined, a pipelined run'; V3 = (New-V3Baseline 'dev' $true 1.9 1.9 $false); Expected = 0; Checks = @("$total skipped") },
        @{ Name = 'v3 recorded, a report without its pipelined state'; V3 = (New-V3Baseline 'dev' $true 2.5 2.5); Pipelined = 'strip'; Expected = 2; Checks = @("$total invalid") },
        @{ Name = 'v3 without a power plan'; V3 = $noPlan; Expected = 2; Checks = @("$total invalid") },
        @{ Name = 'v3 under another power plan is gated and noted'; V3 = (New-V3Baseline 'dev' $true 1.9 2.5); Context = $highContext; Expected = 1; Checks = @("$total over", 'progressed-peak/normal baseline_v3_conditions noted') },
        @{ Name = 'v3 under the same power plan is not noted'; V3 = (New-V3Baseline 'dev' $true 2.5 2.5); Expected = 0; Absent = 'progressed-peak/normal baseline_v3_conditions noted'; Checks = @("$total pass") }
    )
    foreach ($case in $v3Cases) {
        $copy = $budget | ConvertTo-Json -Depth 20 | ConvertFrom-Json
        $copy.baseline.v3 = $case.V3
        $runs = Get-FixtureRuns $vsyncOn
        if ($case.Strip) {
            $target = @($runs | Where-Object { $_.Scene -eq 'progressed-peak' -and $_.Size -eq 'normal' })[0]
            $report = $target.Report | ConvertTo-Json -Depth 20 | ConvertFrom-Json
            $report.phases.PSObject.Properties.Remove('interface_total_ms')
            $target.Report = $report
        }
        if ($case.ContainsKey('Pipelined')) {
            foreach ($run in @($runs | Where-Object { $_.Scene -eq 'progressed-peak' })) {
                $report = $run.Report | ConvertTo-Json -Depth 20 | ConvertFrom-Json
                if ($case.Pipelined -eq 'strip') { $report.present.PSObject.Properties.Remove('pipelined_rendering') }
                else { $report.present.pipelined_rendering = $case.Pipelined }
                $run.Report = $report
            }
        }
        $caseContext = $onContext
        if ($case.Context) { $caseContext = $case.Context }
        if ($case.Absent) {
            $lines = @((Measure-PerfBudget $runs $caseContext $copy).Checks | ForEach-Object { "$($_.scene)/$($_.size) $($_.metric) $($_.status)" })
            if ($lines -contains $case.Absent) { $script:selfTestFailed += 1; Write-Output "FAIL v3: $($case.Name) has the check $($case.Absent)" }
        }
        Test-BudgetCase "v3: $($case.Name)" $runs $caseContext $copy $case.Expected $case.Checks
    }

    # The save clone gate (W1-50) joins the scene checks of a valid run.
    $within = Read-JsonFile (Join-Path $fixtureDirectory 'within-budget.json')
    $withinContext = [pscustomobject]@{ Profile = $within.run.profile; VsyncOff = [bool]$within.run.vsync_off; NoBudget = $false; Informational = $false; PowerPlan = 'Balanced' }
    $cloneLimit = [double]$budget.save_clone.p99_ms_max
    $clone = {
        param($P99, [int]$Items = [int]$budget.save_clone.items_min)
        [pscustomobject]@{ gate = [pscustomobject]@{ items = $Items; workers = [int]$budget.save_clone.workers; p99_ms = $P99 } }
    }
    $cloneLine = "save_clone/$($budget.save_clone.items_min) items save_snapshot_p99_ms"
    $cloneCases = @(
        @{ Name = 'within the limit'; Report = (& $clone ($cloneLimit * 0.5)); Profile = 'dev'; Expected = 0; Checks = @("$cloneLine pass") },
        @{ Name = 'over the limit in dev'; Report = (& $clone ($cloneLimit * 1.5)); Profile = 'dev'; Expected = 1; Checks = @("$cloneLine over") },
        @{ Name = 'over the limit in playtest'; Report = (& $clone ($cloneLimit * 1.5)); Profile = 'playtest'; Expected = 1; Checks = @("$cloneLine over") },
        @{ Name = 'outside the budget profiles'; Report = (& $clone ($cloneLimit * 1.5)); Profile = 'release'; Expected = 0; Checks = @("$cloneLine skipped") },
        @{ Name = 'without a p99'; Report = (& $clone $null); Profile = 'dev'; Expected = 2; Checks = @("$cloneLine invalid") },
        @{ Name = 'another fixture'; Report = (& $clone ($cloneLimit * 0.5) 999); Profile = 'dev'; Expected = 2; Checks = @('save_clone/999 items save_snapshot_p99_ms invalid') }
    )
    foreach ($case in $cloneCases) {
        $extra = Get-SaveCloneCheck $case.Report $case.Profile $budget
        Test-BudgetCase "save clone: $($case.Name)" (Get-FixtureRuns $within) $withinContext $fixtureBudget $case.Expected $case.Checks @($extra)
    }
    $failed = $script:selfTestFailed
    $rerunCases = @(
        @{ Name = 'valid first attempt is not rerun'; Valid = @(1); Attempts = 1; Final = $true },
        @{ Name = 'invalid then valid is rerun once'; Valid = @(2); Attempts = 2; Final = $true },
        @{ Name = 'invalid twice stops after one rerun'; Valid = @(); Attempts = 2; Final = $false }
    )
    foreach ($case in $rerunCases) {
        $script:selfTestCalls = 0
        $outcome = Invoke-WithRerun {
            param([int]$Number)
            $script:selfTestCalls += 1
            $problems = @()
            if ($case.Valid -notcontains $Number) { $problems = @('unfocused_sample_frames 3, expected 0') }
            [pscustomobject]@{ Problems = $problems }
        }
        $final = @($outcome.Result.Problems).Count -eq 0
        if ($outcome.Attempts -eq $case.Attempts -and $script:selfTestCalls -eq $case.Attempts -and $final -eq $case.Final) {
            Write-Output "ok   rerun: $($case.Name)"
        }
        else {
            $failed += 1
            Write-Output "FAIL rerun: $($case.Name) (attempts $($outcome.Attempts), calls $($script:selfTestCalls), valid $final)"
        }
    }
    # Its output is the report lines, so the verdict travels in a script variable, not the pipeline.
    $script:selfTestExit = 0
    if ($failed -gt 0) { $script:selfTestExit = 1; Write-Output "SELFTEST FAIL: $failed case(s)" }
    else { Write-Output 'SELFTEST PASS' }
}

if ($SelfTest) {
    $script:selfTestExit = 1
    Invoke-SelfTest
    exit $script:selfTestExit
}

$budget = Read-JsonFile $budgetPath
if ($Scene -eq 'all') { $sceneNames = @($budget.run.frames.PSObject.Properties | ForEach-Object { $_.Name }) }
else { $sceneNames = @($Scene) }
if ($Warmup -lt 0) { $Warmup = [int]$budget.run.warmup_frames }
$sizeNames = @($budget.sizes.PSObject.Properties | ForEach-Object { $_.Name })
$expect = Get-PerfExpectation $Profile ([bool]$VsyncOff) $Pipelined $Adapter
$context = [pscustomobject]@{ Profile = $Profile; VsyncOff = [bool]$VsyncOff; NoBudget = [bool]$NoBudget; Informational = [bool]$Adapter; PowerPlan = (Get-PowerPlan) }

$target = Resolve-HordeTarget -Workspace $workspace
$startedUtc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
if (-not $OutputDirectory) {
    $suffix = ''
    if ($VsyncOff) { $suffix = '-novsync' }
    $destination = Join-Path $target ('perf\' + (Get-Date).ToString('yyyyMMdd-HHmmss') + '-' + $Profile + $suffix)
}
elseif ([IO.Path]::IsPathRooted($OutputDirectory)) { $destination = [IO.Path]::GetFullPath($OutputDirectory) }
else { $destination = [IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory)) }
if (Test-Path -LiteralPath $destination) {
    if (-not (Test-Path -LiteralPath $destination -PathType Container)) { throw "Output path is a file: $destination" }
    if (@(Get-ChildItem -LiteralPath $destination -Force).Count -gt 0) {
        throw "Refusing non-empty evidence directory $destination; choose a fresh -OutputDirectory"
    }
}
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$tools = Join-Path $destination 'tools'

$runs = New-Object System.Collections.Generic.List[object]
$exitCode = 3
$failure = $null
$verdict = $null
$exe = $null
$stamp = $null
$shipped = $null
$saveCloneReport = $null
$saveCloneCheck = $null
$cpuName = $null
$saved = Reset-LastlightEnv
$savedAdapter = [Environment]::GetEnvironmentVariable('WGPU_ADAPTER_NAME', 'Process')
try {
    if ($ShippedSmoke) {
        # One CARGO_TARGET_DIR: the shipped and review playtest builds share an exe path, so copy the shipped one out first.
        Invoke-Cargo -Arguments @('build', '--locked', '-p', 'horde-defense', '--profile', 'playtest', '--no-default-features') -LogStem (Join-Path $tools 'cargo-shipped') | Out-Null
        $shippedDirectory = Join-Path $destination 'shipped'
        New-Item -ItemType Directory -Force -Path $shippedDirectory | Out-Null
        $shippedExe = Join-Path $shippedDirectory 'horde-defense.exe'
        Copy-Item -LiteralPath (Get-HordeExecutable -Profile playtest -Workspace $workspace) -Destination $shippedExe
        $seconds = New-Object System.Collections.Generic.List[double]
        foreach ($index in 1..3) {
            $env:LASTLIGHT_SAVE_DIR = Join-Path $destination "saves\shipped-smoke-$index"
            $smoke = Invoke-Native -FilePath $shippedExe -Arguments @('--smoke') -LogStem (Join-Path $shippedDirectory "smoke-$index") -TimeoutSeconds 900 -WorkingDirectory $workspace
            if ($smoke.ExitCode -ne 0) { throw "Shipped --smoke $index failed with exit code $($smoke.ExitCode); see $($smoke.Err)" }
            $seconds.Add($smoke.Seconds)
        }
        [Environment]::SetEnvironmentVariable('LASTLIGHT_SAVE_DIR', $null, 'Process')
        $shipped = [ordered]@{
            executable = 'shipped/horde-defense.exe'; executable_sha256 = (Get-FileSha256 -Path $shippedExe)
            smoke_seconds = $seconds.ToArray(); median_seconds = (Get-Median $seconds.ToArray())
        }
    }
    if (-not $NoBuild -or $ShippedSmoke) {
        $buildArguments = @('build', '--locked', '-p', 'horde-defense')
        if ($Profile -eq 'playtest') { $buildArguments += @('--profile', 'playtest') }
        Invoke-Cargo -Arguments $buildArguments -LogStem (Join-Path $tools "cargo-$Profile") | Out-Null
    }
    $exe = Get-HordeExecutable -Profile $Profile -Workspace $workspace

    $version = Invoke-Native -FilePath $exe -Arguments @('--version') -LogStem (Join-Path $tools 'version') -TimeoutSeconds 30 -WorkingDirectory $workspace
    if ($version.ExitCode -eq 0) {
        $line = @(Get-Content -LiteralPath $version.Out | Where-Object { $_.Trim() } | Select-Object -First 1)
        if ($line.Count -gt 0) { $stamp = $line[0].Trim() }
    }
    $listing = Get-ReviewSceneList -Executable $exe -LogStem (Join-Path $tools 'list-scenes') -WorkingDirectory $workspace
    if ($listing.ExitCode -ne 0) { throw "--list-scenes failed with exit code $($listing.ExitCode); see $($listing.Err)" }
    Copy-Item -LiteralPath $listing.Out -Destination (Join-Path $destination 'scenes.tsv')
    $perfRows = @($listing.Scenes | Where-Object { $_.Perf } | ForEach-Object { $_.Name })
    foreach ($name in $sceneNames) {
        if ($perfRows -notcontains $name) { throw "This build lists no perf scene `"$name`" (--list-scenes); is it a review build?" }
    }
    $cpuName = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
    if ($Adapter) { $env:WGPU_ADAPTER_NAME = $Adapter }

    foreach ($sceneName in $sceneNames) {
        $sceneFrames = $Frames
        if ($sceneFrames -eq 0) { $sceneFrames = [int](Get-Field $budget @('run', 'frames', $sceneName)) }
        foreach ($sizeName in $sizeNames) {
            $sizeBudget = Get-Field $budget @('sizes', $sizeName)
            foreach ($repeatIndex in 1..$Repeat) {
                $stem = "$sceneName-$sizeName"
                if ($Repeat -gt 1) { $stem += "-r$repeatIndex" }
                $reportPath = Join-Path $destination "$stem.json"
                $outcome = Invoke-WithRerun {
                    param([int]$Number)
                    $attemptStem = $stem
                    if ($Number -gt 1) {
                        if (Test-Path -LiteralPath $reportPath) { Move-Item -LiteralPath $reportPath -Destination (Join-Path $destination "$stem.invalid.json") -Force }
                        $attemptStem = "$stem-a$Number"
                    }
                    $env:LASTLIGHT_REVIEW_SCREEN = $sceneName
                    $env:LASTLIGHT_REVIEW_MOTION = '1'
                    $env:LASTLIGHT_REVIEW_SIZE = [string]$sizeBudget.size
                    $env:LASTLIGHT_REVIEW_SCALE = ([double]$sizeBudget.scale).ToString('0.0##', $invariant)
                    $env:LASTLIGHT_PERF_REPORT = $reportPath
                    $env:LASTLIGHT_PERF_FRAMES = [string]$sceneFrames
                    $env:LASTLIGHT_PERF_WARMUP_FRAMES = [string]$Warmup
                    $env:LASTLIGHT_PERF_ACTIVE_ONLY = '1'
                    $env:LASTLIGHT_PERF_NPC_SPEED = '1'
                    $env:LASTLIGHT_PERF_PROFILE = $Profile
                    $env:LASTLIGHT_PERF_CPU = $cpuName
                    $env:LASTLIGHT_SAVE_DIR = Join-Path $destination "saves\$attemptStem"
                    if ($VsyncOff) { $env:LASTLIGHT_PERF_PRESENT_MODE = 'autonovsync' }
                    if ($Pipelined -eq 'on') { $env:LASTLIGHT_PERF_PIPELINED = '1' }
                    elseif ($Pipelined -eq 'off') { $env:LASTLIGHT_PERF_PIPELINED = '0' }
                    $result = Invoke-Native -FilePath $exe -LogStem (Join-Path $destination "logs\$attemptStem") -TimeoutSeconds $TimeoutSeconds -WorkingDirectory $workspace -RemoveReviewProfile
                    if ($result.ExitCode -ne 0) { throw "Run $attemptStem failed with exit code $($result.ExitCode); see $($result.Err)" }
                    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) { throw "Run $attemptStem wrote no report; see $($result.Err)" }
                    $report = Read-JsonFile $reportPath
                    [pscustomobject]@{ Report = $report; Problems = (Test-PerfReport $report $sceneName $expect); Seconds = $result.Seconds }
                }
                $runs.Add([pscustomobject]@{
                        Scene = $sceneName; Size = $sizeName; Repeat = $repeatIndex; File = "$stem.json"
                        Report = $outcome.Result.Report; Problems = $outcome.Result.Problems; Seconds = $outcome.Result.Seconds
                        Attempts = $outcome.Attempts; Rejected = @($outcome.Rejected | ForEach-Object { @($_.Problems) -join '; ' })
                    })
                $state = 'ok  '
                if (@($outcome.Result.Problems).Count -gt 0) { $state = 'BAD ' }
                Write-Output "$state $stem.json (attempts $($outcome.Attempts)) $(@($outcome.Result.Problems) -join '; ')"
            }
        }
    }

    if ($SaveClone) {
        $example = Join-Path $workspace 'crates\horde_core\examples\save_clone.rs'
        if (-not (Test-Path -LiteralPath $example)) { throw "-SaveClone needs crates/horde_core/examples/save_clone.rs (W1-50)" }
        # Built, then run directly: the example exits 1 over its gate, which is a budget verdict, not a run failure.
        $cloneArguments = @('build', '--locked', '-p', 'horde_core', '--example', 'save_clone')
        $cloneDirectory = 'debug'
        if ($Profile -eq 'playtest') { $cloneArguments += @('--profile', 'playtest'); $cloneDirectory = 'playtest' }
        Invoke-Cargo -Arguments $cloneArguments -LogStem (Join-Path $tools 'cargo-save-clone') | Out-Null
        $cloneExe = Join-Path $target "$cloneDirectory\examples\save_clone.exe"
        $clonePath = Join-Path $destination 'save-clone.json'
        $env:LASTLIGHT_BENCH_CPU = $cpuName
        $clone = Invoke-Native -FilePath $cloneExe -Arguments @('--profile-name', $Profile, '--out', $clonePath) -LogStem (Join-Path $tools 'save-clone') -TimeoutSeconds 1800 -WorkingDirectory $workspace
        [Environment]::SetEnvironmentVariable('LASTLIGHT_BENCH_CPU', $null, 'Process')
        if (@(0, 1) -notcontains $clone.ExitCode) { throw "save_clone failed with exit code $($clone.ExitCode); see $($clone.Err)" }
        if (-not (Test-Path -LiteralPath $clonePath -PathType Leaf)) { throw "save_clone wrote no report; see $($clone.Err)" }
        $saveCloneReport = 'save-clone.json'
        $saveCloneCheck = Get-SaveCloneCheck (Read-JsonFile $clonePath) $Profile $budget
        Write-Output "save clone p99 $(Format-Number $saveCloneCheck.value) ms (limit $(Format-Number $saveCloneCheck.limit)) $($saveCloneCheck.status)"
    }

    $verdict = Measure-PerfBudget $runs.ToArray() $context $budget @($saveCloneCheck)
    $exitCode = $verdict.ExitCode
}
catch {
    $failure = $_.Exception.Message
    $exitCode = 3
    Write-Output "FAIL $failure"
}
finally {
    Restore-LastlightEnv -Saved $saved
    [Environment]::SetEnvironmentVariable('WGPU_ADAPTER_NAME', $savedAdapter, 'Process')
    if ($null -eq $verdict -and $runs.Count -gt 0) { $verdict = Measure-PerfBudget $runs.ToArray() $context $budget @($saveCloneCheck) }
    $rows = @($runs | ForEach-Object {
            $r = $_.Report
            [ordered]@{
                scene = $_.Scene; size = $_.Size; repeat = $_.Repeat; file = $_.File; attempts = $_.Attempts
                valid = (@($_.Problems).Count -eq 0); problems = @($_.Problems); rejected_attempts = @($_.Rejected); seconds = $_.Seconds
                adapter = (Get-Field $r @('adapter'))
                frames_over_50ms = (Get-Field $r @('frames_over_50ms'))
                frames_over_50ms_driver_stall = (Get-Field $r @('frames_over_50ms_driver_stall'))
                frames_over_50ms_outside_stalls = (Get-Field $r @('driver_stalls', 'outside', 'frames_over_50ms'))
                driver_stalls = @(Get-Field $r @('driver_stalls', 'frames') | Where-Object { $null -ne $_ })
                egui_texture_uploads = (Get-Field $r @('egui', 'texture_uploads', 'count'))
                cpu_frame_p99_ms = (Get-Field $r @('phases', 'cpu_frame_ms', 'p99'))
                interface_draw_mean_ms = (@(Get-Field $r @('work_mean_ms')) + @($null, $null, $null))[2]
                interface_total_mean_ms = (Get-Field $r @('phases', 'interface_total_ms', 'mean'))
                present_mode = (Get-Field $r @('present', 'requested_mode'))
                pipelined_rendering = (Get-Field $r @('present', 'pipelined_rendering'))
                sample_frames = (Get-Field $r @('sample_frames'))
                peak_visible_enemies = (Get-Field $r @('peak_visible_enemies'))
            }
        })
    $checks = @()
    if ($null -ne $verdict) { $checks = @($verdict.Checks) }
    $exeHash = $null
    if ($exe -and (Test-Path -LiteralPath $exe -PathType Leaf)) { $exeHash = Get-FileSha256 -Path $exe }
    $summary = [ordered]@{
        version = 1; started_utc = $startedUtc; finished_utc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
        exit_code = $exitCode; verdict = (Get-ExitLabel $exitCode); failure = $failure
        profile = $Profile; vsync_off = [bool]$VsyncOff; present_mode = $expect.PresentMode; pipelined = $Pipelined
        power_plan = $context.PowerPlan
        adapter = $(if ($Adapter) { $Adapter } else { $null }); informational = [bool]$Adapter; no_budget = [bool]$NoBudget
        repeat = $Repeat; warmup_frames = $Warmup; frames = $(if ($Frames -gt 0) { $Frames } else { 'budget' })
        scenes = $sceneNames; sizes = $sizeNames; cargo_target_dir = $target
        executable = $exe; executable_sha256 = $exeHash; stamp = $stamp; cpu = $cpuName
        wgpu_backend = $(if ($env:WGPU_BACKEND) { $env:WGPU_BACKEND } else { $null })
        budget = 'docs/PERF_BUDGET.json'; budget_sha256 = (Get-FileSha256 -Path $budgetPath)
        runs = $rows; checks = $checks; shipped_smoke = $shipped; save_clone = $saveCloneReport
    }
    Write-Utf8NoBom -Path (Join-Path $destination 'summary.json') -Text ($summary | ConvertTo-Json -Depth 8)

    $md = New-Object System.Collections.Generic.List[string]
    $md.Add('# Performance evidence')
    $md.Add('')
    $md.Add("- Verdict: exit $exitCode ($(Get-ExitLabel $exitCode))")
    if ($failure) { $md.Add("- Failure: $failure") }
    $vsyncLabel = 'on'
    if ($VsyncOff) { $vsyncLabel = 'off' }
    $md.Add("- Profile $Profile, VSync $vsyncLabel (requested $($expect.PresentMode)), pipelined $Pipelined, repeat $Repeat, warmup $Warmup")
    if ($Adapter) { $md.Add("- Adapter $Adapter (informational: the budget never fails this run)") }
    if ($NoBudget) { $md.Add('- -NoBudget: the checks below are recorded, but only validity sets the exit code') }
    $md.Add("- Executable $exe ($exeHash)$(if ($stamp) { ", $stamp" })")
    $md.Add("- CPU $cpuName; power plan $(if ($context.PowerPlan) { $context.PowerPlan } else { 'unknown' }); started $startedUtc")
    if ($null -ne $shipped) { $md.Add("- Shipped --smoke median $(Format-Number $shipped.median_seconds) s over 3 runs") }
    if ($saveCloneReport) { $md.Add("- Save clone report: $saveCloneReport") }
    $md.Add('')
    $md.Add('Frames > 50 ms: every frame, then (driver stalls: frame ms at acquire share). The gate counts the frames outside the driver stalls (W1-61 user ruling).')
    $md.Add('')
    $md.Add('| Scene | Size | Repeat | Attempts | Valid | Adapter | Frames > 50 ms | egui texture uploads | cpu_frame p99 ms | interface_draw mean ms | interface_total mean ms | Present | Pipelined | Problems |')
    $md.Add('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|')
    foreach ($row in $rows) {
        $stalls = @($row.driver_stalls | ForEach-Object { "$(Format-Number $_.frame_ms) ms at $(Format-Number $_.acquire_share)" }) -join ', '
        $md.Add("| $($row.scene) | $($row.size) | $($row.repeat) | $($row.attempts) | $($row.valid) | $($row.adapter) | $($row.frames_over_50ms) ($($row.frames_over_50ms_driver_stall)$(if ($stalls) { ': ' + $stalls })) | $($row.egui_texture_uploads) | $(Format-Number $row.cpu_frame_p99_ms) | $(Format-Number $row.interface_draw_mean_ms) | $(Format-Number $row.interface_total_mean_ms) | $($row.present_mode) | $($row.pipelined_rendering) | $(@($row.problems) -join '; ') |")
    }
    $md.Add('')
    $md.Add('## Budget checks')
    $md.Add('')
    $md.Add('| Scene | Size | Metric | Aggregate | Value | Limit | Status | Note |')
    $md.Add('|---|---|---|---|---|---|---|---|')
    foreach ($c in $checks) {
        $md.Add("| $($c.scene) | $($c.size) | $($c.metric) | $($c.aggregate) | $(Format-Number $c.value) | $(Format-Number $c.limit) | $($c.status) | $($c.note) |")
    }
    Write-Utf8NoBom -Path (Join-Path $destination 'summary.md') -Text (($md -join "`n") + "`n")
}
Write-Output "Evidence: $destination (exit $exitCode, $(Get-ExitLabel $exitCode))"
exit $exitCode
