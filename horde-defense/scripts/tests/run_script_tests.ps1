#requires -Version 5.1
# Self-tests for scripts/lib/Native.ps1, scripts/measure_performance.ps1, the capture_review.ps1 sidecar checks and the
# playtest package scripts (lib/Playtest.ps1, package_playtest.ps1, verify_playtest.ps1), with a fake game exe, under
# Windows PowerShell 5.1.
# Run: powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/tests/run_script_tests.ps1
# The runner is expected to pass with its own output redirected. Prints PASS or FAIL and exits 0 or 1.
# -Only <regex> runs just the cases whose name matches (for work on one script; a full run is the gate).
[CmdletBinding()]
param([string]$Only = '')
$ErrorActionPreference = 'Stop'
$scriptsDirectory = Split-Path -Parent $PSScriptRoot
$workspace = Split-Path -Parent $scriptsDirectory
$nativeLibrary = Join-Path $scriptsDirectory 'lib\Native.ps1'
if (-not (Test-Path -LiteralPath $nativeLibrary)) { throw "FAIL: missing $nativeLibrary" }
. $nativeLibrary
# The display hold and the Kernel-Power 506/507 check (R27). Each case dot-sources it into its own scope and shims the
# functions it calls.
$displayLibrary = Join-Path $scriptsDirectory 'lib\DisplayPower.ps1'
# The quiet window and the per-run load check (R30): the same pattern, with Get-CimInstance and Start-Sleep shimmed.
$machineLibrary = Join-Path $scriptsDirectory 'lib\MachineLoad.ps1'
# The interface_total A/B decision (R31), with fixture runs.
$abLibrary = Join-Path $scriptsDirectory 'lib\AbInterfaceTotal.ps1'

$script:failures = New-Object System.Collections.Generic.List[string]
$script:passed = 0

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Assert-Equal($Expected, $Actual, [string]$Message) {
    if (-not ($Expected -ceq $Actual)) { throw "$Message - expected [$Expected] got [$Actual]" }
}

