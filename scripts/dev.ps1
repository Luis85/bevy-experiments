# Requires PowerShell 7 (pwsh), available on Windows, Linux and macOS.
[CmdletBinding()]
param(
    [ValidateSet('doctor', 'fmt', 'check', 'test', 'clippy', 'verify')]
    [string]$Task = 'doctor',
    [ValidateSet('all', 'bevy-rts', 'legion-battle-lab', 'npc-framework', 'terra-world')]
    [string]$Project = 'all',
    [switch]$IncludeDeviceTests
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path -Parent $PSScriptRoot
$projects = @('bevy-rts', 'legion-battle-lab', 'npc-framework', 'terra-world')
if ($Project -ne 'all') { $projects = @($Project) }

function Invoke-Cargo {
    param([string[]]$CargoArgs)
    Write-Host "cargo $($CargoArgs -join ' ')"
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed with exit code $LASTEXITCODE"
    }
}

if ($Task -eq 'doctor') {
    foreach ($tool in @('git', 'rustup', 'cargo', 'rustc')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "Missing required tool: $tool. See CONTRIBUTING.md."
        }
    }
    & git --version
    & rustup show active-toolchain
}

foreach ($name in $projects) {
    Write-Host "`n=== $name : $Task ==="
    Push-Location (Join-Path $repoRoot $name)
    try {
        $testArgs = @('test', '--locked', '--workspace')
        if ($name -eq 'bevy-rts' -and -not $IncludeDeviceTests) {
            # Match Frontline's ordinary gate: device/BRP and long milestone
            # tests are separate, explicit local verification.
            $testArgs += @('--', '--skip', 'gpu_', '--skip', 'brp_', '--skip', 'milestone_')
            if ($Task -in @('test', 'verify')) {
                Write-Host 'Frontline baseline excludes gpu_*, brp_* and milestone_* tests; use -IncludeDeviceTests for the complete suite.'
            }
        }
        switch ($Task) {
            'doctor' {
                Invoke-Cargo @('version')
                # Run inside the workspace so its aliases/configuration are loaded.
                $metadata = & cargo metadata --locked --no-deps --format-version 1
                if ($LASTEXITCODE -ne 0) { throw "Invalid workspace: $name" }
                $workspace = ($metadata -join "`n") | ConvertFrom-Json
                Write-Host "$($workspace.workspace_members.Count) workspace members; target: $($workspace.target_directory)"
            }
            'fmt' { Invoke-Cargo @('fmt', '--all', '--', '--check') }
            'check' { Invoke-Cargo @('check', '--locked', '--workspace', '--all-targets') }
            'test' { Invoke-Cargo $testArgs }
            'clippy' { Invoke-Cargo @('clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings') }
            'verify' {
                Invoke-Cargo @('fmt', '--all', '--', '--check')
                Invoke-Cargo @('clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings')
                Invoke-Cargo $testArgs
            }
        }
    }
    finally { Pop-Location }
}
