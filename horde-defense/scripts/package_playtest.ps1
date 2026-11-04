[CmdletBinding()]
param(
    [ValidateSet('debug', 'release', 'playtest')][string]$Profile = 'playtest',
    [string]$OutputDirectory = 'target/playtest',
    # Include the `review` feature (captures, review fixtures, perf reports).
    # Ordinary playtest packages leave it out; practice scenes still work.
    [switch]$Review
)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    $buildArguments = @('build', '--locked', '-p', 'horde-defense')
    if (-not $Review) { $buildArguments += '--no-default-features' }
    if ($Profile -eq 'release') { $buildArguments += '--release' }
    elseif ($Profile -ne 'debug') { $buildArguments += @('--profile', $Profile) }
    & cargo @buildArguments
    if ($LASTEXITCODE -ne 0) { throw 'Playtest executable build failed' }
    $outputRoot = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) { [System.IO.Path]::GetFullPath($OutputDirectory) } else { [System.IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory)) }
    New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
    $packageName = 'Lastlight-playtest-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
    $package = Join-Path $outputRoot $packageName
    if (Test-Path -LiteralPath $package) { throw "Package already exists: $package" }
    New-Item -ItemType Directory -Path $package | Out-Null
    Copy-Item -LiteralPath "target/$Profile/horde-defense.exe" -Destination (Join-Path $package 'Lastlight.exe')
    Copy-Item -LiteralPath 'assets' -Destination (Join-Path $package 'assets') -Recurse
    New-Item -ItemType Directory -Path (Join-Path $package 'content') | Out-Null
    Copy-Item -LiteralPath 'crates/horde_core/content.json' -Destination (Join-Path $package 'content/encounters.json')
    foreach ($catalog in @('skills.json', 'enemy_catalog.json', 'ordnance.json', 'progression.json', 'quality.json', 'balance.json')) {
        Copy-Item -LiteralPath (Join-Path 'crates/horde_core' $catalog) -Destination (Join-Path $package "content/$catalog")
    }
    Copy-Item -LiteralPath 'crates/horde_core/src/world.json' -Destination (Join-Path $package 'content/world.json')
    Copy-Item -LiteralPath 'docs' -Destination (Join-Path $package 'docs') -Recurse
    Copy-Item -LiteralPath '../LICENSE' -Destination (Join-Path $package 'LICENSE')
    @'
LASTLIGHT - HORDE DEFENSE PLAYTEST

Run Play Lastlight.cmd for your ordinary persistent campaign.
If automatic graphics fail, Play Lastlight (DirectX12).cmd uses the same campaign
through the separately validated Windows DirectX 12 renderer.
Battle practice.cmd and Strategy practice.cmd use separate practice campaigns.
You can also run Lastlight.exe directly; its diagnostic console may be visible.

No Rust installation is needed for this Windows x64 build. A working graphics
driver is required. This package was validated on the development machine;
it is not a claim of minimum-spec support on every Windows PC.

Read docs/PLAYTEST_GUIDE.md for controls, goals, known limits and feedback prompts.
Your ordinary campaign is stored under LOCALAPPDATA/LastlightHordeDefense.
Practice never grants currency or unlocks to that campaign.
New game in the main menu confirms and archives the previous journey before
starting fresh; display, audio, controls and committed difficulty are retained.
Practice modes use separate profiles. Run one ordinary campaign window at a time.
'@ | Set-Content -LiteralPath (Join-Path $package 'READ ME.txt') -Encoding utf8
    foreach ($launcher in @(
        @{ Name = 'Play Lastlight.cmd'; Scene = '' },
        @{ Name = 'Play Lastlight (DirectX12).cmd'; Scene = ''; Backend = 'dx12' },
        @{ Name = 'Battle practice.cmd'; Scene = 'construction' },
        @{ Name = 'Strategy practice.cmd'; Scene = 'operations' }
    )) {
        $lines = @('@echo off', 'setlocal DisableDelayedExpansion', 'set "LASTLIGHT_PLAYTEST_HOME=%~dp0"', 'set "LASTLIGHT_ASSET_DIR=%~dp0assets"', 'set "LASTLIGHT_WORLD=%~dp0content\world.json"', 'set "LASTLIGHT_CONTENT=%~dp0content\encounters.json"', 'set "LASTLIGHT_SKILLS=%~dp0content\skills.json"', 'set "LASTLIGHT_ENEMIES=%~dp0content\enemy_catalog.json"', 'set "LASTLIGHT_ORDNANCE=%~dp0content\ordnance.json"', 'set "LASTLIGHT_PROGRESSION=%~dp0content\progression.json"', 'set "LASTLIGHT_QUALITY=%~dp0content\quality.json"', 'set "LASTLIGHT_BALANCE=%~dp0content\balance.json"', 'set "LASTLIGHT_REVIEW_EXIT="', 'set "LASTLIGHT_REVIEW_CAPTURE="', 'set "LASTLIGHT_PERF_REPORT="', 'set "LASTLIGHT_CAPTURE_SEQUENCE="', 'set "LASTLIGHT_REVIEW_MOTION="', 'set "LASTLIGHT_REVIEW_REDUCED="')
        if ($launcher.Backend) { $lines += "set `"WGPU_BACKEND=$($launcher.Backend)`"" }
        if ($launcher.Scene) {
            $lines += 'set "LASTLIGHT_PRACTICE=1"'
            $lines += "set `"LASTLIGHT_REVIEW_SCREEN=$($launcher.Scene)`""
        } else {
            $lines += 'set "LASTLIGHT_PRACTICE="'
            $lines += 'set "LASTLIGHT_REVIEW_SCREEN="'
        }
        $lines += 'powershell.exe -NoProfile -Command "$folder=$env:LASTLIGHT_PLAYTEST_HOME; Start-Process -FilePath (Join-Path $folder ''Lastlight.exe'') -WorkingDirectory $folder -WindowStyle Hidden"'
        $lines | Set-Content -LiteralPath (Join-Path $package $launcher.Name) -Encoding ascii
    }
    $files = Get-ChildItem -LiteralPath $package -File -Recurse | Sort-Object FullName | ForEach-Object {
        @{ path = [System.IO.Path]::GetRelativePath($package, $_.FullName).Replace('\', '/').Replace([char]92, [char]47); bytes = $_.Length; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    }
    @{ version = 1; game = 'Lastlight: Horde Defense'; profile = $Profile; platform = 'Windows x64'; source = 'current working tree'; created_utc = [DateTime]::UtcNow.ToString('o'); files = @($files) } |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $package 'manifest.json') -Encoding utf8
    $zip = "$package.zip"
    Compress-Archive -Path (Join-Path $package '*') -DestinationPath $zip
    Write-Output $zip
    Write-Output ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant())
}
finally { Pop-Location }