function Test-Case([string]$Name, [scriptblock]$Body) {
    if ($Only -and $Name -notmatch $Only) { return }
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
        // 'mark <file> <s>' sleeps, then writes <file> as it exits, so a caller can tell a sample taken before the exit.
        if (args.Length == 3 && args[0] == "mark") {
            Thread.Sleep(int.Parse(args[2]) * 1000);
            File.WriteAllText(args[1], "exited");
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
        // HORDE_FAKE_VERSION is the --version line of a stamped build (package_playtest.ps1, verify_playtest.ps1).
        if (args.Length == 1 && args[0] == "--version") {
            string stamped = Environment.GetEnvironmentVariable("HORDE_FAKE_VERSION");
            Console.WriteLine(String.IsNullOrEmpty(stamped) ? "horde-defense fake" : stamped);
            return 0;
        }
        // The packaged smoke: HORDE_FAKE_SMOKE is its first stdout line, HORDE_FAKE_SMOKE_EXIT its exit code.
        if (args.Length == 1 && args[0] == "--smoke") {
            string smoke = Environment.GetEnvironmentVariable("HORDE_FAKE_SMOKE");
            Console.WriteLine(String.IsNullOrEmpty(smoke) ? "PASS: fake smoke" : smoke);
            Console.Error.WriteLine("smoke diagnostics on stderr");
            string smokeExit = Environment.GetEnvironmentVariable("HORDE_FAKE_SMOKE_EXIT");
            return String.IsNullOrEmpty(smokeExit) ? 0 : int.Parse(smokeExit);
        }
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
            Console.WriteLine("rescaled\tStill\tfalse\tfalse\tfalse\t\t\t1");
            Console.WriteLine("clamped\tStill\tfalse\tfalse\tfalse\t\t\t1");
            return 0;
        }
        // A capture_review still: a PNG header at twice the requested size (Windows scale 2) plus a sidecar per scene.
        // battle passes every check, construction moves the arena at small size, max-lanes misses the pre-zoom arena
        // at small size and has 3 HUD rows, menu writes no sidecar, broken writes one that is not JSON, unscaled
        // ignores the requested reduced effects and UI scale, and rescaled ignores only the UI scale. clamped echoes
        // the request but its window did not open at the requested size: the OS clamped the large height to 850, and
        // the small size fell back to launch.rs DEFAULT_WINDOW 1440x900.
        string capture = Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_CAPTURE");
        if (args.Length == 0 && !String.IsNullOrEmpty(capture)) {
            string scene = Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCREEN");
            string[] requested = Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SIZE").Split('x');
            int width = int.Parse(requested[0]);
            int height = int.Parse(requested[1]);
            bool small = width == 1050;
            if (scene == "clamped") {
                if (small) { width = 1440; height = 900; } else { height = 850; }
            }
            byte[] png = new byte[24];
            byte[] head = new byte[] { 137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82 };
            Array.Copy(head, png, head.Length);
            int pixels = width * 2;
            png[16] = (byte)(pixels >> 24); png[17] = (byte)(pixels >> 16); png[18] = (byte)(pixels >> 8); png[19] = (byte)pixels;
            int rows = height * 2;
            png[20] = (byte)(rows >> 24); png[21] = (byte)(rows >> 16); png[22] = (byte)(rows >> 8); png[23] = (byte)rows;
            Directory.CreateDirectory(Path.GetDirectoryName(capture));
            File.WriteAllBytes(capture, png);
            // Every sidecar echoes the requested reduced-effects mode and UI scale, except unscaled, which always
            // reports normal effects at scale 1 (the review overrides did not take), and rescaled, which echoes the
            // reduced-effects mode but always reports scale 1 (only LASTLIGHT_REVIEW_SCALE did not take).
            string reducedEcho = "\"reduced_effects\":" + (Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_REDUCED") == "1" ? "true" : "false");
            string echo = reducedEcho + ",\"ui_scale\":" + Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCALE") + ",";
            string sidecar = null;
            if (scene == "battle") { sidecar = "{" + echo + "\"screen\":\"Battle\",\"hud_rows\":2,\"arena_rect\":[10,20,30,40]}"; }
            if (scene == "construction") { sidecar = "{" + echo + "\"screen\":\"Battle\",\"hud_rows\":null,\"arena_rect\":" + (small ? "[11,20,30,40]" : "[10,20,30,40]") + "}"; }
            if (scene == "max-lanes") {
                sidecar = "{" + echo + "\"screen\":\"Battle\",\"hud_rows\":3,\"lanes\":17,\"width_yards\":30,\"canvas_rect\":[0,0,1440,900],\"arena_rect\":"
                    + (small ? "[0,0,1440,900]" : "[24,124,1416,816]") + "}";
            }
            if (scene == "unscaled") { sidecar = "{\"reduced_effects\":false,\"ui_scale\":1.0,\"screen\":\"Menu\"}"; }
            if (scene == "rescaled") { sidecar = "{" + reducedEcho + ",\"ui_scale\":1.0,\"screen\":\"Menu\"}"; }
            if (scene == "clamped") { sidecar = "{" + echo + "\"screen\":\"Menu\"}"; }
            if (scene == "broken") { sidecar = "{not json"; }
            if (sidecar != null) { File.WriteAllText(Path.ChangeExtension(capture, ".json"), sidecar); }
            return 0;
        }
        string report = Environment.GetEnvironmentVariable("LASTLIGHT_PERF_REPORT");
        if (args.Length == 0 && !String.IsNullOrEmpty(report)) {
            // HORDE_FAKE_EXIT makes the run fail with that exit code before it writes a report.
            string failWith = Environment.GetEnvironmentVariable("HORDE_FAKE_EXIT");
            if (!String.IsNullOrEmpty(failWith)) { return int.Parse(failWith); }
            int unfocused = 0;
            string once = Environment.GetEnvironmentVariable("HORDE_FAKE_UNFOCUSED_ONCE");
            if (!String.IsNullOrEmpty(once) && File.Exists(once)) { File.Delete(once); unfocused = 3; }
            if (Environment.GetEnvironmentVariable("HORDE_FAKE_UNFOCUSED_ALWAYS") == "1") { unfocused = 2; }
            // HORDE_FAKE_UNFOCUSED_SCENE makes every run of that scene unfocused, so only its groups are invalid.
            string screen = Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCREEN");
            if (Environment.GetEnvironmentVariable("HORDE_FAKE_UNFOCUSED_SCENE") == screen) { unfocused = 2; }
            string mode = Environment.GetEnvironmentVariable("LASTLIGHT_PERF_PRESENT_MODE") == "autonovsync" ? "AutoNoVsync" : "Fifo";
            string pipelined = Environment.GetEnvironmentVariable("LASTLIGHT_PERF_PIPELINED") == "0" ? "false" : "true";
            // The adapter the renderer used: Bevy falls back to the default GPU when WGPU_ADAPTER_NAME matches none.
            // Without HORDE_FAKE_ADAPTER, a WGPU_ADAPTER_NAME the fake inherits steers it, as Bevy honours any it sees.
            string adapter = Environment.GetEnvironmentVariable("HORDE_FAKE_ADAPTER");
            string steered = Environment.GetEnvironmentVariable("WGPU_ADAPTER_NAME");
            if (String.IsNullOrEmpty(adapter) && !String.IsNullOrEmpty(steered)) { adapter = "Fake Adapter steered to " + steered; }
            if (String.IsNullOrEmpty(adapter)) { adapter = "Fake Adapter"; }
            // The window opens at the requested size at Windows scale 2, unless HORDE_FAKE_WINDOW names the physical
            // WIDTHxHEIGHT the OS clamped it to.
            string[] size = Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SIZE").Split('x');
            string window = Environment.GetEnvironmentVariable("HORDE_FAKE_WINDOW");
            string[] physical = String.IsNullOrEmpty(window)
                ? new string[] { (int.Parse(size[0]) * 2).ToString(), (int.Parse(size[1]) * 2).ToString() }
                : window.Split('x');
            // HORDE_FAKE_INTERFACE_TOTAL ("exe-a=2.0;exe-b=2.1") sets the interface_total mean by the folder above the
            // exe's debug folder, so the two A/B arms (<out>\exe-a\debug, <out>\exe-b\debug) report their own; else 1.0.
            // HORDE_FAKE_CPU_P99 ("progressed-peak/small=12.7565") sets the cpu_frame p99 of one scene and size; else 5.0.
            string sizeName = size[0] == "1050" ? "small" : "normal";
            string cpuP99 = "5.0";
            string p99s = Environment.GetEnvironmentVariable("HORDE_FAKE_CPU_P99");
            if (!String.IsNullOrEmpty(p99s)) {
                foreach (string entry in p99s.Split(';')) {
                    string[] pair = entry.Split('=');
                    if (pair.Length == 2 && pair[0] == screen + "/" + sizeName) { cpuP99 = pair[1]; }
                }
            }
            string interfaceTotal = "1.0";
            string totals = Environment.GetEnvironmentVariable("HORDE_FAKE_INTERFACE_TOTAL");
            if (!String.IsNullOrEmpty(totals)) {
                string arm = Path.GetFileName(Path.GetDirectoryName(Path.GetDirectoryName(Process.GetCurrentProcess().MainModule.FileName)));
                foreach (string entry in totals.Split(';')) {
                    string[] pair = entry.Split('=');
                    if (pair.Length == 2 && pair[0] == arm) { interfaceTotal = pair[1]; }
                }
            }
            File.WriteAllText(report, "{\"version\":3,\"scene\":\"" + Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCREEN")
                + "\",\"width_physical\":" + physical[0] + ",\"height_physical\":" + physical[1]
                + ",\"windows_scale\":2.0,\"ui_scale\":" + Environment.GetEnvironmentVariable("LASTLIGHT_REVIEW_SCALE")
                + ",\"adapter\":\"" + adapter
                + "\",\"sample_frames\":" + Environment.GetEnvironmentVariable("LASTLIGHT_PERF_FRAMES")
                + ",\"completed_sample_target\":true,\"paused\":false,\"unfocused_sample_frames\":" + unfocused
                + ",\"frames_over_50ms\":0,\"frames_over_50ms_driver_stall\":0,\"driver_stalls\":{\"frames\":[],\"outside\":{\"frames_over_50ms\":0}}"
                + ",\"work_mean_ms\":[0.4,0.0,1.0]"
                + ",\"phases\":{\"cpu_frame_ms\":{\"mean\":4.0,\"p50\":4.0,\"p95\":4.5,\"p99\":" + cpuP99 + ",\"max\":6.0},\"interface_total_ms\":{\"mean\":" + interfaceTotal + ",\"p50\":1.0,\"p95\":1.2,\"p99\":1.3,\"max\":1.5}}"
                + ",\"egui\":{\"texture_uploads\":{\"count\":0,\"bytes\":0,\"frames\":0}"
                + ",\"battlefield_primitives\":{\"mean\":19.0,\"max\":19},\"battlefield_vertices\":{\"mean\":30000.0,\"max\":32000}}"
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

    Test-Case 'Invoke-Native reads the exit code of a child that exited before Invoke-Native looked at it' {
        # Guards against a return to Start-Process -PassThru (ruling R24, plan 2.4). Invoke-Native starts the child with
        # [Diagnostics.Process]::Start, so these runs never reach this shim. If it went back to Start-Process, this shim would
        # return late, standing in for a busy machine: the child has exited, and its creation handle has been closed,
        # before Invoke-Native gets the process object. In 5.1 ExitCode then reads $null (fix 381e563), and the asserts
        # below fail. The exit code must always be read.
        function Start-Process {
            [CmdletBinding()]
            param($FilePath, $ArgumentList, $WorkingDirectory, [switch]$NoNewWindow, [switch]$PassThru, [switch]$Wait,
                $RedirectStandardOutput, $RedirectStandardError)
            $process = Microsoft.PowerShell.Management\Start-Process @PSBoundParameters
            Start-Sleep -Milliseconds 500
            return $process
        }
        $r = Invoke-Native -FilePath (Resolve-Tool 'cmd') -Arguments @('/c', 'exit 3') -LogStem (Join-Path $temporary 'logs\late-exit3') -TimeoutSeconds 30
        Assert-Equal 3 $r.ExitCode 'late exit code 3'
        $r = Invoke-Native -FilePath $helper -Arguments @('echo', 'late') -LogStem (Join-Path $temporary 'logs\late-echo') -TimeoutSeconds 30
        Assert-Equal 0 $r.ExitCode 'late exit code 0'
        Assert-Equal '[late]' (Read-Log $r.Out).Trim() 'late stdout log'
    }

    Test-Case 'Invoke-Native reads the exit code of every instant-exit run' {
        for ($i = 0; $i -lt 150; $i++) {
            $r = Invoke-Native -FilePath $helper -Arguments @('echo') -LogStem (Join-Path $temporary 'logs\instant-echo') -TimeoutSeconds 30
            Assert-Equal 0 $r.ExitCode "echo run $i exit code"
            $r = Invoke-Native -FilePath (Resolve-Tool 'cmd') -Arguments @('/c', 'exit 3') -LogStem (Join-Path $temporary 'logs\instant-exit3') -TimeoutSeconds 30
            Assert-Equal 3 $r.ExitCode "exit 3 run $i exit code"
        }
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

    Test-Case 'Get-MotionFlashProblem requires the flash fields, zeros in reduced effects and a storm pulse' {
        $frame = { param($alpha, $sky, $muzzle) [pscustomobject]@{ file = 'frame.png'; flash_alpha_max = $alpha; sky_flash = $sky; muzzle_flash_count = $muzzle } }
        $dark = @((& $frame 0.0 0.0 0), (& $frame 0.0 0.0 0))
        Assert-True ($null -eq (Get-MotionFlashProblem -Frames $dark)) 'a calm normal export passes'
        Assert-True ($null -eq (Get-MotionFlashProblem -Frames $dark -ReducedEffects)) 'a reduced export of zeros passes'
        Assert-True ($null -eq (Get-MotionFlashProblem -Frames $dark -ReducedEffects -Storm)) 'a reduced storm export of zeros passes'
        # The manifest as the game writes it: JSON numbers, parsed by ConvertFrom-Json.
        $parsed = @(('[{"flash_alpha_max":0.9,"sky_flash":0.0,"muzzle_flash_count":2},{"flash_alpha_max":0.14,"sky_flash":0.14,"muzzle_flash_count":0}]' | ConvertFrom-Json))
        Assert-True ($null -eq (Get-MotionFlashProblem -Frames $parsed -Storm)) 'a normal storm export with a pulse passes'
        Assert-True ((Get-MotionFlashProblem -Frames $parsed -ReducedEffects) -like 'Reduced effects must not flash: frame 0 has flash_alpha_max*') 'a reduced export that flashes fails'
        $muzzle = @((& $frame 0.0 0.0 1))
        Assert-True ((Get-MotionFlashProblem -Frames $muzzle -ReducedEffects) -like '*muzzle_flash_count 1') 'a reduced muzzle flash fails'
        $sky = @((& $frame 0.0 0.01 0))
        Assert-True ((Get-MotionFlashProblem -Frames $sky -ReducedEffects) -like '*sky_flash 0.01') 'a reduced sky flash fails'
        Assert-True ((Get-MotionFlashProblem -Frames $dark -Storm) -like 'The storm export captured no lightning pulse*') 'a storm export without a pulse fails'
        $jump = @((& $frame 0.0 0.0 0), (& $frame 0.25 0.25 0))
        Assert-True ((Get-MotionFlashProblem -Frames $jump -Storm) -like 'sky_flash moves by 0.25 between frames 0 and 1*') 'a sky flash step of 0.2 or more fails'
        $old = @([pscustomobject]@{ file = 'frame.png'; native_frame = 60 })
        Assert-Equal 'Frame 0 has no numeric flash_alpha_max' (Get-MotionFlashProblem -Frames $old) 'a manifest without the fields fails'
        $text = @((& $frame 0.0 '0' 0))
        Assert-Equal 'Frame 0 has no numeric sky_flash' (Get-MotionFlashProblem -Frames $text) 'a text field fails'
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

    Test-Case 'Enter-DisplayHold requests the display and Exit-DisplayHold releases it once (R27)' {
        . $displayLibrary
        $script:executionStates = New-Object System.Collections.Generic.List[string]
        function Set-ThreadExecutionState([uint32]$Flags) {
            $script:executionStates.Add('0x' + $Flags.ToString('X8'))
            if ($Flags -eq [uint32]2147483648) { return [uint32]2147483651 }
            return [uint32]2147483648
        }
        $hold = Enter-DisplayHold
        Assert-True $hold.Held 'the hold is held'
        Assert-Equal 'ES_CONTINUOUS|ES_SYSTEM_REQUIRED|ES_DISPLAY_REQUIRED' $hold.Request 'request'
        Assert-Equal '0x80000003' ($script:executionStates -join ',') 'ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED'
        Assert-True (-not $hold.Released) 'released before Exit-DisplayHold'
        Exit-DisplayHold $hold
        Exit-DisplayHold $hold
        Assert-Equal '0x80000003,0x80000000' ($script:executionStates -join ',') 'one release with ES_CONTINUOUS alone'
        Assert-True $hold.Released 'released'
        Assert-Equal '0x80000003' $hold.ReleasedFrom 'the release replaced the held state'
        function Set-ThreadExecutionState([uint32]$Flags) { return [uint32]0 }
        $threw = $null
        try { Enter-DisplayHold | Out-Null } catch { $threw = $_.Exception.Message }
        Assert-True ($threw -match 'refused') "a refused request did not throw: $threw"
    }

    Test-Case 'Get-DisplayPowerEvent queries Kernel-Power 506/507 in the run window and reads no match as none (R27)' {
        . $displayLibrary
        $script:eventFilters = New-Object System.Collections.Generic.List[object]
        $script:eventMode = 'two'
        function Get-WinEvent {
            [CmdletBinding()]
            param([hashtable]$FilterHashtable)
            $script:eventFilters.Add($FilterHashtable)
            if ($script:eventMode -eq 'two') {
                $at = $FilterHashtable.StartTime
                return @(
                    [pscustomobject]@{ Id = 507; TimeCreated = $at.AddSeconds(20) },
                    [pscustomobject]@{ Id = 506; TimeCreated = $at.AddSeconds(5) }
                )
            }
            if ($script:eventMode -eq 'none') {
                Write-Error -Message 'No events were found that match the specified selection criteria.' -ErrorId 'NoMatchingEventsFound' -Category ObjectNotFound
                return
            }
            Write-Error -Message 'The specified channel could not be found.' -ErrorId 'NoChannel' -Category ObjectNotFound
        }
        $start = New-Object DateTime 2026, 11, 9, 10, 0, 0, ([DateTimeKind]::Utc)
        $end = $start.AddMinutes(1)
        $events = @(Get-DisplayPowerEvent -Start $start -End $end)
        $filter = $script:eventFilters[0]
        Assert-Equal 'System' $filter.LogName 'log'
        Assert-Equal 'Microsoft-Windows-Kernel-Power' $filter.ProviderName 'provider'
        Assert-Equal '506,507' (@($filter.Id) -join ',') 'event ids'
        Assert-True ($filter.StartTime -eq $start -and $filter.EndTime -eq $end) 'the run window'
        Assert-Equal '506 2026-11-09T10:00:05.000Z,507 2026-11-09T10:00:20.000Z' (@($events | ForEach-Object { "$($_.Id) $($_.TimeUtc)" }) -join ',') 'oldest first, UTC'
        $script:eventMode = 'none'
        Assert-Equal 0 @(Get-DisplayPowerEvent -Start $start -End $end).Count 'no match is no event'
        $script:eventMode = 'broken'
        $threw = $null
        try { Get-DisplayPowerEvent -Start $start -End $end | Out-Null } catch { $threw = $_.Exception.Message }
        Assert-True ($threw -match 'channel') "an unreadable log did not throw: $threw"
    }

    Test-Case 'Test-DisplayPower makes a run with a 506/507 event or an unreadable log invalid and keeps one without valid (R27)' {
        . $displayLibrary
        $script:displayEvents = @()
        function Get-DisplayPowerEvent([DateTime]$Start, [DateTime]$End) {
            if ($script:displayEvents -eq 'throw') { throw 'access denied' }
            $script:displayEvents
        }
        $start = [DateTime]::Now
        $quiet = Test-DisplayPower -Start $start -End $start.AddSeconds(30)
        Assert-Equal 0 @($quiet.Problems).Count 'a run without an event is valid'
        Assert-Equal 0 @($quiet.Events).Count 'no events'
        $script:displayEvents = @(
            [pscustomobject]@{ Id = 506; TimeUtc = '2026-11-09T10:00:05.000Z' },
            [pscustomobject]@{ Id = 507; TimeUtc = '2026-11-09T10:00:20.000Z' }
        )
        $dark = Test-DisplayPower -Start $start -End $start.AddSeconds(30)
        Assert-Equal 'display off during the run (Kernel-Power 506 at 2026-11-09T10:00:05.000Z, 507 at 2026-11-09T10:00:20.000Z)' (@($dark.Problems) -join '; ') 'overlap problem'
        Assert-Equal 2 @($dark.Events).Count 'both events kept'
        $script:displayEvents = 'throw'
        $unknown = Test-DisplayPower -Start $start -End $start.AddSeconds(30)
        Assert-Equal 'display power unknown (Kernel-Power 506/507 unreadable: access denied)' (@($unknown.Problems) -join '; ') 'unreadable log'
    }

    # R30: the process and load queries are Get-CimInstance calls, shimmed here; the quiet window's waits are Start-Sleep.
    Test-Case 'Get-ForeignBuildProcess finds cargo, rustc and cargo-nextest outside the pass''s own process tree (R30)' {
        . $machineLibrary
        $script:cimQueries = New-Object System.Collections.Generic.List[string]
        function Get-CimInstance {
            [CmdletBinding()]
            param([Parameter(Position = 0)][string]$ClassName, [string[]]$Property)
            $script:cimQueries.Add("$ClassName|$(@($Property) -join ',')")
            if ($ClassName -ne 'Win32_Process') { throw "unexpected class $ClassName" }
            @(
                [pscustomobject]@{ ProcessId = 100; ParentProcessId = 4; Name = 'explorer.exe' },
                [pscustomobject]@{ ProcessId = 300; ParentProcessId = 4; Name = 'cargo-nextest.exe' },
                [pscustomobject]@{ ProcessId = 200; ParentProcessId = 4; Name = 'CARGO.EXE' },
                [pscustomobject]@{ ProcessId = 201; ParentProcessId = 200; Name = 'rustc.exe' },
                [pscustomobject]@{ ProcessId = 400; ParentProcessId = 4; Name = 'powershell.exe' },
                [pscustomobject]@{ ProcessId = 401; ParentProcessId = 400; Name = 'cargo.exe' },
                [pscustomobject]@{ ProcessId = 402; ParentProcessId = 401; Name = 'rustc.exe' },
                [pscustomobject]@{ ProcessId = 403; ParentProcessId = 402; Name = 'link.exe' },
                [pscustomobject]@{ ProcessId = 500; ParentProcessId = 4; Name = 'Code.exe' }
            )
        }
        $found = @(Get-ForeignBuildProcess -RootId 400)
        Assert-Equal 'CARGO.EXE 200,rustc.exe 201,cargo-nextest.exe 300' (@($found | ForEach-Object { "$($_.Name) $($_.ProcessId)" }) -join ',') 'foreign build processes, by id; the pass''s own tree (400-403) is ignored'
        Assert-Equal 'Win32_Process|ProcessId,ParentProcessId,Name' $script:cimQueries[0] 'one process table query'
        Assert-Equal 'CARGO.EXE (200), rustc.exe (201), cargo-nextest.exe (300)' (Format-BuildProcess $found) 'formatted'
        Assert-Equal 0 @(Get-ForeignBuildProcess -RootId 4).Count 'every process below the root is its own'
    }

    Test-Case 'Get-CpuLoadPercent averages LoadPercentage and reads a failed query as unknown (R30)' {
        . $machineLibrary
        $script:cimQueries = New-Object System.Collections.Generic.List[string]
        $script:loadMode = 'two'
        function Get-CimInstance {
            [CmdletBinding()]
            param([Parameter(Position = 0)][string]$ClassName, [string[]]$Property)
            $script:cimQueries.Add("$ClassName|$(@($Property) -join ',')")
            if ($script:loadMode -eq 'two') { return @([pscustomobject]@{ LoadPercentage = 30 }, [pscustomobject]@{ LoadPercentage = 41 }) }
            if ($script:loadMode -eq 'null') { return [pscustomobject]@{ LoadPercentage = $null } }
            Write-Error -Message 'Invalid class' -Category InvalidOperation
        }
        Assert-Equal 35.5 (Get-CpuLoadPercent) 'the mean over both processors'
        Assert-Equal 'Win32_Processor|LoadPercentage' $script:cimQueries[0] 'the load query'
        $script:loadMode = 'null'
        Assert-True ($null -eq (Get-CpuLoadPercent)) 'no LoadPercentage is unknown'
        $script:loadMode = 'broken'
        Assert-True ($null -eq (Get-CpuLoadPercent)) 'a failed query is unknown'
    }

    # Wait-QuietMachine over scripted samples: each sample takes the next load and the next process list. The clock
    # (Get-MachineLoadClock) is scripted too: it moves only when a shimmed Start-Sleep or a slow query moves it.
    function Set-QuietScript([object[]]$Loads, [object[]]$Builds = @()) {
        $script:quietLoads = New-Object System.Collections.Generic.Queue[object]
        foreach ($load in $Loads) { $script:quietLoads.Enqueue($load) }
        $script:quietBuilds = New-Object System.Collections.Generic.Queue[object]
        foreach ($build in $Builds) { $script:quietBuilds.Enqueue($build) }
        $script:quietSleeps = New-Object System.Collections.Generic.List[int]
        $script:quietLog = New-Object System.Collections.Generic.List[string]
        $script:quietClock = New-Object DateTime 2026, 11, 11, 16, 35, 14, ([DateTimeKind]::Utc)
    }
    $quietShims = {
        function Get-MachineLoadClock { $script:quietClock }
        function Start-Sleep { param([int]$Seconds) $script:quietSleeps.Add($Seconds); $script:quietClock = $script:quietClock.AddSeconds($Seconds) }
        function Get-CpuLoadPercent { if ($script:quietLoads.Count -gt 1) { $script:quietLoads.Dequeue() } else { $script:quietLoads.Peek() } }
        function Get-ForeignBuildProcess([int]$RootId) {
            $next = 'none'
            if ($script:quietBuilds.Count -gt 0) { $next = $script:quietBuilds.Dequeue() }
            if ($next -eq 'cargo') {
                @([pscustomobject]@{ ProcessId = 4242; ParentProcessId = 4; Name = 'cargo.exe' }, [pscustomobject]@{ ProcessId = 4243; ParentProcessId = 4242; Name = 'rustc.exe' })
            }
        }
    }

    Test-Case 'Wait-QuietMachine starts once the mean load over 60 s of 5 s samples is under 35% (R30: a quiet start)' {
        . $machineLibrary
        . $quietShims
        # One sample above 35% does not hold the start: the rule is the mean over the window.
        Set-QuietScript (@(43.0) + @(30.0) * 12)
        $wait = Wait-QuietMachine -TimeoutMinutes 240 -OnLog { param($Line) $script:quietLog.Add($Line) }
        Assert-True $wait.Quiet 'quiet'
        Assert-True (-not $wait.TimedOut) 'not timed out'
        Assert-Equal 13 $wait.Samples '13 samples span 60 s'
        Assert-Equal '5,5,5,5,5,5,5,5,5,5,5,5' ($script:quietSleeps -join ',') '12 waits of 5 s'
        Assert-Equal 31.0 $wait.MeanPercent 'mean over the window'
        Assert-Equal 43.0 $wait.MaxPercent 'max over the window'
        Assert-Equal 0 @($wait.WaitedFor).Count 'it waited for nothing outside its own window'
        Assert-Equal 'quiet window: sampling the CPU load for 60 s|quiet window: quiet after 13 samples (60 s): mean CPU load 31% over 60 s (under 35%), max 43%, no foreign build process' ($script:quietLog -join '|') 'log'
        Assert-Equal ($script:quietLog -join '|') (@($wait.Log) -join '|') 'the result keeps the log'
    }

    Test-Case 'Wait-QuietMachine waits out a foreign build and a loaded CPU, then starts (R30: waiting then starting)' {
        . $machineLibrary
        . $quietShims
        # Samples 1-3 see a foreign cargo build; 4-16 fill the window at 60%; from 17 on the load is 10%, and the mean of
        # the last 13 falls under 35% at the 7th of them (sample 23).
        Set-QuietScript (@(5.0) * 3 + @(60.0) * 13 + @(10.0)) @('cargo', 'cargo', 'cargo')
        $wait = Wait-QuietMachine -TimeoutMinutes 240 -OnLog { param($Line) $script:quietLog.Add($Line) }
        Assert-True $wait.Quiet 'quiet'
        Assert-Equal 23 $wait.Samples 'samples'
        Assert-Equal 22 $script:quietSleeps.Count 'waits'
        Assert-Equal 'build processes: cargo.exe, rustc.exe|mean CPU load' (@($wait.WaitedFor) -join '|') 'what it waited for'
        $expected = @(
            'quiet window: waiting for build processes: cargo.exe (4242), rustc.exe (4243)',
            'quiet window: sampling the CPU load for 60 s',
            'quiet window: waiting for the mean CPU load to fall under 35% (60% over 60 s)',
            'quiet window: quiet after 23 samples (110 s): mean CPU load 33.0769% over 60 s (under 35%), max 60%, no foreign build process'
        )
        Assert-Equal ($expected -join '|') ($script:quietLog -join '|') 'log'
    }

    Test-Case 'Wait-QuietMachine times out after QuietTimeoutMinutes and says what it waited for (R30: a timeout)' {
        . $machineLibrary
        . $quietShims
        Set-QuietScript @(50.0)
        $wait = Wait-QuietMachine -TimeoutMinutes 1 -OnLog { param($Line) $script:quietLog.Add($Line) }
        Assert-True (-not $wait.Quiet) 'not quiet'
        Assert-True $wait.TimedOut 'timed out'
        Assert-Equal 13 $wait.Samples 'a 1-minute timeout is 13 samples'
        Assert-Equal 12 $script:quietSleeps.Count 'waits'
        Assert-Equal 'mean CPU load' (@($wait.WaitedFor) -join '|') 'what it waited for'
        Assert-Equal 60 $wait.Seconds 'it waited 60 s by the clock'
        Assert-Equal 'quiet window: timed out after 1 min (13 samples, 60 s): still waiting for the mean CPU load to fall under 35% (50% over 60 s)' $script:quietLog[$script:quietLog.Count - 1] 'last log line'
        Set-QuietScript @($null)
        $wait = Wait-QuietMachine -TimeoutMinutes 1
        Assert-True $wait.TimedOut 'an unreadable load never counts as quiet'
        Assert-Equal 'readable CPU load' (@($wait.WaitedFor) -join '|') 'unreadable load'
    }

    Test-Case 'Wait-QuietMachine times out by the clock when the system queries are slow (R30: QuietTimeoutMinutes is wall time)' {
        # The W2-close passes waited 5 h 44 min and 6 h 02 min on a 240-minute timeout: the limit was a sample count and
        # each sample also paid two slow Get-CimInstance queries. Here each query takes 20 s, so a sample costs 40 s
        # beside its 5 s sleep: sample 1 ends at 40 s, sample 2 at 85 s, and the 1-minute deadline stops the wait there.
        . $machineLibrary
        Set-QuietScript @()
        function Get-MachineLoadClock { $script:quietClock }
        function Start-Sleep { param([int]$Seconds) $script:quietSleeps.Add($Seconds); $script:quietClock = $script:quietClock.AddSeconds($Seconds) }
        function Get-CimInstance {
            [CmdletBinding()]
            param([Parameter(Position = 0)][string]$ClassName, [string[]]$Property)
            $script:quietClock = $script:quietClock.AddSeconds(20)
            if ($ClassName -eq 'Win32_Process') { return [pscustomobject]@{ ProcessId = 4; ParentProcessId = 0; Name = 'System' } }
            return [pscustomobject]@{ LoadPercentage = 10 }
        }
        $wait = Wait-QuietMachine -TimeoutMinutes 1 -RootId 99999 -OnLog { param($Line) $script:quietLog.Add($Line) }
        Assert-True $wait.TimedOut 'timed out'
        Assert-Equal 2 $wait.Samples 'the deadline, not 13 samples, ends the wait'
        Assert-Equal 1 $script:quietSleeps.Count 'one 5 s wait'
        Assert-Equal 85 $wait.Seconds 'the elapsed wall time, not the nominal 5 s steps'
        Assert-Equal 'quiet window: timed out after 1 min (2 samples, 85 s): still waiting for 13 load samples (2 so far)' $script:quietLog[$script:quietLog.Count - 1] 'last log line'
    }

    Test-Case 'Test-RunLoad makes a run with a load sample of 90% or more invalid (R30)' {
        . $machineLibrary
        $calm = Test-RunLoad -Samples @(12.0, 40.0, 89.0)
        Assert-Equal 0 @($calm.Problems).Count 'under 90% is valid'
        Assert-Equal 3 $calm.Samples 'samples'
        Assert-Equal 47.0 $calm.Mean 'mean'
        Assert-Equal 89.0 $calm.Max 'max'
        $loaded = Test-RunLoad -Samples @(12.0, 90.0)
        Assert-Equal 'CPU load 90% during the run (a sample at 90% or more is invalid evidence; samples 12, 90)' (@($loaded.Problems) -join '; ') '90% is invalid'
        $unknown = Test-RunLoad -Samples @(12.0, $null)
        Assert-Equal 'CPU load unknown in 1 of 2 samples' (@($unknown.Problems) -join '; ') 'an unreadable sample'
        $none = Test-RunLoad -Samples @()
        Assert-Equal 'CPU load unknown (no sample)' (@($none.Problems) -join '; ') 'no sample'
        Assert-True ($null -eq $none.Mean -and $null -eq $none.Max) 'no mean or max without samples'
    }

    # The -Sample cases assert the sampling structure, not the wall clock: machine load stretches process start-up and the
    # child's lifetime, so no bound below depends on how long a run took on this machine. The 'mark' helper writes its
    # marker only as it exits, so a sample that sees no marker was taken while the child still ran. The sample counts are
    # bounded below by the structure and above by twice the wait slices of the run's own measured length (or its timeout),
    # which load can only lengthen.
    Test-Case 'Invoke-Native -Sample samples once right after the start, before the first wait (R30)' {
        $script:sampleMarker = Join-Path $temporary 'sampled-start.exited'
        $script:sampleSeen = New-Object System.Collections.Generic.List[bool]
        # A 600 s interval: the first wait outlasts the child, so every sample but the first would come after the exit.
        $r = Invoke-Native -FilePath $helper -Arguments @('mark', $script:sampleMarker, '2') -LogStem (Join-Path $temporary 'logs\sampled-start') -TimeoutSeconds 30 -Sample { $script:sampleSeen.Add((Test-Path -LiteralPath $script:sampleMarker)) } -SampleMilliseconds 600000
        Assert-Equal 0 $r.ExitCode 'exit code'
        Assert-True (Test-Path -LiteralPath $script:sampleMarker) 'the helper marks its exit'
        Assert-Equal 'False' ($script:sampleSeen -join ',') 'exactly one sample, taken before the child exited'
    }

    Test-Case 'Invoke-Native -Sample samples again every interval until the exit (R30)' {
        $script:sampleMarker = Join-Path $temporary 'sampled.exited'
        $script:sampleSeen = New-Object System.Collections.Generic.List[bool]
        $r = Invoke-Native -FilePath $helper -Arguments @('mark', $script:sampleMarker, '2') -LogStem (Join-Path $temporary 'logs\sampled') -TimeoutSeconds 30 -Sample { $script:sampleSeen.Add((Test-Path -LiteralPath $script:sampleMarker)) } -SampleMilliseconds 500
        Assert-Equal 0 $r.ExitCode 'exit code'
        Assert-True (-not $script:sampleSeen[0]) "the first sample comes before the exit: $($script:sampleSeen -join ', ')"
        # The child sleeps 2 s after its own start-up, so it outlives at least two 500 ms intervals.
        Assert-True ($script:sampleSeen.Count -ge 2) "a child that outlives two intervals is sampled again: $($script:sampleSeen.Count)"
        $most = 2 * (1 + [Math]::Ceiling($r.Seconds / 0.5))
        Assert-True ($script:sampleSeen.Count -le $most) "one sample per 500 ms wait slice of the $($r.Seconds) s run (at most $most): $($script:sampleSeen.Count)"
    }

    Test-Case 'Invoke-Native -Sample keeps sampling up to the timeout, and a sampled run still times out (R30)' {
        $script:sampleSeen = New-Object System.Collections.Generic.List[bool]
        $message = $null
        try {
            Invoke-Native -FilePath $helper -Arguments @('sleep', '30') -LogStem (Join-Path $temporary 'logs\sampled-hung') -TimeoutSeconds 1 -Sample { $script:sampleSeen.Add($false) } -SampleMilliseconds 400 | Out-Null
        }
        catch { $message = $_.Exception.Message }
        Assert-True ($message -match 'Timed out after 1 s') "a sampled run still times out: $message"
        Assert-True ($script:sampleSeen.Count -ge 1) "a sample before the timeout: $($script:sampleSeen.Count)"
        # A 1 s timeout holds at most three 400 ms wait slices and the sample after the last.
        Assert-True ($script:sampleSeen.Count -le 8) "one sample per wait slice of the 1 s timeout (at most 8): $($script:sampleSeen.Count)"
    }

    Test-Case 'Exit-DisplayHold restores an outer hold it did not create (R31: the A/B sitting holds the display around each pass)' {
        . $displayLibrary
        # The shim keeps a thread state as Windows does and returns the state each call replaced.
        $script:threadState = [uint32]2147483648
        $script:executionStates = New-Object System.Collections.Generic.List[string]
        function Set-ThreadExecutionState([uint32]$Flags) {
            $previous = $script:threadState
            $script:threadState = $Flags
            $script:executionStates.Add('0x' + $Flags.ToString('X8'))
            return $previous
        }
        $outer = Enter-DisplayHold
        $inner = Enter-DisplayHold
        Exit-DisplayHold $inner
        Assert-Equal '0x80000003' ('0x' + $script:threadState.ToString('X8')) 'the inner release keeps the outer hold'
        Assert-Equal '0x80000003' $inner.ReleasedFrom 'the inner release replaced the held state'
        Exit-DisplayHold $outer
        Assert-Equal '0x80000000' ('0x' + $script:threadState.ToString('X8')) 'the outer release clears it'
        Assert-Equal '0x80000003,0x80000003,0x80000003,0x80000000' ($script:executionStates -join ',') 'requests'
    }

    # R31 fixtures: one A/B run per pair, arm and size, as ConvertTo-AbRun returns them.
    function New-AbRun([int]$Pair, [string]$Arm, [string]$Size, $Value, [string[]]$Problems = @()) {
        [pscustomobject]@{ Pair = $Pair; Arm = $Arm; Size = $Size; Value = $Value; Valid = ($Problems.Count -eq 0); Problems = $Problems; File = "$Size.json" }
    }
    function New-AbSitting([double[]]$A, [double[]]$B, [string]$Size) {
        for ($i = 0; $i -lt $A.Count; $i++) {
            New-AbRun ($i + 1) 'A' $Size $A[$i]
            New-AbRun ($i + 1) 'B' $Size $B[$i]
        }
    }
    function Format-AbCheck($Verdict) {
        @($Verdict.Checks | ForEach-Object { "$($_.size) $($_.status) $($_.a_median) $($_.b_median)" }) -join '|'
    }

    Test-Case 'Measure-AbInterfaceTotal passes when B''s median is at most 1.05 x A''s per size, at the limit too (R31: pass)' {
        . $abLibrary
        $runs = @(New-AbSitting @(2.0, 2.1, 1.9, 2.0, 2.2) @(2.1, 2.0, 2.05, 2.1, 2.0) 'normal') +
            @(New-AbSitting @(1.8, 1.8, 1.8, 1.8, 1.8) @(1.89, 1.80, 1.89, 1.95, 1.70) 'small')
        $verdict = Measure-AbInterfaceTotal $runs @('normal', 'small') 5 1.05
        Assert-Equal 0 $verdict.ExitCode 'exit code'
        Assert-Equal 'normal pass 2 2.05|small pass 1.8 1.89' (Format-AbCheck $verdict) 'medians; small sits at 1.05 x A'
        $normal = @($verdict.Checks)[0]
        Assert-Equal '2|2.1|1.9|2|2.2' (@($normal.a_values | ForEach-Object { "$_" }) -join '|') 'A values in pair order'
        Assert-True ([Math]::Abs($normal.limit - 2.1) -lt 1e-9) "limit $($normal.limit)"
        Assert-True ([Math]::Abs($normal.ratio - 1.025) -lt 1e-9) "ratio $($normal.ratio)"
        Assert-Equal 5 (Get-AbRule).Pairs 'the gate is 5 pairs'
        Assert-Equal 1.05 (Get-AbRule).Tolerance 'the gate tolerance'
        Assert-Equal 'ab03da9' (Get-AbRule).Commit 'A is the wave-1 close'
    }

    Test-Case 'Measure-AbInterfaceTotal fails when B''s median is above 1.05 x A''s (R31: fail)' {
        . $abLibrary
        $runs = @(New-AbSitting @(2.0, 2.1, 1.9, 2.0, 2.2) @(2.2, 2.3, 2.1, 2.25, 2.2) 'normal') +
            @(New-AbSitting @(1.8, 1.8, 1.8, 1.8, 1.8) @(1.8, 1.8, 1.8, 1.8, 1.8) 'small')
        $verdict = Measure-AbInterfaceTotal $runs @('normal', 'small') 5 1.05
        Assert-Equal 1 $verdict.ExitCode 'exit code'
        Assert-Equal 'normal over 2 2.2|small pass 1.8 1.8' (Format-AbCheck $verdict) 'normal is over'
    }

    Test-Case 'Measure-AbInterfaceTotal makes a size with an invalid or missing pair invalid evidence (R31: invalid pair)' {
        . $abLibrary
        $normal = @(New-AbSitting @(2.0, 2.0, 2.0, 2.0, 2.0) @(2.0, 2.0, 2.0, 2.0, 2.0) 'normal' | Where-Object { -not ($_.Pair -eq 3 -and $_.Arm -eq 'B') })
        $normal += New-AbRun 3 'B' 'normal' 2.0 @('CPU load 95% during the run (a sample at 90% or more is invalid evidence; samples 95)')
        # small is over, but an invalid size makes the whole A/B invalid evidence.
        $small = @(New-AbSitting @(1.8, 1.8, 1.8, 1.8, 1.8) @(2.5, 2.5, 2.5, 2.5, 2.5) 'small')
        $verdict = Measure-AbInterfaceTotal ($normal + $small) @('normal', 'small') 5 1.05
        Assert-Equal 2 $verdict.ExitCode 'exit code'
        Assert-Equal 'normal invalid  |small over 1.8 2.5' (Format-AbCheck $verdict) 'statuses'
        Assert-Equal 'pair 3: B invalid (CPU load 95% during the run (a sample at 90% or more is invalid evidence; samples 95))' @($verdict.Checks)[0].note 'the invalid pair is named'
        $short = @(New-AbSitting @(2.0, 2.0, 2.0, 2.0) @(2.0, 2.0, 2.0, 2.0) 'normal') + @(New-AbSitting @(1.8, 1.8, 1.8, 1.8, 1.8) @(1.8, 1.8, 1.8, 1.8, 1.8) 'small')
        $verdict = Measure-AbInterfaceTotal $short @('normal', 'small') 5 1.05
        Assert-Equal 2 $verdict.ExitCode 'a missing pair'
        Assert-Equal 'pair 5: A missing, B missing' @($verdict.Checks)[0].note 'the missing pair is named'
    }

    Test-Case 'ConvertTo-AbRun reads a measure_performance summary and rejects another present mode or pipelined state (R31)' {
        . $abLibrary
        $summary = @'
{"runs":[
 {"size":"normal","file":"progressed-peak-normal.json","valid":true,"problems":[],"interface_total_mean_ms":2.0,"present_mode":"Fifo","pipelined_rendering":true},
 {"size":"small","file":"progressed-peak-small.json","valid":false,"problems":["unfocused_sample_frames 2, expected 0"],"interface_total_mean_ms":1.9,"present_mode":"Fifo","pipelined_rendering":true}
]}
'@ | ConvertFrom-Json
        $runs = @(ConvertTo-AbRun $summary 2 'B' 'Fifo' $true)
        Assert-Equal 'normal 2 B True 2|small 2 B False 1.9' (@($runs | ForEach-Object { "$($_.Size) $($_.Pair) $($_.Arm) $($_.Valid) $($_.Value)" }) -join '|') 'runs'
        Assert-Equal 'unfocused_sample_frames 2, expected 0' (@($runs[1].Problems) -join '; ') 'measure_performance problems carry over'
        $other = @'
{"runs":[{"size":"normal","file":"x.json","valid":true,"problems":[],"interface_total_mean_ms":null,"present_mode":"AutoNoVsync","pipelined_rendering":false}]}
'@ | ConvertFrom-Json
        $run = @(ConvertTo-AbRun $other 1 'A' 'Fifo' $true)[0]
        Assert-True (-not $run.Valid) 'not the gate''s flags'
        Assert-Equal "no interface_total_mean_ms; present mode 'AutoNoVsync', the gate's is 'Fifo'; pipelined_rendering 'False', the gate's is 'True'" (@($run.Problems) -join '; ') 'problems'
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

    # R27: measure_performance.ps1 holds the display on and treats a run that overlaps a Kernel-Power 506/507 event as
    # invalid evidence. R30: it waits for a quiet machine first and samples the CPU load beside every run. The wrapper
    # runs a script under test in one process with the system queries behind those checks shimmed:
    # - Get-WinEvent (behind Get-DisplayPowerEvent): -ShimOnce fires a 506 inside the first run queried only,
    #   -ShimAlways inside every run, neither answers 'no events' as the real cmdlet does. Every query's filter goes to
    #   -ShimCallLog.
    # - Get-CimInstance Win32_Processor LoadPercentage and Win32_Process (behind Get-CpuLoadPercent and
    #   Get-ForeignBuildProcess): -ShimLoad and -ShimProcesses are comma lists, 'vxN' repeats v N times and the last entry
    #   repeats for ever. A load 'x' fails the query. A process entry 'cargo' adds a foreign cargo.exe 4242 with its
    #   rustc.exe 4243, 'child' a cargo.exe 4244 and rustc.exe 4245 below the wrapper's own process, 'none' neither.
    # - Start-Sleep (the quiet window's 5 s waits) returns at once.
    # Every load, process and sleep call goes to -ShimMachineLog. The arguments after the shim's own go to the script as
    # -Name value pairs (a -Name followed by another -Name, or last, is a switch).
    $shimWrapper = Join-Path $temporary 'shimmed.ps1'
    $shimWrapperSource = @(
        '#requires -Version 5.1',
        'param([string]$ShimScript, [string]$ShimCallLog, [string]$ShimOnce, [switch]$ShimAlways, [string]$ShimMachineLog,',
        '    [string]$ShimLoad = ''10'', [string]$ShimProcesses = ''none'', [Parameter(ValueFromRemainingArguments = $true)][string[]]$Rest)',
        '$ErrorActionPreference = ''Stop''',
        'function Expand-ShimList([string]$Text) {',
        '    $list = New-Object System.Collections.Generic.List[string]',
        '    foreach ($token in ($Text -split '','')) {',
        '        $count = 1',
        '        if ($token -match ''^(.+)x(\d+)$'') { $token = $Matches[1]; $count = [int]$Matches[2] }',
        '        for ($i = 0; $i -lt $count; $i++) { $list.Add($token) }',
        '    }',
        '    return , $list',
        '}',
        '$global:ShimState = @{ CallLog = $ShimCallLog; Once = $ShimOnce; Always = [bool]$ShimAlways; MachineLog = $ShimMachineLog',
        '    Load = (Expand-ShimList $ShimLoad); Processes = (Expand-ShimList $ShimProcesses) }',
        'function Get-ShimNext([string]$Key) {',
        '    $list = $global:ShimState[$Key]',
        '    $value = $list[0]',
        '    if ($list.Count -gt 1) { $list.RemoveAt(0) }',
        '    return $value',
        '}',
        'function Write-ShimMachineLog([string]$Line) {',
        '    if ($global:ShimState.MachineLog) { [IO.File]::AppendAllText($global:ShimState.MachineLog, $Line + "`n") }',
        '}',
        'function Get-WinEvent {',
        '    [CmdletBinding()]',
        '    param([hashtable]$FilterHashtable)',
        '    $f = $FilterHashtable',
        '    $ordered = [int]($f.StartTime -lt $f.EndTime)',
        '    if ($global:ShimState.CallLog) {',
        '        [IO.File]::AppendAllText($global:ShimState.CallLog, (''{0}|{1}|{2}|{3}'' -f $f.LogName, $f.ProviderName, (@($f.Id) -join '',''), $ordered) + "`n")',
        '    }',
        '    $fire = $global:ShimState.Always',
        '    $once = $global:ShimState.Once',
        '    if (-not $fire -and $once -and (Test-Path -LiteralPath $once)) { Remove-Item -LiteralPath $once -Force; $fire = $true }',
        '    if ($fire) { return [pscustomobject]@{ Id = 506; TimeCreated = $f.StartTime.AddTicks([long](($f.EndTime - $f.StartTime).Ticks / 2)) } }',
        '    Write-Error -Message ''No events were found that match the specified selection criteria.'' -ErrorId ''NoMatchingEventsFound'' -Category ObjectNotFound',
        '}',
        'function Get-CimInstance {',
        '    [CmdletBinding()]',
        '    param([Parameter(Position = 0)][string]$ClassName, [string[]]$Property)',
        '    if ($ClassName -eq ''Win32_Processor'' -and @($Property) -contains ''LoadPercentage'') {',
        '        $value = Get-ShimNext ''Load''',
        '        Write-ShimMachineLog "load|$value"',
        '        if ($value -eq ''x'') { Write-Error -Message ''Invalid query'' -Category InvalidOperation; return }',
        '        return [pscustomobject]@{ LoadPercentage = [int]$value; Name = ''Fake CPU'' }',
        '    }',
        '    if ($ClassName -eq ''Win32_Processor'') { return [pscustomobject]@{ Name = ''Fake CPU''; LoadPercentage = 0 } }',
        '    if ($ClassName -eq ''Win32_Process'') {',
        '        $value = Get-ShimNext ''Processes''',
        '        Write-ShimMachineLog "ps|$value"',
        '        $table = @([pscustomobject]@{ ProcessId = $PID; ParentProcessId = 4; Name = ''powershell.exe'' })',
        '        if ($value -eq ''cargo'') {',
        '            $table += [pscustomobject]@{ ProcessId = 4242; ParentProcessId = 4; Name = ''cargo.exe'' }',
        '            $table += [pscustomobject]@{ ProcessId = 4243; ParentProcessId = 4242; Name = ''rustc.exe'' }',
        '        }',
        '        if ($value -eq ''child'') {',
        '            $table += [pscustomobject]@{ ProcessId = 4244; ParentProcessId = $PID; Name = ''cargo.exe'' }',
        '            $table += [pscustomobject]@{ ProcessId = 4245; ParentProcessId = 4244; Name = ''rustc.exe'' }',
        '        }',
        '        return $table',
        '    }',
        '    throw "unshimmed Get-CimInstance $ClassName"',
        '}',
        'function Start-Sleep {',
        '    param([int]$Seconds, [int]$Milliseconds)',
        '    Write-ShimMachineLog "sleep|$Seconds"',
        '}',
        '$named = @{}',
        'for ($i = 0; $i -lt $Rest.Count; $i++) {',
        '    if ($Rest[$i] -notmatch ''^-([A-Za-z][A-Za-z0-9]*)$'') { throw "not a -Name argument: $($Rest[$i])" }',
        '    $key = $Matches[1]',
        '    if ($i + 1 -lt $Rest.Count -and $Rest[$i + 1] -notmatch ''^-[A-Za-z]'') { $named[$key] = $Rest[$i + 1]; $i += 1 }',
        '    else { $named[$key] = $true }',
        '}',
        '& $ShimScript @named',
        'exit $LASTEXITCODE'
    ) -join "`n"
    Write-Utf8NoBom -Path $shimWrapper -Text $shimWrapperSource
    function Invoke-Shimmed([string]$Script, [string]$Name, [string[]]$Arguments, [string[]]$Shim = @()) {
        # Runs $Script under the shims with $Arguments; the machine log is <temp>\<Name>.machine.txt.
        $machineLog = Join-Path $temporary "$Name.machine.txt"
        $all = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $shimWrapper, '-ShimScript', $Script,
            '-ShimMachineLog', $machineLog) + $Shim + $Arguments
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $all -LogStem (Join-Path $temporary "logs\$Name") -TimeoutSeconds 300
        $machine = @((Read-Log $machineLog) -split "`n" | Where-Object { $_ })
        return [pscustomobject]@{ Result = $r; Machine = $machine; Log = ('out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err)) }
    }
    function Invoke-ShimMeasure([string[]]$Arguments, [string]$Stem, [string[]]$Shim = @()) {
        # measure_performance.ps1 on a quiet shimmed machine (load 10%, no foreign build) unless $Shim says otherwise.
        return (Invoke-Shimmed $measure $Stem $Arguments $Shim).Result
    }
    function Invoke-DisplayMeasure([string]$Name, [string[]]$Extra = @()) {
        $shim = @('-ShimCallLog', (Join-Path $temporary "$Name.calls.txt")) + $Extra
        $m = Invoke-Shimmed $measure $Name @('-Scene', 'peak', '-NoBuild', '-OutputDirectory', (Join-Path $temporary $Name)) $shim
        $calls = @((Read-Log (Join-Path $temporary "$Name.calls.txt")) -split "`n" | Where-Object { $_ })
        $summaryPath = Join-Path $temporary "$Name\summary.json"
        $summary = $null
        if (Test-Path -LiteralPath $summaryPath) { $summary = Get-Content -Raw -LiteralPath $summaryPath | ConvertFrom-Json }
        return [pscustomobject]@{ Result = $m.Result; Calls = $calls; Summary = $summary; Machine = $m.Machine; Log = $m.Log }
    }
    $kernelPowerQuery = 'System|Microsoft-Windows-Kernel-Power|506,507|1'

    Test-Case 'measure_performance -SelfTest reports the expected exit code for every fixture under redirect' {
        $r = Invoke-Measure @('-SelfTest') 'measure-selftest'
        $out = Read-Log $r.Out
        Assert-Equal 0 $r.ExitCode ("self-test exit code; out: $out err: " + (Read-Log $r.Err))
        $lines = @(
            'ok   within-budget -> 0', 'ok   over-budget-cpu -> 1', 'ok   over-50ms -> 1', 'ok   unfocused -> 2',
            'ok   vsync-on-within-budget -> 0', 'ok   over-interface-draw -> 1', 'ok   playtest-vsync-on -> 0',
            'ok   missing-metric -> 2', 'ok   missing-cpu-frame -> 2', 'ok   over-texture-uploads -> 1',
            'ok   missing-texture-uploads -> 2', 'ok   driver-stall -> 0', 'ok   missing-driver-stalls -> 2', 'ok   wrong-size -> 2',
            'ok   over-battlefield-primitives -> 1', 'ok   over-battlefield-vertices -> 1', 'ok   missing-battlefield-counts -> 2',
            'ok   window size: a clamped height', 'ok   window size: no windows_scale',
            'ok   v3: v3 interface_total above the baseline -> 1',
            'ok   v3: v3 without a small value -> 2', 'ok   save clone: another fixture -> 2', 'ok   save clone: another worker count -> 2',
            "ok   power plan: Power saver -> 'Power saver'", 'SELFTEST PASS'
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
            $r = Invoke-ShimMeasure @('-Scene', 'all', '-Profile', 'dev', '-NoBuild', '-NoBudget', '-OutputDirectory', $out) 'measure-relative'
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
            $r = Invoke-ShimMeasure @('-Scene', 'peak', '-VsyncOff', '-Pipelined', 'off', '-NoBuild', '-OutputDirectory', $out) 'measure-rooted'
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
            $r = Invoke-ShimMeasure @('-Scene', 'peak', '-NoBuild', '-Adapter', 'Intel(R) Arc(TM) Graphics', '-OutputDirectory', $out) 'measure-adapter-other'
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
            $r = Invoke-ShimMeasure @('-Scene', 'peak', '-NoBuild', '-Adapter', 'intel(r) arc', '-OutputDirectory', $out) 'measure-adapter-same'
            Assert-Equal 0 $r.ExitCode ("matching adapter: exit code; out: " + (Read-Log $r.Out) + " err: " + (Read-Log $r.Err))
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            foreach ($run in @($summary.runs)) {
                Assert-Equal 1 $run.attempts "$($run.file) attempts"
                Assert-Equal 'Intel(R) Arc(TM) Graphics' $run.adapter "$($run.file) adapter column"
            }
        }
        finally { [Environment]::SetEnvironmentVariable('HORDE_FAKE_ADAPTER', $null, 'Process') }
    }

    Test-Case 'measure_performance clears an inherited WGPU_ADAPTER_NAME without -Adapter' {
        # Bevy honours any WGPU_ADAPTER_NAME it inherits, so a shell left steered to another GPU would gate that GPU
        # against the budget's hardware; only -Adapter steers a run, and that run is informational.
        $inheritTarget = Join-Path $temporary 'perf-inherit-target'
        $savedSteer = [Environment]::GetEnvironmentVariable('WGPU_ADAPTER_NAME', 'Process')
        try {
            New-FakeTarget $inheritTarget
            $env:CARGO_TARGET_DIR = $inheritTarget
            $env:WGPU_ADAPTER_NAME = 'intel'
            $out = Join-Path $temporary 'perf-inherit'
            $r = Invoke-ShimMeasure @('-Scene', 'peak', '-NoBuild', '-OutputDirectory', $out) 'measure-inherit'
            Assert-Equal 0 $r.ExitCode ("inherited adapter: exit code; out: " + (Read-Log $r.Out) + " err: " + (Read-Log $r.Err))
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            Assert-Equal $null $summary.adapter 'summary adapter'
            Assert-Equal $false $summary.informational 'summary informational'
            Assert-Equal 2 @($summary.runs).Count 'one run per size'
            foreach ($run in @($summary.runs)) {
                Assert-Equal 'Fake Adapter' $run.adapter "$($run.file) ran on the default adapter"
            }
        }
        finally { [Environment]::SetEnvironmentVariable('WGPU_ADAPTER_NAME', $savedSteer, 'Process') }
    }

    Test-Case 'measure_performance exits 2 when the window did not open at the requested size' {
        # The budget gates per size: a window the OS clamped (or launch.rs's 1440x900 fallback) is not normal or small.
        $sizeTarget = Join-Path $temporary 'perf-size-target'
        try {
            New-FakeTarget $sizeTarget
            $env:CARGO_TARGET_DIR = $sizeTarget
            $env:HORDE_FAKE_WINDOW = '2880x1700'
            $out = Join-Path $temporary 'perf-size-clamped'
            $r = Invoke-ShimMeasure @('-Scene', 'peak', '-NoBuild', '-OutputDirectory', $out) 'measure-size-clamped'
            Assert-Equal 2 $r.ExitCode ("clamped window: exit code; out: " + (Read-Log $r.Out) + " err: " + (Read-Log $r.Err))
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            $problems = @($summary.runs | ForEach-Object { "$($_.file): $(@($_.problems) -join '; ')" })
            Assert-Equal 2 @($summary.runs | Where-Object { $_.attempts -eq 2 }).Count 'each size was rerun once'
            Assert-Equal 'peak-normal.json: window 1440x850 (2880x1700 physical at windows_scale 2), requested 1440x900' $problems[0] 'normal problems'
            Assert-Equal 'peak-small.json: window 1440x850 (2880x1700 physical at windows_scale 2), requested 1050x700' $problems[1] 'small problems'
        }
        finally { [Environment]::SetEnvironmentVariable('HORDE_FAKE_WINDOW', $null, 'Process') }
    }


    Test-Case 'measure_performance names a valid gated over even when invalid evidence sets the exit code (R34)' {
        # R34: invalid evidence still exits 2, but a valid gated over beside it is printed and listed in summary.json.
        $overTarget = Join-Path $temporary 'perf-over-target'
        try {
            New-FakeTarget $overTarget
            $env:CARGO_TARGET_DIR = $overTarget
            $env:HORDE_FAKE_CPU_P99 = 'progressed-peak/small=12.7565'
            $line = 'VALID OVER: progressed-peak small cpu_frame_p99_ms 12.7565 > 10'
            $env:HORDE_FAKE_UNFOCUSED_SCENE = 'stress'
            $out = Join-Path $temporary 'perf-over-masked'
            $r = Invoke-ShimMeasure @('-Scene', 'all', '-VsyncOff', '-NoBuild', '-OutputDirectory', $out) 'measure-over-masked'
            $log = 'out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err)
            Assert-Equal 2 $r.ExitCode "invalid evidence keeps exit 2; $log"
            Assert-True ((Read-Log $r.Out).Contains($line)) "no '$line'; $log"
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            Assert-Equal 2 @($summary.runs | Where-Object { -not $_.valid }).Count 'the two stress groups are invalid'
            Assert-True (@($summary.PSObject.Properties.Name) -contains 'valid_overs') 'summary.json has no valid_overs'
            $overs = @($summary.valid_overs)
            Assert-Equal 1 $overs.Count 'one valid over'
            Assert-Equal 'progressed-peak|small|cpu_frame_p99_ms|median' (@($overs[0].scene, $overs[0].size, $overs[0].metric, $overs[0].aggregate) -join '|') 'the valid over'
            Assert-Equal 12.7565 ([double]$overs[0].value) 'the valid over value'
            Assert-Equal 10.0 ([double]$overs[0].limit) 'the valid over gate'
            Assert-True ((Read-Log (Join-Path $out 'summary.md')).Contains($line)) 'summary.md names the valid over'

            # With no invalid run the same over exits 1 and is still named.
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_UNFOCUSED_SCENE', $null, 'Process')
            $out = Join-Path $temporary 'perf-over-plain'
            $r = Invoke-ShimMeasure @('-Scene', 'progressed-peak', '-VsyncOff', '-NoBuild', '-OutputDirectory', $out) 'measure-over-plain'
            Assert-Equal 1 $r.ExitCode ('over budget; out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err))
            Assert-True ((Read-Log $r.Out).Contains($line)) "no '$line' on exit 1"
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            Assert-Equal 1 @($summary.valid_overs).Count 'one valid over on exit 1'

            # A pass within the budget lists none.
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_CPU_P99', $null, 'Process')
            $out = Join-Path $temporary 'perf-over-none'
            $r = Invoke-ShimMeasure @('-Scene', 'progressed-peak', '-VsyncOff', '-NoBuild', '-OutputDirectory', $out) 'measure-over-none'
            Assert-Equal 0 $r.ExitCode ('within budget; out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err))
            Assert-True (-not (Read-Log $r.Out).Contains('VALID OVER')) 'a VALID OVER line without an over'
            $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
            Assert-True (@($summary.PSObject.Properties.Name) -contains 'valid_overs') 'summary.json has no valid_overs when within budget'
            Assert-Equal 0 @($summary.valid_overs).Count 'no valid over'
        }
        finally {
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_CPU_P99', $null, 'Process')
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_UNFOCUSED_SCENE', $null, 'Process')
        }
    }

    Test-Case 'measure_performance holds the display on and keeps a run without a 506/507 event valid (R27)' {
        $displayTarget = Join-Path $temporary 'perf-display-target'
        New-FakeTarget $displayTarget
        $env:CARGO_TARGET_DIR = $displayTarget
        $m = Invoke-DisplayMeasure 'perf-display-quiet'
        Assert-Equal 0 $m.Result.ExitCode "exit code; $($m.Log)"
        Assert-Equal "$kernelPowerQuery`n$kernelPowerQuery" ($m.Calls -join "`n") 'one Kernel-Power query per run, over its own window'
        $hold = $m.Summary.display_hold
        Assert-True ($null -ne $hold) 'summary.json has no display_hold'
        Assert-Equal 'ES_CONTINUOUS|ES_SYSTEM_REQUIRED|ES_DISPLAY_REQUIRED' $hold.request 'hold request'
        Assert-Equal $true $hold.held 'held'
        Assert-Equal $true $hold.released 'released at the end'
        Assert-Equal '0x80000003' $hold.released_from 'the hold was in effect until its release'
        foreach ($run in @($m.Summary.runs)) {
            Assert-Equal 1 $run.attempts "$($run.file) attempts"
            Assert-Equal $true $run.display_held "$($run.file) display_held"
            Assert-Equal 0 @($run.display_power_events).Count "$($run.file) display_power_events"
        }
        $md = Read-Log (Join-Path $temporary 'perf-display-quiet\summary.md')
        Assert-True ($md -match 'Display held on') 'summary.md names the display hold'
        Assert-True ($md -match '\| held \|') 'summary.md display column'
    }

    Test-Case 'measure_performance marks a run that overlaps a 506/507 event invalid and reruns it (R27)' {
        $displayTarget = Join-Path $temporary 'perf-display-target'
        $env:CARGO_TARGET_DIR = $displayTarget
        $once = Join-Path $temporary 'display-off-once'
        Write-Utf8NoBom -Path $once -Text 'x'
        $m = Invoke-DisplayMeasure 'perf-display-once' @('-ShimOnce', $once)
        Assert-Equal 0 $m.Result.ExitCode "a valid rerun passes; $($m.Log)"
        Assert-Equal 3 $m.Calls.Count 'queries: the first run twice, the second once'
        $first = @($m.Summary.runs)[0]
        Assert-Equal 2 $first.attempts 'the overlapping run was rerun'
        Assert-True $first.valid 'the rerun is valid'
        Assert-True (@($first.rejected_attempts)[0] -like 'display off during the run (Kernel-Power 506 at *)') "rejected attempt: $(@($first.rejected_attempts) -join '; ')"
        Assert-Equal '1:506' (@($first.display_power_events | ForEach-Object { "$($_.attempt):$($_.id)" }) -join ',') 'the event is recorded with its attempt'
        Assert-True (Test-Path -LiteralPath (Join-Path $temporary 'perf-display-once\peak-normal.invalid.json')) 'the rejected report was not kept'
        Assert-Equal 1 @($m.Summary.runs)[1].attempts 'the second run was not rerun'

        $m = Invoke-DisplayMeasure 'perf-display-always' @('-ShimAlways')
        Assert-Equal 2 $m.Result.ExitCode "invalid twice is invalid evidence; $($m.Log)"
        Assert-Equal 4 $m.Calls.Count 'two attempts per run'
        foreach ($run in @($m.Summary.runs)) {
            Assert-Equal 2 $run.attempts "$($run.file) attempts"
            Assert-True (@($run.problems)[0] -like 'display off during the run (Kernel-Power 506 at *)') "$($run.file) problems: $(@($run.problems) -join '; ')"
            Assert-Equal '1:506,2:506' (@($run.display_power_events | ForEach-Object { "$($_.attempt):$($_.id)" }) -join ',') "$($run.file) events"
        }
    }

    Test-Case 'measure_performance releases the display hold when a run throws (R27)' {
        $displayTarget = Join-Path $temporary 'perf-display-target'
        $env:CARGO_TARGET_DIR = $displayTarget
        try {
            $env:HORDE_FAKE_EXIT = '4'
            $m = Invoke-DisplayMeasure 'perf-display-throws'
            Assert-Equal 3 $m.Result.ExitCode "a failed run is a run failure; $($m.Log)"
            Assert-True ($m.Summary.failure -match 'exit code 4') "failure: $($m.Summary.failure)"
            Assert-Equal $true $m.Summary.display_hold.released 'released after the throw'
            Assert-Equal '0x80000003' $m.Summary.display_hold.released_from 'the hold was in effect until its release'
        }
        finally { [Environment]::SetEnvironmentVariable('HORDE_FAKE_EXIT', $null, 'Process') }
    }

    # R30: the quiet window before the runs and the load sampler beside each run, on the shimmed machine.
    function Invoke-LoadMeasure([string]$Name, [string[]]$Shim, [string[]]$Extra = @()) {
        $loadTarget = Join-Path $temporary 'perf-load-target'
        if (-not (Test-Path -LiteralPath $loadTarget)) { New-FakeTarget $loadTarget }
        $env:CARGO_TARGET_DIR = $loadTarget
        $out = Join-Path $temporary $Name
        $m = Invoke-Shimmed $measure $Name (@('-Scene', 'peak', '-NoBuild', '-OutputDirectory', $out) + $Extra) $Shim
        $summary = $null
        if (Test-Path -LiteralPath (Join-Path $out 'summary.json')) { $summary = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json }
        $m | Add-Member -NotePropertyName Summary -NotePropertyValue $summary
        $m | Add-Member -NotePropertyName Markdown -NotePropertyValue (Read-Log (Join-Path $out 'summary.md'))
        return $m
    }

    Test-Case 'measure_performance waits for a quiet window before the runs and records the load of every run (R30: a quiet start)' {
        $m = Invoke-LoadMeasure 'perf-quiet-start' @('-ShimLoad', '20')
        Assert-Equal 0 $m.Result.ExitCode "exit code; $($m.Log)"
        $loads = @($m.Machine | Where-Object { $_ -like 'load|*' })
        Assert-Equal 15 $loads.Count "13 quiet-window samples and one per run; $($m.Machine -join ' ')"
        Assert-Equal 12 @($m.Machine | Where-Object { $_ -eq 'sleep|5' }).Count 'the window waited 12 times 5 s'
        Assert-Equal 13 @($m.Machine | Where-Object { $_ -like 'ps|*' }).Count 'one process query per window sample'
        $quiet = $m.Summary.quiet_window
        Assert-Equal $true $quiet.enabled 'quiet window on by default'
        Assert-Equal 240 $quiet.timeout_minutes 'default timeout'
        Assert-Equal 35 $quiet.quiet_below_percent 'threshold'
        Assert-Equal 90 $quiet.invalid_at_percent 'run threshold'
        Assert-Equal 1 @($quiet.waits).Count 'one wait, before the runs'
        $wait = @($quiet.waits)[0]
        Assert-Equal 'runs' $wait.before 'before'
        Assert-Equal $true $wait.quiet 'quiet'
        Assert-Equal 13 $wait.samples 'samples'
        Assert-Equal 20 $wait.mean_percent 'window mean'
        foreach ($run in @($m.Summary.runs)) {
            Assert-Equal 1 $run.attempts "$($run.file) attempts"
            Assert-Equal 1 $run.load_samples "$($run.file) load samples"
            Assert-Equal 20 $run.load_mean_percent "$($run.file) load mean"
            Assert-Equal 20 $run.load_max_percent "$($run.file) load max"
        }
        Assert-True ((Read-Log $m.Result.Out) -match 'quiet window: quiet after 13 samples') "the log says it started; $($m.Log)"
        Assert-True ($m.Markdown -match 'Quiet window') 'summary.md names the quiet window'
        Assert-True ($m.Markdown -match '\| 20 / 20 \|') 'summary.md load column'
    }

    Test-Case 'measure_performance waits out a foreign build, ignores its own children, then starts (R30: waiting then starting)' {
        $m = Invoke-LoadMeasure 'perf-quiet-wait' @('-ShimLoad', '20', '-ShimProcesses', 'cargox3,childx2,none')
        Assert-Equal 0 $m.Result.ExitCode "exit code; $($m.Log)"
        $wait = @($m.Summary.quiet_window.waits)[0]
        Assert-Equal $true $wait.quiet 'quiet'
        Assert-Equal 16 $wait.samples 'three samples with the foreign build, then 13 quiet ones (its own cargo does not count)'
        Assert-Equal 'build processes: cargo.exe, rustc.exe' (@($wait.waited_for) -join '|') 'what it waited for'
        $out = Read-Log $m.Result.Out
        Assert-True ($out -match 'quiet window: waiting for build processes: cargo\.exe \(4242\), rustc\.exe \(4243\)') "the log names the foreign build; $($m.Log)"
        Assert-True (-not ($out -match '4244')) 'its own cargo is never named'
    }

    Test-Case 'measure_performance exits 2 when the quiet window times out (R30: a timeout)' {
        $m = Invoke-LoadMeasure 'perf-quiet-timeout' @('-ShimLoad', '50') @('-QuietTimeoutMinutes', '1')
        Assert-Equal 2 $m.Result.ExitCode "a timed-out window is invalid evidence; $($m.Log)"
        Assert-Equal 13 @($m.Machine | Where-Object { $_ -like 'load|*' }).Count 'a 1-minute timeout is 13 samples, and no run started'
        Assert-Equal 'invalid evidence' $m.Summary.verdict 'verdict'
        Assert-True ($m.Summary.failure -match '^quiet window timed out after 1 min') "failure: $($m.Summary.failure)"
        Assert-Equal 0 @($m.Summary.runs).Count 'no runs'
        $wait = @($m.Summary.quiet_window.waits)[0]
        Assert-Equal $true $wait.timed_out 'timed out'
        Assert-Equal 'mean CPU load' (@($wait.waited_for) -join '|') 'what it waited for'
        Assert-Equal $true $m.Summary.display_hold.released 'the display hold is released'
        Assert-True ((Read-Log $m.Result.Out) -match 'INVALID quiet window timed out') "stdout; $($m.Log)"
    }

    Test-Case 'measure_performance marks a run with a 90% load sample invalid and reruns it (R30)' {
        $m = Invoke-LoadMeasure 'perf-load-once' @('-ShimLoad', '95,10') @('-NoWaitQuiet')
        Assert-Equal 0 $m.Result.ExitCode "a valid rerun passes; $($m.Log)"
        Assert-Equal 0 @($m.Machine | Where-Object { $_ -like 'sleep|*' -or $_ -like 'ps|*' }).Count '-NoWaitQuiet skips the window'
        Assert-Equal $false $m.Summary.quiet_window.enabled 'the window is off'
        $first = @($m.Summary.runs)[0]
        Assert-Equal 2 $first.attempts 'the loaded run was rerun'
        Assert-True $first.valid 'the rerun is valid'
        Assert-Equal 'CPU load 95% during the run (a sample at 90% or more is invalid evidence; samples 95)' (@($first.rejected_attempts) -join '; ') 'rejected attempt'
        Assert-Equal 10 $first.load_max_percent 'the kept attempt''s load'
        Assert-Equal '1:95,2:10' (@($first.load_attempts | ForEach-Object { "$($_.attempt):$($_.max_percent)" }) -join ',') 'every attempt''s load'
        Assert-Equal 1 @($m.Summary.runs)[1].attempts 'the second run was not rerun'

        $m = Invoke-LoadMeasure 'perf-load-always' @('-ShimLoad', '95') @('-NoWaitQuiet')
        Assert-Equal 2 $m.Result.ExitCode "invalid twice is invalid evidence; $($m.Log)"
        foreach ($run in @($m.Summary.runs)) {
            Assert-Equal 2 $run.attempts "$($run.file) attempts"
            Assert-Equal 'CPU load 95% during the run (a sample at 90% or more is invalid evidence; samples 95)' (@($run.problems) -join '; ') "$($run.file) problems"
        }
    }

    Test-Case 'measure_performance accepts -WaitQuiet and rejects bad quiet-window arguments with exit 3 (R30)' {
        $m = Invoke-LoadMeasure 'perf-quiet-explicit' @() @('-WaitQuiet', '-QuietTimeoutMinutes', '5')
        Assert-Equal 0 $m.Result.ExitCode "explicit -WaitQuiet; $($m.Log)"
        Assert-Equal 5 $m.Summary.quiet_window.timeout_minutes 'timeout'
        $cases = @(@('-QuietTimeoutMinutes', '0'), @('-QuietTimeoutMinutes', 'x'), @('-QuietTimeoutMinutes', '1441'), @('-WaitQuiet', '-NoWaitQuiet'))
        $index = 0
        foreach ($arguments in $cases) {
            $index += 1
            $r = Invoke-Measure ($arguments + @('-NoBuild', '-OutputDirectory', (Join-Path $temporary "perf-quiet-bad-$index"))) "measure-quiet-bad-$index"
            Assert-Equal 3 $r.ExitCode ("$($arguments -join ' '): exit code; out: " + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err))
            Assert-True (-not (Test-Path -LiteralPath (Join-Path $temporary "perf-quiet-bad-$index"))) "$($arguments -join ' '): created its output directory"
        }
    }

    # W2-47: the two arms are distinct files (the same fake game, compiled twice), because the script refuses an A and a
    # B with the same sha256.
    . (Join-Path $scriptsDirectory 'lib\AbInterfaceTotal.ps1')
    $helperB = Join-Path $temporary 'native-helper-b.exe'
    Add-Type -TypeDefinition ($helperSource + "`n// the A/B's B arm`n") -OutputType ConsoleApplication -OutputAssembly $helperB

    Test-Case 'Invoke-AbArmBuild cleans the workspace packages before it builds an arm (W2-47)' {
        $calls = New-Object System.Collections.Generic.List[string]
        $cargo = { param($Arguments, $LogStem, $WorkingDirectory) $calls.Add("$($Arguments -join ' ')|$(Split-Path -Leaf $LogStem)|$WorkingDirectory") }
        Invoke-AbArmBuild -Cargo $cargo -LogStem (Join-Path $temporary 'logs\cargo-a') -WorkingDirectory 'C:\src-a' -Target 'C:\tgt'
        Assert-Equal 'clean -p horde-defense -p horde_core|cargo-a-clean|C:\src-a,build --locked -p horde-defense|cargo-a|C:\src-a' ($calls -join ',') 'clean, then build, in the arm''s tree'
    }

    Test-Case 'Invoke-AbArmBuild builds both arms into the resolved target when CARGO_TARGET_DIR is unset or relative (W2 review)' {
        # A is built in <target>\ab-w1\src\horde-defense: with CARGO_TARGET_DIR unset cargo wrote it to that tree's own
        # target, and with a relative one it resolved it against that tree, while the script read A from the workspace's.
        $calls = New-Object System.Collections.Generic.List[string]
        $cargo = { param($Arguments, $LogStem, $WorkingDirectory) $calls.Add("$($Arguments[0])|$([Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process'))") }
        try {
            foreach ($configured in @($null, 'rel\tgt')) {
                [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $configured, 'Process')
                $calls.Clear()
                $resolved = Resolve-HordeTarget -Workspace $workspace
                Invoke-AbArmBuild -Cargo $cargo -LogStem (Join-Path $temporary 'logs\cargo-a') -WorkingDirectory 'C:\src-a' -Target $resolved
                Invoke-AbArmBuild -Cargo $cargo -LogStem (Join-Path $temporary 'logs\cargo-b') -WorkingDirectory $workspace -Target $resolved
                Assert-Equal "clean|$resolved,build|$resolved,clean|$resolved,build|$resolved" ($calls -join ',') "CARGO_TARGET_DIR '$configured': every arm build sees the resolved target"
                Assert-Equal $configured ([Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')) "CARGO_TARGET_DIR '$configured' is restored"
            }
            $message = $null
            try { Invoke-AbArmBuild -Cargo $cargo -LogStem (Join-Path $temporary 'logs\cargo-a') -WorkingDirectory 'C:\src-a' -Target 'rel\tgt' } catch { $message = $_.Exception.Message }
            Assert-Equal 'Invoke-AbArmBuild needs a rooted -Target, not ''rel\tgt''' $message 'a relative target is refused'
        }
        finally { [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $savedTarget, 'Process') }
    }

    Test-Case 'Restore-AbWorkspaceBuild cleans the workspace packages when A was built and B was not (W2-47 review)' {
        $calls = New-Object System.Collections.Generic.List[string]
        $cargo = { param($Arguments, $LogStem, $WorkingDirectory) $calls.Add("$($Arguments -join ' ')|$(Split-Path -Leaf $LogStem)|$WorkingDirectory") }
        $stem = Join-Path $temporary 'logs\cargo-restore'
        Assert-Equal $false (Restore-AbWorkspaceBuild -Cargo $cargo -ABuilt $false -BBuilt $false -LogStem $stem -Workspace 'C:\ws') 'A not built'
        Assert-Equal $false (Restore-AbWorkspaceBuild -Cargo $cargo -ABuilt $true -BBuilt $true -LogStem $stem -Workspace 'C:\ws') 'B built after A'
        Assert-Equal 0 $calls.Count 'no clean while the workspace holds HEAD''s build'
        Assert-Equal $true (Restore-AbWorkspaceBuild -Cargo $cargo -ABuilt $true -BBuilt $false -LogStem $stem -Workspace 'C:\ws') 'A built, B not'
        Assert-Equal 'clean -p horde-defense -p horde_core|cargo-restore|C:\ws' ($calls -join ',') 'A''s packages cleaned in the workspace'
    }

    Test-Case 'Assert-AbArmsDiffer refuses an A and a B with the same executable (W2-47)' {
        Assert-AbArmsDiffer 'aa11' 'bb22'
        $message = $null
        try { Assert-AbArmsDiffer 'AA11' 'aa11' } catch { $message = $_.Exception.Message }
        Assert-Equal 'A and B are the same executable (sha256 aa11); an arm''s build did not replace the other''s' $message 'the guard'
    }

    # R31: ab_interface_total.ps1 end to end on the shimmed machine, both arms copies of the fake game. The fake reports
    # the interface_total mean HORDE_FAKE_INTERFACE_TOTAL gives its arm folder (exe-a or exe-b).
    $abScript = Join-Path $scriptsDirectory 'ab_interface_total.ps1'
    function Invoke-AbGate([string]$Name, [string]$Totals, [string[]]$Shim = @(), [string[]]$Extra = @(), [string]$ExeB = $helperB) {
        $out = Join-Path $temporary $Name
        $env:HORDE_FAKE_INTERFACE_TOTAL = $Totals
        try {
            $m = Invoke-Shimmed $abScript $Name (@('-NoBuild', '-ExecutableA', $helper, '-ExecutableB', $ExeB, '-Pairs', '2', '-OutputDirectory', $out) + $Extra) $Shim
        }
        finally { [Environment]::SetEnvironmentVariable('HORDE_FAKE_INTERFACE_TOTAL', $null, 'Process') }
        $record = $null
        if (Test-Path -LiteralPath (Join-Path $out 'ab.json')) { $record = Get-Content -Raw -LiteralPath (Join-Path $out 'ab.json') | ConvertFrom-Json }
        $m | Add-Member -NotePropertyName Record -NotePropertyValue $record
        $m | Add-Member -NotePropertyName Markdown -NotePropertyValue (Read-Log (Join-Path $out 'ab.md'))
        return $m
    }
    function Format-AbRecord($Record) {
        @($Record.checks | ForEach-Object { "$($_.size) $($_.status) $($_.a_median) $($_.b_median)" }) -join '|'
    }

    Test-Case 'ab_interface_total alternates A and B and passes B within 1.05 x A per size (R31: pass)' {
        $m = Invoke-AbGate 'ab-pass' 'exe-a=2.0;exe-b=2.08'
        Assert-Equal 0 $m.Result.ExitCode "exit code; $($m.Log)"
        Assert-Equal 'pair-1-a,pair-1-b,pair-2-a,pair-2-b' (@($m.Record.order | ForEach-Object { $_.directory }) -join ',') 'A and B alternate'
        Assert-Equal 'normal pass 2 2.08|small pass 2 2.08' (Format-AbRecord $m.Record) 'checks'
        Assert-Equal 'progressed-peak dev Fifo True' "$($m.Record.scene) $($m.Record.profile) $($m.Record.present_mode) $($m.Record.pipelined)" 'the gate''s scene, profile and flags'
        Assert-Equal 1.05 $m.Record.tolerance 'tolerance'
        Assert-Equal 8 @($m.Record.runs | Where-Object { $_.valid }).Count 'two pairs at two sizes, every run valid'
        Assert-Equal $true $m.Record.quiet_window.wait.quiet 'one quiet window before the sitting'
        Assert-Equal 13 @($m.Machine | Where-Object { $_ -like 'ps|*' }).Count 'the measure passes skip their own window'
        Assert-Equal 21 @($m.Machine | Where-Object { $_ -like 'load|*' }).Count '13 window samples and one per run'
        Assert-Equal $true $m.Record.display_hold.released 'the sitting''s display hold is released'
        Assert-Equal $false $m.Record.workspace_cleaned 'nothing was built, so the workspace is not cleaned'
        Assert-True ($m.Markdown -match '\| normal \| 2, 2 \| 2.08, 2.08 \| 2 \| 2.08 \| 1.04 \| 2.1 \| pass \|') "ab.md check row; $($m.Markdown)"
        foreach ($arm in @('a', 'b')) {
            Assert-True (Test-Path -LiteralPath (Join-Path $temporary "ab-pass\pair-1-$arm\summary.json")) "pair 1 $arm keeps its measure_performance evidence"
        }
    }

    Test-Case 'ab_interface_total fails when B''s median is above 1.05 x A''s (R31: fail)' {
        $m = Invoke-AbGate 'ab-fail' 'exe-a=2.0;exe-b=2.2'
        Assert-Equal 1 $m.Result.ExitCode "exit code; $($m.Log)"
        Assert-Equal 'normal over 2 2.2|small over 2 2.2' (Format-AbRecord $m.Record) 'checks'
        Assert-Equal 'B over 1.05 x A' $m.Record.verdict 'verdict'
    }

    Test-Case 'ab_interface_total makes a size with an invalid pair invalid evidence (R31: invalid pair)' {
        # Load samples: 13 for the window, then pair 1 A normal and small, then B normal twice at 95% (invalid twice).
        $m = Invoke-AbGate 'ab-invalid' 'exe-a=2.0;exe-b=2.0' @('-ShimLoad', '10x15,95,95,10')
        Assert-Equal 2 $m.Result.ExitCode "exit code; $($m.Log)"
        Assert-Equal 'normal invalid  |small pass 2 2' (Format-AbRecord $m.Record) 'checks'
        Assert-True (@($m.Record.checks)[0].note -like 'pair 1: B invalid (CPU load 95% during the run*') "note: $(@($m.Record.checks)[0].note)"
        Assert-Equal 'invalid evidence' $m.Record.verdict 'verdict'
    }

    Test-Case 'ab_interface_total exits 3 before any pair when A and B are the same executable (W2-47)' {
        $m = Invoke-AbGate 'ab-same' 'exe-a=2.0;exe-b=2.0' @() @() $helper
        Assert-Equal 3 $m.Result.ExitCode "exit code; $($m.Log)"
        Assert-Equal 0 @($m.Record.order).Count 'no pair ran'
        Assert-True ($m.Record.failure -like 'A and B are the same executable (sha256 *') "failure: $($m.Record.failure)"
    }

    Test-Case 'ab_interface_total exits 3 on bad arguments and 2 when the quiet window times out (R31)' {
        $index = 0
        foreach ($arguments in @(@('-Pairs', '0'), @('-Pairs', 'x'), @('-NoBuild'), @('-Bogus'))) {
            $index += 1
            $all = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $abScript) + $arguments
            $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $all -LogStem (Join-Path $temporary "logs\ab-bad-$index") -TimeoutSeconds 120
            Assert-Equal 3 $r.ExitCode ("$($arguments -join ' '): exit code; out: " + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err))
        }
        # W2-47 review: building A and not B would leave A's fingerprints and exe in the workspace target, so -ExecutableB
        # needs -ExecutableA. -Pairs 0 is also bad, so the refusal must come first (and nothing is ever built here).
        $all = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $abScript, '-ExecutableB', $helperB, '-Pairs', '0')
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $all -LogStem (Join-Path $temporary 'logs\ab-bad-b-only') -TimeoutSeconds 120
        Assert-Equal 3 $r.ExitCode ('-ExecutableB alone: exit code; out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err))
        Assert-True ((Read-Log $r.Out) -like '*-ExecutableB needs -ExecutableA*') ('-ExecutableB alone is refused; out: ' + (Read-Log $r.Out))
        $m = Invoke-AbGate 'ab-timeout' 'exe-a=2.0;exe-b=2.0' @('-ShimLoad', '50') @('-QuietTimeoutMinutes', '1')
        Assert-Equal 2 $m.Result.ExitCode "exit code; $($m.Log)"
        Assert-Equal 0 @($m.Record.order).Count 'no pair ran'
        Assert-True ($m.Record.failure -match '^quiet window timed out after 1 min before the A/B') "failure: $($m.Record.failure)"
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
            'sidecar_echoes_request small battle pass', 'sidecar_echoes_request small construction pass', 'sidecar_echoes_request small max-lanes pass',
            'window_matches_request large battle pass', 'window_matches_request large broken pass', 'window_matches_request large construction pass',
            'window_matches_request large max-lanes pass', 'window_matches_request large menu pass',
            'window_matches_request small battle pass', 'window_matches_request small broken pass', 'window_matches_request small construction pass',
            'window_matches_request small max-lanes pass', 'window_matches_request small menu pass'
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

    Test-Case 'capture_review fails a sidecar that echoes the reduced effects but not the requested UI scale' {
        # rescaled reports the requested reduced-effects mode but always scale 1: the large still matches the request,
        # the small one is not small-size evidence on the UI scale alone.
        $out = Join-Path $temporary 'captures-rescaled'
        $review = Join-Path $scriptsDirectory 'capture_review.ps1'
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $review, '-NoBuild', '-Executable', $helper,
            '-Scenes', 'rescaled', '-Modes', 'normal', '-OutputDirectory', $out)
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $arguments -LogStem (Join-Path $temporary 'logs\capture-review-rescaled') -TimeoutSeconds 300
        $log = 'out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err)
        Assert-Equal 1 $r.ExitCode "a sidecar that ignores the UI scale exits 1; $log"
        $report = Get-Content -Raw -LiteralPath (Join-Path $out 'captures.json') | ConvertFrom-Json
        $rows = @($report.sidecar_checks | Where-Object { $_.check -eq 'sidecar_echoes_request' } | ForEach-Object { "$($_.mode) $($_.size) $($_.scene) $($_.result)" } | Sort-Object)
        Assert-Equal "normal large rescaled pass`nnormal small rescaled fail" ($rows -join "`n") 'echo rows'
        $details = @{}
        foreach ($row in @($report.sidecar_checks)) { $details["$($row.check) $($row.size) $($row.scene)"] = $row.detail }
        Assert-Equal 'reduced_effects false (requested false), ui_scale 1 (requested 1.35)' $details['sidecar_echoes_request small rescaled'] 'small detail'
        $failLines = @((Read-Log $r.Out) -split "`r?`n" | Where-Object { $_ -like 'FAIL Sidecar check failed: sidecar_echoes_request *' })
        Assert-Equal 1 $failLines.Count "one FAIL line for the small still; $log"
    }

    Test-Case 'capture_review fails a still that was not drawn at the requested window size' {
        # The OS can clamp the window (a work-area clamp on the height) and launch.rs opens DEFAULT_WINDOW 1440x900 when
        # LASTLIGHT_REVIEW_SIZE is malformed. Either way the still is not evidence for its nominal size, though its
        # sidecar echoes the request and its relative checks would agree with themselves.
        $out = Join-Path $temporary 'captures-clamped'
        $review = Join-Path $scriptsDirectory 'capture_review.ps1'
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $review, '-NoBuild', '-Executable', $helper,
            '-Scenes', 'battle,clamped', '-Modes', 'normal', '-OutputDirectory', $out)
        $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $arguments -LogStem (Join-Path $temporary 'logs\capture-review-clamped') -TimeoutSeconds 300
        $log = 'out: ' + (Read-Log $r.Out) + ' err: ' + (Read-Log $r.Err)
        Assert-Equal 1 $r.ExitCode "a still at the wrong window size exits 1; $log"
        $report = Get-Content -Raw -LiteralPath (Join-Path $out 'captures.json') | ConvertFrom-Json
        Assert-Equal 2.0 $report.windows_scale 'windows_scale from the first capture'
        $rows = @($report.sidecar_checks | Where-Object { $_.check -eq 'window_matches_request' } | ForEach-Object { "$($_.size) $($_.scene) $($_.result)" } | Sort-Object)
        $expected = @('large battle pass', 'large clamped fail', 'small battle pass', 'small clamped fail') | Sort-Object
        Assert-Equal ($expected -join "`n") ($rows -join "`n") 'window rows'
        $details = @{}
        foreach ($row in @($report.sidecar_checks)) { $details["$($row.check) $($row.size) $($row.scene)"] = $row.detail }
        Assert-Equal 'PNG 2880x1700 at windows_scale 2, requested 1440x900 (2880x1800)' $details['window_matches_request large clamped'] 'clamped detail'
        Assert-Equal 'PNG 2880x1800 at windows_scale 2, requested 1050x700 (2100x1400)' $details['window_matches_request small clamped'] 'fallback detail'
        Assert-Equal 'PNG 2100x1400 at windows_scale 2, requested 1050x700 (2100x1400)' $details['window_matches_request small battle'] 'pass detail'
        $large = @($report.captures | Where-Object { $_.scene -eq 'clamped' -and $_.size -eq 'large' })
        Assert-Equal 2880 $large[0].width_physical 'PNG width recorded'
        Assert-Equal 1700 $large[0].height_physical 'PNG height recorded'
        $failLines = @((Read-Log $r.Out) -split "`r?`n" | Where-Object { $_ -like 'FAIL Sidecar check failed: window_matches_request *' })
        Assert-Equal 2 $failLines.Count "one FAIL line per wrong-size still; $log"
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

    # W7-02: the playtest package scripts. lib\Playtest.ps1 holds the pure parts; package_playtest.ps1 and
    # verify_playtest.ps1 run end to end against the fake game (its --version and --smoke follow HORDE_FAKE_*).
    $playtestLibrary = Join-Path $scriptsDirectory 'lib\Playtest.ps1'
    if (Test-Path -LiteralPath $playtestLibrary) { . $playtestLibrary }
    $playtestCommit = '0123456789abcdef0123456789abcdef01234567'
    $playtestVersion = '0.0.0'
    if (Get-Command Get-WorkspaceVersion -ErrorAction SilentlyContinue) { $playtestVersion = Get-WorkspaceVersion -Workspace $workspace }
    $playtestTarget = Join-Path $temporary 'playtest-target'

    function New-TestPackage([string]$Root, [string[]]$Features = @(), [string[]]$ExtraDocs = @()) {
        # A small unpacked package with a version 2 manifest: the fake game, one asset, one catalog and the guide.
        New-Item -ItemType Directory -Force -Path (Join-Path $Root 'assets\art'), (Join-Path $Root 'content'), (Join-Path $Root 'docs') | Out-Null
        Copy-Item -LiteralPath $helper -Destination (Join-Path $Root 'Lastlight.exe')
        Write-Utf8NoBom -Path (Join-Path $Root 'assets\art\sheet.txt') -Text 'art'
        Write-Utf8NoBom -Path (Join-Path $Root 'content\world.json') -Text '{}'
        Write-Utf8NoBom -Path (Join-Path $Root 'docs\PLAYTEST_GUIDE.md') -Text "# Guide`n[top](#guide) [web](https://example.com/a.md)`n"
        foreach ($doc in $ExtraDocs) { Write-Utf8NoBom -Path (Join-Path (Join-Path $Root 'docs') $doc) -Text 'extra' }
        $files = @(Get-PackageFileRecord -PackageRoot $Root)
        $manifest = New-PlaytestManifest -GameVersion $playtestVersion -Commit $playtestCommit -Branch 'main' -Profile 'playtest' -Features $Features -Rustc 'rustc fake' -CreatedUtc ([DateTime]::UtcNow) -CuratedDocs @('PLAYTEST_GUIDE.md') -Files $files
        Write-Utf8NoBom -Path (Join-Path $Root 'manifest.json') -Text ($manifest | ConvertTo-Json -Depth 6)
        return $manifest
    }
    function Invoke-Verify([string[]]$Arguments, [string]$Stem) {
        $all = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $scriptsDirectory 'verify_playtest.ps1')) + $Arguments
        return Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $all -LogStem (Join-Path $temporary "logs\$Stem") -TimeoutSeconds 300
    }
    function Get-VerificationFolders {
        $root = Join-Path $playtestTarget 'playtest-verification'
        if (-not (Test-Path -LiteralPath $root)) { return @() }
        return @(Get-ChildItem -LiteralPath $root -Directory | Sort-Object CreationTimeUtc)
    }

    Test-Case 'Get-PackageRelativePath gives forward-slash paths below the root and refuses a path outside it' {
        $root = Join-Path $temporary 'relative root'
        Assert-Equal 'docs/PLAYTEST_GUIDE.md' (Get-PackageRelativePath -Root $root -Path (Join-Path $root 'docs\PLAYTEST_GUIDE.md')) 'nested'
        Assert-Equal 'Lastlight.exe' (Get-PackageRelativePath -Root ($root + '\') -Path (Join-Path $root 'Lastlight.exe')) 'top level, root with a trailing separator'
        $threw = $false
        try { Get-PackageRelativePath -Root $root -Path (Join-Path $temporary 'relative rooted\x.txt') | Out-Null } catch { $threw = $true }
        Assert-True $threw 'a sibling folder that shares the prefix is outside the root'
    }

    Test-Case 'New-PlaytestZip writes forward-slash entries for nested folders (no backslash entries under 5.1)' {
        $source = Join-Path $temporary 'zip-source'
        New-Item -ItemType Directory -Force -Path (Join-Path $source 'assets\art\pixel') | Out-Null
        Write-Utf8NoBom -Path (Join-Path $source 'assets\art\pixel\a.txt') -Text 'a'
        Write-Utf8NoBom -Path (Join-Path $source 'READ ME.txt') -Text 'r'
        $zip = New-PlaytestZip -SourceDirectory $source -Destination (Join-Path $temporary 'package.zip')
        $names = @(Get-ZipEntryName -Path $zip)
        Assert-Equal 'READ ME.txt|assets/art/pixel/a.txt' ($names -join "|") "entries are relative to the folder contents, in ordinal order"
        Assert-Equal 0 @(Get-ZipEntryViolation -Path $zip).Count 'no violations'
        $threw = $false
        try { New-PlaytestZip -SourceDirectory $source -Destination $zip | Out-Null } catch { $threw = $true }
        Assert-True $threw 'an existing zip is never overwritten'
    }

    Test-Case 'Get-ZipEntryViolation and Expand-PlaytestZip reject backslash and parent entries before unpacking' {
        $bad = Join-Path $temporary 'bad-entries.zip'
        $stream = New-Object IO.FileStream($bad, [IO.FileMode]::Create)
        $archive = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create)
        try {
            foreach ($name in @('docs\PLAYTEST_GUIDE.md', '../escape.txt', 'fine/a.txt')) {
                $writer = New-Object IO.StreamWriter($archive.CreateEntry($name).Open())
                $writer.Write('x'); $writer.Dispose()
            }
        }
        finally { $archive.Dispose(); $stream.Dispose() }
        $violations = @(Get-ZipEntryViolation -Path $bad)
        Assert-Equal 2 $violations.Count "violations: $($violations -join '; ')"
        Assert-True ($violations[0] -like 'backslash entry: docs\PLAYTEST_GUIDE.md') $violations[0]
        Assert-True ($violations[1] -like 'parent entry: ../escape.txt') $violations[1]
        $destination = Join-Path $temporary 'bad-entries-unpacked'
        $message = ''
        try { Expand-PlaytestZip -Path $bad -Destination $destination } catch { $message = $_.Exception.Message }
        Assert-True ($message -like 'Unsafe archive:*') "message: $message"
        Assert-True (-not (Test-Path -LiteralPath $destination)) 'nothing was unpacked'
    }

    Test-Case 'Test-DocLinkClosure catches a dead relative link and accepts web, anchor and in-package links' {
        $root = Join-Path $temporary 'link-package'
        New-Item -ItemType Directory -Force -Path (Join-Path $root 'docs') | Out-Null
        Write-Utf8NoBom -Path (Join-Path $root 'READ ME.txt') -Text 'r'
        Write-Utf8NoBom -Path (Join-Path $root 'docs\PLAYTEST_GUIDE.md') -Text "# Guide`n[web](https://example.com/JOURNEYS.md) [top](#guide) [self](PLAYTEST_GUIDE.md#guide) [readme](../READ%20ME.txt)`n"
        Assert-Equal 0 @(Test-DocLinkClosure -PackageRoot $root).Count 'closed docs'
        Write-Utf8NoBom -Path (Join-Path $root 'docs\PLAYTEST_GUIDE.md') -Text "# Guide`nSee [JOURNEYS.md](JOURNEYS.md).`nThe [reel](audio/survival-audition.wav) and [out](../../outside.md).`n"
        $dead = @(Test-DocLinkClosure -PackageRoot $root)
        Assert-Equal 3 $dead.Count "dead links: $($dead -join '; ')"
        Assert-Equal "docs/PLAYTEST_GUIDE.md:2: 'JOURNEYS.md' does not resolve inside the package" $dead[0] 'names the file, line and target'
        Assert-True ($dead[2] -like "docs/PLAYTEST_GUIDE.md:3: '../../outside.md'*") $dead[2]
    }

    Test-Case 'New-PlaytestManifest is version 2 with the stamp, commit and curated docs, in a fixed key order' {
        $root = Join-Path $temporary 'manifest-package'
        $manifest = New-TestPackage $root
        Assert-Equal 'version|game|game_version|stamp|commit|commit_short|dirty|branch|profile|features|platform|rustc|created_utc|curated_docs|files' (@($manifest.Keys) -join '|') 'key order'
        Assert-Equal 2 $manifest.version 'version'
        Assert-Equal "v$playtestVersion (0123456)" $manifest.stamp 'stamp'
        Assert-Equal "v$playtestVersion (0123456+dirty)" (Get-BuildStamp -Version $playtestVersion -Commit $playtestCommit -Dirty $true) 'dirty stamp'
        Assert-Equal '0123456+dirty' (Get-BuildCommitLabel -Commit $playtestCommit -Dirty $true) 'LASTLIGHT_BUILD_COMMIT of a dirty tree'
        Assert-Equal "Lastlight-playtest-v$playtestVersion-0123456-20261113-090807" (Get-PlaytestPackageName -Version $playtestVersion -Commit $playtestCommit -CreatedUtc ([DateTime]'2026-11-13T09:08:07Z').ToUniversalTime()) 'package name'
        Assert-Equal 'Lastlight.exe|assets/art/sheet.txt|content/world.json|docs/PLAYTEST_GUIDE.md' (@($manifest.files | ForEach-Object { $_.path }) -join '|') 'files, ordinal order, forward slashes'
        $bytes = [IO.File]::ReadAllBytes((Join-Path $root 'manifest.json'))
        Assert-True ($bytes[0] -eq 0x7B) 'manifest.json has no BOM'
        $read = [IO.File]::ReadAllText((Join-Path $root 'manifest.json')) | ConvertFrom-Json
        Assert-Equal 0 @(Test-PackageIntegrity -Manifest $read -PackageRoot $root).Count 'integrity'
        Assert-Equal 0 @(Test-PlaytestManifest -Manifest $read -PackageRoot $root -GameVersion $playtestVersion).Count 'manifest rules'
    }

    Test-Case 'Test-PlaytestManifest and Test-PackageIntegrity report each broken rule' {
        $root = Join-Path $temporary 'rules-package'
        New-TestPackage $root | Out-Null
        $path = Join-Path $root 'manifest.json'
        function Read-Manifest { return ([IO.File]::ReadAllText($path) | ConvertFrom-Json) }
        function Get-Rules($Manifest) { return (@(Test-PlaytestManifest -Manifest $Manifest -PackageRoot $root -GameVersion $playtestVersion) -join '; ') }
        $m = Read-Manifest; $m.version = 1
        Assert-True ((Get-Rules $m) -like "manifest version is '1', expected 2") (Get-Rules $m)
        $m = Read-Manifest; $m.commit = 'current working tree'
        Assert-True ((Get-Rules $m) -like '*commit is not 40 lower-case hex digits*') (Get-Rules $m)
        $m = Read-Manifest; $m.stamp = 'v9.9.9 (dev build)'
        Assert-True ((Get-Rules $m) -like "*stamp 'v9.9.9 (dev build)' is not*") (Get-Rules $m)
        $m = Read-Manifest; $m.game_version = '9.9.9'
        Assert-True ((Get-Rules $m) -like "*game_version '9.9.9' is not the workspace version*") (Get-Rules $m)
        $m = Read-Manifest; $m.PSObject.Properties.Remove('features')
        Assert-True ((Get-Rules $m) -like "*manifest has no 'features'*") (Get-Rules $m)
        $m = Read-Manifest; $m.files = @($m.files) + [pscustomobject]@{ path = 'docs/GDD.md'; bytes = 5; sha256 = 'x' }
        Assert-True ((Get-Rules $m) -like '*docs are `[PLAYTEST_GUIDE.md, GDD.md`], curated_docs is `[PLAYTEST_GUIDE.md`]*') (Get-Rules $m)
        $m = Read-Manifest; (@($m.files) | Where-Object { $_.path -eq 'docs/PLAYTEST_GUIDE.md' }).bytes = 65537
        Assert-True ((Get-Rules $m) -like '*docs are 65537 B, above 65536 B*') (Get-Rules $m)
        $m = Read-Manifest; $m.files = @($m.files) + [pscustomobject]@{ path = 'archive/art/actors-v1.png'; bytes = 5; sha256 = 'x' }
        Assert-True ((Get-Rules $m) -like '*archived file in the package: archive/art/actors-v1.png*') (Get-Rules $m)
        # Integrity: a changed file, an unlisted file and a manifest path that leaves the package.
        Write-Utf8NoBom -Path (Join-Path $root 'content\world.json') -Text '{ }'
        Write-Utf8NoBom -Path (Join-Path $root 'extra.txt') -Text 'x'
        $m = Read-Manifest; $m.files = @($m.files) + [pscustomobject]@{ path = '../outside.txt'; bytes = 1; sha256 = 'x' }
        $integrity = @(Test-PackageIntegrity -Manifest $m -PackageRoot $root) -join '; '
        Assert-True ($integrity -like '*size differs: content/world.json*') $integrity
        Assert-True ($integrity -like '*unlisted file: extra.txt*') $integrity
        Assert-True ($integrity -like '*manifest path escapes the package: ../outside.txt*') $integrity
    }

    $savedFake = @{}
    foreach ($name in @('HORDE_FAKE_VERSION', 'HORDE_FAKE_SMOKE', 'HORDE_FAKE_SMOKE_EXIT')) { $savedFake[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    try {
        [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $playtestTarget, 'Process')
        [Environment]::SetEnvironmentVariable('HORDE_FAKE_VERSION', "Lastlight: Horde Defense v$playtestVersion (0123456)", 'Process')
        [Environment]::SetEnvironmentVariable('HORDE_FAKE_SMOKE', $null, 'Process')
        [Environment]::SetEnvironmentVariable('HORDE_FAKE_SMOKE_EXIT', $null, 'Process')

        Test-Case 'verify_playtest passes a version 2 package, as a folder and as a zip, with stderr output from the smoke' {
            $root = Join-Path $temporary 'verify-pass'
            New-TestPackage $root | Out-Null
            $r = Invoke-Verify @('-Package', $root) 'verify-folder'
            $log = (Read-Log $r.Out) + (Read-Log $r.Err)
            Assert-Equal 0 $r.ExitCode "folder; $log"
            Assert-True ((Read-Log $r.Out) -match "(?m)^PASS: 4 manifest hashes; v$([regex]::Escape($playtestVersion)) \(0123456\); copied executable smoke; native capture=False") "PASS line; $log"
            $zip = New-PlaytestZip -SourceDirectory $root -Destination (Join-Path $temporary 'verify-pass.zip')
            $r = Invoke-Verify @('-Package', $zip) 'verify-zip'
            Assert-Equal 0 $r.ExitCode ('zip; ' + (Read-Log $r.Out) + (Read-Log $r.Err))
            $unpacked = Join-Path (@(Get-VerificationFolders)[-1].FullName) 'unpacked'
            Assert-True (Test-Path -LiteralPath (Join-Path $unpacked 'docs\PLAYTEST_GUIDE.md') -PathType Leaf) 'the zip was unpacked into folders'
        }

        Test-Case 'verify_playtest refuses -NativeCapture on a package without the review feature before launching anything' {
            $root = Join-Path $temporary 'verify-no-review'
            New-TestPackage $root | Out-Null
            $before = @(Get-VerificationFolders).Count
            $r = Invoke-Verify @('-Package', $root, '-NativeCapture') 'verify-no-review'
            $log = (Read-Log $r.Out) + (Read-Log $r.Err)
            Assert-True ($r.ExitCode -ne 0) "must fail; $log"
            Assert-True ($log -match 'no review feature') "names the cause; $log"
            Assert-True ($r.Seconds -lt 60) "ended by itself in $($r.Seconds) s"
            $folders = @(Get-VerificationFolders)
            Assert-Equal ($before + 1) $folders.Count 'one verification folder'
            Assert-True (-not (Test-Path -LiteralPath (Join-Path $folders[-1].FullName 'logs'))) 'no process was started'
        }

        Test-Case 'verify_playtest fails a version 1 manifest, a changed file, a stale stamp and a failed smoke' {
            $root = Join-Path $temporary 'verify-v1'
            New-TestPackage $root | Out-Null
            $path = Join-Path $root 'manifest.json'
            $m = [IO.File]::ReadAllText($path) | ConvertFrom-Json
            $m.version = 1
            Write-Utf8NoBom -Path $path -Text ($m | ConvertTo-Json -Depth 6)
            $r = Invoke-Verify @('-Package', $root) 'verify-v1'
            $log = (Read-Log $r.Out) + (Read-Log $r.Err)
            Assert-True ($r.ExitCode -ne 0 -and $log -match "manifest version is '1', expected 2") "version 1; $log"

            $root = Join-Path $temporary 'verify-tampered'
            New-TestPackage $root | Out-Null
            Write-Utf8NoBom -Path (Join-Path $root 'assets\art\sheet.txt') -Text 'ART'
            $r = Invoke-Verify @('-Package', $root) 'verify-tampered'
            $log = (Read-Log $r.Out) + (Read-Log $r.Err)
            Assert-True ($r.ExitCode -ne 0 -and $log -match 'sha256 differs: assets/art/sheet.txt') "changed file; $log"

            $root = Join-Path $temporary 'verify-stale'
            New-TestPackage $root | Out-Null
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_VERSION', "Lastlight: Horde Defense v$playtestVersion (dev build)", 'Process')
            try { $r = Invoke-Verify @('-Package', $root) 'verify-stale' }
            finally { [Environment]::SetEnvironmentVariable('HORDE_FAKE_VERSION', "Lastlight: Horde Defense v$playtestVersion (0123456)", 'Process') }
            $log = (Read-Log $r.Out) + (Read-Log $r.Err)
            Assert-True ($r.ExitCode -ne 0 -and $log -match 'dev build' -and $log -match 'manifest stamp') "stale stamp; $log"

            foreach ($case in @(@{ Line = 'PASS: but exit 1'; Exit = '1' }, @{ Line = 'Smoke report without the marker'; Exit = '0' })) {
                [Environment]::SetEnvironmentVariable('HORDE_FAKE_SMOKE', $case.Line, 'Process')
                [Environment]::SetEnvironmentVariable('HORDE_FAKE_SMOKE_EXIT', $case.Exit, 'Process')
                try { $r = Invoke-Verify @('-Package', $root) "verify-smoke-$($case.Exit)" }
                finally {
                    [Environment]::SetEnvironmentVariable('HORDE_FAKE_SMOKE', $null, 'Process')
                    [Environment]::SetEnvironmentVariable('HORDE_FAKE_SMOKE_EXIT', $null, 'Process')
                }
                $log = (Read-Log $r.Out) + (Read-Log $r.Err)
                Assert-True ($r.ExitCode -ne 0 -and $log -match 'campaign smoke failed') "smoke '$($case.Line)' exit $($case.Exit); $log"
            }
        }

        Test-Case 'package_playtest packs a stamped executable into a version 2 zip that verify_playtest passes' {
            $package = Join-Path $scriptsDirectory 'package_playtest.ps1'
            $output = Join-Path $temporary 'packages'
            $git = Invoke-Native -FilePath (Resolve-Tool 'git') -Arguments @('rev-parse', 'HEAD') -LogStem (Join-Path $temporary 'logs\package-git') -TimeoutSeconds 30 -WorkingDirectory $workspace
            $commit = (Read-Log $git.Out).Trim()
            $short = $commit.Substring(0, 7)
            $base = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $package, '-NoBuild', '-Executable', $helper, '-AllowDirty', '-OutputDirectory', $output)
            # An executable that does not carry this tree's stamp is refused, and nothing is left behind.
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_VERSION', "Lastlight: Horde Defense v$playtestVersion (dev build)", 'Process')
            $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $base -LogStem (Join-Path $temporary 'logs\package-stale') -TimeoutSeconds 600
            $log = (Read-Log $r.Out) + (Read-Log $r.Err)
            Assert-True ($r.ExitCode -ne 0 -and $log -match 'expected the stamp') "stale executable; $log"
            Assert-Equal 0 @(Get-ChildItem -LiteralPath $output | Where-Object { $_.Name -like 'Lastlight-playtest-*' }).Count 'no package after a refusal'
            # The fake names both stamps, so the case holds for a clean and for a dirty working tree.
            [Environment]::SetEnvironmentVariable('HORDE_FAKE_VERSION', "Lastlight: Horde Defense v$playtestVersion ($short) v$playtestVersion ($short+dirty)", 'Process')
            $r = Invoke-Native -FilePath (Resolve-Tool 'powershell') -Arguments $base -LogStem (Join-Path $temporary 'logs\package') -TimeoutSeconds 600
            $log = (Read-Log $r.Out) + (Read-Log $r.Err)
            Assert-Equal 0 $r.ExitCode "package_playtest; $log"
            $lines = @((Read-Log $r.Out) -split "`r?`n" | Where-Object { $_ })
            $zip = $lines[0]
            Assert-True ([IO.Path]::GetFileName($zip) -cmatch "^Lastlight-playtest-v$([regex]::Escape($playtestVersion))-$short-\d{8}-\d{6}\.zip$") "zip name: $zip"
            Assert-Equal (Get-FileSha256 -Path $zip) $lines[1] 'the second line is the zip SHA-256'
            Assert-Equal 0 @(Get-ZipEntryViolation -Path $zip).Count 'forward-slash entries only'
            $folder = $zip.Substring(0, $zip.Length - 4)
            $manifest = [IO.File]::ReadAllText((Join-Path $folder 'manifest.json')) | ConvertFrom-Json
            Assert-Equal 2 $manifest.version 'manifest version'
            Assert-Equal $commit $manifest.commit 'commit'
            Assert-Equal $playtestVersion $manifest.game_version 'game version'
            Assert-Equal 0 @($manifest.features).Count 'no review feature'
            Assert-Equal 'PLAYTEST_GUIDE.md' (@($manifest.curated_docs) -join '|') 'curated docs'
            $paths = @($manifest.files | ForEach-Object { $_.path })
            Assert-Equal 'docs/PLAYTEST_GUIDE.md' (@($paths | Where-Object { $_ -like 'docs/*' }) -join '|') 'docs holds the guide only'
            Assert-Equal 0 @($paths | Where-Object { $_ -like 'archive/*' -or $_ -like '*\*' }).Count 'no archive files, no backslash paths'
            foreach ($expected in @('Lastlight.exe', 'LICENSE', 'READ ME.txt', 'Play Lastlight.cmd', 'Battle practice.cmd', 'content/encounters.json', 'content/world.json', 'assets/CREDITS.md')) {
                Assert-True ($paths -ccontains $expected) "the package has $expected"
            }
            $readMe = [IO.File]::ReadAllText((Join-Path $folder 'READ ME.txt'))
            Assert-True ($readMe -match "(?m)^Build: Lastlight $([regex]::Escape($manifest.stamp)), playtest profile, created \d{4}-\d\d-\d\d \d\d:\d\d:\d\d UTC\r?$") "READ ME build line; $readMe"
            Assert-True ([string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable('LASTLIGHT_BUILD_COMMIT', 'Process'))) 'LASTLIGHT_BUILD_COMMIT is not left set'
            $v = Invoke-Verify @('-Package', $zip) 'verify-packaged'
            Assert-Equal 0 $v.ExitCode ('verify_playtest on the packed zip; ' + (Read-Log $v.Out) + (Read-Log $v.Err))
        }
    }
    finally {
        foreach ($name in $savedFake.Keys) { [Environment]::SetEnvironmentVariable($name, $savedFake[$name], 'Process') }
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
