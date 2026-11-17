#requires -Version 5.1
# Builds the game and packs a zipped Windows x64 playtest folder (Windows PowerShell 5.1).
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package_playtest.ps1
# The build carries the commit as its stamp (LASTLIGHT_BUILD_COMMIT, set only here and restored afterwards), the
# manifest is version 2 (game version, commit, features, curated docs, one record per file), and the docs folder
# holds only the curated list. A dirty tree is refused unless -AllowDirty is passed; the stamp then ends in +dirty.
# Prints the zip path and its SHA-256; a failed run leaves no package folder or zip behind. Check the result with
# scripts/verify_playtest.ps1.
[CmdletBinding()]
param(
    [ValidateSet('debug', 'release', 'playtest')][string]$Profile = 'playtest',
    # Default: <target>\playtest, where <target> honours CARGO_TARGET_DIR.
    [string]$OutputDirectory = '',
    # Include the `review` feature (captures, review fixtures, perf reports).
    # Ordinary playtest packages leave it out; practice scenes still work.
    [switch]$Review,
    # Package a tree with uncommitted changes under crates, assets, scripts, the guide or the Cargo files.
    [switch]$AllowDirty,
    # Test seam (scripts/tests/run_script_tests.ps1): pack this executable instead of building one. Its --version
    # must still print this tree's stamp.
    [switch]$NoBuild,
    [string]$Executable = '',
    # Test seam (with -NoBuild): copy the curated docs from this folder instead of docs/.
    [string]$DocsDirectory = ''
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\Native.ps1')
. (Join-Path $PSScriptRoot 'lib\Playtest.ps1')

# The docs a package ships, relative to docs/.
$CuratedDocs = @('PLAYTEST_GUIDE.md')
# What `git status` looks at to call the tree dirty.
$DirtyPaths = @('crates', 'assets', 'docs/PLAYTEST_GUIDE.md', 'Cargo.toml', 'Cargo.lock', 'scripts')

$workspace = Split-Path -Parent $PSScriptRoot
if ($NoBuild -and -not $Executable) { throw '-NoBuild needs -Executable' }
if ($Executable -and -not $NoBuild) { throw '-Executable needs -NoBuild' }
if ($DocsDirectory -and -not $NoBuild) { throw '-DocsDirectory needs -NoBuild' }
if ($OutputDirectory) { $outputRoot = Resolve-WorkspacePath -Path $OutputDirectory -Workspace $workspace }
else { $outputRoot = Join-Path (Resolve-HordeTarget -Workspace $workspace) 'playtest' }
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$createdUtc = [DateTime]::UtcNow
$logs = Join-Path $outputRoot ('logs\' + $createdUtc.ToString('yyyyMMdd-HHmmss') + '-' + $PID)

function Get-ToolLine([string]$Tool, [string[]]$Arguments, [string]$Stem) {
    # The trimmed stdout of a short tool run in the workspace; throws on a non-zero exit.
    $run = Invoke-Native -FilePath (Resolve-Tool $Tool) -Arguments $Arguments -LogStem (Join-Path $logs $Stem) -TimeoutSeconds 30 -WorkingDirectory $workspace
    if ($run.ExitCode -ne 0) { throw "$Tool $($Arguments -join ' ') failed with exit code $($run.ExitCode); see $($run.Err)" }
    return [IO.File]::ReadAllText($run.Out).Trim()
}

$gameVersion = Get-WorkspaceVersion -Workspace $workspace
$commit = Get-ToolLine 'git' @('rev-parse', 'HEAD') 'git-commit'
if ($commit -cnotmatch '^[0-9a-f]{40}$') { throw "git rev-parse HEAD did not print a commit id: $commit" }
$branch = Get-ToolLine 'git' @('rev-parse', '--abbrev-ref', 'HEAD') 'git-branch'
$status = Get-ToolLine 'git' (@('status', '--porcelain=v1', '--untracked-files=all', '--') + $DirtyPaths) 'git-status'
$dirty = ($status.Length -gt 0)
if ($dirty -and -not $AllowDirty) {
    throw "The working tree has uncommitted changes; commit them or pass -AllowDirty (the stamp then ends in +dirty):`n$status"
}
$rustc = Get-ToolLine 'rustc' @('--version') 'rustc-version'
$features = @()
if ($Review) { $features = @('review') }
$stamp = Get-BuildStamp -Version $gameVersion -Commit $commit -Dirty $dirty
$packageName = Get-PlaytestPackageName -Version $gameVersion -Commit $commit -CreatedUtc $createdUtc
$package = Join-Path $outputRoot $packageName
$zip = "$package.zip"
if (Test-Path -LiteralPath $package) { throw "Package already exists: $package" }
if (Test-Path -LiteralPath $zip) { throw "Package already exists: $zip" }
if ($DocsDirectory) { $docsSource = Resolve-WorkspacePath -Path $DocsDirectory -Workspace $workspace }
else { $docsSource = Join-Path $workspace 'docs' }

if ($NoBuild) { $built = Resolve-WorkspacePath -Path $Executable -Workspace $workspace }
else {
    $buildArguments = @('build', '--locked', '-p', 'horde-defense')
    if (-not $Review) { $buildArguments += '--no-default-features' }
    if ($Profile -eq 'release') { $buildArguments += '--release' }
    elseif ($Profile -ne 'debug') { $buildArguments += @('--profile', $Profile) }
    # The stamp is set for this build only (Invoke-WithBuildCommit restores it, scripts/lib/Playtest.ps1).
    $built = Invoke-WithBuildCommit -Label (Get-BuildCommitLabel -Commit $commit -Dirty $dirty) -Build {
        Invoke-HordeBuild -Arguments $buildArguments -LogStem (Join-Path $logs 'cargo-build') -DepInfo (Join-Path (Resolve-HordeTarget -Workspace $workspace) "$Profile\horde-defense.d") -Workspace $workspace | Out-Null
        Get-HordeExecutable -Profile $Profile -Workspace $workspace
    }
}
if (-not (Test-Path -LiteralPath $built -PathType Leaf)) { throw "No executable at $built" }

# Both were checked absent above, so a failure removes only what this run made: a folder with a package's name but
# no manifest would send verify_playtest after "No manifest.json" instead of the real cause.
New-Item -ItemType Directory -Path $package | Out-Null
$packed = $false
try {
    Copy-Item -LiteralPath $built -Destination (Join-Path $package 'Lastlight.exe')
    # The packed executable must carry this tree's stamp: a stale or unstamped build is not a package.
    $versionRun = Invoke-Native -FilePath (Join-Path $package 'Lastlight.exe') -Arguments @('--version') -LogStem (Join-Path $logs 'version') -TimeoutSeconds 30 -WorkingDirectory $package
    $versionLine = [IO.File]::ReadAllText($versionRun.Out).Trim()
    if ($versionRun.ExitCode -ne 0 -or -not $versionLine.Contains($stamp)) {
        throw "The executable reports '$versionLine' (exit $($versionRun.ExitCode)), expected the stamp '$stamp'"
    }

    # assets\ only: archive\ (retired art and audio) and docs\audio are never packaged.
    Copy-Item -LiteralPath (Join-Path $workspace 'assets') -Destination (Join-Path $package 'assets') -Recurse
    New-Item -ItemType Directory -Path (Join-Path $package 'content') | Out-Null
    $core = Join-Path $workspace 'crates\horde_core'
    Copy-Item -LiteralPath (Join-Path $core 'content.json') -Destination (Join-Path $package 'content\encounters.json')
    foreach ($catalog in @('skills.json', 'enemy_catalog.json', 'ordnance.json', 'progression.json', 'quality.json', 'balance.json')) {
        Copy-Item -LiteralPath (Join-Path $core $catalog) -Destination (Join-Path $package "content\$catalog")
    }
    Copy-Item -LiteralPath (Join-Path $core 'src\world.json') -Destination (Join-Path $package 'content\world.json')
    New-Item -ItemType Directory -Path (Join-Path $package 'docs') | Out-Null
    foreach ($doc in $CuratedDocs) {
        Copy-Item -LiteralPath (Join-Path $docsSource $doc) -Destination (Join-Path (Join-Path $package 'docs') $doc)
    }
    Copy-Item -LiteralPath (Join-Path $workspace '..\LICENSE') -Destination (Join-Path $package 'LICENSE')

    $readMe = @(
        'LASTLIGHT - HORDE DEFENSE PLAYTEST',
        '',
        ('Build: Lastlight {0}, {1} profile, created {2} UTC' -f $stamp, $Profile, $createdUtc.ToString('yyyy-MM-dd HH:mm:ss')),
        '',
        'Run Play Lastlight.cmd for your ordinary persistent campaign.',
        'If automatic graphics fail, Play Lastlight (DirectX12).cmd uses the same campaign',
        'through the separately validated Windows DirectX 12 renderer.',
        'Battle practice.cmd and Strategy practice.cmd use separate practice campaigns.',
        'You can also run Lastlight.exe directly; its diagnostic console may be visible.',
        '',
        'No Rust installation is needed for this Windows x64 build. A working graphics',
        'driver is required. This package was validated on the development machine;',
        'it is not a claim of minimum-spec support on every Windows PC.',
        '',
        'Read docs/PLAYTEST_GUIDE.md for controls, goals, known limits and feedback prompts.',
        'Your ordinary campaign is stored under LOCALAPPDATA/LastlightHordeDefense.',
        'Practice never grants currency or unlocks to that campaign.',
        'New game in the main menu confirms and archives the previous journey before',
        'starting fresh; display, audio, controls and committed difficulty are retained.',
        'Practice modes use separate profiles. Run one ordinary campaign window at a time.'
    )
    Write-Utf8NoBom -Path (Join-Path $package 'READ ME.txt') -Text (($readMe -join "`r`n") + "`r`n")

    foreach ($launcher in @(
            @{ Name = 'Play Lastlight.cmd'; Scene = '' },
            @{ Name = 'Play Lastlight (DirectX12).cmd'; Scene = ''; Backend = 'dx12' },
            @{ Name = 'Battle practice.cmd'; Scene = 'construction' },
            @{ Name = 'Strategy practice.cmd'; Scene = 'operations' }
        )) {
        $lines = @('@echo off', 'setlocal DisableDelayedExpansion', 'set "LASTLIGHT_PLAYTEST_HOME=%~dp0"', 'set "LASTLIGHT_ASSET_DIR=%~dp0assets"', 'set "LASTLIGHT_WORLD=%~dp0content\world.json"', 'set "LASTLIGHT_CONTENT=%~dp0content\encounters.json"', 'set "LASTLIGHT_SKILLS=%~dp0content\skills.json"', 'set "LASTLIGHT_ENEMIES=%~dp0content\enemy_catalog.json"', 'set "LASTLIGHT_ORDNANCE=%~dp0content\ordnance.json"', 'set "LASTLIGHT_PROGRESSION=%~dp0content\progression.json"', 'set "LASTLIGHT_QUALITY=%~dp0content\quality.json"', 'set "LASTLIGHT_BALANCE=%~dp0content\balance.json"', 'set "LASTLIGHT_REVIEW_EXIT="', 'set "LASTLIGHT_REVIEW_CAPTURE="', 'set "LASTLIGHT_PERF_REPORT="', 'set "LASTLIGHT_CAPTURE_SEQUENCE="', 'set "LASTLIGHT_REVIEW_MOTION="', 'set "LASTLIGHT_REVIEW_REDUCED="', 'set "LASTLIGHT_REVIEW_SCALE="', 'set "LASTLIGHT_REVIEW_INPUT_SAVE="', 'set "LASTLIGHT_REVIEW_CUE_LOG="', 'set "LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT="', 'set "LASTLIGHT_PLAYER_MODEL_PARITY_REPORT="', 'set "LASTLIGHT_PERF_WARMUP_FRAMES="', 'set "LASTLIGHT_PERF_FRAMES="', 'set "LASTLIGHT_PERF_ACTIVE_ONLY="', 'set "LASTLIGHT_PERF_CPU="', 'set "LASTLIGHT_PERF_AUDIO="', 'set "LASTLIGHT_PERF_NPC_SPEED="', 'set "LASTLIGHT_PERF_PROFILE="', 'set "LASTLIGHT_PERF_PRESENT_MODE="', 'set "LASTLIGHT_PERF_PIPELINED="')
        if ($launcher.Backend) { $lines += "set `"WGPU_BACKEND=$($launcher.Backend)`"" }
        if ($launcher.Scene) {
            $lines += 'set "LASTLIGHT_PRACTICE=1"'
            $lines += "set `"LASTLIGHT_REVIEW_SCREEN=$($launcher.Scene)`""
        }
        else {
            $lines += 'set "LASTLIGHT_PRACTICE="'
            $lines += 'set "LASTLIGHT_REVIEW_SCREEN="'
        }
        $lines += 'powershell.exe -NoProfile -Command "$folder=$env:LASTLIGHT_PLAYTEST_HOME; Start-Process -FilePath (Join-Path $folder ''Lastlight.exe'') -WorkingDirectory $folder -WindowStyle Hidden"'
        [IO.File]::WriteAllText((Join-Path $package $launcher.Name), (($lines -join "`r`n") + "`r`n"), [Text.Encoding]::ASCII)
    }

    $dead = @(Test-DocLinkClosure -PackageRoot $package)
    if ($dead.Count -gt 0) { throw "Shipped docs link outside the package:`n$($dead -join "`n")" }
    $docBytes = [long]0
    foreach ($doc in @(Get-ChildItem -LiteralPath (Join-Path $package 'docs') -File -Recurse)) { $docBytes += $doc.Length }
    if ($docBytes -gt $script:PlaytestDocsLimitBytes) { throw "The packaged docs are $docBytes B, above $script:PlaytestDocsLimitBytes B" }

    $files = @(Get-PackageFileRecord -PackageRoot $package)
    $manifest = New-PlaytestManifest -GameVersion $gameVersion -Commit $commit -Dirty $dirty -Branch $branch -Profile $Profile -Features $features -Rustc $rustc -CreatedUtc $createdUtc -CuratedDocs $CuratedDocs -Files $files
    Write-Utf8NoBom -Path (Join-Path $package 'manifest.json') -Text (($manifest | ConvertTo-Json -Depth 6) + "`r`n")
    $zip = New-PlaytestZip -SourceDirectory $package -Destination $zip
    $packed = $true
}
finally {
    if (-not $packed) {
        foreach ($partial in @($package, $zip)) {
            if (Test-Path -LiteralPath $partial) {
                try { Remove-Item -LiteralPath $partial -Recurse -Force }
                catch { Write-Warning "Could not remove the partial package $partial`: $($_.Exception.Message)" }
            }
        }
    }
}
Write-Output $zip
Write-Output (Get-FileSha256 -Path $zip)
