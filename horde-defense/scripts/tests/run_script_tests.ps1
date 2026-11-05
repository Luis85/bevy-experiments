#requires -Version 5.1
# Self-tests for scripts/lib/Native.ps1 under Windows PowerShell 5.1.
# Run: powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/tests/run_script_tests.ps1
# The runner is expected to pass with its own output redirected. Prints PASS or FAIL and exits 0 or 1.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$scriptsDirectory = Split-Path -Parent $PSScriptRoot
$workspace = Split-Path -Parent $scriptsDirectory
$nativeLibrary = Join-Path $scriptsDirectory 'lib\Native.ps1'
if (-not (Test-Path -LiteralPath $nativeLibrary)) { throw "FAIL: missing $nativeLibrary" }
. $nativeLibrary

$script:failures = New-Object System.Collections.Generic.List[string]
$script:passed = 0

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Assert-Equal($Expected, $Actual, [string]$Message) {
    if (-not ($Expected -ceq $Actual)) { throw "$Message - expected [$Expected] got [$Actual]" }
}

function Test-Case([string]$Name, [scriptblock]$Body) {
    try {
        & $Body
        $script:passed += 1
        Write-Output "ok   $Name"
    }
    catch {
        $script:failures.Add("$Name - $($_.Exception.Message)")
        Write-Output "FAIL $Name - $($_.Exception.Message)"
    }
}

