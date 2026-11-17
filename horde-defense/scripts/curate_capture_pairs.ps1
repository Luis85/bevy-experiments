#requires -Version 5.1
# Curated before/after pairs for docs/screenshots/production (W7-11, design-qa-captures 1.12) under Windows
# PowerShell 5.1. Run:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/curate_capture_pairs.ps1 -After target/production-after/final -Pairs 'battle,fx-stress:small:reduced'
# Each -Pairs item is <scene>[:<size>[:<mode>]], size large or small (default small), mode normal or reduced
# (default normal); 'a,b' counts as two items, because -File passes commas through verbatim.
# The after still is <After>/<mode>/<scene>[-small].png, a capture_review.ps1 run with captures.json and scenes.tsv.
# The before still has the same name in -Before (default target/production-before) or its supplement folder
# (<Before>/supplement, which wins where both hold one), else the name of the scene's before_proxy from
# <After>/scenes.tsv, as compare_captures.py pairs them.
# Writes <OutputDirectory>/before-<scene>[-small][-reduced].png and after-<...>.png (System.Drawing), each 2:1
# downsampled when its run's windows_scale is 2 and copied as-is at 1; any other scale is refused. A run's scale is
# its captures.json windows_scale; a before set without one (the wave-0 stills) is read from each PNG's width
# (1440 or 1050 times the scale). Pairs are comparable only at equal windows_scale, so a before still at another
# scale than the after run is refused too. Also writes captures-after.json (the after run's captures.json, byte for
# byte) and captures-before.json (an index of the merged before set: scene, mode, size, file, bytes, sha256).
# -OutputDirectory defaults to docs/screenshots/production; relative paths resolve against the workspace.
# Exit codes: 0 written and the folder within -MaxBytes (40,000,000 B, the G10.1 pair budget); 1 a still is missing
# or the folder is over budget (what was written stays); 2 invalid arguments or evidence.
[CmdletBinding()]
param(
    [string]$Before = 'target/production-before',
    [string]$After,
    [string[]]$Pairs,
    [string]$OutputDirectory = 'docs/screenshots/production',
    [long]$MaxBytes = 40000000
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
$workspace = Split-Path -Parent $PSScriptRoot
$logicalWidth = @{ large = 1440; small = 1050 }

function Stop-Invalid([string]$Message) {
    Write-Output "INVALID: $Message"
    exit 2
}

function Get-PngWidth([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $header = New-Object byte[] 24
        if ($stream.Read($header, 0, 24) -ne 24) { return 0 }
        return ([int]$header[16] -shl 24) -bor ([int]$header[17] -shl 16) -bor ([int]$header[18] -shl 8) -bor [int]$header[19]
    }
    finally { $stream.Dispose() }
}

function Read-JsonFile([string]$Path) {
    return ([IO.File]::ReadAllText($Path, [Text.Encoding]::UTF8) | ConvertFrom-Json)
}

function Get-RunScale([string]$Folder) {
    # The recorded windows_scale of a run, or $null when it has no captures.json or none recorded.
    $index = Join-Path $Folder 'captures.json'
    if (-not (Test-Path -LiteralPath $index -PathType Leaf)) { return $null }
    $report = Read-JsonFile $index
    if ($null -eq $report.windows_scale) { return $null }
    return [double]$report.windows_scale
}

function Get-StillScale([string]$Path, [string]$Size, $Recorded) {
    if ($null -ne $Recorded) { return [double]$Recorded }
    return [Math]::Round((Get-PngWidth $Path) / $logicalWidth[$Size], 3)
}

function Get-StillName([string]$Scene, [string]$Size) {
    if ($Size -eq 'small') { return "$Scene-small.png" }
    return "$Scene.png"
}

function Get-RelativeName([string]$Path) {
    # Workspace-relative with forward slashes when below the workspace, else the full path.
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetFullPath($workspace).TrimEnd('\') + '\'
    if ($full.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { return $full.Substring($root.Length).Replace('\', '/') }
    return $full.Replace('\', '/')
}

function Save-Still([string]$Source, [string]$Destination, [double]$Scale) {
    if ($Scale -eq 1) {
        [IO.File]::Copy($Source, $Destination, $true)
        return
    }
    $bytes = [IO.File]::ReadAllBytes($Source)
    $stream = New-Object IO.MemoryStream(, $bytes)
    $image = [Drawing.Image]::FromStream($stream)
    try {
        $width = [int][Math]::Round($image.Width / 2)
        $height = [int][Math]::Round($image.Height / 2)
        $bitmap = New-Object Drawing.Bitmap($width, $height, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $attributes = New-Object Drawing.Imaging.ImageAttributes
            try {
                $graphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
                $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::HighQuality
                $attributes.SetWrapMode([Drawing.Drawing2D.WrapMode]::TileFlipXY)
                $target = New-Object Drawing.Rectangle(0, 0, $width, $height)
                $graphics.DrawImage($image, $target, 0, 0, $image.Width, $image.Height, [Drawing.GraphicsUnit]::Pixel, $attributes)
            }
            finally {
                $attributes.Dispose()
                $graphics.Dispose()
            }
            $bitmap.Save($Destination, [Drawing.Imaging.ImageFormat]::Png)
        }
        finally { $bitmap.Dispose() }
    }
    finally {
        $image.Dispose()
        $stream.Dispose()
    }
}

if (-not $After) { Stop-Invalid '-After names the capture_review.ps1 run to curate' }
$pairList = New-Object System.Collections.Generic.List[object]
foreach ($value in @($Pairs)) {
    if ($null -eq $value) { continue }
    foreach ($part in ($value -split ',')) {
        $item = $part.Trim()
        if (-not $item) { continue }
        $fields = $item -split ':'
        $size = 'small'
        $mode = 'normal'
        if ($fields.Count -ge 2 -and $fields[1]) { $size = $fields[1] }
        if ($fields.Count -ge 3 -and $fields[2]) { $mode = $fields[2] }
        if ($fields.Count -gt 3 -or -not $fields[0] -or $size -notin @('large', 'small') -or $mode -notin @('normal', 'reduced')) {
            Stop-Invalid "pair '$item' must read <scene>[:large|small[:normal|reduced]]"
        }
        $pairList.Add([pscustomobject]@{ Scene = $fields[0]; Size = $size; Mode = $mode })
    }
}
if ($pairList.Count -eq 0) { Stop-Invalid '-Pairs names no pair' }

$beforeRoot = Resolve-WorkspacePath $Before -Workspace $workspace
$afterRoot = Resolve-WorkspacePath $After -Workspace $workspace
$destination = Resolve-WorkspacePath $OutputDirectory -Workspace $workspace
$supplementRoot = Join-Path $beforeRoot 'supplement'
$afterIndex = Join-Path $afterRoot 'captures.json'
if (-not (Test-Path -LiteralPath $afterIndex -PathType Leaf)) { Stop-Invalid "$afterIndex is missing: -After must be a capture_review.ps1 run" }
if (-not (Test-Path -LiteralPath $beforeRoot -PathType Container)) { Stop-Invalid "the before set $beforeRoot is missing" }
$afterScale = Get-RunScale $afterRoot
if ($null -eq $afterScale) { Stop-Invalid "$afterIndex records no windows_scale" }
if ($afterScale -ne 1 -and $afterScale -ne 2) { Stop-Invalid "windows_scale ${afterScale}: only 1 (copied) and 2 (2:1 downsample) are curated" }
$proxies = @{}
$sceneList = Join-Path $afterRoot 'scenes.tsv'
if (Test-Path -LiteralPath $sceneList -PathType Leaf) {
    foreach ($row in @(Import-Csv -LiteralPath $sceneList -Delimiter "`t")) {
        if ($row.before_proxy) { $proxies[$row.name] = $row.before_proxy }
    }
}
else { Write-Warning "No scenes.tsv in $afterRoot; before stills pair by name only" }

# The merged before set: every <mode>/<scene>[-small].png of the before folder, then of its supplement.
$beforeSets = @(@{ Root = $beforeRoot; Scale = (Get-RunScale $beforeRoot) })
if (Test-Path -LiteralPath $supplementRoot -PathType Container) { $beforeSets += @{ Root = $supplementRoot; Scale = (Get-RunScale $supplementRoot) } }
$beforeStills = @{}
foreach ($set in $beforeSets) {
    foreach ($mode in @('normal', 'reduced')) {
        $folder = Join-Path $set.Root $mode
        if (-not (Test-Path -LiteralPath $folder -PathType Container)) { continue }
        foreach ($file in @(Get-ChildItem -LiteralPath $folder -Filter '*.png' -File)) {
            $stem = [IO.Path]::GetFileNameWithoutExtension($file.Name)
            $size = $(if ($stem.EndsWith('-small')) { 'small' } else { 'large' })
            $scene = $(if ($size -eq 'small') { $stem.Substring(0, $stem.Length - 6) } else { $stem })
            $beforeStills["$mode|$scene|$size"] = [pscustomobject]@{
                Scene = $scene; Mode = $mode; Size = $size; Path = $file.FullName; Recorded = $set.Scale
            }
        }
    }
}

$problems = New-Object System.Collections.Generic.List[string]
$jobs = New-Object System.Collections.Generic.List[object]
$seen = @{}
foreach ($pair in $pairList) {
    $stem = $pair.Scene + $(if ($pair.Size -eq 'small') { '-small' } else { '' }) + $(if ($pair.Mode -eq 'reduced') { '-reduced' } else { '' })
    if ($seen.ContainsKey($stem)) { continue }
    $seen[$stem] = $true
    $afterStill = Join-Path (Join-Path $afterRoot $pair.Mode) (Get-StillName $pair.Scene $pair.Size)
    if (-not (Test-Path -LiteralPath $afterStill -PathType Leaf)) {
        $problems.Add("no after still $(Get-RelativeName $afterStill)")
        continue
    }
    $beforeStill = $beforeStills["$($pair.Mode)|$($pair.Scene)|$($pair.Size)"]
    if ($null -eq $beforeStill -and $proxies.ContainsKey($pair.Scene)) {
        $beforeStill = $beforeStills["$($pair.Mode)|$($proxies[$pair.Scene])|$($pair.Size)"]
    }
    if ($null -eq $beforeStill) {
        $problems.Add("no before still for $($pair.Mode)/$(Get-StillName $pair.Scene $pair.Size) (nor its before_proxy) in $(Get-RelativeName $beforeRoot)")
        continue
    }
    $beforeScale = Get-StillScale $beforeStill.Path $beforeStill.Size $beforeStill.Recorded
    if ($beforeScale -ne $afterScale) {
        Stop-Invalid "windows_scale differs: before $beforeScale ($(Get-RelativeName $beforeStill.Path)), after $afterScale; pairs are comparable only at equal scale"
    }
    $jobs.Add([pscustomobject]@{ Stem = $stem; Before = $beforeStill.Path; After = $afterStill })
}
if ($problems.Count -gt 0) {
    foreach ($problem in $problems) { Write-Output "FAIL $problem" }
    exit 1
}

Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force -Path $destination | Out-Null
foreach ($job in $jobs) {
    Save-Still $job.Before (Join-Path $destination "before-$($job.Stem).png") $afterScale
    Save-Still $job.After (Join-Path $destination "after-$($job.Stem).png") $afterScale
    Write-Output "ok   before-$($job.Stem).png <- $(Get-RelativeName $job.Before)"
    Write-Output "ok   after-$($job.Stem).png <- $(Get-RelativeName $job.After)"
}
[IO.File]::Copy($afterIndex, (Join-Path $destination 'captures-after.json'), $true)
$entries = New-Object System.Collections.Generic.List[object]
foreach ($key in @($beforeStills.Keys | Sort-Object)) {
    $still = $beforeStills[$key]
    $entries.Add([ordered]@{
        scene  = $still.Scene
        mode   = $still.Mode
        size   = $still.Size
        file   = Get-RelativeName $still.Path
        bytes  = (Get-Item -LiteralPath $still.Path).Length
        sha256 = Get-FileSha256 -Path $still.Path
    })
}
$beforeReport = [ordered]@{
    version       = 1
    before        = Get-RelativeName $beforeRoot
    supplement    = $(if (Test-Path -LiteralPath $supplementRoot -PathType Container) { Get-RelativeName $supplementRoot } else { $null })
    windows_scale = $afterScale
    captures      = $entries.ToArray()
}
Write-Utf8NoBom -Path (Join-Path $destination 'captures-before.json') -Text ($beforeReport | ConvertTo-Json -Depth 4)
$total = [long]0
foreach ($file in @(Get-ChildItem -LiteralPath $destination -Recurse -File)) { $total += $file.Length }
Write-Output "Pairs: $($jobs.Count) in $(Get-RelativeName $destination), $total B of $MaxBytes B"
if ($total -gt $MaxBytes) {
    Write-Output "FAIL $(Get-RelativeName $destination) holds $total B, over the $MaxBytes B budget"
    exit 1
}
exit 0
