<#
Reproducible native proof for the original-source modular default bind.
Run inside this workspace, with at most the repository's permitted builders.
The PNGs are emitted by the native application; this script never edits
images, changes a screenshot baseline, or accepts approximate parity.
#>
[CmdletBinding()]
param(
    [string] $OutputDirectory = ('target/player-model-review/' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
    [switch] $NoBuild,
    [switch] $BindOnly,
    [switch] $IncludeRepairs,
    [string] $Executable,
    [string] $PythonCommand = 'python'
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$names = @(
    'LASTLIGHT_REVIEW_SCREEN', 'LASTLIGHT_REVIEW_CAPTURE', 'LASTLIGHT_REVIEW_EXIT',
    'LASTLIGHT_REVIEW_SIZE', 'LASTLIGHT_REVIEW_SCALE', 'LASTLIGHT_PRACTICE',
    'LASTLIGHT_PERF_REPORT', 'LASTLIGHT_REVIEW_MOTION', 'LASTLIGHT_CAPTURE_SEQUENCE',
    'LASTLIGHT_REVIEW_REDUCED', 'LASTLIGHT_PLAYER_MODEL_PARITY_REPORT',
    'LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT', 'LASTLIGHT_SAVE_DIR'
)
$previous = @{}
foreach ($name in $names) {
    $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
Push-Location $workspace
try {
    $destination = [IO.Path]::GetFullPath($OutputDirectory, $workspace)
    if (Test-Path -LiteralPath $destination) {
        throw "Use a fresh native evidence directory: $destination"
    }
    if (-not $NoBuild) {
        & cargo build --locked -p horde-defense
        if ($LASTEXITCODE -ne 0) { throw 'Native player-model build failed.' }
    }
    if ([string]::IsNullOrWhiteSpace($Executable)) {
        $buildDirectory = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
            Join-Path $workspace 'target'
        } else {
            [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR, $workspace)
        }
        $Executable = Join-Path $buildDirectory 'debug/horde-defense.exe'
    } else {
        $Executable = [IO.Path]::GetFullPath($Executable, $workspace)
    }
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Native executable is unavailable: $Executable"
    }
    New-Item -ItemType Directory -Path $destination | Out-Null
    foreach ($name in $names) {
        [Environment]::SetEnvironmentVariable($name, $null, 'Process')
    }
    $env:LASTLIGHT_REVIEW_EXIT = '1'
    $env:LASTLIGHT_REVIEW_SIZE = '1800x1000'
    $env:LASTLIGHT_REVIEW_SCALE = '1.0'
    $env:LASTLIGHT_SAVE_DIR = Join-Path $destination 'isolated-profile'
    $captures = @()
    $unsupported = @()
    $scenes = @('model-parity-rook', 'model-parity-june', 'model-parts-rook', 'model-parts-june')
    if (-not $BindOnly) { $scenes += @('model-matrix-rook', 'model-matrix-june') }
    if ($IncludeRepairs) { $scenes += @('model-repairs-rook', 'model-repairs-june') }
    foreach ($scene in $scenes) {
        $env:LASTLIGHT_REVIEW_SCREEN = $scene
        $capturePath = Join-Path $destination "$scene.png"
        $metadataPath = Join-Path $destination "$scene.json"
        $env:LASTLIGHT_REVIEW_CAPTURE = $capturePath
        [Environment]::SetEnvironmentVariable('LASTLIGHT_PLAYER_MODEL_PARITY_REPORT', $null, 'Process')
        [Environment]::SetEnvironmentVariable('LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT', $null, 'Process')
        $isParity = $scene.StartsWith('model-parity-', [StringComparison]::Ordinal)
        $isMatrix = $scene.StartsWith('model-matrix-', [StringComparison]::Ordinal)
        $isRepairs = $scene.StartsWith('model-repairs-', [StringComparison]::Ordinal)
        if ($isParity) { $env:LASTLIGHT_PLAYER_MODEL_PARITY_REPORT = $metadataPath }
        if ($isMatrix -or $isRepairs) { $env:LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT = $metadataPath }
        $logPath = Join-Path $destination "$scene.log"
        & $Executable *> $logPath
        if ($LASTEXITCODE -ne 0) { throw "Native fixture failed: $scene. See $logPath" }
        if (-not (Test-Path -LiteralPath $capturePath -PathType Leaf) -or (Get-Item -LiteralPath $capturePath).Length -eq 0) {
            throw "Native fixture produced no completed PNG: $scene"
        }
        if ($isParity) {
            $proofText = & (Join-Path $PSScriptRoot 'verify_player_models.ps1') -Capture $capturePath -Metadata $metadataPath -PythonCommand $PythonCommand
            $proof = ($proofText | Out-String) | ConvertFrom-Json
            $captures += [ordered]@{ scene = $scene; screenshot = $capturePath; manifest = $metadataPath; proof = $proof }
            Write-Output "$scene : exact source reconstruction and zero native differences on black/white."
        } elseif ($isMatrix) {
            $matrix = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
            $expectedModel = $scene.Substring('model-matrix-'.Length)
            if ($matrix.schema_version -ne 1 -or $matrix.kind -ne 'player_model_matrix' -or $matrix.model_id -ne $expectedModel -or @($matrix.cases).Count -ne 34) {
                throw "Native model matrix did not render the complete fixture: $scene"
            }
            if (@($matrix.cases | Select-Object -ExpandProperty label -Unique).Count -ne 34) { throw 'Repeated model matrix case labels.' }
            foreach ($case in $matrix.cases) {
                if ($case.error -or $case.packet_count -le 0 -or $case.vertices -le 0 -or $case.indices -le 0 -or $case.part_ids -notcontains 'head' -or $case.part_ids -notcontains 'boot.near' -or $case.part_ids -notcontains 'boot.far') {
                    $unsupported += "$scene/$($case.label): $($case.error)"
                }
            }
            $captures += [ordered]@{ scene = $scene; screenshot = $capturePath; manifest = $metadataPath; actual_cases = @($matrix.cases).Count; semantic_visual_review = 'required' }
            Write-Output "$scene : all34 appearance/action results captured, including explicit unsupported cells; visual pose/skin review required."
        } elseif ($isRepairs) {
            $repairs = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
            if ($repairs.schema_version -ne 1 -or $repairs.kind -ne 'player_model_repairs' -or @($repairs.cases).Count -ne 8) {
                throw "Causal repair fixture was not fully rendered: $scene"
            }
            foreach ($row in @(0, 4)) {
                $full = $repairs.cases[$row]
                $without = $repairs.cases[$row + 1]
                $only = $repairs.cases[$row + 2]
                if ($full.packet_count -ne ($without.packet_count + $only.packet_count)) { throw 'HiddenFill filters do not partition actual packets.' }
                if (@($without.part_ids | Where-Object { $only.part_ids -contains $_ }).Count -ne 0) { throw 'HiddenFill diagnostic partitions overlap.' }
            }
            $captures += [ordered]@{ scene = $scene; screenshot = $capturePath; manifest = $metadataPath; causal_isolation = 'same resolved pose / HiddenFill packet partition'; semantic_visual_review = 'required' }
            Write-Output "$scene : causal HiddenFill isolation captured; this diagnostic is not an appearance approval."
        } else {
            $captures += [ordered]@{ scene = $scene; screenshot = $capturePath; semantic_visual_review = 'required' }
            Write-Output "$scene : native part contact sheet captured; semantic visual review required."
        }
    }
    $report = [ordered]@{
        schema_version = 1
        executable = $Executable
        window_logical_size = '1800x1000'
        source_sampling = 'bind parity panels: one texel per physical pixel; diagnostic grids: common uniform camera scale; nearest managed PMA'
        native_bind_exact = $true
        native_matrix_supported = if ($BindOnly) { $null } else { $unsupported.Count -eq 0 }
        unsupported_cases = $unsupported
        semantic_visual_review = 'required; a zero-difference whole image does not prove sensible part boundaries'
        captures = $captures
    }
    $reportPath = Join-Path $destination 'summary.json'
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8
    Write-Output $reportPath
    if ($unsupported.Count -gt 0) {
        throw ("Native model matrix remains incomplete; retained all evidence and both survivor reports:`n" + ($unsupported -join "`n"))
    }
}
finally {
    foreach ($name in $names) {
        [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process')
    }
    Pop-Location
}