function Read-Log([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    return [IO.File]::ReadAllText($Path)
}

function Test-ProcessGone([int]$Id, [int]$WithinMilliseconds) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($WithinMilliseconds)
    do {
        if ($null -eq (Get-Process -Id $Id -ErrorAction SilentlyContinue)) { return $true }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

$temporary = Join-Path ([IO.Path]::GetTempPath()) ('lastlight-script-tests-' + $PID)
if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
New-Item -ItemType Directory -Path $temporary | Out-Null

# A tiny console helper with exact argv semantics (CommandLineToArgvW rules) and a parent/child sleeper.
# 'parent <pidfile> <parent s> <child s>' writes both ids; the child outlives the parent so an orphan is visible.
$helper = Join-Path $temporary 'native-helper.exe'
$helperSource = @'
using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
public static class LastLightNativeHelper {
    public static int Main(string[] args) {
        if (args.Length >= 1 && args[0] == "echo") {
            for (int i = 1; i < args.Length; i++) { Console.WriteLine("[" + args[i] + "]"); }
            return 0;
        }
        if (args.Length == 2 && args[0] == "sleep") {
            Thread.Sleep(int.Parse(args[1]) * 1000);
            return 0;
        }
        if (args.Length == 4 && args[0] == "parent") {
            ProcessStartInfo info = new ProcessStartInfo(Process.GetCurrentProcess().MainModule.FileName, "sleep " + args[3]);
            info.UseShellExecute = false;
            Process child = Process.Start(info);
            File.WriteAllText(args[1], Process.GetCurrentProcess().Id + " " + child.Id);
            Thread.Sleep(int.Parse(args[2]) * 1000);
            return 0;
        }
        return 2;
    }
}
'@
Add-Type -TypeDefinition $helperSource -OutputType ConsoleApplication -OutputAssembly $helper

$savedTarget = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
try {
    Test-Case 'Invoke-Native returns exit code 3 and both logs' {
        $stem = Join-Path $temporary 'logs\exit3'
        $r = Invoke-Native -FilePath (Resolve-Tool 'cmd') -Arguments @('/c', 'echo out & echo err 1>&2 & exit 3') -LogStem $stem -TimeoutSeconds 30
        Assert-Equal 3 $r.ExitCode 'exit code'
        Assert-True ($r.Id -gt 0) 'process id'
        Assert-True ($r.Seconds -ge 0) 'seconds'
        Assert-Equal "$stem.out.txt" $r.Out 'out log path'
        Assert-Equal "$stem.err.txt" $r.Err 'err log path'
        Assert-Equal 'out' (Read-Log $r.Out).Trim() 'stdout log'
        Assert-Equal 'err' (Read-Log $r.Err).Trim() 'stderr log'
    }

    Test-Case 'Invoke-Native survives a caller redirecting every stream' {
        # The caller's all-streams redirect is generated so the 5.1 lint, which bans that
        # operator in shipped scripts, does not flag this file.
        $inner = Join-Path $temporary 'caller.ps1'
        $innerSource = @(
            'param([string]$Native, [string]$Json, [string]$Stem, [string]$CallerLog)',
            '$ErrorActionPreference = ''Stop''',
            '. $Native',
            '& {',
            '    $r = Invoke-Native -FilePath (Resolve-Tool ''cmd'') -Arguments @(''/c'', ''echo out & echo err 1>&2 & exit 3'') -LogStem $Stem -TimeoutSeconds 30',
            '    Write-Utf8NoBom -Path $Json -Text ($r | ConvertTo-Json -Compress)',
            '    Write-Output ''caller output''',
            ('} ' + '*' + '> $CallerLog')
        ) -join "`n"
        Write-Utf8NoBom -Path $inner -Text $innerSource
        $json = Join-Path $temporary 'caller.json'
        $stem = Join-Path $temporary 'logs\caller-exit3'
        $callerLog = Join-Path $temporary 'caller.log'
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $inner, $nativeLibrary, $json, $stem, $callerLog)
        $outer = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $arguments -LogStem (Join-Path $temporary 'logs\caller-host') -TimeoutSeconds 120
        Assert-Equal 0 $outer.ExitCode ("redirected caller exit code; err: " + (Read-Log $outer.Err))
        $r = Get-Content -Raw -LiteralPath $json | ConvertFrom-Json
        Assert-Equal 3 $r.ExitCode 'exit code under redirect'
        Assert-Equal 'out' (Read-Log "$stem.out.txt").Trim() 'stdout log under redirect'
        Assert-Equal 'err' (Read-Log "$stem.err.txt").Trim() 'stderr log under redirect'
        Assert-True ((Read-Log $callerLog) -match 'caller output') 'caller redirect captured its own output'
    }

    Test-Case 'ConvertTo-NativeArgument quotes with MSVC rules' {
        Assert-Equal '"a b"' (ConvertTo-NativeArgument @('a b')) 'whitespace'
        Assert-Equal '"x\"y"' (ConvertTo-NativeArgument @('x"y')) 'inner quote'
        Assert-Equal 'c:\p\' (ConvertTo-NativeArgument @('c:\p\')) 'trailing backslash, unquoted'
        Assert-Equal '""' (ConvertTo-NativeArgument @('')) 'empty'
        Assert-Equal '"c:\p q\\"' (ConvertTo-NativeArgument @('c:\p q\')) 'trailing backslash, quoted'
        Assert-Equal '"a\\\"b"' (ConvertTo-NativeArgument @('a\"b')) 'backslash before quote'
        Assert-Equal '"a b" "x\"y" c:\p\ ""' (ConvertTo-NativeArgument @('a b', 'x"y', 'c:\p\', '')) 'joined'
        Assert-Equal '' (ConvertTo-NativeArgument @()) 'no arguments'
    }

    Test-Case 'ConvertTo-NativeArgument round-trips through argv' {
        $cases = @('a b', 'x"y', 'c:\p\', '', 'c:\p q\', 'a\"b', 'plain', "tab`there")
        $r = Invoke-Native -FilePath $helper -Arguments (@('echo') + $cases) -LogStem (Join-Path $temporary 'logs\echo') -TimeoutSeconds 30
        Assert-Equal 0 $r.ExitCode 'echo exit code'
        $lines = @((Read-Log $r.Out) -split "`r?`n" | Where-Object { $_ -ne '' })
        Assert-Equal $cases.Count $lines.Count 'argument count'
        for ($i = 0; $i -lt $cases.Count; $i++) { Assert-Equal ('[' + $cases[$i] + ']') $lines[$i] "argument $i" }
    }

    Test-Case 'Invoke-Native timeout kills the process tree' {
        $pidFile = Join-Path $temporary 'sleeper.pid'
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $message = $null
        try {
            Invoke-Native -FilePath $helper -Arguments @('parent', $pidFile, '5', '30') -LogStem (Join-Path $temporary 'logs\sleeper') -TimeoutSeconds 1 | Out-Null
        }
        catch { $message = $_.Exception.Message }
        $watch.Stop()
        Assert-True ($null -ne $message) 'timeout did not throw'
        Assert-True ($message -match 'Timed out after 1 s') "timeout message: $message"
        Assert-True ($watch.Elapsed.TotalSeconds -lt 4.5) "timeout took $($watch.Elapsed.TotalSeconds) s"
        Assert-True (Test-Path -LiteralPath $pidFile) 'sleeper never started its child'
        $ids = @((Get-Content -Raw -LiteralPath $pidFile).Trim() -split ' ' | ForEach-Object { [int]$_ })
        $parentGone = Test-ProcessGone $ids[0] 3000
        $childGone = Test-ProcessGone $ids[1] 3000
        foreach ($id in $ids) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
        Assert-True $parentGone "parent $($ids[0]) still running"
        Assert-True $childGone "child $($ids[1]) still running"
    }

    Test-Case 'Resolve-HordeTarget honours CARGO_TARGET_DIR' {
        [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $null, 'Process')
        Assert-Equal ([IO.Path]::GetFullPath((Join-Path $workspace 'target'))) (Resolve-HordeTarget) 'unset'
        $env:CARGO_TARGET_DIR = 'custom\tgt'
        Assert-Equal ([IO.Path]::GetFullPath((Join-Path $workspace 'custom\tgt'))) (Resolve-HordeTarget) 'relative'
        $rooted = Join-Path $temporary 'rooted-target'
        $env:CARGO_TARGET_DIR = $rooted
        Assert-Equal $rooted (Resolve-HordeTarget) 'rooted'
    }

    Test-Case 'Get-HordeExecutable maps profiles and refuses missing builds' {
        $target = Join-Path $temporary 'exe-target'
        $env:CARGO_TARGET_DIR = $target
        New-Item -ItemType Directory -Force -Path (Join-Path $target 'debug') | Out-Null
        $exe = Join-Path $target 'debug\horde-defense.exe'
        Write-Utf8NoBom -Path $exe -Text 'stub'
        Assert-Equal $exe (Get-HordeExecutable -Profile dev) 'dev maps to debug'
        Assert-Equal $exe (Get-HordeExecutable -Profile debug) 'debug'
        $threw = $false
        try { Get-HordeExecutable -Profile playtest | Out-Null } catch { $threw = $_.Exception.Message -match 'playtest' }
        Assert-True $threw 'missing playtest build did not throw'
    }

    Test-Case 'Reset/Restore-LastlightEnv round-trips LASTLIGHT_CONTENT' {
        $env:LASTLIGHT_CONTENT = 'bogus-content-dir'
        [Environment]::SetEnvironmentVariable('LASTLIGHT_SCRIPT_TEST_LEAK', $null, 'Process')
        $saved = Reset-LastlightEnv
        Assert-True ($null -eq [Environment]::GetEnvironmentVariable('LASTLIGHT_CONTENT', 'Process')) 'LASTLIGHT_CONTENT still set after reset'
        $leftover = @(Get-ChildItem Env: | Where-Object { $_.Name -like 'LASTLIGHT_*' })
        Assert-Equal 0 $leftover.Count 'LASTLIGHT_ variables left after reset'
        $env:LASTLIGHT_SCRIPT_TEST_LEAK = 'set inside'
        Restore-LastlightEnv -Saved $saved
        Assert-Equal 'bogus-content-dir' $env:LASTLIGHT_CONTENT 'restored value'
        Assert-True ($null -eq [Environment]::GetEnvironmentVariable('LASTLIGHT_SCRIPT_TEST_LEAK', 'Process')) 'variable set inside leaked out'
        [Environment]::SetEnvironmentVariable('LASTLIGHT_CONTENT', $null, 'Process')
    }

    Test-Case 'Write-Utf8NoBom writes UTF-8 without BOM' {
        $path = Join-Path $temporary 'nobom\text.txt'
        Write-Utf8NoBom -Path $path -Text ('h' + [char]0xE9)
        $bytes = [IO.File]::ReadAllBytes($path)
        Assert-Equal '68-C3-A9' ([BitConverter]::ToString($bytes)) 'bytes'
    }

    Test-Case 'Get-FileSha256 hashes abc' {
        $path = Join-Path $temporary 'abc.txt'
        Write-Utf8NoBom -Path $path -Text 'abc'
        Assert-Equal 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad' (Get-FileSha256 -Path $path) 'sha256'
    }

    Test-Case 'Resolve-Tool finds applications only' {
        Assert-True ((Resolve-Tool 'cmd') -match '\\cmd\.exe$') 'cmd'
        $threw = $false
        try { Resolve-Tool 'Get-ChildItem' | Out-Null } catch { $threw = $true }
        Assert-True $threw 'a cmdlet resolved as a tool'
    }

    Test-Case 'Invoke-Cargo runs cargo and throws on failure' {
        $r = Invoke-Cargo -Arguments @('--version') -LogStem (Join-Path $temporary 'logs\cargo-version') -TimeoutSeconds 60
        Assert-Equal 0 $r.ExitCode 'cargo --version'
        Assert-True ((Read-Log $r.Out) -match '^cargo ') 'cargo --version output'
        $threw = $false
        try { Invoke-Cargo -Arguments @('no-such-subcommand-lastlight') -LogStem (Join-Path $temporary 'logs\cargo-bad') -TimeoutSeconds 60 | Out-Null } catch { $threw = $true }
        Assert-True $threw 'failing cargo did not throw'
    }
}
finally {
    [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $savedTarget, 'Process')
    try { Remove-Item -LiteralPath $temporary -Recurse -Force } catch { Write-Output "note: could not remove $temporary" }
}

if ($script:failures.Count -gt 0) {
    Write-Output ("FAIL: {0} of {1} cases failed" -f $script:failures.Count, ($script:failures.Count + $script:passed))
    foreach ($failure in $script:failures) { Write-Output "  $failure" }
    exit 1
}
Write-Output ("PASS: {0} cases" -f $script:passed)
exit 0
