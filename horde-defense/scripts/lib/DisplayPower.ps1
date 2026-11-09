#requires -Version 5.1
# Display power for perf evidence (R27, plan 2.4). measure_performance.ps1 holds the display on for its whole run, and a
# run that overlaps a display-off interval (a System log Kernel-Power event 506 or 507 between its start and end) is
# invalid evidence. W2-50 found that every 500 ms or 1,000 ms driver stall came from Windows turning the display off
# (Modern Standby on the idle timeout) during unattended passes, and that a display-required request removed them.
# Dot-source it: . (Join-Path $PSScriptRoot 'lib\DisplayPower.ps1')
# Self-tests: scripts/tests/run_script_tests.ps1 shims Set-ThreadExecutionState, Get-WinEvent and Get-DisplayPowerEvent.

if (-not ('LastLight.DisplayPower' -as [type])) {
    Add-Type -TypeDefinition @'
namespace LastLight {
    public static class DisplayPower {
        [System.Runtime.InteropServices.DllImport("kernel32.dll")]
        public static extern uint SetThreadExecutionState(uint flags);
    }
}
'@
}

function Get-DisplayHoldFlags {
    # ES_CONTINUOUS (0x80000000) | ES_SYSTEM_REQUIRED (0x1) | ES_DISPLAY_REQUIRED (0x2), the W2-50 hold arm's request.
    return [uint32]2147483651
}

function Set-ThreadExecutionState([uint32]$Flags) {
    # The one call into kernel32 (the script tests shim it). It returns the previous state, or 0 when Windows refuses.
    return [LastLight.DisplayPower]::SetThreadExecutionState($Flags)
}

function Enter-DisplayHold {
    # Holds the display and the system on for this thread until Exit-DisplayHold, so the idle timeout cannot turn the
    # display off during an evidence run. Throws when Windows refuses the request. The caller releases it in a finally.
    $previous = Set-ThreadExecutionState (Get-DisplayHoldFlags)
    if ($previous -eq 0) { throw 'SetThreadExecutionState refused the display-required request' }
    return [pscustomobject]@{
        Request = 'ES_CONTINUOUS|ES_SYSTEM_REQUIRED|ES_DISPLAY_REQUIRED'; Held = $true
        SetUtc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'); Released = $false; ReleasedUtc = $null; ReleasedFrom = $null
        Previous = [uint32]$previous
    }
}

function Exit-DisplayHold($Hold) {
    # Restores the state the hold replaced (with ES_CONTINUOUS): ES_CONTINUOUS alone for a plain hold, so the request is
    # cleared, and the outer request for a hold inside another one on the same thread (R31: the A/B sitting holds the
    # display around each measure_performance pass, which runs in its process). ReleasedFrom is the state the release
    # replaced, 0x80000003 while held.
    if ($null -eq $Hold -or -not $Hold.Held -or $Hold.Released) { return }
    $restore = [uint32]2147483648
    if ($null -ne $Hold.PSObject.Properties['Previous']) { $restore = [uint32]($restore -bor [uint32]$Hold.Previous) }
    $previous = Set-ThreadExecutionState $restore
    $Hold.Released = $true
    $Hold.ReleasedUtc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
    $Hold.ReleasedFrom = '0x' + ([uint32]$previous).ToString('X8')
}

function Get-DisplayPowerEvent([DateTime]$Start, [DateTime]$End) {
    # Every System log Kernel-Power 506 (Modern Standby entered: the display went off) and 507 (left) from Start to End,
    # oldest first, as objects with Id and TimeUtc. Throws when the log cannot be read.
    $filter = @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-Power'; Id = @(506, 507); StartTime = $Start; EndTime = $End }
    try { $found = @(Get-WinEvent -FilterHashtable $filter -ErrorAction Stop) }
    catch {
        if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') { return }
        throw
    }
    $found | Sort-Object -Property TimeCreated | ForEach-Object {
        [pscustomobject]@{ Id = [int]$_.Id; TimeUtc = $_.TimeCreated.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ') }
    }
}

function Test-DisplayPower([DateTime]$Start, [DateTime]$End) {
    # The display check of one run: its Kernel-Power 506/507 events, and the problem that makes the run invalid evidence
    # when it has any or when the log cannot be read (an unverified run is not evidence either).
    try { $events = @(Get-DisplayPowerEvent -Start $Start -End $End) }
    catch {
        return [pscustomobject]@{ Events = @(); Problems = @("display power unknown (Kernel-Power 506/507 unreadable: $($_.Exception.Message))") }
    }
    $problems = @()
    if ($events.Count -gt 0) {
        $list = @($events | ForEach-Object { "$($_.Id) at $($_.TimeUtc)" }) -join ', '
        $problems = @("display off during the run (Kernel-Power $list)")
    }
    return [pscustomobject]@{ Events = $events; Problems = $problems }
}
