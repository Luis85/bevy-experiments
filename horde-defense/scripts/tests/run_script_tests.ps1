#requires -Version 5.1
# Self-tests for scripts/lib/Native.ps1, scripts/measure_performance.ps1 and the capture_review.ps1 sidecar checks (with a
# fake game exe) under Windows PowerShell 5.1.
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
        if (args.Length == 1 && args[0] == "cwd") { Console.WriteLine(Environment.CurrentDirectory); return 0; }
        // 'review <pidfile> <s>' leaves a per-PID review profile behind, as profile.rs does.
        if (args.Length == 3 && args[0] == "review") {
            int id = Process.GetCurrentProcess().Id;
            string logs = Path.Combine(Path.Combine(Path.GetTempPath(), "lastlight-review-" + id), "logs");
            Directory.CreateDirectory(logs);
            File.WriteAllText(Path.Combine(logs, "review.log"), "review");
            File.WriteAllText(args[1], id.ToString());
            Thread.Sleep(int.Parse(args[2]) * 1000);
            return 0;
        }
        // Copied to <target>\debug\horde-defense.exe it stands in for the game in measure_performance.ps1 runs.
        if (args.Length == 1 && args[0] == "--version") { Console.WriteLine("horde-defense fake"); return 0; }
        if (args.Length == 1 && args[0] == "--list-scenes") {
            Console.WriteLine("name\tkind\tmotion\tperf\tpractice\tprimary\tbefore_proxy\tsince_wave");
            Console.WriteLine("menu\tStill\ttrue\tfalse\tfalse\t\t\t0");
            Console.WriteLine("progressed-peak\tStill\ttrue\ttrue\tfalse\t\t\t0");
            Console.WriteLine("peak\tStill\ttrue\ttrue\tfalse\t\t\t0");
            Console.WriteLine("stress\tMotionOnly\ttrue\ttrue\tfalse\t\tmax-lanes\t1");
            Console.WriteLine("battle\tStill\tfalse\tfalse\tfalse\t\t\t0");
            Console.WriteLine("construction\tStill\tfalse\tfalse\ttrue\t\t\t0");
            Console.WriteLine("max-lanes\tStill\tfalse\tfalse\tfalse\t\t\t1");
            Console.WriteLine("broken\tStill\tfalse\tfalse\tfalse\t\t\t1");
            Console.WriteLine("unscaled\tStill\tfalse\tfalse\tfalse\t\t\t1");
            return 0;
        }
        // A capture_review still: a PNG header at twice the requested width (Windows scale 2) plus a sidecar per scene.
        // battle passes every check, construction moves the arena at small size, max-lanes misses the pre-zoom arena
        // at small size and has 3 HUD rows, menu writes no sidecar, broken writes one that is not JSON and unscaled
        // ignores the requested reduced effects and UI scale.
        string capture = Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_CAPTURE");
        if (args.Length == 0 && !String.IsNullOrEmpty(capture)) {
            string scene = Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCREEN");
            int width = int.Parse(Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SIZE").Split('x')[0]);
            bool small = width == 1050;
            byte[] png = new byte[24];
            byte[] head = new byte[] { 137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82 };
            Array.Copy(head, png, head.Length);
            int pixels = width * 2;
            png[16] = (byte)(pixels >> 24); png[17] = (byte)(pixels >> 16); png[18] = (byte)(pixels >> 8); png[19] = (byte)pixels;
            Directory.CreateDirectory(Path.GetDirectoryName(capture));
            File.WriteAllBytes(capture, png);
            // Every sidecar echoes the requested reduced-effects mode and UI scale, except unscaled, which always
            // reports normal effects at scale 1 (the review overrides did not take).
            string echo = "\"reduced_effects\":" + (Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_REDUCED") == "1" ? "true" : "false")
                + ",\"ui_scale\":" + Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCALE") + ",";
            string sidecar = null;
            if (scene == "battle") { sidecar = "{" + echo + "\"screen\":\"Battle\",\"hud_rows\":2,\"arena_rect\":[10,20,30,40]}"; }
            if (scene == "construction") { sidecar = "{" + echo + "\"screen\":\"Battle\",\"hud_rows\":null,\"arena_rect\":" + (small ? "[11,20,30,40]" : "[10,20,30,40]") + "}"; }
            if (scene == "max-lanes") {
                sidecar = "{" + echo + "\"screen\":\"Battle\",\"hud_rows\":3,\"lanes\":17,\"width_yards\":30,\"canvas_rect\":[0,0,1440,900],\"arena_rect\":"
                    + (small ? "[0,0,1440,900]" : "[24,124,1416,816]") + "}";
            }
            if (scene == "unscaled") { sidecar = "{\"reduced_effects\":false,\"ui_scale\":1.0,\"screen\":\"Menu\"}"; }
            if (scene == "broken") { sidecar = "{not json"; }
            if (sidecar != null) { File.WriteAllText(Path.ChangeExtension(capture, ".json"), sidecar); }
            return 0;
        }
        string report = Environment.GetEnvironmentVariable("LASTLIGHT_PERF_REPORT");
        if (args.Length == 0 && !String.IsNullOrEmpty(report)) {
            int unfocused = 0;
            string once = Environment.GetEnvironmentVariable("HORDE_FAKE_UNFOCUSED_ONCE");
            if (!String.IsNullOrEmpty(once) && File.Exists(once)) { File.Delete(once); unfocused = 3; }
            if (Environment.GetEnvironmentVariable("HORDE_FAKE_UNFOCUSED_ALWAYS") == "1") { unfocused = 2; }
            string mode = Environment.GetEnvironmentVariable("LASTLIGHT_PERF_PRESENT_MODE") == "autonovsync" ? "AutoNoVsync" : "Fifo";
            string pipelined = Environment.GetEnvironmentVariable("LASTLIGHT_PERF_PIPELINED") == "0" ? "false" : "true";
            // The adapter the renderer used: Bevy falls back to the default GPU when WGPU_ADAPTER_NAME matches none.
            string adapter = Environment.GetEnvironmentVariable("HORDE_FAKE_ADAPTER");
            if (String.IsNullOrEmpty(adapter)) { adapter = "Fake Adapter"; }
            File.WriteAllText(report, "{\"version\":3,\"scene\":\"" + Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCREEN")
                + "\",\"adapter\":\"" + adapter
                + "\",\"sample_frames\":" + Environment.GetEnvironmentVariable("LASTLIGHT_PERF_FRAMES")
                + ",\"completed_sample_target\":true,\"paused\":false,\"unfocused_sample_frames\":" + unfocused
                + ",\"frames_over_50ms\":0,\"frames_over_50ms_driver_stall\":0,\"driver_stalls\":{\"frames\":[],\"outside\":{\"frames_over_50ms\":0}}"
                + ",\"work_mean_ms\":[0.4,0.0,1.0]"
                + ",\"phases\":{\"cpu_frame_ms\":{\"mean\":4.0,\"p50\":4.0,\"p95\":4.5,\"p99\":5.0,\"max\":6.0},\"interface_total_ms\":{\"mean\":1.0,\"p50\":1.0,\"p95\":1.2,\"p99\":1.3,\"max\":1.5}}"
                + ",\"egui\":{\"texture_uploads\":{\"count\":0,\"bytes\":0,\"frames\":0}}"
                + ",\"present\":{\"requested_mode\":\"" + mode + "\",\"pipelined_rendering\":" + pipelined + "}"
                + ",\"build\":{\"profile_label\":\"" + Environment.GetEnvironmentVariable("LASTLIGHT_PERF_PROFILE") + "\"}}");
            return 0;
        }
        Console.WriteLine("unhandled argc " + args.Length);
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

    Test-Case 'Invoke-Native runs an exe with no arguments' {
        # 5.1 Start-Process rejects an empty -ArgumentList; the review captures launch the game this way.
        [Environment]::SetEnvironmentVariable('LASTLIGHT_PERF_REPORT', $null, 'Process')
        $r = Invoke-Native -FilePath $helper -LogStem (Join-Path $temporary 'logs\no-arguments') -TimeoutSeconds 30
        Assert-Equal 2 $r.ExitCode ('no-argument exit code; err: ' + (Read-Log $r.Err))
        Assert-Equal 'unhandled argc 0' (Read-Log $r.Out).Trim() 'no-argument argv'
    }

    Test-Case 'Invoke-Native and Invoke-Cargo run in their working directory, not the caller location' {
        $probe = Join-Path $temporary 'cwd probe'
        New-Item -ItemType Directory -Force -Path $probe | Out-Null
        Push-Location -LiteralPath $temporary
        try {
            $r = Invoke-Native -FilePath $helper -Arguments @('cwd') -LogStem (Join-Path $temporary 'logs\cwd') -TimeoutSeconds 30 -WorkingDirectory $probe
            Assert-Equal 0 $r.ExitCode 'cwd exit code'
            Assert-Equal $probe (Read-Log $r.Out).Trim() 'Invoke-Native working directory'
            # Outside any Cargo workspace, only Invoke-Cargo's default working directory finds this one.
            $c = Invoke-Cargo -Arguments @('locate-project', '--workspace', '--message-format', 'plain') -LogStem (Join-Path $temporary 'logs\cargo-locate') -TimeoutSeconds 60
            Assert-Equal (Join-Path $workspace 'Cargo.toml') (Read-Log $c.Out).Trim() 'Invoke-Cargo default working directory'
        }
        finally { Pop-Location }
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

    Test-Case 'Invoke-Native -RemoveReviewProfile removes the per-PID profile on exit and on timeout' {
        $pidFile = Join-Path $temporary 'review.pid'
        $r = Invoke-Native -FilePath $helper -Arguments @('review', $pidFile, '0') -LogStem (Join-Path $temporary 'logs\review-exit') -TimeoutSeconds 30 -RemoveReviewProfile
        Assert-Equal 0 $r.ExitCode ('review helper exit code; err: ' + (Read-Log $r.Err))
        $exited = Join-Path ([IO.Path]::GetTempPath()) ('lastlight-review-' + $r.Id)
        $exitedLeft = Test-Path -LiteralPath $exited
        if ($exitedLeft) { Remove-Item -LiteralPath $exited -Recurse -Force }
        Assert-True (-not $exitedLeft) "profile left after a clean exit: $exited"
        Remove-Item -LiteralPath $pidFile -Force
        $message = $null
        try {
            Invoke-Native -FilePath $helper -Arguments @('review', $pidFile, '30') -LogStem (Join-Path $temporary 'logs\review-hung') -TimeoutSeconds 1 -RemoveReviewProfile | Out-Null
        }
        catch { $message = $_.Exception.Message }
        Assert-True ($null -ne $message) 'timeout did not throw'
        Assert-True (Test-Path -LiteralPath $pidFile) 'review helper never started'
        $id = [int](Get-Content -Raw -LiteralPath $pidFile).Trim()
        $hung = Join-Path ([IO.Path]::GetTempPath()) ('lastlight-review-' + $id)
        $hungLeft = Test-Path -LiteralPath $hung
        Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
        if ($hungLeft) { Remove-Item -LiteralPath $hung -Recurse -Force }
        Assert-True ($message -match "\(PID $id\)") "timeout message lacks the PID: $message"
        Assert-True (-not $hungLeft) "profile left after a timeout: $hung"
    }

    Test-Case 'Get-ReviewSceneList parses the --list-scenes TSV and reports a build without it' {
        $list = Get-ReviewSceneList -Executable $helper -LogStem (Join-Path $temporary 'logs\list-scenes') -WorkingDirectory $temporary
        Assert-Equal 0 $list.ExitCode ('exit code; err: ' + (Read-Log $list.Err))
        Assert-True ((Read-Log $list.Out).StartsWith("name`tkind`t")) 'the raw TSV stays in the out log'
        $names = @($list.Scenes | ForEach-Object { $_.Name })
        Assert-Equal 'menu,progressed-peak,peak,stress' ($names[0..3] -join ',') 'rows in order, header skipped'
        $menu = @($list.Scenes)[0]
        $stress = @($list.Scenes)[3]
        Assert-Equal 'Still' $menu.Kind 'menu kind'
        Assert-Equal 'MotionOnly' $stress.Kind 'stress kind'
        Assert-True ($menu.Motion -is [bool] -and $menu.Motion) 'menu motion'
        Assert-True ($menu.Perf -is [bool] -and -not $menu.Perf) 'menu perf'
        Assert-True ($stress.Perf -and -not $stress.Practice) 'stress perf and practice'
        Assert-True ($null -eq $menu.Primary -and $null -eq $menu.BeforeProxy) 'empty columns read as null'
        Assert-Equal 'max-lanes' $stress.BeforeProxy 'before proxy'
        Assert-Equal 1 $stress.SinceWave 'since wave'
        $old = Get-ReviewSceneList -Executable (Resolve-Tool 'whoami') -LogStem (Join-Path $temporary 'logs\list-scenes-old') -WorkingDirectory $temporary
        Assert-True ($old.ExitCode -ne 0) 'an exe without --list-scenes exits non-zero'
        Assert-True ($null -eq $old.Scenes) 'no rows from a failed listing'
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

    Test-Case 'Resolve-WorkspacePath keeps a rooted path and joins a relative one to the workspace' {
        $rooted = Join-Path $temporary 'rooted\file.exe'
        Assert-Equal $rooted (Resolve-WorkspacePath $rooted) 'rooted'
        Assert-Equal ([IO.Path]::GetFullPath((Join-Path $workspace 'target\debug\x.exe'))) (Resolve-WorkspacePath 'target/debug/x.exe') 'relative'
        Assert-Equal (Join-Path $temporary 'x.exe') (Resolve-WorkspacePath 'x.exe' -Workspace $temporary) 'explicit workspace'
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

    $measure = Join-Path $scriptsDirectory 'measure_performance.ps1'
    function Invoke-Measure([string[]]$Arguments, [string]$Stem) {
        $all = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $measure) + $Arguments
        return Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $all -LogStem (Join-Path $temporary "logs\$Stem") -TimeoutSeconds 300
    }
    function New-FakeTarget([string]$Target) {
        New-Item -ItemType Directory -Force -Path (Join-Path $Target 'debug') | Out-Null
        Copy-Item -LiteralPath $helper -Destination (Join-Path $Target 'debug\horde-defense.exe')
    }

    Test-Case 'measure_performance -SelfTest reports the expected exit code for every fixture under redirect' {
        $r = Invoke-Measure @('-SelfTest') 'measure-selftest'
        $out = Read-Log $r.Out
        Assert-Equal 0 $r.ExitCode ("self-test exit code; out: $out err: " + (Read-Log $r.Err))
        $lines = @(
            'ok   within-budget -> 0', 'ok   over-budget-cpu -> 1', 'ok   over-50ms -> 1', 'ok   unfocused -> 2',
            'ok   vsync-on-within-budget -> 0', 'ok   over-interface-draw -> 1', 'ok   playtest-vsync-on -> 0',
            'ok   missing-metric -> 2', 'ok   missing-cpu-frame -> 2', 'ok   over-texture-uploads -> 1',
            'ok   missing-texture-uploads -> 2', 'ok   driver-stall -> 0', 'ok   missing-driver-stalls -> 2',
            'ok   v3: v3 interface_total above the baseline -> 1',
            'ok   v3: v3 without a small value -> 2', "ok   power plan: Power saver -> 'Power saver'", 'SELFTEST PASS'
        )
        foreach ($line in $lines) {
            Assert-True ($out.Contains($line)) "self-test output lacks '$line'"
        }
        Assert-Equal '' (Read-Log $r.Err).Trim() 'self-test stderr'
    }

    Test-Case 'measure_performance exits 3, never 1, on bad arguments' {
        $cases = @(
            @('-Scene', 'bogus'), @('-Scene', 'stres'), @('-Profile', 'release'), @('-Pipelined', 'maybe'),
            @('-Repeat', '0'), @('-Repeat', 'x'), @('-Frames', '4000'), @('-Frames', '100'), @('-Warmup', '900'),
            @('-TimeoutSeconds', '0'), @('-Bogus'), @('peak')
        )
        $index = 0
        foreach ($arguments in $cases) {
            $index += 1
            $r = Invoke-Measure ($arguments + @('-NoBuild', '-OutputDirectory', (Join-Path $temporary "perf-bad-$index"))) "measure-bad-$index"
            Assert-Equal 3 $r.ExitCode ("$($arguments -join ' '): exit code; out: " + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err))
            Assert-True ((Read-Log $r.Out) -match 'FAIL ') "$($arguments -join ' '): no FAIL line"
            Assert-True (-not (Test-Path -LiteralPath (Join-Path $temporary "perf-bad-$index"))) "$($arguments -join ' '): created its output directory"
        }
    }

    Test-Case 'measure_performance uses a relative CARGO_TARGET_DIR and reruns an unfocused run once' {
        $relative = "target\script-tests-$PID-relative"
        $full = [IO.Path]::GetFullPath((Join-Path $workspace $relative))
        try {
            New-FakeTarget $full
            $once = Join-Path $temporary 'unfocused-once'
            Write-Utf8NoBom -Path $once -Text 'x'
            $env:HORDE_FAKE_UNFOCUSED_ONCE = $once
            $env:CARGO_TARGET_DIR = $relative
            $out = Join-Path $temporary 'perf-relative'
            $r = Invoke-Measure @('-Scene', 'all', '-Profile', 'dev', '-NoBuild', '-NoBudget', '-OutputDirectory', $out) 'measure-relative'
            Assert-Equal 0 $r.ExitCode ("exit code; out: " + (Read-Log $r.Out) + " err: " + (Read-Log $r.Err))
            $reports = @(Get-ChildItem -LiteralPath $out -Filter '*.json' -File | Where-Object { $_.Name -ne 'summary.json' -and $_.Name -notlike '*.invalid.json' } | ForEach-Object { $_.Name } | Sort-Object)
            Assert-Equal 'peak-normal.json,peak-small.json,progressed-peak-normal.json,progressed-peak-small.json,stress-normal.json,stress-small.json' ($reports -join ',') 'v3 reports'
            Assert-True (Test-Path -LiteralPath (Join-Path $out 'peak-normal.invalid.json')) 'the rejected attempt was not kept'
            Assert-True (Test-Path -LiteralPath (Join-Path $out 'summary.md')) 'summary.md'
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            Assert-Equal 0 $summary.exit_code 'summary exit code'
            Assert-Equal $full $summary.cargo_target_dir 'resolved target'
            Assert-Equal 2 $summary.runs[0].attempts 'first run attempts'
            Assert-Equal 1 $summary.runs[1].attempts 'second run attempts'
            Assert-Equal 'Fifo' $summary.runs[0].present_mode 'present mode echo'
            Assert-True (-not (Test-Path -LiteralPath $once)) 'the fake never saw the first launch'
        }
        finally {
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_UNFOCUSED_ONCE', $null, 'Process')
            if (Test-Path -LiteralPath $full) { Remove-Item -LiteralPath $full -Recurse -Force }
        }
    }

    Test-Case 'measure_performance uses a rooted CARGO_TARGET_DIR and exits 2 when the rerun is invalid too' {
        $rooted = Join-Path $temporary 'perf-rooted-target'
        try {
            New-FakeTarget $rooted
            $env:HORDE_FAKE_UNFOCUSED_ALWAYS = '1'
            $env:CARGO_TARGET_DIR = $rooted
            $out = Join-Path $temporary 'perf-rooted'
            $r = Invoke-Measure @('-Scene', 'peak', '-VsyncOff', '-Pipelined', 'off', '-NoBuild', '-OutputDirectory', $out) 'measure-rooted'
            Assert-Equal 2 $r.ExitCode ("exit code; out: " + (Read-Log $r.Out) + " err: " + (Read-Log $r.Err))
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            Assert-Equal $rooted $summary.cargo_target_dir 'resolved target'
            Assert-Equal 'AutoNoVsync' $summary.present_mode 'requested present mode'
            Assert-Equal 2 @($summary.runs).Count 'one run per size'
            foreach ($run in @($summary.runs)) {
                Assert-Equal 2 $run.attempts "$($run.file) attempts"
                Assert-Equal 'AutoNoVsync' $run.present_mode "$($run.file) present mode echo"
                Assert-Equal $false $run.pipelined_rendering "$($run.file) pipelined echo"
                Assert-Equal 'unfocused_sample_frames 2, expected 0' (@($run.problems) -join '; ') "$($run.file) problems"
            }
        }
        finally { [Environment]::SetEnvironmentVariable('HORDE_FAKE_UNFOCUSED_ALWAYS', $null, 'Process') }
    }

    Test-Case 'measure_performance -Adapter exits 2 when the report names another adapter' {
        # Bevy never fails on an unmatched WGPU_ADAPTER_NAME; it renders on the default GPU, so the report must echo it.
        $adapterTarget = Join-Path $temporary 'perf-adapter-target'
        try {
            New-FakeTarget $adapterTarget
            $env:CARGO_TARGET_DIR = $adapterTarget
            $env:HORDE_FAKE_ADAPTER = 'NVIDIA GeForce RTX 4070 Laptop GPU'
            $out = Join-Path $temporary 'perf-adapter-other'
            $r = Invoke-Measure @('-Scene', 'peak', '-NoBuild', '-Adapter', 'Intel(R) Arc(TM) Graphics', '-OutputDirectory', $out) 'measure-adapter-other'
            Assert-Equal 2 $r.ExitCode ("another adapter: exit code; out: " + (Read-Log $r.Out) + " err: " + (Read-Log $r.Err))
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            Assert-Equal 2 @($summary.runs).Count 'one run per size'
            foreach ($run in @($summary.runs)) {
                Assert-Equal 2 $run.attempts "$($run.file) attempts"
                Assert-Equal 'NVIDIA GeForce RTX 4070 Laptop GPU' $run.adapter "$($run.file) adapter column"
                Assert-Equal "adapter 'NVIDIA GeForce RTX 4070 Laptop GPU', requested 'Intel(R) Arc(TM) Graphics'" (@($run.problems) -join '; ') "$($run.file) problems"
            }
            Assert-True ((Read-Log (Join-Path $out 'summary.md')) -match '\| NVIDIA GeForce RTX 4070 Laptop GPU \|') 'summary.md adapter column'

            # Bevy matches a lowercased substring of the adapter name.
            $env:HORDE_FAKE_ADAPTER = 'Intel(R) Arc(TM) Graphics'
            $out = Join-Path $temporary 'perf-adapter-same'
            $r = Invoke-Measure @('-Scene', 'peak', '-NoBuild', '-Adapter', 'intel(r) arc', '-OutputDirectory', $out) 'measure-adapter-same'
            Assert-Equal 0 $r.ExitCode ("matching adapter: exit code; out: " + (Read-Log $r.Out) + " err: " + (Read-Log $r.Err))
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            foreach ($run in @($summary.runs)) {
                Assert-Equal 1 $run.attempts "$($run.file) attempts"
                Assert-Equal 'Intel(R) Arc(TM) Graphics' $run.adapter "$($run.file) adapter column"
            }
        }
        finally { [Environment]::SetEnvironmentVariable('HORDE_FAKE_ADAPTER', $null, 'Process') }
    }

    Test-Case 'capture_review reports every sidecar check row and fails the run on a failed one' {
        $out = Join-Path $temporary 'captures'
        $review = Join-Path $scriptsDirectory 'capture_review.ps1'
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $review, '-NoBuild', '-Executable', $helper,
            '-Scenes', 'battle,construction,max-lanes,menu,broken', '-OutputDirectory', $out)
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $arguments -LogStem (Join-Path $temporary 'logs\capture-review') -TimeoutSeconds 300
        $log = 'out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err)
        Assert-Equal 1 $r.ExitCode "a failed sidecar check exits 1; $log"
        $report = Get-Content -Raw -LiteralPath (Join-Path $out 'captures.json') | ConvertFrom-Json
        Assert-Equal 10 @($report.captures).Count 'five scenes at two sizes'
        Assert-Equal 0 @($report.captures | Where-Object { $null -ne $_.error }).Count "no capture itself failed; $log"
        Assert-Equal 2.0 $report.windows_scale 'windows_scale from the PNG width'
        Assert-Equal 'scenes.tsv' $report.scene_list 'scenes.tsv kept'
        $rows = @($report.sidecar_checks | ForEach-Object { "$($_.check) $($_.size) $($_.scene) $($_.result)" } | Sort-Object)
        $expected = @(
            'construction_keeps_battle_arena large construction pass', 'construction_keeps_battle_arena small construction fail',
            'hud_rows_at_most_2 small battle pass', 'hud_rows_at_most_2 small construction pending', 'hud_rows_at_most_2 small max-lanes fail',
            'max_lanes_keeps_pre_zoom_arena large max-lanes pass', 'max_lanes_keeps_pre_zoom_arena small max-lanes fail',
            'sidecar_present large battle pass', 'sidecar_present large broken fail', 'sidecar_present large construction pass',
            'sidecar_present large max-lanes pass', 'sidecar_present large menu fail',
            'sidecar_present small battle pass', 'sidecar_present small broken fail', 'sidecar_present small construction pass',
            'sidecar_present small max-lanes pass', 'sidecar_present small menu fail',
            'sidecar_echoes_request large battle pass', 'sidecar_echoes_request large construction pass', 'sidecar_echoes_request large max-lanes pass',
            'sidecar_echoes_request small battle pass', 'sidecar_echoes_request small construction pass', 'sidecar_echoes_request small max-lanes pass'
        ) | Sort-Object
        Assert-Equal ($expected -join "`n") ($rows -join "`n") 'sidecar check rows'
        $details = @{}
        foreach ($row in @($report.sidecar_checks)) { $details["$($row.check) $($row.size) $($row.scene)"] = $row.detail }
        Assert-Equal 'no sidecar beside the PNG' $details['sidecar_present large menu'] 'missing sidecar detail'
        Assert-True ($details['sidecar_present large broken'] -like 'unreadable sidecar: *') "unreadable detail: $($details['sidecar_present large broken'])"
        Assert-Equal 'construction [11, 20, 30, 40], battle [10, 20, 30, 40]' $details['construction_keeps_battle_arena small construction'] 'arena detail'
        Assert-Equal 'arena [0, 0, 1440, 900], pre-zoom arena [24, 124, 1416, 816], 17x30' $details['max_lanes_keeps_pre_zoom_arena small max-lanes'] 'pre-zoom detail'
        $failLines = @((Read-Log $r.Out) -split "`r?`n" | Where-Object { $_ -like 'FAIL Sidecar check failed: *' })
        Assert-Equal 7 $failLines.Count "one FAIL line per failed row; $log"
    }

    Test-Case 'capture_review fails a sidecar that does not echo the requested reduced effects and UI scale' {
        # A still drawn without the LASTLIGHT_REVIEW_REDUCED or LASTLIGHT_REVIEW_SCALE override is not reduced-effects
        # or small-size evidence, even when its PNG looks plausible.
        $out = Join-Path $temporary 'captures-echo'
        $review = Join-Path $scriptsDirectory 'capture_review.ps1'
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $review, '-NoBuild', '-Executable', $helper,
            '-Scenes', 'battle,unscaled', '-Modes', 'reduced', '-OutputDirectory', $out)
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $arguments -LogStem (Join-Path $temporary 'logs\capture-review-echo') -TimeoutSeconds 300
        $log = 'out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err)
        Assert-Equal 1 $r.ExitCode "a sidecar that ignores the request exits 1; $log"
        $report = Get-Content -Raw -LiteralPath (Join-Path $out 'captures.json') | ConvertFrom-Json
        $rows = @($report.sidecar_checks | Where-Object { $_.check -eq 'sidecar_echoes_request' } | ForEach-Object { "$($_.mode) $($_.size) $($_.scene) $($_.result)" } | Sort-Object)
        $expected = @(
            'reduced large battle pass', 'reduced large unscaled fail', 'reduced small battle pass', 'reduced small unscaled fail'
        ) | Sort-Object
        Assert-Equal ($expected -join "`n") ($rows -join "`n") 'echo rows'
        $details = @{}
        foreach ($row in @($report.sidecar_checks)) { $details["$($row.check) $($row.size) $($row.scene)"] = $row.detail }
        Assert-Equal 'reduced_effects false (requested true), ui_scale 1 (requested 1)' $details['sidecar_echoes_request large unscaled'] 'large detail'
        Assert-Equal 'reduced_effects false (requested true), ui_scale 1 (requested 1.35)' $details['sidecar_echoes_request small unscaled'] 'small detail'
        Assert-Equal 'reduced_effects true (requested true), ui_scale 1.35 (requested 1.35)' $details['sidecar_echoes_request small battle'] 'pass detail'
        $failLines = @((Read-Log $r.Out) -split "`r?`n" | Where-Object { $_ -like 'FAIL Sidecar check failed: sidecar_echoes_request *' })
        Assert-Equal 2 $failLines.Count "one FAIL line per unechoed still; $log"
    }

    Test-Case 'capture_review and capture_motion resolve a relative -Executable against the workspace' {
        # Started from another directory, the path must still name the helper relative to the workspace,
        # as -OutputDirectory does, not the process directory.
        $base = New-Object Uri (([IO.Path]::GetFullPath($workspace)).TrimEnd('\') + '\')
        $relative = [Uri]::UnescapeDataString($base.MakeRelativeUri((New-Object Uri $helper)).ToString()).Replace('/', '\')
        Assert-True (-not [IO.Path]::IsPathRooted($relative)) "relative helper path: $relative"
        $elsewhere = Join-Path $temporary 'elsewhere'
        New-Item -ItemType Directory -Force -Path $elsewhere | Out-Null
        $review = Join-Path $scriptsDirectory 'capture_review.ps1'
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $review, '-NoBuild', '-Executable', $relative,
            '-Scenes', 'battle', '-OutputDirectory', (Join-Path $temporary 'relative-captures'))
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $arguments -LogStem (Join-Path $temporary 'logs\capture-review-relative') -TimeoutSeconds 300 -WorkingDirectory $elsewhere
        Assert-Equal 0 $r.ExitCode ('capture_review; out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err))
        $motion = Join-Path $scriptsDirectory 'capture_motion.ps1'
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $motion, '-NoBuild', '-Executable', $relative,
            '-Scene', 'no-such-scene', '-OutputDirectory', (Join-Path $temporary 'relative-motion'))
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $arguments -LogStem (Join-Path $temporary 'logs\capture-motion-relative') -TimeoutSeconds 300 -WorkingDirectory $elsewhere
        $log = (Read-Log $r.Out) + (Read-Log $r.Err)
        Assert-True ($log -match 'Unknown motion scene') "capture_motion reached the scene list; $log"
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
